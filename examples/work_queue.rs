use blazing_mpmc::Queue;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    println!("Work Queue Example\n");

    const NUM_WORKERS: usize = 4;
    const NUM_JOBS: usize = 20;

    let jobs = Arc::new(Queue::<String, 128>::new());
    let results = Arc::new(Queue::<String, 128>::new());

    let jobs_tx = jobs.clone();
    let producer = thread::spawn(move || {
        for i in 0..NUM_JOBS {
            let job = format!("Job-{:02}", i);
            while jobs_tx.send(job.clone()).is_err() {
                std::hint::spin_loop();
            }
            println!("📝 Enqueued: {}", job);
            thread::sleep(Duration::from_millis(50));
        }
        println!("✅ All jobs enqueued!");
    });

    let mut workers = vec![];
    for worker_id in 0..NUM_WORKERS {
        let jobs_rx = jobs.clone();
        let results_tx = results.clone();

        workers.push(thread::spawn(move || {
            let mut processed = 0;
            loop {
                match jobs_rx.recv() {
                    Ok(job) => {
                        println!("🔨 Worker {} processing: {}", worker_id, job);
                        
                        thread::sleep(Duration::from_millis(200));
                        
                        let result = format!("{} -> completed by worker {}", job, worker_id);
                        while results_tx.send(result.clone()).is_err() {
                            std::hint::spin_loop();
                        }
                        
                        processed += 1;
                    }
                    Err(_) => {
                        thread::sleep(Duration::from_millis(10));
                        if jobs_rx.is_empty() && processed > 0 {
                            break;
                        }
                    }
                }
            }
            println!("Worker {} finished ({} jobs)", worker_id, processed);
        }));
    }

    let results_rx = results.clone();
    let collector = thread::spawn(move || {
        let mut collected = 0;
        while collected < NUM_JOBS {
            match results_rx.recv() {
                Ok(result) => {
                    println!("✨ Result: {}", result);
                    collected += 1;
                }
                Err(_) => {
                    std::hint::spin_loop();
                }
            }
        }
        println!("✅ All results collected!");
    });

    producer.join().unwrap();
    for worker in workers {
        worker.join().unwrap();
    }
    collector.join().unwrap();

    println!("\n🎉 Work queue example completed!");
}
