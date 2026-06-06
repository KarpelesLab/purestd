//! aarch64 Linux: syscall instruction wrappers, entry point, syscall numbers.
//!
//! This is the backend `fullrust` doesn't have yet. The ABI:
//!
//! ## Syscall ABI
//! Number in `x8`; arguments in `x0..x5`; result in `x0`. Unlike Darwin, Linux
//! returns `-errno` directly in `x0` (no carry flag), so no normalization is
//! needed — [`crate::syscall::from_ret`] handles the negative range.
//!
//! ## Process entry ABI
//! Same initial-stack layout as every Linux arch: `sp -> argc`, then `argv`,
//! NULL, `envp`, NULL, auxv. `_start` must capture `sp` before touching it.
//!
//! Numbers follow the `asm-generic` table shared by aarch64/riscv.

use core::arch::asm;

#[inline]
pub unsafe fn syscall6(
    n: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> usize {
    let ret;
    asm!(
        "svc #0",
        in("x8") n,
        inout("x0") a0 => ret,
        in("x1") a1,
        in("x2") a2,
        in("x3") a3,
        in("x4") a4,
        in("x5") a5,
        options(nostack),
    );
    ret
}

#[inline]
pub unsafe fn syscall0(n: usize) -> usize {
    syscall6(n, 0, 0, 0, 0, 0, 0)
}
#[inline]
pub unsafe fn syscall1(n: usize, a0: usize) -> usize {
    syscall6(n, a0, 0, 0, 0, 0, 0)
}
#[inline]
pub unsafe fn syscall2(n: usize, a0: usize, a1: usize) -> usize {
    syscall6(n, a0, a1, 0, 0, 0, 0)
}
#[inline]
pub unsafe fn syscall3(n: usize, a0: usize, a1: usize, a2: usize) -> usize {
    syscall6(n, a0, a1, a2, 0, 0, 0)
}
#[inline]
pub unsafe fn syscall4(n: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> usize {
    syscall6(n, a0, a1, a2, a3, 0, 0)
}
#[inline]
pub unsafe fn syscall5(n: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> usize {
    syscall6(n, a0, a1, a2, a3, a4, 0)
}

/// Linux/aarch64 syscall numbers (asm-generic table).
pub mod nr {
    pub const MKDIRAT: usize = 34;
    pub const UNLINKAT: usize = 35;
    pub const OPENAT: usize = 56;
    pub const CLOSE: usize = 57;
    pub const LSEEK: usize = 62;
    pub const READ: usize = 63;
    pub const WRITE: usize = 64;
    pub const EXIT: usize = 93;
    pub const EXIT_GROUP: usize = 94;
    pub const CLOCK_GETTIME: usize = 113;
    pub const GETTIMEOFDAY: usize = 169;
    pub const GETPID: usize = 172;
    pub const DUP: usize = 23;
    pub const MUNMAP: usize = 215;
    pub const MMAP: usize = 222;
    pub const GETRANDOM: usize = 278;
    pub const GETENTROPY: usize = GETRANDOM;
    pub const SCHED_YIELD: usize = 124;
    pub const NANOSLEEP: usize = 101;
    pub const FUTEX: usize = 98;
    pub const CLONE: usize = 220;
    pub const GETTID: usize = 178;
    // ---- sockets (asm-generic) ----
    pub const SOCKET: usize = 198;
    pub const BIND: usize = 200;
    pub const LISTEN: usize = 201;
    pub const ACCEPT: usize = 202;
    pub const CONNECT: usize = 203;
    pub const GETSOCKNAME: usize = 204;
    pub const GETPEERNAME: usize = 205;
    pub const SENDTO: usize = 206;
    pub const RECVFROM: usize = 207;
    pub const SETSOCKOPT: usize = 208;
    pub const GETSOCKOPT: usize = 209;
    pub const SHUTDOWN: usize = 210;
    pub const FTRUNCATE: usize = 46;
    pub const FSYNC: usize = 82;
    pub const RENAMEAT: usize = 38;
    pub const FSTAT: usize = 80;
    pub const NEWFSTATAT: usize = 79;
    pub const GETDENTS64: usize = 61;
    pub const CHDIR: usize = 49;
    pub const GETCWD: usize = 17;
}

pub const AT_FDCWD: isize = -100;

pub const PROT_READ: usize = 0x1;
pub const PROT_WRITE: usize = 0x2;
pub const MAP_PRIVATE: usize = 0x2;
pub const MAP_ANONYMOUS: usize = 0x20;
