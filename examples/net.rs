#![no_std]
#![no_main]

use purestd::io::{Read, Write};
use purestd::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use purestd::prelude::*;
use purestd::thread;

fn main() {
    // --- TCP loopback echo (server in a thread) ---
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    println!("listening on {}", addr);
    let server = thread::spawn(move || {
        let (mut conn, peer) = listener.accept().unwrap();
        let mut buf = [0u8; 64];
        let n = conn.read(&mut buf).unwrap();
        conn.write_all(&buf[..n]).unwrap(); // echo back
        peer
    });

    let mut client = TcpStream::connect(addr).unwrap();
    client.write_all(b"ping").unwrap();
    let mut resp = [0u8; 64];
    let n = client.read(&mut resp).unwrap();
    println!("tcp echo got: {:?}", core::str::from_utf8(&resp[..n]).unwrap());
    let peer = server.join().unwrap();
    println!("server accepted peer {}", peer);

    // --- UDP loopback ---
    let a = UdpSocket::bind("127.0.0.1:0").unwrap();
    let b = UdpSocket::bind("127.0.0.1:0").unwrap();
    let b_addr = b.local_addr().unwrap();
    a.send_to(b"hello-udp", b_addr).unwrap();
    let mut ub = [0u8; 64];
    let (m, from) = b.recv_from(&mut ub).unwrap();
    println!("udp got {:?} from {}", core::str::from_utf8(&ub[..m]).unwrap(), from);

    // --- name resolution: localhost via /etc/hosts ---
    let resolved: Vec<_> = ("localhost", 8080).to_socket_addrs().unwrap().collect();
    println!("localhost:8080 -> {:?}", resolved);

    println!("net: OK");
}

purestd::entry!(main);
