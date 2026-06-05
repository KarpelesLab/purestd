#![no_std]
#![no_main]

extern crate purert;

use purestd::prelude::*;

fn main() {
    println!("hello from purestd");
    println!("no libc — just raw syscalls ({} + {} = {})", 2, 2, 2 + 2);

    // Exercise the allocator: Vec, String, format!, Box.
    let mut v: Vec<u32> = Vec::new();
    for i in 0..5 {
        v.push(i * i);
    }
    println!("squares = {:?}", v);

    let s: String = v.iter().map(|n| format!("{n} ")).collect();
    println!("as string = {}", s.trim());

    let boxed = Box::new([0u8; 4096]); // forces a larger allocation
    println!("boxed {} bytes on the heap", boxed.len());

    let big: Vec<u64> = (0..100_000).collect();
    let sum: u64 = big.iter().sum();
    println!("sum 0..100000 = {} ({} elems)", sum, big.len());
}

purestd::entry!(main);
