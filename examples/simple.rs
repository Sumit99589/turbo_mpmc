//! Simple usage example

use blazing_mpmc::Queue;
use std::sync::Arc;
use std::thread;

fn main() {
    println!("Blazing MPMC - Simple Example\n");

    // Create a queue with 16 slots
    let queue = Arc::new(Queue::<String, 16>::new());

    // Clone handles for different threads
    let producer_queue = queue.clone();
    let consumer_queue = queue.clone();

    // Producer thread
    let producer = thread::spawn(move || {
        for i in 0..10 {
            let message = format!("Message {}", i);
            println!("Sending: {}", message);
            
            while producer_queue.send(message.clone()).is_err() {
                // Queue is full, spin and retry
                std::hint::spin_loop();
            }
            
            // Small delay to make output readable
            thread::sleep(std::time::Duration::from_millis(100));
        }
        println!("Producer finished!");
    });

    // Consumer thread
    let consumer = thread::spawn(move || {
        for _ in 0..10 {
            loop {
                match consumer_queue.recv() {
                    Ok(message) => {
                        println!("Received: {}", message);
                        break;
                    }
                    Err(_) => {
                        // Queue is empty, spin and retry
                        std::hint::spin_loop();
                    }
                }
            }
        }
        println!("Consumer finished!");
    });

    // Wait for both threads to complete
    producer.join().unwrap();
    consumer.join().unwrap();

    println!("\nExample completed successfully!");
}
