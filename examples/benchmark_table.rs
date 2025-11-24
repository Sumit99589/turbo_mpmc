use turbo_mpmc::Queue as TurboQueue;
use crossbeam_channel::bounded;
use flume::bounded as flume_bounded;
use std::sync::Arc;
use std::sync::mpsc::sync_channel;
use std::thread;
use std::time::{Duration, Instant};

const MESSAGES: usize = 1_000_000;
const BUFFER_SIZE: usize = 1024;

struct BenchResult {
    name: &'static str,
    duration: Duration,
    throughput: f64,
}

fn main() {
    println!("\n╔═══════════════════════════════════════════════════════════════════════════╗");
    println!("║                    MPMC Queue Benchmark Results                          ║");
    println!("║                       {} messages per test                             ║", MESSAGES);
    println!("╚═══════════════════════════════════════════════════════════════════════════╝\n");

    bench_1p_1c();
    bench_4p_1c();
    bench_1p_4c();
    bench_4p_4c();
}

fn bench_1p_1c() {
    println!("┌───────────────────────────────────────────────────────────────────────────┐");
    println!("│ 1 Producer → 1 Consumer                                                  │");
    println!("├────────────────────────┬──────────────┬─────────────────┬────────────────┤");
    println!("│ Implementation         │ Time (ms)    │ Throughput      │ Speedup        │");
    println!("├────────────────────────┼──────────────┼─────────────────┼────────────────┤");

    let mut results = Vec::new();

    let turbo = run_bench_1p_1c_turbo();
    results.push(turbo);

    let crossbeam = run_bench_1p_1c_crossbeam();
    results.push(crossbeam);

    let flume = run_bench_1p_1c_flume();
    results.push(flume);

    let std_mpsc = run_bench_1p_1c_std();
    results.push(std_mpsc);

    let baseline = results[1].throughput;
    for result in &results {
        let speedup = result.throughput / baseline;
        println!(
            "│ {:<22} │ {:>12.2} │ {:>13.2} M/s │ {:>13.2}x │",
            result.name,
            result.duration.as_secs_f64() * 1000.0,
            result.throughput / 1_000_000.0,
            speedup
        );
    }
    println!("└────────────────────────┴──────────────┴─────────────────┴────────────────┘\n");
}

fn bench_4p_1c() {
    println!("┌───────────────────────────────────────────────────────────────────────────┐");
    println!("│ 4 Producers → 1 Consumer                                                 │");
    println!("├────────────────────────┬──────────────┬─────────────────┬────────────────┤");
    println!("│ Implementation         │ Time (ms)    │ Throughput      │ Speedup        │");
    println!("├────────────────────────┼──────────────┼─────────────────┼────────────────┤");

    let mut results = Vec::new();

    let turbo = run_bench_4p_1c_turbo();
    results.push(turbo);

    let crossbeam = run_bench_4p_1c_crossbeam();
    results.push(crossbeam);

    let flume = run_bench_4p_1c_flume();
    results.push(flume);

    let baseline = results[1].throughput;
    for result in &results {
        let speedup = result.throughput / baseline;
        println!(
            "│ {:<22} │ {:>12.2} │ {:>13.2} M/s │ {:>13.2}x │",
            result.name,
            result.duration.as_secs_f64() * 1000.0,
            result.throughput / 1_000_000.0,
            speedup
        );
    }
    println!("└────────────────────────┴──────────────┴─────────────────┴────────────────┘\n");
}

fn bench_1p_4c() {
    println!("┌───────────────────────────────────────────────────────────────────────────┐");
    println!("│ 1 Producer → 4 Consumers                                                 │");
    println!("├────────────────────────┬──────────────┬─────────────────┬────────────────┤");
    println!("│ Implementation         │ Time (ms)    │ Throughput      │ Speedup        │");
    println!("├────────────────────────┼──────────────┼─────────────────┼────────────────┤");

    let mut results = Vec::new();

    let turbo = run_bench_1p_4c_turbo();
    results.push(turbo);

    let crossbeam = run_bench_1p_4c_crossbeam();
    results.push(crossbeam);

    let flume = run_bench_1p_4c_flume();
    results.push(flume);

    let baseline = results[1].throughput;
    for result in &results {
        let speedup = result.throughput / baseline;
        println!(
            "│ {:<22} │ {:>12.2} │ {:>13.2} M/s │ {:>13.2}x │",
            result.name,
            result.duration.as_secs_f64() * 1000.0,
            result.throughput / 1_000_000.0,
            speedup
        );
    }
    println!("└────────────────────────┴──────────────┴─────────────────┴────────────────┘\n");
}

fn bench_4p_4c() {
    println!("┌───────────────────────────────────────────────────────────────────────────┐");
    println!("│ 4 Producers → 4 Consumers                                                │");
    println!("├────────────────────────┬──────────────┬─────────────────┬────────────────┤");
    println!("│ Implementation         │ Time (ms)    │ Throughput      │ Speedup        │");
    println!("├────────────────────────┼──────────────┼─────────────────┼────────────────┤");

    let mut results = Vec::new();

    let turbo = run_bench_4p_4c_turbo();
    results.push(turbo);

    let crossbeam = run_bench_4p_4c_crossbeam();
    results.push(crossbeam);

    let flume = run_bench_4p_4c_flume();
    results.push(flume);

    let baseline = results[1].throughput;
    for result in &results {
        let speedup = result.throughput / baseline;
        println!(
            "│ {:<22} │ {:>12.2} │ {:>13.2} M/s │ {:>13.2}x │",
            result.name,
            result.duration.as_secs_f64() * 1000.0,
            result.throughput / 1_000_000.0,
            speedup
        );
    }
    println!("└────────────────────────┴──────────────┴─────────────────┴────────────────┘\n");
}

fn run_bench_1p_1c_turbo() -> BenchResult {
    let start = Instant::now();
    let queue = Arc::new(TurboQueue::<usize, BUFFER_SIZE>::new());
    let q_send = queue.clone();
    let q_recv = queue.clone();

    let producer = thread::spawn(move || {
        for i in 0..MESSAGES {
            q_send.send(i);
        }
    });

    let consumer = thread::spawn(move || {
        for _ in 0..MESSAGES {
            let _ = q_recv.recv();
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "turbo_mpmc",
        duration,
        throughput,
    }
}

fn run_bench_1p_1c_crossbeam() -> BenchResult {
    let start = Instant::now();
    let (tx, rx) = bounded::<usize>(BUFFER_SIZE);

    let producer = thread::spawn(move || {
        for i in 0..MESSAGES {
            tx.send(i).unwrap();
        }
    });

    let consumer = thread::spawn(move || {
        for _ in 0..MESSAGES {
            rx.recv().unwrap();
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "crossbeam_channel",
        duration,
        throughput,
    }
}

fn run_bench_1p_1c_flume() -> BenchResult {
    let start = Instant::now();
    let (tx, rx) = flume_bounded::<usize>(BUFFER_SIZE);

    let producer = thread::spawn(move || {
        for i in 0..MESSAGES {
            tx.send(i).unwrap();
        }
    });

    let consumer = thread::spawn(move || {
        for _ in 0..MESSAGES {
            rx.recv().unwrap();
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "flume",
        duration,
        throughput,
    }
}

fn run_bench_1p_1c_std() -> BenchResult {
    let start = Instant::now();
    let (tx, rx) = sync_channel::<usize>(BUFFER_SIZE);

    let producer = thread::spawn(move || {
        for i in 0..MESSAGES {
            tx.send(i).unwrap();
        }
    });

    let consumer = thread::spawn(move || {
        for _ in 0..MESSAGES {
            rx.recv().unwrap();
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "std::mpsc",
        duration,
        throughput,
    }
}

fn run_bench_4p_1c_turbo() -> BenchResult {
    const PRODUCERS: usize = 4;
    const MSGS_PER_PRODUCER: usize = MESSAGES / PRODUCERS;

    let start = Instant::now();
    let queue = Arc::new(TurboQueue::<usize, BUFFER_SIZE>::new());
    let mut handles = vec![];

    for p in 0..PRODUCERS {
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for i in 0..MSGS_PER_PRODUCER {
                q.send(p * MSGS_PER_PRODUCER + i);
            }
        }));
    }

    let q = queue.clone();
    handles.push(thread::spawn(move || {
        for _ in 0..MESSAGES {
            let _ = q.recv();
        }
    }));

    for h in handles {
        h.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "turbo_mpmc",
        duration,
        throughput,
    }
}

fn run_bench_4p_1c_crossbeam() -> BenchResult {
    const PRODUCERS: usize = 4;
    const MSGS_PER_PRODUCER: usize = MESSAGES / PRODUCERS;

    let start = Instant::now();
    let (tx, rx) = bounded::<usize>(BUFFER_SIZE);
    let mut handles = vec![];

    for p in 0..PRODUCERS {
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            for i in 0..MSGS_PER_PRODUCER {
                tx.send(p * MSGS_PER_PRODUCER + i).unwrap();
            }
        }));
    }
    drop(tx);

    handles.push(thread::spawn(move || {
        for _ in 0..MESSAGES {
            rx.recv().unwrap();
        }
    }));

    for h in handles {
        h.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "crossbeam_channel",
        duration,
        throughput,
    }
}

fn run_bench_4p_1c_flume() -> BenchResult {
    const PRODUCERS: usize = 4;
    const MSGS_PER_PRODUCER: usize = MESSAGES / PRODUCERS;

    let start = Instant::now();
    let (tx, rx) = flume_bounded::<usize>(BUFFER_SIZE);
    let mut handles = vec![];

    for p in 0..PRODUCERS {
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            for i in 0..MSGS_PER_PRODUCER {
                tx.send(p * MSGS_PER_PRODUCER + i).unwrap();
            }
        }));
    }
    drop(tx);

    handles.push(thread::spawn(move || {
        for _ in 0..MESSAGES {
            rx.recv().unwrap();
        }
    }));

    for h in handles {
        h.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "flume",
        duration,
        throughput,
    }
}

fn run_bench_1p_4c_turbo() -> BenchResult {
    const CONSUMERS: usize = 4;
    const MSGS_PER_CONSUMER: usize = MESSAGES / CONSUMERS;

    let start = Instant::now();
    let queue = Arc::new(TurboQueue::<usize, BUFFER_SIZE>::new());
    let mut handles = vec![];

    let q = queue.clone();
    handles.push(thread::spawn(move || {
        for i in 0..MESSAGES {
            q.send(i);
        }
    }));

    for _ in 0..CONSUMERS {
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..MSGS_PER_CONSUMER {
                let _ = q.recv();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "turbo_mpmc",
        duration,
        throughput,
    }
}

fn run_bench_1p_4c_crossbeam() -> BenchResult {
    const CONSUMERS: usize = 4;
    const MSGS_PER_CONSUMER: usize = MESSAGES / CONSUMERS;

    let start = Instant::now();
    let (tx, rx) = bounded::<usize>(BUFFER_SIZE);
    let mut handles = vec![];

    handles.push(thread::spawn(move || {
        for i in 0..MESSAGES {
            tx.send(i).unwrap();
        }
    }));

    for _ in 0..CONSUMERS {
        let rx = rx.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..MSGS_PER_CONSUMER {
                rx.recv().unwrap();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "crossbeam_channel",
        duration,
        throughput,
    }
}

fn run_bench_1p_4c_flume() -> BenchResult {
    const CONSUMERS: usize = 4;
    const MSGS_PER_CONSUMER: usize = MESSAGES / CONSUMERS;

    let start = Instant::now();
    let (tx, rx) = flume_bounded::<usize>(BUFFER_SIZE);
    let mut handles = vec![];

    handles.push(thread::spawn(move || {
        for i in 0..MESSAGES {
            tx.send(i).unwrap();
        }
    }));

    for _ in 0..CONSUMERS {
        let rx = rx.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..MSGS_PER_CONSUMER {
                rx.recv().unwrap();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "flume",
        duration,
        throughput,
    }
}

fn run_bench_4p_4c_turbo() -> BenchResult {
    const PRODUCERS: usize = 4;
    const CONSUMERS: usize = 4;
    const MSGS_PER_PRODUCER: usize = MESSAGES / PRODUCERS;
    const MSGS_PER_CONSUMER: usize = MESSAGES / CONSUMERS;

    let start = Instant::now();
    let queue = Arc::new(TurboQueue::<usize, BUFFER_SIZE>::new());
    let mut handles = vec![];

    for p in 0..PRODUCERS {
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for i in 0..MSGS_PER_PRODUCER {
                q.send(p * MSGS_PER_PRODUCER + i);
            }
        }));
    }

    for _ in 0..CONSUMERS {
        let q = queue.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..MSGS_PER_CONSUMER {
                let _ = q.recv();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "turbo_mpmc",
        duration,
        throughput,
    }
}

fn run_bench_4p_4c_crossbeam() -> BenchResult {
    const PRODUCERS: usize = 4;
    const CONSUMERS: usize = 4;
    const MSGS_PER_PRODUCER: usize = MESSAGES / PRODUCERS;
    const MSGS_PER_CONSUMER: usize = MESSAGES / CONSUMERS;

    let start = Instant::now();
    let (tx, rx) = bounded::<usize>(BUFFER_SIZE);
    let mut handles = vec![];

    for p in 0..PRODUCERS {
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            for i in 0..MSGS_PER_PRODUCER {
                tx.send(p * MSGS_PER_PRODUCER + i).unwrap();
            }
        }));
    }
    drop(tx);

    for _ in 0..CONSUMERS {
        let rx = rx.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..MSGS_PER_CONSUMER {
                rx.recv().unwrap();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "crossbeam_channel",
        duration,
        throughput,
    }
}

fn run_bench_4p_4c_flume() -> BenchResult {
    const PRODUCERS: usize = 4;
    const CONSUMERS: usize = 4;
    const MSGS_PER_PRODUCER: usize = MESSAGES / PRODUCERS;
    const MSGS_PER_CONSUMER: usize = MESSAGES / CONSUMERS;

    let start = Instant::now();
    let (tx, rx) = flume_bounded::<usize>(BUFFER_SIZE);
    let mut handles = vec![];

    for p in 0..PRODUCERS {
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            for i in 0..MSGS_PER_PRODUCER {
                tx.send(p * MSGS_PER_PRODUCER + i).unwrap();
            }
        }));
    }
    drop(tx);

    for _ in 0..CONSUMERS {
        let rx = rx.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..MSGS_PER_CONSUMER {
                rx.recv().unwrap();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let duration = start.elapsed();
    let throughput = MESSAGES as f64 / duration.as_secs_f64();

    BenchResult {
        name: "flume",
        duration,
        throughput,
    }
}
