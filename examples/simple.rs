use blazing_mpmc::Queue;
use std::sync::Arc;
use std::thread;

fn main() {
    println!("Blazing MPMC - Simple Example\n");

    let queue = Arc::new(Queue::<String, 16>::new());

    let producer_queue = queue.clone();
    let consumer_queue = queue.clone();

    let producer = thread::spawn(move || {
        for i in 0..10 {
            let message = format!("Message {}", i);
            println!("Sending: {}", message);
            
            while producer_queue.send(message.clone()).is_err() {
                std::hint::spin_loop();
            }
            
            thread::sleep(std::time::Duration::from_millis(100));
        }
        println!("Producer finished!");
    });

    let consumer = thread::spawn(move || {
        for _ in 0..10 {
            loop {
                match consumer_queue.recv() {
                    Ok(message) => {
                        println!("Received: {}", message);
                        break;
                    }
                    Err(_) => {
                        std::hint::spin_loop();
                    }
                }
            }
        }
        println!("Consumer finished!");
    });

    producer.join().unwrap();
    consumer.join().unwrap();

    println!("\nExample completed successfully!");
}
