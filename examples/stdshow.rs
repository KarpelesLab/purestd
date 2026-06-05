#![no_std]
#![no_main]

use purestd::prelude::*;
use purestd::{collections::HashMap, env, fs, io::Write, path::Path, process, sync::Mutex, time};

fn main() -> Result<(), purestd::io::Error> {
    // io::Write trait on stdout
    let mut out = purestd::io::stdout();
    writeln!(out, "== purestd std surface ==").unwrap();
    writeln!(out, "pid = {}", process::id()).unwrap();

    // HashMap (hashbrown, no libc)
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for w in "the quick brown fox the lazy dog the end".split_whitespace() {
        *counts.entry(w).or_insert(0) += 1;
    }
    writeln!(out, "count('the') = {}", counts[&"the"]).unwrap();

    // Mutex
    let m = Mutex::new(0u64);
    {
        let mut g = m.lock().unwrap();
        *g += 41;
        *g += 1;
    }
    writeln!(out, "mutex value = {}", *m.lock().unwrap()).unwrap();

    // fs round-trip via real open/write/read/close syscalls
    let p = "/tmp/purestd_demo.txt";
    fs::write(p, b"persisted by purestd, no libc\n")?;
    let back = fs::read_to_string(p)?;
    write!(out, "file readback: {}", back).unwrap();
    writeln!(out, "path file_name = {:?}", Path::new(p).file_name()).unwrap();
    fs::remove_file(p)?;

    // time
    let t0 = time::Instant::now();
    let mut acc = 0u64;
    for i in 0..1_000_000 {
        acc = acc.wrapping_add(i);
    }
    writeln!(out, "loop acc={} took {:?}", acc, t0.elapsed()).unwrap();

    // env
    writeln!(out, "args: {:?}", env::args().collect::<Vec<_>>()).unwrap();

    writeln!(out, "ok").unwrap();
    Ok(())
}

purestd::entry!(main);
