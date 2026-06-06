//! Darwin (macOS) arm64 syscall primitives.
//!
//! Syscalls are issued with `svc #0x80`; the number goes in `x16`, OR'd with the
//! BSD class selector `0x2000000`. Args are in `x0..x5`. On return the carry
//! flag is set on error and `x0` holds the positive `errno` — we normalize that
//! to the Linux `-errno` convention so [`crate::syscall::from_ret`] is shared
//! across every backend.

use core::arch::asm;

/// Syscall numbers (already OR'd with the BSD UNIX class selector).
pub mod nr {
    const C: usize = 0x2000000;
    pub const EXIT: usize = C | 1;
    pub const READ: usize = C | 3;
    pub const WRITE: usize = C | 4;
    pub const OPEN: usize = C | 5;
    pub const CLOSE: usize = C | 6;
    pub const GETPID: usize = C | 20;
    pub const LSEEK: usize = C | 199;
    pub const GETTIMEOFDAY: usize = C | 116;
    pub const UNLINKAT: usize = C | 472;
    pub const MKDIRAT: usize = C | 475;
    pub const MUNMAP: usize = C | 73;
    pub const MMAP: usize = C | 197;
    pub const OPENAT: usize = C | 463;
    pub const GETENTROPY: usize = C | 500;
    // ---- sockets (BSD numbers) ----
    pub const SOCKET: usize = C | 97;
    pub const CONNECT: usize = C | 98;
    pub const ACCEPT: usize = C | 30;
    pub const BIND: usize = C | 104;
    pub const LISTEN: usize = C | 106;
    pub const GETSOCKNAME: usize = C | 32;
    pub const GETPEERNAME: usize = C | 31;
    pub const SENDTO: usize = C | 133;
    pub const RECVFROM: usize = C | 29;
    pub const SETSOCKOPT: usize = C | 105;
    pub const SHUTDOWN: usize = C | 134;
    /// Darwin has no thread groups; map the Linux name to plain `exit`.
    pub const EXIT_GROUP: usize = EXIT;
    /// Linux has a dedicated `getrandom`; Darwin uses `getentropy`.
    pub const GETRANDOM: usize = GETENTROPY;
}

/// `openat`/`fstatat` "current working directory" sentinel.
pub const AT_FDCWD: isize = -2;

// ---- mmap protection / flags (Darwin values) ----
pub const PROT_READ: usize = 0x1;
pub const PROT_WRITE: usize = 0x2;
pub const MAP_PRIVATE: usize = 0x0002;
pub const MAP_ANONYMOUS: usize = 0x1000;

/// Issue a syscall with up to 6 arguments. Returns the raw value on success or
/// `-errno` on error (carry flag set).
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
    let mut ret: usize;
    let err: usize;
    asm!(
        "svc #0x80",
        "cset {err}, cs",        // err = 1 on carry (error)
        inout("x16") n => _,     // kernel may clobber x16/x17
        lateout("x17") _,
        inout("x0") a0 => ret,
        in("x1") a1,
        in("x2") a2,
        in("x3") a3,
        in("x4") a4,
        in("x5") a5,
        err = lateout(reg) err,
        options(nostack),
    );
    if err != 0 {
        // x0 holds positive errno on error; mirror Linux's -errno encoding.
        (ret as isize).wrapping_neg() as usize
    } else {
        ret
    }
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
