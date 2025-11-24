use turbo_mpmc::Queue;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

const MESSAGES: usize = 1_000_000;
const BUFFER_SIZE: usize = 1024;

fn main() {
    println!("Turbo MPMC Performance Test");
    println!("==============================\n");

    // 1:1 Test
    println!("1 Producer, 1 Consumer ({} messages):", MESSAGES);
    let start = Instant::now();
    test_1p_1c();
    let elapsed = start.elapsed();
    let throughput = MESSAGES as f64 / elapsed.as_secs_f64();
    println!("  Time: {:?}", elapsed);
    println!("  Throughput: {:.2} msgs/sec", throughput);
    println!("  Latency: {:.0} ns/op\n", elapsed.as_nanos() as f64 / MESSAGES as f64);

    // 4:1 Test
    println!("4 Producers, 1 Consumer ({} messages):", MESSAGES);
    let start = Instant::now();
    test_4p_1c();
    let elapsed = start.elapsed();
    let throughput = MESSAGES as f64 / elapsed.as_secs_f64();
    println!("  Time: {:?}", elapsed);
    println!("  Throughput: {:.2} msgs/sec", throughput);
    println!("  Latency: {:.0} ns/op\n", elapsed.as_nanos() as f64 / MESSAGES as f64);

    // 1:4 Test
    println!("1 Producer, 4 Consumers ({} messages):", MESSAGES);
    let start = Instant::now();
    test_1p_4c();
    let elapsed = start.elapsed();
    let throughput = MESSAGES as f64 / elapsed.as_secs_f64();
    println!("  Time: {:?}", elapsed);
    println!("  Throughput: {:.2} msgs/sec", throughput);
    println!("  Latency: {:.0} ns/op\n", elapsed.as_nanos() as f64 / MESSAGES as f64);

    // 4:4 Test
    println!("4 Producers, 4 Consumers ({} messages):", MESSAGES);
    let start = Instant::now();
    test_4p_4c();
    let elapsed = start.elapsed();
    let throughput = MESSAGES as f64 / elapsed.as_secs_f64();
    println!("  Time: {:?}", elapsed);
    println!("  Throughput: {:.2} msgs/sec", throughput);
    println!("  Latency: {:.0} ns/op\n", elapsed.as_nanos() as f64 / MESSAGES as f64);
}

fn test_1p_1c() {
    let queue = Arc::new(Queue::<usize, BUFFER_SIZE>::new());
    let q_send = queue.clone();
    let q_recv = queue.clone();

    let producer = thread::spawn(move || {
        for i in 0..MESSAGES {
            while q_send.send(i).is_err() {
                std::hint::spin_loop();
            }
        }
    });

    let consumer = thread::spawn(move || {
        for _ in 0..MESSAGES {
            while q_recv.recv().is_err() {
                std::hint::spin_loop();
            }
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}

fn test_4p_1c() {
    const PRODUCERS: usize = 4;
    const MSGS_PER_PRODUCER: usize = MESSAGES / PRODUCERS;

    let queue = Arc::new(Queue::<usize, BUFFER_SIZE>::new());
    let mut handles = vec![];

    for p in 0..PRODUCERS {
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for i in 0..MSGS_PER_PRODUCER {
                while q.send(p * MSGS_PER_PRODUCER + i).is_err() {
                    std::hint::spin_loop();
                }
            }
        }));
    }

    let q = queue.clone();
    handles.push(thread::spawn(move || {
        for _ in 0..MESSAGES {
            while q.recv().is_err() {
                std::hint::spin_loop();
            }
        }
    }));

    for h in handles {
        h.join().unwrap();
    }
}

fn test_1p_4c() {
    const CONSUMERS: usize = 4;
    const MSGS_PER_CONSUMER: usize = MESSAGES / CONSUMERS;

    let queue = Arc::new(Queue::<usize, BUFFER_SIZE>::new());
    let mut handles = vec![];

    let q = queue.clone();
    handles.push(thread::spawn(move || {
        for i in 0..MESSAGES {
            while q.send(i).is_err() {
                std::hint::spin_loop();
            }
        }
    }));

    for _ in 0..CONSUMERS {
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..MSGS_PER_CONSUMER {
                while q.recv().is_err() {
                    std::hint::spin_loop();
                }
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

fn test_4p_4c() {
    const PRODUCERS: usize = 4;
    const CONSUMERS: usize = 4;
    const MSGS_PER_PRODUCER: usize = MESSAGES / PRODUCERS;
    const MSGS_PER_CONSUMER: usize = MESSAGES / CONSUMERS;

    let queue = Arc::new(Queue::<usize, BUFFER_SIZE>::new());
    let mut handles = vec![];

    for p in 0..PRODUCERS {
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for i in 0..MSGS_PER_PRODUCER {
                while q.send(p * MSGS_PER_PRODUCER + i).is_err() {
                    std::hint::spin_loop();
                }
            }
        }));
    }

    for _ in 0..CONSUMERS {
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..MSGS_PER_CONSUMER {
                while q.recv().is_err() {
                    std::hint::spin_loop();
                }
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}
