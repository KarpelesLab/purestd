//! `std::net` — placeholder.
//!
//! Socket support (TCP/UDP via `socket`/`connect`/`bind` syscalls) lands after
//! the Linux backend. For now this re-exports the address types from `core::net`
//! so code that only parses/holds addresses compiles unchanged.

pub use core::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
