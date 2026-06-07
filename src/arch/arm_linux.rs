//! arm (32-bit ARMv7 EABI) Linux: `svc #0` syscall wrappers and numbers.
//!
//! ## Syscall ABI
//! Number in `r7`; arguments in `r0..r5`; result in `r0`. Errors come back as
//! `-errno` in `r0`. In ARM mode (the default for `*-gnueabihf`) the frame
//! pointer is `r11`, so `r7` is free to use as an `asm!` operand — matching how
//! rust's own `libc` issues ARM syscalls.
//!
//! A 32-bit arch: 64-bit offsets/lengths use the `*64` variants (`_llseek`,
//! `ftruncate64`, `mmap2`, `*stat64`). Unlike i386, ARM EABI has individual
//! socket syscalls (no `socketcall`).

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
        in("r7") n,
        inout("r0") a0 => ret,
        in("r1") a1,
        in("r2") a2,
        in("r3") a3,
        in("r4") a4,
        in("r5") a5,
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

/// Linux/arm EABI syscall numbers (`unistd-eabi`).
pub mod nr {
    pub const EXIT: usize = 1;
    pub const FORK: usize = 2;
    pub const READ: usize = 3;
    pub const WRITE: usize = 4;
    pub const CLOSE: usize = 6;
    pub const EXECVE: usize = 11;
    pub const CHDIR: usize = 12;
    pub const GETPID: usize = 20;
    pub const DUP: usize = 41;
    pub const DUP2: usize = 63;
    pub const GETTIMEOFDAY: usize = 78;
    pub const MUNMAP: usize = 91;
    pub const WAIT4: usize = 114;
    pub const FSYNC: usize = 118;
    pub const CLONE: usize = 120;
    pub const LLSEEK: usize = 140;
    pub const READV: usize = 145;
    pub const WRITEV: usize = 146;
    pub const SCHED_YIELD: usize = 158;
    pub const NANOSLEEP: usize = 162;
    pub const GETCWD: usize = 183;
    pub const MMAP: usize = 192; // mmap2
    pub const FTRUNCATE: usize = 194; // ftruncate64
    pub const FSTAT: usize = 197; // fstat64
    pub const GETDENTS64: usize = 217;
    pub const FCNTL: usize = 221; // fcntl64
    pub const GETTID: usize = 224;
    pub const FUTEX: usize = 240;
    pub const SCHED_GETAFFINITY: usize = 242;
    pub const EXIT_GROUP: usize = 248;
    pub const CLOCK_GETTIME: usize = 263;
    // ---- sockets (individual; ARM EABI) ----
    pub const SOCKET: usize = 281;
    pub const BIND: usize = 282;
    pub const CONNECT: usize = 283;
    pub const LISTEN: usize = 284;
    pub const ACCEPT: usize = 285;
    pub const GETSOCKNAME: usize = 286;
    pub const GETPEERNAME: usize = 287;
    pub const SENDTO: usize = 290;
    pub const RECVFROM: usize = 292;
    pub const SHUTDOWN: usize = 293;
    pub const SETSOCKOPT: usize = 294;
    pub const GETSOCKOPT: usize = 295;
    pub const FSTATAT64: usize = 327;
    pub const NEWFSTATAT: usize = 327;
    pub const OPENAT: usize = 322;
    pub const MKDIRAT: usize = 323;
    pub const UNLINKAT: usize = 328;
    pub const RENAMEAT: usize = 329;
    pub const DUP3: usize = 358;
    pub const PIPE2: usize = 359;
    pub const RENAMEAT2: usize = 382;
    pub const GETRANDOM: usize = 384;
    pub const GETENTROPY: usize = GETRANDOM;
}

pub const AT_FDCWD: isize = -100;

pub const PROT_READ: usize = 0x1;
pub const PROT_WRITE: usize = 0x2;
pub const MAP_PRIVATE: usize = 0x2;
pub const MAP_ANONYMOUS: usize = 0x20;
