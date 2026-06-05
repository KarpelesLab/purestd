#![no_std]
#![no_main]
use purestd::prelude::*;
use purestd::env;

fn main() {
    println!("argc + argv:");
    for (i, a) in env::args().enumerate() {
        println!("  [{i}] {a}");
    }
    match env::var("HOME") {
        Ok(h) => println!("HOME = {h}"),
        Err(e) => println!("HOME error: {e}"),
    }
    match env::var("PURESTD_TEST") {
        Ok(v) => println!("PURESTD_TEST = {v}"),
        Err(e) => println!("PURESTD_TEST: {e}"),
    }
    let path_present = env::vars().any(|(k, _)| k == "PATH");
    println!("PATH present in env: {path_present}");
}
purestd::entry!(main);
