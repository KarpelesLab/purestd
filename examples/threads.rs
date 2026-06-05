#![no_std]
#![no_main]

#[path = "common/rt.rs"]
mod rt;

use purestd::prelude::*;
use purestd::sync::{Arc, Mutex};
use purestd::{thread, time::Duration};

fn main() {
    // Basic spawn + join returning a value.
    let h = thread::spawn(|| {
        let mut s = 0u64;
        for i in 0..1_000_000u64 { s = s.wrapping_add(i); }
        s
    });
    let sum = h.join().unwrap();
    println!("worker returned {}", sum);

    // Many threads incrementing a shared Mutex.
    let counter = Arc::new(Mutex::new(0u64));
    let mut handles = Vec::new();
    for t in 0..8u64 {
        let c = counter.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..10_000 {
                *c.lock().unwrap() += 1;
            }
            t
        }));
    }
    let mut ids = Vec::new();
    for h in handles {
        ids.push(h.join().unwrap());
    }
    println!("8 threads x 10000 increments => counter = {}", *counter.lock().unwrap());
    println!("joined thread ids: {:?}", ids);

    // sleep
    let t0 = purestd::time::Instant::now();
    thread::sleep(Duration::from_millis(50));
    println!("slept ~{:?}", t0.elapsed());

    println!("threads: OK");
}

purestd::entry!(main);
