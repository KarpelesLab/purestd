//! Hashing — `std::hash`-shaped.
//!
//! Re-exports the `core::hash` traits and adds the concrete hasher `std` uses by
//! default: SipHash-1-3, plus [`RandomState`] (the default `HashMap` hasher,
//! seeded once per process from kernel entropy) and [`DefaultHasher`] (fixed
//! keys). Implementing this ourselves is what lets `HashMap` stay dependency-
//! free — and, because the seed comes from a `getrandom`/`getentropy` syscall,
//! it is the canonical example of a collection that needs the OS.

pub use core::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher};

use crate::sync::OnceLock;

#[inline]
fn u8to64_le(buf: &[u8], start: usize, len: usize) -> u64 {
    let mut out = 0u64;
    let mut i = 0;
    while i < len {
        out |= (buf[start + i] as u64) << (8 * i);
        i += 1;
    }
    out
}

macro_rules! sipround {
    ($v0:expr, $v1:expr, $v2:expr, $v3:expr) => {{
        $v0 = $v0.wrapping_add($v1);
        $v1 = $v1.rotate_left(13);
        $v1 ^= $v0;
        $v0 = $v0.rotate_left(32);
        $v2 = $v2.wrapping_add($v3);
        $v3 = $v3.rotate_left(16);
        $v3 ^= $v2;
        $v0 = $v0.wrapping_add($v3);
        $v3 = $v3.rotate_left(21);
        $v3 ^= $v0;
        $v2 = $v2.wrapping_add($v1);
        $v1 = $v1.rotate_left(17);
        $v1 ^= $v2;
        $v2 = $v2.rotate_left(32);
    }};
}

/// SipHash-1-3 — the `HashMap` default hasher (1 compression round per 8-byte
/// block, 3 finalization rounds). Streaming, so it handles arbitrary `write`
/// chunk boundaries.
#[derive(Clone)]
pub struct SipHasher13 {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    length: usize,
    tail: u64,
    ntail: usize,
}

impl SipHasher13 {
    pub fn new() -> SipHasher13 {
        SipHasher13::new_with_keys(0, 0)
    }

    pub fn new_with_keys(k0: u64, k1: u64) -> SipHasher13 {
        SipHasher13 {
            v0: k0 ^ 0x736f6d6570736575,
            v1: k1 ^ 0x646f72616e646f6d,
            v2: k0 ^ 0x6c7967656e657261,
            v3: k1 ^ 0x7465646279746573,
            length: 0,
            tail: 0,
            ntail: 0,
        }
    }
}

impl Default for SipHasher13 {
    fn default() -> Self {
        SipHasher13::new()
    }
}

impl Hasher for SipHasher13 {
    fn write(&mut self, msg: &[u8]) {
        let len = msg.len();
        self.length += len;

        let mut needed = 0;
        if self.ntail != 0 {
            needed = 8 - self.ntail;
            let take = core::cmp::min(len, needed);
            self.tail |= u8to64_le(msg, 0, take) << (8 * self.ntail);
            if len < needed {
                self.ntail += len;
                return;
            }
            // Completed an 8-byte block from the tail.
            self.v3 ^= self.tail;
            sipround!(self.v0, self.v1, self.v2, self.v3);
            self.v0 ^= self.tail;
            self.ntail = 0;
            self.tail = 0;
        }

        let remaining = len - needed;
        let left = remaining % 8;
        let mut i = needed;
        while i < len - left {
            let mi = u8to64_le(msg, i, 8);
            self.v3 ^= mi;
            sipround!(self.v0, self.v1, self.v2, self.v3);
            self.v0 ^= mi;
            i += 8;
        }

        self.tail = u8to64_le(msg, i, left);
        self.ntail = left;
    }

    fn finish(&self) -> u64 {
        let (mut v0, mut v1, mut v2, mut v3) = (self.v0, self.v1, self.v2, self.v3);
        let b: u64 = ((self.length as u64 & 0xff) << 56) | self.tail;
        v3 ^= b;
        sipround!(v0, v1, v2, v3);
        v0 ^= b;
        v2 ^= 0xff;
        sipround!(v0, v1, v2, v3);
        sipround!(v0, v1, v2, v3);
        sipround!(v0, v1, v2, v3);
        v0 ^ v1 ^ v2 ^ v3
    }
}

/// The default `Hasher` used by [`crate::collections::HashMap`] when no hasher
/// is supplied. Fixed keys, matching `std::hash::DefaultHasher`.
pub struct DefaultHasher(SipHasher13);

impl DefaultHasher {
    pub fn new() -> DefaultHasher {
        DefaultHasher(SipHasher13::new_with_keys(0, 0))
    }
}
impl Default for DefaultHasher {
    fn default() -> Self {
        DefaultHasher::new()
    }
}
impl Hasher for DefaultHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes)
    }
    fn finish(&self) -> u64 {
        self.0.finish()
    }
}

/// Per-process random hash keys, fetched once from the kernel CSPRNG. If the
/// syscall fails (extremely unlikely), we fall back to fixed keys — degraded
/// HashDoS resistance, but still correct.
fn process_keys() -> (u64, u64) {
    static KEYS: OnceLock<(u64, u64)> = OnceLock::new();
    *KEYS.get_or_init(|| {
        let mut buf = [0u8; 16];
        match crate::syscall::getrandom(&mut buf) {
            Ok(()) => (
                u64::from_le_bytes(buf[0..8].try_into().unwrap()),
                u64::from_le_bytes(buf[8..16].try_into().unwrap()),
            ),
            Err(_) => (0x0706050403020100, 0x0f0e0d0c0b0a0908),
        }
    })
}

/// The default `BuildHasher` for `HashMap`. Drop-in for `std`'s `RandomState`:
/// seeded from process-wide kernel entropy so hash iteration order and bucket
/// placement are unpredictable to an attacker.
#[derive(Clone)]
pub struct RandomState {
    k0: u64,
    k1: u64,
}

impl RandomState {
    pub fn new() -> RandomState {
        let (k0, k1) = process_keys();
        RandomState { k0, k1 }
    }
}
impl Default for RandomState {
    fn default() -> Self {
        RandomState::new()
    }
}
impl BuildHasher for RandomState {
    type Hasher = SipHasher13;
    fn build_hasher(&self) -> SipHasher13 {
        SipHasher13::new_with_keys(self.k0, self.k1)
    }
}
