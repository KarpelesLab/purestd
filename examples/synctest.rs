#![no_std]
#![no_main]
use purestd::prelude::*;
use purestd::sync::{mpsc, Arc, Barrier, Condvar, LazyLock, Mutex, RwLock};
use purestd::thread;

static LAZY: LazyLock<u64> = LazyLock::new(|| {
    let mut s = 0u64;
    for i in 1..=100 { s += i; }
    s
});

fn main() {
    // futex Mutex under heavy contention
    let c = Arc::new(Mutex::new(0u64));
    let ts: Vec<_> = (0..16).map(|_| { let c = c.clone();
        thread::spawn(move || for _ in 0..10_000 { *c.lock().unwrap() += 1; }) }).collect();
    for t in ts { t.join().unwrap(); }
    println!("mutex (16x10000) = {}", *c.lock().unwrap());

    // Condvar: producer sets flag, worker waits
    let pair = Arc::new((Mutex::new(false), Condvar::new()));
    let p2 = pair.clone();
    let w = thread::spawn(move || {
        let (lock, cv) = &*p2;
        let g = lock.lock().unwrap();
        let _g = cv.wait_while(g, |ready| !*ready).unwrap();
    });
    thread::sleep(purestd::time::Duration::from_millis(20));
    { let (lock, cv) = &*pair; *lock.lock().unwrap() = true; cv.notify_one(); }
    w.join().unwrap();
    println!("condvar: released");

    // Barrier: 4 threads rendezvous, exactly one leader
    let b = Arc::new(Barrier::new(4));
    let leaders = Arc::new(Mutex::new(0u32));
    let ts: Vec<_> = (0..4).map(|_| { let b = b.clone(); let l = leaders.clone();
        thread::spawn(move || { if b.wait().is_leader() { *l.lock().unwrap() += 1; } }) }).collect();
    for t in ts { t.join().unwrap(); }
    println!("barrier leaders = {} (expect 1)", *leaders.lock().unwrap());

    // mpsc: 4 producers send, one consumer sums
    let (tx, rx) = mpsc::channel();
    let mut hs = Vec::new();
    for p in 0..4u64 {
        let tx = tx.clone();
        hs.push(thread::spawn(move || { for i in 0..100 { tx.send(p * 100 + i).unwrap(); } }));
    }
    drop(tx); // drop the original so recv terminates when producers finish
    for h in hs { h.join().unwrap(); }
    let mut count = 0u64;
    let mut sum = 0u64;
    while let Ok(v) = rx.recv() { count += 1; sum += v; }
    println!("mpsc: got {} msgs, sum {}", count, sum);

    // futex RwLock: many readers + writers contend on the same lock. 8 writers
    // each bump the counter 5000x; 8 readers each take 5000 read locks and
    // observe a monotonically non-decreasing value. Final must be 40000.
    let rw = Arc::new(RwLock::new(0u64));
    let mut hs = Vec::new();
    for _ in 0..8 {
        let rw = rw.clone();
        hs.push(thread::spawn(move || for _ in 0..5_000 { *rw.write().unwrap() += 1; }));
    }
    let bad = Arc::new(Mutex::new(0u64));
    for _ in 0..8 {
        let rw = rw.clone();
        let bad = bad.clone();
        hs.push(thread::spawn(move || {
            let mut last = 0u64;
            for _ in 0..5_000 {
                let v = *rw.read().unwrap();
                if v < last { *bad.lock().unwrap() += 1; }
                last = v;
            }
        }));
    }
    for h in hs { h.join().unwrap(); }
    println!(
        "rwlock (8w*5000) = {} (expect 40000), ordering violations = {} (expect 0)",
        *rw.read().unwrap(),
        *bad.lock().unwrap()
    );

    println!("LazyLock = {} (expect 5050)", *LAZY);
    println!("synctest: OK");
}
purestd::entry!(main);
