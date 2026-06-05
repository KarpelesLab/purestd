#![no_std]
#![no_main]

//! Rough throughput numbers for the from-scratch HashMap, SipHash, and the
//! allocator. Build optimized for speed:
//!
//! ```sh
//! CARGO_PROFILE_RELEASE_OPT_LEVEL=3 cargo run --release --example bench
//! ```

#[path = "common/rt.rs"]
mod rt;

use purestd::collections::HashMap;
use purestd::hash::{Hasher, SipHasher13};
use purestd::prelude::*;
use purestd::time::Instant;
use core::hint::black_box;

fn rate(ops: u64, secs: f64) -> f64 {
    ops as f64 / secs / 1e6 // millions of ops per second
}

fn main() {
    // ---- HashMap<u64,u64> insert ----
    let n: u64 = 1_000_000;
    let t = Instant::now();
    let mut m: HashMap<u64, u64> = HashMap::with_capacity(n as usize);
    for i in 0..n {
        m.insert(black_box(i.wrapping_mul(0x9E3779B97F4A7C15)), i);
    }
    let ins = t.elapsed().as_secs_f64();
    black_box(&m);

    // ---- HashMap get (all present) ----
    let t = Instant::now();
    let mut sum = 0u64;
    for i in 0..n {
        if let Some(v) = m.get(&black_box(i.wrapping_mul(0x9E3779B97F4A7C15))) {
            sum = sum.wrapping_add(*v);
        }
    }
    let get = t.elapsed().as_secs_f64();
    black_box(sum);

    // ---- HashMap get (all miss) ----
    let t = Instant::now();
    let mut miss = 0u64;
    for i in 0..n {
        if m.get(&black_box(i | 0x8000_0000_0000_0000)).is_none() {
            miss += 1;
        }
    }
    let getmiss = t.elapsed().as_secs_f64();
    black_box(miss);

    // ---- SipHash-1-3 throughput on a large buffer ----
    let buf = {
        let mut v: Vec<u8> = Vec::with_capacity(64 << 20); // 64 MiB
        for i in 0..v.capacity() {
            v.push(i as u8);
        }
        v
    };
    let t = Instant::now();
    let mut h = SipHasher13::new_with_keys(1, 2);
    h.write(black_box(&buf));
    black_box(h.finish());
    let sip = t.elapsed().as_secs_f64();
    let mbps = buf.len() as f64 / sip / (1 << 20) as f64;

    // ---- SipHash on tiny (8-byte) keys: the HashMap hot path ----
    let t = Instant::now();
    let mut acc = 0u64;
    for i in 0..n {
        let mut hh = SipHasher13::new_with_keys(1, 2);
        hh.write(&black_box(i).to_le_bytes());
        acc ^= hh.finish();
    }
    let sip8 = t.elapsed().as_secs_f64();
    black_box(acc);

    println!("HashMap<u64,u64>, {} keys:", n);
    println!("  insert : {:7.1} M ops/s  ({:.0} ns/op)", rate(n, ins), ins / n as f64 * 1e9);
    println!("  get hit: {:7.1} M ops/s  ({:.0} ns/op)", rate(n, get), get / n as f64 * 1e9);
    println!("  get miss:{:7.1} M ops/s  ({:.0} ns/op)", rate(n, getmiss), getmiss / n as f64 * 1e9);
    println!("SipHash-1-3:");
    println!("  bulk 64MiB : {:7.0} MiB/s", mbps);
    println!("  8-byte keys: {:7.1} M hashes/s ({:.0} ns/hash)", rate(n, sip8), sip8 / n as f64 * 1e9);
}

purestd::entry!(main);
