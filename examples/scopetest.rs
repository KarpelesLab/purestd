#![no_std]
#![no_main]
use purestd::prelude::*;
use purestd::thread;
fn main() {
    // Scoped threads borrowing local data
    let data = [1u64, 2, 3, 4, 5, 6, 7, 8];
    let mut sums = [0u64; 2];
    thread::scope(|s| {
        let (a, b) = data.split_at(4);
        let (s0, s1) = sums.split_at_mut(1);
        s.spawn(|| { s0[0] = a.iter().sum(); });
        let h = s.spawn(|| -> u64 { b.iter().sum() });
        s1[0] = h.join().unwrap();
    });
    println!("scoped sums = {:?} (expect [10, 26])", sums);

    // Scope returns a value; all threads joined before it returns
    let total = thread::scope(|s| {
        let hs: Vec<_> = (1..=4u64).map(|i| s.spawn(move || i * i)).collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum::<u64>()
    });
    println!("scope returned {} (expect 30)", total);
    println!("scopetest: OK");
}
purestd::entry!(main);
