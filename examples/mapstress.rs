#![no_std]
#![no_main]

//! Stress the from-scratch HashMap: growth across many resizes, get/remove,
//! tombstone reuse, string keys, and entry counting. Aborts (panics) on any
//! invariant violation, so a clean exit 0 means all checks passed.

#[path = "common/rt.rs"]
mod rt;

use purestd::collections::{HashMap, HashSet};
use purestd::prelude::*;

fn main() {
    let n: u64 = 5000;

    // Insert 0..n, forcing many resizes.
    let mut m: HashMap<u64, u64> = HashMap::new();
    for i in 0..n {
        assert!(m.insert(i, i * i).is_none());
    }
    assert_eq!(m.len(), n as usize);

    // All present and correct.
    for i in 0..n {
        assert_eq!(m.get(&i), Some(&(i * i)));
    }
    assert_eq!(m.get(&n), None);

    // Remove evens; check tombstone reuse keeps lookups correct.
    let mut removed = 0;
    for i in (0..n).step_by(2) {
        assert_eq!(m.remove(&i), Some(i * i));
        removed += 1;
    }
    assert_eq!(m.len(), (n as usize) - removed);
    for i in 0..n {
        if i % 2 == 0 {
            assert_eq!(m.get(&i), None);
        } else {
            assert_eq!(m.get(&i), Some(&(i * i)));
        }
    }

    // Re-insert removed keys (reusing tombstones), then update existing.
    for i in (0..n).step_by(2) {
        assert!(m.insert(i, i).is_none());
    }
    assert_eq!(m.len(), n as usize);
    assert_eq!(m.insert(3, 99), Some(9)); // overwrite returns old value
    assert_eq!(m.get(&3), Some(&99));

    // Iteration visits every entry exactly once.
    let mut seen = HashSet::new();
    let mut iters = 0;
    for (k, _) in &m {
        assert!(seen.insert(*k));
        iters += 1;
    }
    assert_eq!(iters, n as usize);

    // String keys + entry() word count.
    let text = "a b a c a b d a b c a";
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for w in text.split_whitespace() {
        *counts.entry(w).or_insert(0) += 1;
    }
    assert_eq!(counts[&"a"], 5);
    assert_eq!(counts[&"b"], 3);
    assert_eq!(counts[&"c"], 2);
    assert_eq!(counts[&"d"], 1);
    assert_eq!(counts.get(&"z"), None);

    // and_modify / or_default.
    let mut e: HashMap<&str, i32> = HashMap::new();
    e.entry("x").and_modify(|v| *v += 1).or_insert(10);
    e.entry("x").and_modify(|v| *v += 1).or_insert(10);
    assert_eq!(e["x"], 11);

    // String (owned) keys via Borrow lookup with &str.
    let mut owned: HashMap<String, u32> = HashMap::new();
    owned.insert(String::from("hello"), 1);
    assert_eq!(owned.get("hello"), Some(&1)); // &str query on String keys

    println!("mapstress: all {} checks passed", n);
}

purestd::entry!(main);
