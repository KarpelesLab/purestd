//! `std::net` subset: `TcpStream`, `TcpListener`, `UdpSocket`, and
//! `ToSocketAddrs`, backed by raw socket syscalls. Name resolution uses
//! `/etc/hosts` plus plain DNS (`/etc/resolv.conf`, A/AAAA over UDP) — there is
//! no NSS.

pub use core::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

use crate::alloc::string::String;
use crate::alloc::vec::Vec;
use crate::io::{self, Error, ErrorKind, Read, Write};
use crate::syscall;

// ---- per-OS socket constants ----
const AF_INET: i32 = 2;
const SOCK_STREAM: i32 = 1;
const SOCK_DGRAM: i32 = 2;
const SHUT_RD: i32 = 0;
const SHUT_WR: i32 = 1;
const SHUT_RDWR: i32 = 2;

#[cfg(target_os = "macos")]
mod c {
    pub const AF_INET6: i32 = 30;
    pub const SOL_SOCKET: i32 = 0xffff;
    pub const SO_REUSEADDR: i32 = 0x0004;
}
#[cfg(not(target_os = "macos"))]
mod c {
    pub const AF_INET6: i32 = 10;
    pub const SOL_SOCKET: i32 = 1;
    pub const SO_REUSEADDR: i32 = 2;
}

// ---------------------------------------------------------------------------
// sockaddr encode / decode (the layout differs between Linux and Darwin)
// ---------------------------------------------------------------------------

fn write_sockaddr(addr: &SocketAddr) -> ([u8; 28], u32) {
    let mut b = [0u8; 28];
    match addr {
        SocketAddr::V4(a) => {
            #[cfg(target_os = "macos")]
            {
                b[0] = 16;
                b[1] = AF_INET as u8;
            }
            #[cfg(not(target_os = "macos"))]
            {
                b[0..2].copy_from_slice(&(AF_INET as u16).to_ne_bytes());
            }
            b[2..4].copy_from_slice(&a.port().to_be_bytes());
            b[4..8].copy_from_slice(&a.ip().octets());
            (b, 16)
        }
        SocketAddr::V6(a) => {
            #[cfg(target_os = "macos")]
            {
                b[0] = 28;
                b[1] = c::AF_INET6 as u8;
            }
            #[cfg(not(target_os = "macos"))]
            {
                b[0..2].copy_from_slice(&(c::AF_INET6 as u16).to_ne_bytes());
            }
            b[2..4].copy_from_slice(&a.port().to_be_bytes());
            b[8..24].copy_from_slice(&a.ip().octets());
            b[24..28].copy_from_slice(&a.scope_id().to_ne_bytes());
            (b, 28)
        }
    }
}

fn read_sockaddr(b: &[u8]) -> Option<SocketAddr> {
    if b.len() < 8 {
        return None;
    }
    #[cfg(target_os = "macos")]
    let fam = b[1] as i32;
    #[cfg(not(target_os = "macos"))]
    let fam = u16::from_ne_bytes([b[0], b[1]]) as i32;

    if fam == AF_INET {
        let port = u16::from_be_bytes([b[2], b[3]]);
        let ip = Ipv4Addr::new(b[4], b[5], b[6], b[7]);
        Some(SocketAddr::V4(SocketAddrV4::new(ip, port)))
    } else if fam == c::AF_INET6 && b.len() >= 28 {
        let port = u16::from_be_bytes([b[2], b[3]]);
        let mut oct = [0u8; 16];
        oct.copy_from_slice(&b[8..24]);
        let scope = u32::from_ne_bytes([b[24], b[25], b[26], b[27]]);
        Some(SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::from(oct), port, 0, scope)))
    } else {
        None
    }
}

#[inline]
fn family_of(addr: &SocketAddr) -> i32 {
    match addr {
        SocketAddr::V4(_) => AF_INET,
        SocketAddr::V6(_) => c::AF_INET6,
    }
}

// ---------------------------------------------------------------------------
// Socket: an owned fd that closes on drop
// ---------------------------------------------------------------------------

struct Socket(i32);

impl Drop for Socket {
    fn drop(&mut self) {
        let _ = syscall::close(self.0);
    }
}

impl Socket {
    fn new(family: i32, ty: i32) -> io::Result<Socket> {
        Ok(Socket(syscall::socket(family, ty, 0).map_err(Error::from)?))
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        let mut b = [0u8; 28];
        let mut len = b.len() as u32;
        syscall::getsockname(self.0, b.as_mut_ptr(), &mut len).map_err(Error::from)?;
        read_sockaddr(&b).ok_or_else(|| Error::from(ErrorKind::InvalidData))
    }
}

// ---------------------------------------------------------------------------
// TcpStream
// ---------------------------------------------------------------------------

/// A TCP connection. Drop-in for `std::net::TcpStream`.
pub struct TcpStream(Socket);

impl TcpStream {
    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
        let mut last = Error::from(ErrorKind::InvalidInput);
        for addr in addr.to_socket_addrs()? {
            match Self::connect_one(&addr) {
                Ok(s) => return Ok(s),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn connect_one(addr: &SocketAddr) -> io::Result<TcpStream> {
        let sock = Socket::new(family_of(addr), SOCK_STREAM)?;
        let (sa, len) = write_sockaddr(addr);
        syscall::connect(sock.0, sa.as_ptr(), len).map_err(Error::from)?;
        Ok(TcpStream(sock))
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        let mut b = [0u8; 28];
        let mut len = b.len() as u32;
        syscall::getpeername(self.0 .0, b.as_mut_ptr(), &mut len).map_err(Error::from)?;
        read_sockaddr(&b).ok_or_else(|| Error::from(ErrorKind::InvalidData))
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let h = match how {
            Shutdown::Read => SHUT_RD,
            Shutdown::Write => SHUT_WR,
            Shutdown::Both => SHUT_RDWR,
        };
        syscall::shutdown(self.0 .0, h).map_err(Error::from)
    }
}

impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        crate::io::Fd(self.0 .0).read(buf)
    }
}
impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        crate::io::Fd(self.0 .0).write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Possible values for `TcpStream::shutdown`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shutdown {
    Read,
    Write,
    Both,
}

// ---------------------------------------------------------------------------
// TcpListener
// ---------------------------------------------------------------------------

/// A TCP socket server. Drop-in for `std::net::TcpListener`.
pub struct TcpListener(Socket);

impl TcpListener {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<TcpListener> {
        let mut last = Error::from(ErrorKind::InvalidInput);
        for addr in addr.to_socket_addrs()? {
            match Self::bind_one(&addr) {
                Ok(l) => return Ok(l),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn bind_one(addr: &SocketAddr) -> io::Result<TcpListener> {
        let sock = Socket::new(family_of(addr), SOCK_STREAM)?;
        let one: i32 = 1;
        let _ = syscall::setsockopt(
            sock.0,
            c::SOL_SOCKET,
            c::SO_REUSEADDR,
            &one as *const i32 as *const u8,
            4,
        );
        let (sa, len) = write_sockaddr(addr);
        syscall::bind(sock.0, sa.as_ptr(), len).map_err(Error::from)?;
        syscall::listen(sock.0, 128).map_err(Error::from)?;
        Ok(TcpListener(sock))
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let mut b = [0u8; 28];
        let mut len = b.len() as u32;
        let fd = loop {
            match syscall::accept(self.0 .0, b.as_mut_ptr(), &mut len) {
                Err(crate::syscall::Errno(4)) => continue, // EINTR
                other => break other.map_err(Error::from)?,
            }
        };
        let peer = read_sockaddr(&b).unwrap_or(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            0,
        )));
        Ok((TcpStream(Socket(fd)), peer))
    }

    /// Iterator over incoming connections.
    pub fn incoming(&self) -> Incoming<'_> {
        Incoming { listener: self }
    }
}

pub struct Incoming<'a> {
    listener: &'a TcpListener,
}
impl Iterator for Incoming<'_> {
    type Item = io::Result<TcpStream>;
    fn next(&mut self) -> Option<io::Result<TcpStream>> {
        Some(self.listener.accept().map(|(s, _)| s))
    }
}

// ---------------------------------------------------------------------------
// UdpSocket
// ---------------------------------------------------------------------------

/// A UDP socket. Drop-in for `std::net::UdpSocket`.
pub struct UdpSocket(Socket);

impl UdpSocket {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<UdpSocket> {
        let mut last = Error::from(ErrorKind::InvalidInput);
        for addr in addr.to_socket_addrs()? {
            let sock = match Socket::new(family_of(&addr), SOCK_DGRAM) {
                Ok(s) => s,
                Err(e) => {
                    last = e;
                    continue;
                }
            };
            let (sa, len) = write_sockaddr(&addr);
            match syscall::bind(sock.0, sa.as_ptr(), len) {
                Ok(()) => return Ok(UdpSocket(sock)),
                Err(e) => last = Error::from(e),
            }
        }
        Err(last)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }

    pub fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()> {
        let addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| Error::from(ErrorKind::InvalidInput))?;
        let (sa, len) = write_sockaddr(&addr);
        syscall::connect(self.0 .0, sa.as_ptr(), len).map_err(Error::from)
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        syscall::sendto(self.0 .0, buf, 0, core::ptr::null(), 0).map_err(Error::from)
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        syscall::recvfrom(self.0 .0, buf, 0, core::ptr::null_mut(), core::ptr::null_mut())
            .map_err(Error::from)
    }

    pub fn send_to<A: ToSocketAddrs>(&self, buf: &[u8], addr: A) -> io::Result<usize> {
        let addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| Error::from(ErrorKind::InvalidInput))?;
        let (sa, len) = write_sockaddr(&addr);
        syscall::sendto(self.0 .0, buf, 0, sa.as_ptr(), len).map_err(Error::from)
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let mut b = [0u8; 28];
        let mut len = b.len() as u32;
        let n = syscall::recvfrom(self.0 .0, buf, 0, b.as_mut_ptr(), &mut len)
            .map_err(Error::from)?;
        let from = read_sockaddr(&b)
            .unwrap_or(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)));
        Ok((n, from))
    }
}

// ---------------------------------------------------------------------------
// ToSocketAddrs
// ---------------------------------------------------------------------------

/// Objects that can be turned into one or more [`SocketAddr`]s.
pub trait ToSocketAddrs {
    type Iter: Iterator<Item = SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter>;
}

impl ToSocketAddrs for SocketAddr {
    type Iter = core::iter::Once<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        Ok(core::iter::once(*self))
    }
}
impl ToSocketAddrs for SocketAddrV4 {
    type Iter = core::iter::Once<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        Ok(core::iter::once(SocketAddr::V4(*self)))
    }
}
impl ToSocketAddrs for SocketAddrV6 {
    type Iter = core::iter::Once<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        Ok(core::iter::once(SocketAddr::V6(*self)))
    }
}
impl ToSocketAddrs for (IpAddr, u16) {
    type Iter = core::iter::Once<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        Ok(core::iter::once(SocketAddr::new(self.0, self.1)))
    }
}
impl ToSocketAddrs for (Ipv4Addr, u16) {
    type Iter = core::iter::Once<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        Ok(core::iter::once(SocketAddr::V4(SocketAddrV4::new(self.0, self.1))))
    }
}

impl ToSocketAddrs for str {
    type Iter = crate::alloc::vec::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        // "host:port" — split on the LAST ':' to tolerate bracketed IPv6.
        let (host, port) = self
            .rsplit_once(':')
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "missing port in address"))?;
        let port: u16 = port
            .parse()
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "invalid port"))?;
        let host = host.trim_start_matches('[').trim_end_matches(']');
        Ok(resolve(host, port)?.into_iter())
    }
}
impl ToSocketAddrs for String {
    type Iter = crate::alloc::vec::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        self.as_str().to_socket_addrs()
    }
}
impl ToSocketAddrs for (&str, u16) {
    type Iter = crate::alloc::vec::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        Ok(resolve(self.0, self.1)?.into_iter())
    }
}
impl<T: ToSocketAddrs + ?Sized> ToSocketAddrs for &T {
    type Iter = T::Iter;
    fn to_socket_addrs(&self) -> io::Result<T::Iter> {
        (**self).to_socket_addrs()
    }
}

// ---------------------------------------------------------------------------
// Name resolution: numeric → /etc/hosts → DNS (A + AAAA over UDP)
// ---------------------------------------------------------------------------

/// Resolve `host` to a list of `SocketAddr`s with the given `port`.
pub fn resolve(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    // 1. Numeric address — no lookup.
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec_one(SocketAddr::new(ip, port)));
    }

    // 2. /etc/hosts.
    if let Some(ip) = hosts_lookup(host) {
        return Ok(vec_one(SocketAddr::new(ip, port)));
    }

    // 3. DNS. Try A then AAAA.
    let mut out = Vec::new();
    if let Some(ns) = nameserver() {
        if let Ok(v4) = dns_query(ns, host, 1) {
            for ip in v4 {
                out.push(SocketAddr::new(ip, port));
            }
        }
        if out.is_empty() {
            if let Ok(v6) = dns_query(ns, host, 28) {
                for ip in v6 {
                    out.push(SocketAddr::new(ip, port));
                }
            }
        }
    }
    if out.is_empty() {
        Err(Error::new(ErrorKind::NotFound, "failed to resolve host"))
    } else {
        Ok(out)
    }
}

fn vec_one(a: SocketAddr) -> Vec<SocketAddr> {
    let mut v = Vec::with_capacity(1);
    v.push(a);
    v
}

fn hosts_lookup(host: &str) -> Option<IpAddr> {
    let text = crate::fs::read_to_string("/etc/hosts").ok()?;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut it = line.split_whitespace();
        let ip = match it.next() {
            Some(ip) => ip,
            None => continue, // blank/comment line
        };
        if it.any(|name| name.eq_ignore_ascii_case(host)) {
            if let Ok(ip) = ip.parse::<IpAddr>() {
                return Some(ip);
            }
        }
    }
    None
}

fn nameserver() -> Option<IpAddr> {
    let text = crate::fs::read_to_string("/etc/resolv.conf").ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("nameserver") {
            if let Ok(ip) = rest.trim().parse::<IpAddr>() {
                return Some(ip);
            }
        }
    }
    None
}

/// Send a DNS query for `host` of `qtype` (1 = A, 28 = AAAA) to `ns:53` and
/// parse the answer records of that type.
fn dns_query(ns: IpAddr, host: &str, qtype: u16) -> io::Result<Vec<IpAddr>> {
    // ---- build the query ----
    let mut id = [0u8; 2];
    let _ = syscall::getrandom(&mut id);
    let mut q: Vec<u8> = Vec::with_capacity(64);
    q.extend_from_slice(&id);
    q.extend_from_slice(&[0x01, 0x00]); // flags: recursion desired
    q.extend_from_slice(&[0x00, 0x01]); // qdcount = 1
    q.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // an/ns/ar = 0
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(Error::from(ErrorKind::InvalidInput));
        }
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0); // root label
    q.extend_from_slice(&qtype.to_be_bytes());
    q.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = IN

    // ---- send / receive over UDP ----
    let sock = UdpSocket::bind(match ns {
        IpAddr::V4(_) => "0.0.0.0:0",
        IpAddr::V6(_) => "[::]:0",
    })?;
    sock.send_to(&q, SocketAddr::new(ns, 53))?;
    let mut buf = [0u8; 1500];
    let n = sock.recv(&mut buf)?;
    let msg = &buf[..n];
    parse_dns_answers(msg, qtype).ok_or_else(|| Error::from(ErrorKind::InvalidData))
}

fn parse_dns_answers(msg: &[u8], qtype: u16) -> Option<Vec<IpAddr>> {
    if msg.len() < 12 {
        return None;
    }
    let qd = u16::from_be_bytes([msg[4], msg[5]]) as usize;
    let an = u16::from_be_bytes([msg[6], msg[7]]) as usize;
    let mut p = 12;
    // Skip the question section.
    for _ in 0..qd {
        p = skip_name(msg, p)?;
        p += 4; // QTYPE + QCLASS
    }
    let mut out = Vec::new();
    for _ in 0..an {
        p = skip_name(msg, p)?;
        if p + 10 > msg.len() {
            break;
        }
        let rtype = u16::from_be_bytes([msg[p], msg[p + 1]]);
        let rdlen = u16::from_be_bytes([msg[p + 8], msg[p + 9]]) as usize;
        p += 10;
        if p + rdlen > msg.len() {
            break;
        }
        if rtype == qtype {
            if qtype == 1 && rdlen == 4 {
                out.push(IpAddr::V4(Ipv4Addr::new(msg[p], msg[p + 1], msg[p + 2], msg[p + 3])));
            } else if qtype == 28 && rdlen == 16 {
                let mut o = [0u8; 16];
                o.copy_from_slice(&msg[p..p + 16]);
                out.push(IpAddr::V6(Ipv6Addr::from(o)));
            }
        }
        p += rdlen;
    }
    Some(out)
}

/// Skip a (possibly compressed) DNS name, returning the offset just past it.
fn skip_name(msg: &[u8], mut p: usize) -> Option<usize> {
    loop {
        let len = *msg.get(p)?;
        if len & 0xc0 == 0xc0 {
            // Compression pointer: two bytes, and the name ends here.
            return Some(p + 2);
        } else if len == 0 {
            return Some(p + 1);
        } else {
            p += 1 + len as usize;
        }
    }
}

impl crate::os::fd::AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> i32 {
        self.0 .0
    }
}
impl crate::os::fd::AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> i32 {
        self.0 .0
    }
}
impl crate::os::fd::AsRawFd for UdpSocket {
    fn as_raw_fd(&self) -> i32 {
        self.0 .0
    }
}
