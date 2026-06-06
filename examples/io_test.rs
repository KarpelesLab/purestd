#![no_std]
#![no_main]
use purestd::io::{self, BufRead, Cursor, Read, Seek, SeekFrom, Write};
use purestd::prelude::*;
fn main() {
    // Cursor read + seek
    let mut cur = Cursor::new(b"hello world".to_vec());
    let mut head = [0u8; 5];
    cur.read_exact(&mut head).unwrap();
    println!("cursor head = {:?}", core::str::from_utf8(&head).unwrap());
    cur.seek(SeekFrom::Start(6)).unwrap();
    let mut tail = String::new();
    cur.read_to_string(&mut tail).unwrap();
    println!("cursor tail = {:?}", tail);

    // BufReader::lines over an in-memory reader
    let data: &[u8] = b"line1\nline2\r\nline3";
    let reader = io::BufReader::new(data);
    let lines: Vec<String> = reader.lines().map(|l| l.unwrap()).collect();
    println!("lines = {:?}", lines);

    // Write to Vec<u8>, then io::copy into another Vec via Cursor
    let mut sink: Vec<u8> = Vec::new();
    write!(sink, "n={}", 42).unwrap();
    let mut src = Cursor::new(b" copied".to_vec());
    io::copy(&mut src, &mut sink).unwrap();
    println!("sink = {:?}", core::str::from_utf8(&sink).unwrap());

    // File round-trip with seek + set_len
    let p = "/tmp/purestd_io_test.txt";
    {
        let mut f = purestd::fs::File::create(p).unwrap();
        f.write_all(b"0123456789").unwrap();
        f.seek(SeekFrom::Start(2)).unwrap();
        f.write_all(b"XY").unwrap();
        f.set_len(6).unwrap();
    }
    println!("file = {:?}", purestd::fs::read_to_string(p).unwrap());
    purestd::fs::remove_file(p).unwrap();

    // Vectored I/O: writev a file from three slices, then readv it back into two.
    {
        use purestd::io::{IoSlice, IoSliceMut};
        let vp = "/tmp/purestd_iovec.txt";
        {
            let mut f = purestd::fs::File::create(vp).unwrap();
            let bufs = [IoSlice::new(b"foo"), IoSlice::new(b"-"), IoSlice::new(b"bar")];
            let n = f.write_vectored(&bufs).unwrap();
            println!("writev wrote {} bytes (expect 7)", n);
        }
        let mut f = purestd::fs::File::open(vp).unwrap();
        let mut a = [0u8; 4];
        let mut b = [0u8; 8];
        let n = {
            let mut bufs = [IoSliceMut::new(&mut a), IoSliceMut::new(&mut b)];
            f.read_vectored(&mut bufs).unwrap()
        };
        println!(
            "readv read {} bytes: {:?}{:?}",
            n,
            core::str::from_utf8(&a).unwrap(),
            core::str::from_utf8(&b[..n.saturating_sub(4)]).unwrap()
        );
        purestd::fs::remove_file(vp).unwrap();
    }

    // monotonic Instant: elapsed should be small and non-negative
    let t = purestd::time::Instant::now();
    let mut acc = 0u64;
    for i in 0..200_000u64 { acc = acc.wrapping_add(i); }
    core::hint::black_box(acc);
    println!("monotonic elapsed = {:?}", t.elapsed());
    assert!(purestd::panic::catch_unwind(|| 2 + 2).unwrap() == 4);
    {
        use purestd::io::Write as _;
        let so = purestd::io::stdout();
        let mut l = so.lock();
        writeln!(l, "stdout lock ok").unwrap();
    }
    println!("io_test: OK");
}
purestd::entry!(main);
