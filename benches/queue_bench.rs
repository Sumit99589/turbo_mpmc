use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::sync::Arc;
use std::thread;

use turbo_mpmc::Queue as TurboQueue;
use crossbeam_channel::bounded;
use flume::bounded as flume_bounded;
use std::sync::mpsc::sync_channel;

const MESSAGES: usize = 1_000_000;
const BUFFER_SIZE: usize = 1024;

fn bench_1p_1c_turbo(c: &mut Criterion) {
    let mut group = c.benchmark_group("1p_1c");
    group.throughput(Throughput::Elements(MESSAGES as u64));

    group.bench_function("turbo_mpmc", |b| {
        b.iter(|| {
            let queue = Arc::new(TurboQueue::<usize, BUFFER_SIZE>::new());
            let q_send = queue.clone();
            let q_recv = queue.clone();

            let producer = thread::spawn(move || {
                for i in 0..MESSAGES {
                    q_send.send(black_box(i));
                }
            });

            let consumer = thread::spawn(move || {
                for _ in 0..MESSAGES {
                    let _ = q_recv.recv();
                }
            });

            producer.join().unwrap();
            consumer.join().unwrap();
        });
    });

    group.bench_function("crossbeam_channel", |b| {
        b.iter(|| {
            let (tx, rx) = bounded::<usize>(BUFFER_SIZE);

            let producer = thread::spawn(move || {
                for i in 0..MESSAGES {
                    tx.send(black_box(i)).unwrap();
                }
            });

            let consumer = thread::spawn(move || {
                for _ in 0..MESSAGES {
                    rx.recv().unwrap();
                }
            });

            producer.join().unwrap();
            consumer.join().unwrap();
        });
    });

    group.bench_function("flume", |b| {
        b.iter(|| {
            let (tx, rx) = flume_bounded::<usize>(BUFFER_SIZE);

            let producer = thread::spawn(move || {
                for i in 0..MESSAGES {
                    tx.send(black_box(i)).unwrap();
                }
            });

            let consumer = thread::spawn(move || {
                for _ in 0..MESSAGES {
                    rx.recv().unwrap();
                }
            });

            producer.join().unwrap();
            consumer.join().unwrap();
        });
    });

    group.bench_function("std_mpsc", |b| {
        b.iter(|| {
            let (tx, rx) = sync_channel::<usize>(BUFFER_SIZE);

            let producer = thread::spawn(move || {
                for i in 0..MESSAGES {
                    tx.send(black_box(i)).unwrap();
                }
            });

            let consumer = thread::spawn(move || {
                for _ in 0..MESSAGES {
                    rx.recv().unwrap();
                }
            });

            producer.join().unwrap();
            consumer.join().unwrap();
        });
    });

    group.finish();
}

fn bench_np_1c(c: &mut Criterion) {
    let mut group = c.benchmark_group("4p_1c");
    group.throughput(Throughput::Elements(MESSAGES as u64));
    const PRODUCERS: usize = 4;
    const MSGS_PER_PRODUCER: usize = MESSAGES / PRODUCERS;

    group.bench_function("turbo_mpmc", |b| {
        b.iter(|| {
            let queue = Arc::new(TurboQueue::<usize, BUFFER_SIZE>::new());
            let mut handles = vec![];

            for p in 0..PRODUCERS {
                let q = queue.clone();
                handles.push(thread::spawn(move || {
                    for i in 0..MSGS_PER_PRODUCER {
                        q.send(black_box(p * MSGS_PER_PRODUCER + i));
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
        });
    });

    // Crossbeam
    group.bench_function("crossbeam_channel", |b| {
        b.iter(|| {
            let (tx, rx) = bounded::<usize>(BUFFER_SIZE);
            let mut handles = vec![];

            for p in 0..PRODUCERS {
                let tx = tx.clone();
                handles.push(thread::spawn(move || {
                    for i in 0..MSGS_PER_PRODUCER {
                        tx.send(black_box(p * MSGS_PER_PRODUCER + i)).unwrap();
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
        });
    });

    group.bench_function("flume", |b| {
        b.iter(|| {
            let (tx, rx) = flume_bounded::<usize>(BUFFER_SIZE);
            let mut handles = vec![];

            for p in 0..PRODUCERS {
                let tx = tx.clone();
                handles.push(thread::spawn(move || {
                    for i in 0..MSGS_PER_PRODUCER {
                        tx.send(black_box(p * MSGS_PER_PRODUCER + i)).unwrap();
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
        });
    });

    group.finish();
}

fn bench_1p_nc(c: &mut Criterion) {
    let mut group = c.benchmark_group("1p_4c");
    group.throughput(Throughput::Elements(MESSAGES as u64));
    const CONSUMERS: usize = 4;
    const MSGS_PER_CONSUMER: usize = MESSAGES / CONSUMERS;

    group.bench_function("turbo_mpmc", |b| {
        b.iter(|| {
            let queue = Arc::new(TurboQueue::<usize, BUFFER_SIZE>::new());
            let mut handles = vec![];

            let q = queue.clone();
            handles.push(thread::spawn(move || {
                for i in 0..MESSAGES {
                    q.send(black_box(i));
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
        });
    });

    // Crossbeam
    group.bench_function("crossbeam_channel", |b| {
        b.iter(|| {
            let (tx, rx) = bounded::<usize>(BUFFER_SIZE);
            let mut handles = vec![];

            handles.push(thread::spawn(move || {
                for i in 0..MESSAGES {
                    tx.send(black_box(i)).unwrap();
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
        });
    });

    // Flume
    group.bench_function("flume", |b| {
        b.iter(|| {
            let (tx, rx) = flume_bounded::<usize>(BUFFER_SIZE);
            let mut handles = vec![];

            handles.push(thread::spawn(move || {
                for i in 0..MESSAGES {
                    tx.send(black_box(i)).unwrap();
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
        });
    });

    group.finish();
}

fn bench_np_mc(c: &mut Criterion) {
    let mut group = c.benchmark_group("4p_4c");
    group.throughput(Throughput::Elements(MESSAGES as u64));
    const PRODUCERS: usize = 4;
    const CONSUMERS: usize = 4;
    const MSGS_PER_PRODUCER: usize = MESSAGES / PRODUCERS;
    const MSGS_PER_CONSUMER: usize = MESSAGES / CONSUMERS;

    group.bench_function("turbo_mpmc", |b| {
        b.iter(|| {
            let queue = Arc::new(TurboQueue::<usize, BUFFER_SIZE>::new());
            let mut handles = vec![];

            for p in 0..PRODUCERS {
                let q = queue.clone();
                handles.push(thread::spawn(move || {
                    for i in 0..MSGS_PER_PRODUCER {
                        q.send(black_box(p * MSGS_PER_PRODUCER + i));
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
        });
    });

    // Crossbeam
    group.bench_function("crossbeam_channel", |b| {
        b.iter(|| {
            let (tx, rx) = bounded::<usize>(BUFFER_SIZE);
            let mut handles = vec![];

            for p in 0..PRODUCERS {
                let tx = tx.clone();
                handles.push(thread::spawn(move || {
                    for i in 0..MSGS_PER_PRODUCER {
                        tx.send(black_box(p * MSGS_PER_PRODUCER + i)).unwrap();
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
        });
    });

    group.bench_function("flume", |b| {
        b.iter(|| {
            let (tx, rx) = flume_bounded::<usize>(BUFFER_SIZE);
            let mut handles = vec![];

            for p in 0..PRODUCERS {
                let tx = tx.clone();
                handles.push(thread::spawn(move || {
                    for i in 0..MSGS_PER_PRODUCER {
                        tx.send(black_box(p * MSGS_PER_PRODUCER + i)).unwrap();
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
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_1p_1c_turbo,
    bench_np_1c,
    bench_1p_nc,
    bench_np_mc
);
criterion_main!(benches);
