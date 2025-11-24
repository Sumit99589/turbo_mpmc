use turbo_mpmc::{Queue, RecvError, SendError};
use std::sync::Arc;
use std::thread;

#[test]
fn test_basic_send_recv() {
    let queue = Queue::<i32, 8>::new();
    
    queue.send(42);
    assert_eq!(queue.recv(), 42);
}

#[test]
fn test_fifo_order() {
    let queue = Queue::<i32, 16>::new();
    
    for i in 0..10 {
        queue.send(i);
    }
    
    for i in 0..10 {
        assert_eq!(queue.recv(), i);
    }
}

#[test]
fn test_full_queue() {
    let queue = Queue::<i32, 4>::new();
    
    for i in 0..4 {
        assert!(queue.try_send(i).is_ok());
    }
    
    assert_eq!(queue.try_send(99), Err(SendError(99)));
}

#[test]
fn test_empty_queue() {
    let queue = Queue::<i32, 4>::new();
    assert_eq!(queue.try_recv(), Err(RecvError));
}

#[test]
fn test_capacity() {
    let queue = Queue::<i32, 1024>::new();
    assert_eq!(queue.capacity(), 1024);
}

#[test]
fn test_len_and_empty() {
    let queue = Queue::<i32, 8>::new();
    
    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
    
    queue.send(1);
    queue.send(2);
    
    assert!(!queue.is_empty());
    assert!(queue.len() > 0);
}

#[test]
fn test_spsc_threaded() {
    let queue = Arc::new(Queue::<usize, 128>::new());
    let q_send = queue.clone();
    let q_recv = queue.clone();
    
    let producer = thread::spawn(move || {
        for i in 0..1000 {
            q_send.send(i);
        }
    });
    
    let consumer = thread::spawn(move || {
        for i in 0..1000 {
            let val = q_recv.recv();
            assert_eq!(val, i);
        }
    });
    
    producer.join().unwrap();
    consumer.join().unwrap();
}

#[test]
fn test_mpsc_threaded() {
    const PRODUCERS: usize = 4;
    const MESSAGES_PER_PRODUCER: usize = 250;
    
    let queue = Arc::new(Queue::<usize, 512>::new());
    let mut handles = vec![];
    
    for p in 0..PRODUCERS {
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for i in 0..MESSAGES_PER_PRODUCER {
                while q.send(p * 10000 + i).is_err() {
                    std::hint::spin_loop();
                }
            }
        }));
    }
    
    let q = queue.clone();
    let consumer = thread::spawn(move || {
        let mut received = vec![];
        for _ in 0..(PRODUCERS * MESSAGES_PER_PRODUCER) {
            loop {
                match q.recv() {
                    Ok(val) => {
                        received.push(val);
                        break;
                    }
                    Err(_) => std::hint::spin_loop(),
                }
            }
        }
        received
    });

    for h in handles {
        h.join().unwrap();
    }
    
    let received = consumer.join().unwrap();
    assert_eq!(received.len(), PRODUCERS * MESSAGES_PER_PRODUCER);
}

#[test]
fn test_spmc_threaded() {
    const CONSUMERS: usize = 4;
    const TOTAL_MESSAGES: usize = 1000;
    
    let queue = Arc::new(Queue::<usize, 512>::new());
    let mut handles = vec![];
    
    let q = queue.clone();
    handles.push(thread::spawn(move || {
        for i in 0..TOTAL_MESSAGES {
            while q.send(i).is_err() {
                std::hint::spin_loop();
            }
        }
    }));
    
    let consumed_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for _ in 0..CONSUMERS {
        let q = queue.clone();
        let count = consumed_count.clone();
        handles.push(thread::spawn(move || {
            loop {
                match q.recv() {
                    Ok(_) => {
                        count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(_) => {
                        if count.load(std::sync::atomic::Ordering::Relaxed) >= TOTAL_MESSAGES {
                            break;
                        }
                        std::hint::spin_loop();
                    }
                }
            }
        }));
    }
    
    for h in handles {
        h.join().unwrap();
    }
    
    assert_eq!(
        consumed_count.load(std::sync::atomic::Ordering::Relaxed),
        TOTAL_MESSAGES
    );
}

#[test]
fn test_mpmc_threaded() {
    const PRODUCERS: usize = 4;
    const CONSUMERS: usize = 4;
    const MESSAGES_PER_PRODUCER: usize = 250;
    const TOTAL_MESSAGES: usize = PRODUCERS * MESSAGES_PER_PRODUCER;
    
    let queue = Arc::new(Queue::<usize, 512>::new());
    let mut handles = vec![];
    
    for p in 0..PRODUCERS {
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for i in 0..MESSAGES_PER_PRODUCER {
                while q.send(p * 10000 + i).is_err() {
                    std::hint::spin_loop();
                }
            }
        }));
    }
    
    let consumed_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for _ in 0..CONSUMERS {
        let q = queue.clone();
        let count = consumed_count.clone();
        handles.push(thread::spawn(move || {
            loop {
                match q.recv() {
                    Ok(_) => {
                        count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(_) => {
                        if count.load(std::sync::atomic::Ordering::Relaxed) >= TOTAL_MESSAGES {
                            break;
                        }
                        std::hint::spin_loop();
                    }
                }
            }
        }));
    }
    
    for h in handles {
        h.join().unwrap();
    }
    
    assert_eq!(
        consumed_count.load(std::sync::atomic::Ordering::Relaxed),
        TOTAL_MESSAGES
    );
}

#[test]
fn test_drop_elements() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
    
    #[derive(Debug)]
    struct DropCounter;
    
    impl Drop for DropCounter {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    {
        let queue = Queue::<DropCounter, 8>::new();
        for _ in 0..5 {
            queue.send(DropCounter).unwrap();
        }
    }
    
    assert_eq!(DROP_COUNT.load(Ordering::Relaxed), 5);
}

#[test]
fn test_stress_rapid_send_recv() {
    let queue = Arc::new(Queue::<usize, 64>::new());
    let q1 = queue.clone();
    let q2 = queue.clone();
    
    let producer = thread::spawn(move || {
        for i in 0..10000 {
            while q1.send(i).is_err() {
                std::hint::spin_loop();
            }
        }
    });
    
    let consumer = thread::spawn(move || {
        for _ in 0..10000 {
            while q2.recv().is_err() {
                std::hint::spin_loop();
            }
        }
    });
    
    producer.join().unwrap();
    consumer.join().unwrap();
}

#[test]
fn test_alternating_send_recv() {
    let queue = Queue::<i32, 4>::new();
    
    for i in 0..100 {
        queue.send(i).unwrap();
        assert_eq!(queue.recv().unwrap(), i);
    }
}

#[test]
fn test_wrap_around() {
    let queue = Queue::<usize, 8>::new();
    
    for round in 0..10 {
        for i in 0..8 {
            queue.send(round * 100 + i).unwrap();
        }
        for i in 0..8 {
            assert_eq!(queue.recv().unwrap(), round * 100 + i);
        }
    }
}

#[test]
#[should_panic(expected = "capacity must be greater than 0")]
fn test_zero_capacity_panics() {
    let _queue = Queue::<i32, 0>::new();
}

#[test]
#[should_panic(expected = "capacity must be a power of 2")]
fn test_non_power_of_2_capacity_panics() {
    let _queue = Queue::<i32, 7>::new();
}

#[test]
fn test_send_error_returns_value() {
    let queue = Queue::<String, 2>::new();
    
    queue.send("first".to_string()).unwrap();
    queue.send("second".to_string()).unwrap();
    
    let result = queue.send("third".to_string());
    match result {
        Err(SendError(value)) => {
            assert_eq!(value, "third");
        }
        _ => panic!("Expected SendError"),
    }
}
