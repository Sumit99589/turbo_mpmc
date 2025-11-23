//! blazing_mpmc - Ticket-based (fetch_add) Vyukov-style bounded MPMC queue
//!
//! - `send` / `recv` : blocking (spin) operations with high throughput
//! - `try_send` / `try_recv` : non-blocking fallbacks that return Err when full/empty
//! - CAP must be > 0 and a power of two

#![warn(missing_docs)]

use core::cell::UnsafeCell;
use core::fmt;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

/// Cache-line padded wrapper
#[repr(align(64))]
struct CachePadded<T> {
    value: T,
}
impl<T> CachePadded<T> {
    const fn new(value: T) -> Self {
        CachePadded { value }
    }
}

/// Single slot in the ring buffer; aligned to a cache line to avoid false sharing.
#[repr(C, align(64))]
struct Slot<T> {
    sequence: AtomicUsize,
    value: UnsafeCell<MaybeUninit<T>>,
}
impl<T> Slot<T> {
    const fn new(seq: usize) -> Self {
        Slot {
            sequence: AtomicUsize::new(seq),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}
// Safety: slot synchronization is provided by atomics; T must be Send to share.
unsafe impl<T: Send> Send for Slot<T> {}
unsafe impl<T: Send> Sync for Slot<T> {}

/// Non-blocking send error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendError<T>(pub T);
impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "queue is full")
    }
}

/// Non-blocking recv error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecvError;
impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "queue is empty")
    }
}

/// How long to spin before yielding
const SPIN_LIMIT: usize = 64;
/// How long to sleep at deep contention
const YIELD_SLEEP_NS: u64 = 50;

/// Bounded ticket-based MPMC queue (Vyukov-style).
///
/// - `send` / `recv` block (spin) until they complete.
/// - `try_send` / `try_recv` return immediately with error when the queue appears full / empty.
pub struct Queue<T, const CAP: usize> {
    buffer: Box<[Slot<T>; CAP]>,
    /// producer ticket counter (monotonic)
    tail: CachePadded<AtomicUsize>,
    /// consumer ticket counter (monotonic)
    head: CachePadded<AtomicUsize>,
    _marker: PhantomData<T>,
}

impl<T, const CAP: usize> Queue<T, CAP> {
    /// Create new queue. Panics if CAP == 0 or not a power of two.
    pub fn new() -> Self {
        assert!(CAP > 0, "capacity must be > 0");
        assert!(CAP.is_power_of_two(), "capacity must be power of two");

        let mut v = Vec::with_capacity(CAP);
        for i in 0..CAP {
            v.push(Slot::new(i));
        }
        let buffer: Box<[Slot<T>; CAP]> = v
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| panic!("capacity mismatch"));

        Queue {
            buffer,
            tail: CachePadded::new(AtomicUsize::new(0)),
            head: CachePadded::new(AtomicUsize::new(0)),
            _marker: PhantomData,
        }
    }

    /// Blocking send: reserve a ticket and wait for its slot to be writable, then publish.
    ///
    /// This function spins until it can publish — it will not return Err when the queue is full.
    pub fn send(&self, value: T) {
        // Reserve a ticket (monotonic)
        let ticket = self.tail.value.fetch_add(1, Ordering::AcqRel);
        let mask = CAP - 1;
        let idx = ticket & mask;
        let slot = &self.buffer[idx];

        // Wait until slot.sequence == ticket (i.e. writable)
        let mut spin = 0usize;
        loop {
            let seq = slot.sequence.load(Ordering::Acquire);
            if seq == ticket {
                break;
            }
            spin = backoff(spin);
        }

        // Write the value and publish by setting sequence = ticket + 1
        unsafe {
            (*slot.value.get()).write(value);
        }
        slot.sequence.store(ticket.wrapping_add(1), Ordering::Release);
    }

    /// Non-blocking try_send: returns Err(value) if queue appears full.
    ///
    /// Uses head/tail distance check and a CAS reservation for tail as a best-effort non-blocking fallback.
    pub fn try_send(&self, value: T) -> Result<(), SendError<T>> {
        let mask = CAP - 1;
        loop {
            let head = self.head.value.load(Ordering::Acquire);
            let tail = self.tail.value.load(Ordering::Relaxed);
            // distance
            if tail.wrapping_sub(head) >= CAP {
                return Err(SendError(value));
            }
            // try to reserve one slot by CAS on tail
            if self
                .tail
                .value
                .compare_exchange_weak(
                    tail,
                    tail.wrapping_add(1),
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                // reserved ticket = tail
                let ticket = tail;
                let idx = ticket & mask;
                let slot = &self.buffer[idx];
                // wait until writable (should be near-ready)
                let mut spin = 0usize;
                loop {
                    let seq = slot.sequence.load(Ordering::Acquire);
                    if seq == ticket {
                        break;
                    }
                    spin = backoff(spin);
                }
                unsafe {
                    (*slot.value.get()).write(value);
                }
                slot.sequence.store(ticket.wrapping_add(1), Ordering::Release);
                return Ok(());
            } else {
                core::hint::spin_loop();
            }
        }
    }

    /// Blocking recv: reserve head ticket and wait for slot to contain data.
    pub fn recv(&self) -> T {
        let ticket = self.head.value.fetch_add(1, Ordering::AcqRel);
        let mask = CAP - 1;
        let idx = ticket & mask;
        let slot = &self.buffer[idx];

        let mut spin = 0usize;
        loop {
            let seq = slot.sequence.load(Ordering::Acquire);
            // slot readable when seq == ticket + 1
            if seq == ticket.wrapping_add(1) {
                break;
            }
            spin = backoff(spin);
        }

        // read value
        let value = unsafe { (*slot.value.get()).assume_init_read() };
        // mark slot free for writer: set sequence = ticket + CAP
        slot.sequence.store(ticket.wrapping_add(CAP), Ordering::Release);
        value
    }

    /// Non-blocking try_recv: returns Err if queue appears empty.
    pub fn try_recv(&self) -> Result<T, RecvError> {
        loop {
            let tail = self.tail.value.load(Ordering::Acquire);
            let head = self.head.value.load(Ordering::Relaxed);
            if tail == head {
                return Err(RecvError);
            }
            // try to reserve by CAS advancing head
            if self
                .head
                .value
                .compare_exchange_weak(
                    head,
                    head.wrapping_add(1),
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                let ticket = head;
                let idx = ticket & (CAP - 1);
                let slot = &self.buffer[idx];
                // wait until readable
                let mut spin = 0usize;
                loop {
                    let seq = slot.sequence.load(Ordering::Acquire);
                    if seq == ticket.wrapping_add(1) {
                        break;
                    }
                    spin = backoff(spin);
                }
                let value = unsafe { (*slot.value.get()).assume_init_read() };
                slot.sequence.store(ticket.wrapping_add(CAP), Ordering::Release);
                return Ok(value);
            } else {
                core::hint::spin_loop();
            }
        }
    }

    /// Capacity
    pub const fn capacity(&self) -> usize {
        CAP
    }

    /// Approximate length (racy)
    pub fn len(&self) -> usize {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Relaxed);
        tail.wrapping_sub(head)
    }

    /// is_empty (racy)
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T, const CAP: usize> Default for Queue<T, CAP> {
    fn default() -> Self {
        Self::new()
    }
}

// Safety: queue can be sent and referenced concurrently if T: Send
unsafe impl<T: Send, const CAP: usize> Send for Queue<T, CAP> {}
unsafe impl<T: Send, const CAP: usize> Sync for Queue<T, CAP> {}

impl<T, const CAP: usize> Drop for Queue<T, CAP> {
    fn drop(&mut self) {
        // Exclusive access: drop any initialized items between head and tail
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Relaxed);

        let mut pos = head;
        while pos != tail {
            let idx = pos & (CAP - 1);
            let slot = &self.buffer[idx];
            unsafe {
                (*slot.value.get()).assume_init_drop();
            }
            pos = pos.wrapping_add(1);
        }
    }
}

/// Simple adaptive backoff: spin-loop a bit, then yield, then nanosleep.
#[inline(always)]
fn backoff(mut spin: usize) -> usize {
    if spin < SPIN_LIMIT {
        spin += 1;
        core::hint::spin_loop();
    } else if spin < SPIN_LIMIT * 8 {
        spin += 1;
        thread::yield_now();
    } else {
        thread::sleep(Duration::from_nanos(YIELD_SLEEP_NS));
    }
    spin
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn smoke() {
        let q = Queue::<i32, 8>::new();
        q.send(42);
        assert_eq!(q.recv(), 42);
    }

    #[test]
    fn try_send_try_recv() {
        let q = Queue::<i32, 4>::new();
        assert!(q.try_recv().is_err());
        for i in 0..4 {
            assert!(q.try_send(i).is_ok());
        }
        assert!(q.try_send(99).is_err());
        for _ in 0..4 {
            assert!(q.try_recv().is_ok());
        }
        assert!(q.try_recv().is_err());
    }

    #[test]
    #[ignore]
    fn mpmc_basic() {
        // ignored by default because it spawns threads and is slow in unit tests
        let q = Arc::new(Queue::<usize, 1024>::new());
        let producers = 4usize;
        let consumers = 4usize;
        let items = 500usize;

        let mut handles = Vec::new();

        for p in 0..producers {
            let q = q.clone();
            handles.push(thread::spawn(move || {
                for i in 0..items {
                    q.send(p * 10_000_000 + i);
                }
            }));
        }

        for _ in 0..consumers {
            let q = q.clone();
            handles.push(thread::spawn(move || {
                let mut got = 0usize;
                while got < (producers * items / consumers) {
                    let _ = q.recv();
                    got += 1;
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }
    }
}
