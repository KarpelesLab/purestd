#![no_std]
#![no_main]
use purestd::prelude::*;
use purestd::{env, fs};

fn main() {
    let dir = "/tmp/purestd_fstest";
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(&format!("{dir}/sub")).unwrap();
    fs::write(&format!("{dir}/a.txt"), b"hello").unwrap();
    fs::write(&format!("{dir}/sub/b.txt"), b"world!!").unwrap();

    // metadata
    let m = fs::metadata(&format!("{dir}/a.txt")).unwrap();
    println!("a.txt len={} is_file={} is_dir={}", m.len(), m.is_file(), m.is_dir());
    let dm = fs::metadata(dir).unwrap();
    println!("dir is_dir={}", dm.is_dir());

    // read_dir (sorted for determinism)
    let mut names: Vec<String> = fs::read_dir(dir).unwrap()
        .map(|e| { let e = e.unwrap(); format!("{}{}", e.file_name(), if e.file_type().unwrap().is_dir() {"/"} else {""}) })
        .collect();
    names.sort();
    println!("entries = {:?}", names);

    // rename + copy
    fs::rename(&format!("{dir}/a.txt"), &format!("{dir}/a2.txt")).unwrap();
    let n = fs::copy(&format!("{dir}/sub/b.txt"), &format!("{dir}/b_copy.txt")).unwrap();
    println!("copied {} bytes; b_copy = {:?}", n, fs::read_to_string(&format!("{dir}/b_copy.txt")).unwrap());

    fs::remove_dir_all(dir).unwrap();
    println!("removed; exists={}", fs::metadata(dir).is_ok());

    // env
    let cwd = env::current_dir().unwrap();
    println!("cwd nonempty = {}", !cwd.as_str().is_empty());
    env::set_var("PURESTD_X", "42");
    println!("PURESTD_X = {:?}", env::var("PURESTD_X"));
    env::remove_var("PURESTD_X");
    println!("after remove = {:?}", env::var("PURESTD_X"));
    println!("temp_dir = {}", env::temp_dir());
    println!("HOME present = {}", env::var("HOME").is_ok());

    {
        use purestd::os::unix::fs::MetadataExt;
        fs::write("/tmp/purestd_mx.txt", b"x").unwrap();
        let m = fs::metadata("/tmp/purestd_mx.txt").unwrap();
        println!("metadata ext: mode={:#o} uid={} ino_nonzero={}", m.mode() & 0o777, m.uid(), m.ino() != 0);
        let _ = fs::remove_file("/tmp/purestd_mx.txt");
    }
    {
        use purestd::collections::HashSet;
        let a: HashSet<i32> = [1,2,3].into_iter().collect();
        let b: HashSet<i32> = [2,3,4].into_iter().collect();
        let mut inter: Vec<_> = a.intersection(&b).copied().collect(); inter.sort();
        println!("hashset intersection = {:?} subset={}", inter, [2,3].into_iter().collect::<HashSet<_>>().is_subset(&a));
    }
    println!("backtrace status = {:?}", purestd::backtrace::Backtrace::capture().status());
    println!("fstest: OK");
}
purestd::entry!(main);
