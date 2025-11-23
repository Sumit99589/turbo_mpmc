#![cfg(loom)]

use blazing_mpmc::Queue;
use loom::sync::Arc;
use loom::thread;

#[test]
fn loom_spsc() {
    loom::model(|| {
        let queue = Arc::new(Queue::<i32, 4>::new());
        let q_send = queue.clone();
        let q_recv = queue.clone();

        let producer = thread::spawn(move || {
            for i in 0..2 {
                while q_send.send(i).is_err() {
                    thread::yield_now();
                }
            }
        });

        let consumer = thread::spawn(move || {
            let mut received = vec![];
            for _ in 0..2 {
                loop {
                    if let Ok(val) = q_recv.recv() {
                        received.push(val);
                        break;
                    }
                    thread::yield_now();
                }
            }
            received
        });

        producer.join().unwrap();
        let received = consumer.join().unwrap();
        assert_eq!(received.len(), 2);
    });
}

#[test]
fn loom_mpsc() {
    loom::model(|| {
        let queue = Arc::new(Queue::<i32, 8>::new());
        let mut handles = vec![];

        // Two producers
        for i in 0..2 {
            let q = queue.clone();
            handles.push(thread::spawn(move || {
                while q.send(i).is_err() {
                    thread::yield_now();
                }
            }));
        }

        // One consumer
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            let mut received = vec![];
            for _ in 0..2 {
                loop {
                    if let Ok(val) = q.recv() {
                        received.push(val);
                        break;
                    }
                    thread::yield_now();
                }
            }
            received
        }));

        for h in handles.into_iter().take(2) {
            h.join().unwrap();
        }
    });
}

#[test]
fn loom_spmc() {
    loom::model(|| {
        let queue = Arc::new(Queue::<i32, 8>::new());
        let mut handles = vec![];

        // One producer
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for i in 0..2 {
                while q.send(i).is_err() {
                    thread::yield_now();
                }
            }
        }));

        // Two consumers
        for _ in 0..2 {
            let q = queue.clone();
            handles.push(thread::spawn(move || {
                loop {
                    if q.recv().is_ok() {
                        break;
                    }
                    thread::yield_now();
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }
    });
}

#[test]
fn loom_mpmc() {
    loom::model(|| {
        let queue = Arc::new(Queue::<i32, 8>::new());
        let mut handles = vec![];

        // Two producers
        for i in 0..2 {
            let q = queue.clone();
            handles.push(thread::spawn(move || {
                while q.send(i * 10).is_err() {
                    thread::yield_now();
                }
            }));
        }

        // Two consumers
        for _ in 0..2 {
            let q = queue.clone();
            handles.push(thread::spawn(move || {
                loop {
                    if q.recv().is_ok() {
                        break;
                    }
                    thread::yield_now();
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }
    });
}

#[test]
fn loom_full_queue() {
    loom::model(|| {
        let queue = Arc::new(Queue::<i32, 2>::new());
        let q1 = queue.clone();
        let q2 = queue.clone();

        let t1 = thread::spawn(move || {
            q1.send(1).ok();
        });

        let t2 = thread::spawn(move || {
            q2.send(2).ok();
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // At least one send should succeed
        let mut count = 0;
        while queue.recv().is_ok() {
            count += 1;
        }
        assert!(count > 0 && count <= 2);
    });
}

#[test]
fn loom_empty_queue() {
    loom::model(|| {
        let queue = Arc::new(Queue::<i32, 4>::new());
        let q1 = queue.clone();
        let q2 = queue.clone();

        let t1 = thread::spawn(move || {
            q1.recv().ok();
        });

        let t2 = thread::spawn(move || {
            q2.send(42).ok();
        });

        t1.join().unwrap();
        t2.join().unwrap();
    });
}

#[test]
fn loom_concurrent_push_pop() {
    loom::model(|| {
        let queue = Arc::new(Queue::<usize, 4>::new());
        
        let q1 = queue.clone();
        let q2 = queue.clone();
        let q3 = queue.clone();

        let h1 = thread::spawn(move || {
            q1.send(1).ok();
        });

        let h2 = thread::spawn(move || {
            q2.recv().ok();
        });

        let h3 = thread::spawn(move || {
            q3.send(2).ok();
        });

        h1.join().unwrap();
        h2.join().unwrap();
        h3.join().unwrap();
    });
}
