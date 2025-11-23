//! blazing_mpmc - Ticket-based (fetch_add) Vyukov-style bounded MPMC queue
//!
//! Adds batch send/recv APIs to amortize atomic operations and improve throughput
#![warn(missing_docs)]

use core::cell::UnsafeCell;
use core::fmt;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

#[repr(align(64))]
struct CachePadded<T> { value: T }
impl<T> CachePadded<T> { const fn new(value: T) -> Self { CachePadded { value } } }

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
unsafe impl<T: Send> Send for Slot<T> {}
unsafe impl<T: Send> Sync for Slot<T> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendError<T>(pub T);
impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "queue is full") }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecvError;
impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "queue is empty") }
}

const SPIN_LIMIT: usize = 64;

/// Bounded ticket-based MPMC queue (Vyukov-style) with batch APIs.
pub struct Queue<T, const CAP: usize> {
    buffer: Box<[Slot<T>; CAP]>,
    tail: CachePadded<AtomicUsize>,
    head: CachePadded<AtomicUsize>,
    _marker: PhantomData<T>,
}

impl<T, const CAP: usize> Queue<T, CAP> {
    pub fn new() -> Self {
        assert!(CAP > 0, "capacity must be > 0");
        assert!(CAP.is_power_of_two(), "capacity must be power of two");

        let mut v = Vec::with_capacity(CAP);
        for i in 0..CAP { v.push(Slot::new(i)); }
        let buffer: Box<[Slot<T>; CAP]> = v.into_boxed_slice().try_into().unwrap_or_else(|_| panic!("capacity mismatch"));

        Queue {
            buffer,
            tail: CachePadded::new(AtomicUsize::new(0)),
            head: CachePadded::new(AtomicUsize::new(0)),
            _marker: PhantomData,
        }
    }

    /// Blocking send: single-element API (unchanged)
    pub fn send(&self, value: T) {
        let ticket = self.tail.value.fetch_add(1, Ordering::Relaxed);
        let mask = CAP - 1;
        let idx = ticket & mask;
        let slot = &self.buffer[idx];

        let mut spin = 0usize;
        loop {
            let seq = slot.sequence.load(Ordering::Acquire);
            if seq == ticket { break; }
            spin = backoff(spin);
        }

        unsafe { (*slot.value.get()).write(value); }
        slot.sequence.store(ticket.wrapping_add(1), Ordering::Release);
    }

    /// Blocking recv: single-element API (unchanged)
    pub fn recv(&self) -> T {
        let ticket = self.head.value.fetch_add(1, Ordering::Relaxed);
        let mask = CAP - 1;
        let idx = ticket & mask;
        let slot = &self.buffer[idx];

        let mut spin = 0usize;
        loop {
            let seq = slot.sequence.load(Ordering::Acquire);
            if seq == ticket.wrapping_add(1) { break; }
            spin = backoff(spin);
        }

        let value = unsafe { (*slot.value.get()).assume_init_read() };
        slot.sequence.store(ticket.wrapping_add(CAP), Ordering::Release);
        value
    }

    /// Non-blocking try_send
    pub fn try_send(&self, value: T) -> Result<(), SendError<T>> {
        let mask = CAP - 1;
        loop {
            let head = self.head.value.load(Ordering::Acquire);
            let tail = self.tail.value.load(Ordering::Relaxed);
            if tail.wrapping_sub(head) >= CAP { return Err(SendError(value)); }
            if self.tail.value.compare_exchange_weak(
                tail, tail.wrapping_add(1), Ordering::Relaxed, Ordering::Relaxed).is_ok()
            {
                let ticket = tail;
                let idx = ticket & mask;
                let slot = &self.buffer[idx];
                let mut spin = 0usize;
                loop {
                    let seq = slot.sequence.load(Ordering::Acquire);
                    if seq == ticket { break; }
                    spin = backoff(spin);
                }
                unsafe { (*slot.value.get()).write(value); }
                slot.sequence.store(ticket.wrapping_add(1), Ordering::Release);
                return Ok(());
            } else { core::hint::spin_loop(); }
        }
    }

    /// Non-blocking try_recv
    pub fn try_recv(&self) -> Result<T, RecvError> {
        loop {
            let tail = self.tail.value.load(Ordering::Acquire);
            let head = self.head.value.load(Ordering::Relaxed);
            if tail == head { return Err(RecvError); }
            if self.head.value.compare_exchange_weak(
                head, head.wrapping_add(1), Ordering::Relaxed, Ordering::Relaxed).is_ok()
            {
                let ticket = head;
                let idx = ticket & (CAP - 1);
                let slot = &self.buffer[idx];
                let mut spin = 0usize;
                loop {
                    let seq = slot.sequence.load(Ordering::Acquire);
                    if seq == ticket.wrapping_add(1) { break; }
                    spin = backoff(spin);
                }
                let value = unsafe { (*slot.value.get()).assume_init_read() };
                slot.sequence.store(ticket.wrapping_add(CAP), Ordering::Release);
                return Ok(value);
            } else { core::hint::spin_loop(); }
        }
    }

    // ---------------------------------------------------------------------
    // BATCH APIs: these amortize atomic operations across multiple messages
    // ---------------------------------------------------------------------

    /// Send many items in one reservation.
    /// Consumes the Vec<T> and publishes all items. Order is preserved: the first element of the vec
    /// will be published to the earliest reserved slot.
    pub fn send_batch(&self, mut items: Vec<T>) {
        let n = items.len();
        if n == 0 { return; }
        // Reserve n tickets in one atomic
        let first_ticket = self.tail.value.fetch_add(n, Ordering::Relaxed);
        let mask = CAP - 1;

        // We will publish in order: items[0] -> ticket first_ticket
        // To consume items without extra copies, pop from the end and store to ticket+ (n-1-i).
        // Simpler and cheap: iterate index and move out using swap_remove(0) is O(n^2),
        // so instead we reverse once then pop (O(n)).
        items.reverse(); // now pop() yields original first element last -> we'll write accordingly
        for i in 0..n {
            let ticket = first_ticket.wrapping_add(i);
            let idx = ticket & mask;
            let slot = &self.buffer[idx];

            // wait until writable
            let mut spin = 0usize;
            loop {
                let seq = slot.sequence.load(Ordering::Acquire);
                if seq == ticket { break; }
                spin = backoff(spin);
            }

            let v = items.pop().expect("item present");
            unsafe { (*slot.value.get()).write(v); }
            slot.sequence.store(ticket.wrapping_add(1), Ordering::Release);
        }
    }

    /// Reserve n items in one go and return them as a Vec<T>.
    /// Blocks until all n items are available.
    pub fn recv_batch(&self, n: usize) -> Vec<T> {
        if n == 0 { return Vec::new(); }
        // Reserve n tickets in one atomic
        let first_ticket = self.head.value.fetch_add(n, Ordering::Relaxed);
        let mask = CAP - 1;
        let mut out = Vec::with_capacity(n);

        for i in 0..n {
            let ticket = first_ticket.wrapping_add(i);
            let idx = ticket & mask;
            let slot = &self.buffer[idx];

            let mut spin = 0usize;
            loop {
                let seq = slot.sequence.load(Ordering::Acquire);
                if seq == ticket.wrapping_add(1) { break; }
                spin = backoff(spin);
            }

            let v = unsafe { (*slot.value.get()).assume_init_read() };
            slot.sequence.store(ticket.wrapping_add(CAP), Ordering::Release);
            out.push(v);
        }
        out
    }

    // ---------------------------------------------------------------------

    pub const fn capacity(&self) -> usize { CAP }
    pub fn len(&self) -> usize {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Relaxed);
        tail.wrapping_sub(head)
    }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

impl<T, const CAP: usize> Default for Queue<T, CAP> { fn default() -> Self { Self::new() } }

unsafe impl<T: Send, const CAP: usize> Send for Queue<T, CAP> {}
unsafe impl<T: Send, const CAP: usize> Sync for Queue<T, CAP> {}

impl<T, const CAP: usize> Drop for Queue<T, CAP> {
    fn drop(&mut self) {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Relaxed);
        let mut pos = head;
        while pos != tail {
            let idx = pos & (CAP - 1);
            let slot = &self.buffer[idx];
            unsafe { (*slot.value.get()).assume_init_drop(); }
            pos = pos.wrapping_add(1);
        }
    }
}

/// Lighter backoff: spin a bit then yield.
#[inline(always)]
fn backoff(mut spin: usize) -> usize {
    if spin < SPIN_LIMIT {
        spin += 1;
        core::hint::spin_loop();
    } else {
        thread::yield_now();
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
        for i in 0..4 { assert!(q.try_send(i).is_ok()); }
        assert!(q.try_send(99).is_err());
        for _ in 0..4 { assert!(q.try_recv().is_ok()); }
        assert!(q.try_recv().is_err());
    }

    #[test]
    fn batch_roundtrip() {
        let q = Queue::<usize, 64>::new();
        q.send_batch(vec![1,2,3,4]);
        let v = q.recv_batch(4);
        assert_eq!(v, vec![1,2,3,4]);
    }
}
