#![no_std]
#![no_main]
use purestd::prelude::*;
use purestd::net::{TcpListener, TcpStream};
use purestd::path::Path;
use purestd::thread;
use purestd::time::Duration;

fn main() {
    // available_parallelism
    let n = thread::available_parallelism().unwrap();
    println!("cpus = {} (>=1: {})", n, n.get() >= 1);

    // path methods
    let p = Path::new("/usr/local/bin/rustc");
    println!("stem={:?} ext={:?}", p.file_stem(), p.extension());
    println!("starts_with /usr = {}", p.starts_with("/usr"));
    println!("strip /usr/local = {:?}", p.strip_prefix("/usr/local").map(|x| x.as_str()));
    println!("with_ext exe = {}", p.with_extension(&"exe").display());
    println!("/bin exists = {}, is_dir = {}", Path::new("/bin").exists(), Path::new("/bin").is_dir());

    // Components: leading RootDir, then normals; "." dropped, ".." kept.
    let comps: Vec<&str> = Path::new("/usr/./local/../bin")
        .components()
        .map(|c| c.as_str())
        .collect();
    println!("components = {:?}", comps);
    // ancestors: self, then each parent down to root.
    let anc: Vec<&str> = Path::new("/a/b/c").ancestors().map(|p| p.as_str()).collect();
    println!("ancestors = {:?}", anc);

    // net options on a loopback connection
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let srv = thread::spawn(move || { let _ = l.accept().unwrap(); });
    let s = TcpStream::connect(addr).unwrap();
    s.set_nodelay(true).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
    s.set_nonblocking(false).unwrap();
    let s2 = s.try_clone().unwrap();
    println!("net opts set ok; clone fd != orig: {}", {
        use purestd::os::fd::AsRawFd; s2.as_raw_fd() != s.as_raw_fd()
    });
    drop(s); drop(s2);
    srv.join().unwrap();

    println!("misc_test: OK");
}
purestd::entry!(main);
