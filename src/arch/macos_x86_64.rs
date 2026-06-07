//! Darwin (macOS) x86-64 syscall primitives.
//!
//! Syscalls use the `syscall` instruction; the number goes in `rax`, OR'd with
//! the BSD class selector `0x2000000` (already baked into the [`nr`] table —
//! identical to the arm64 backend, since Darwin's syscall numbers are
//! arch-independent). Args are in `rdi, rsi, rdx, r10, r8, r9` (`r10` rather than
//! `rcx`, which `syscall` clobbers along with `r11`). On error the carry flag is
//! set and `rax` holds the positive `errno`; we capture CF with `setc` and
//! normalize to the Linux `-errno` convention that [`crate::syscall::from_ret`]
//! shares across backends.

use core::arch::asm;

/// Syscall numbers (already OR'd with the BSD UNIX class selector).
pub mod nr {
    const C: usize = 0x2000000;
    pub const EXIT: usize = C | 1;
    pub const READ: usize = C | 3;
    pub const WRITE: usize = C | 4;
    pub const READV: usize = C | 120;
    pub const WRITEV: usize = C | 121;
    pub const OPEN: usize = C | 5;
    pub const CLOSE: usize = C | 6;
    pub const GETPID: usize = C | 20;
    pub const DUP: usize = C | 41;
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
    pub const GETSOCKOPT: usize = C | 118;
    pub const SHUTDOWN: usize = C | 134;
    pub const FTRUNCATE: usize = C | 201;
    pub const FSYNC: usize = C | 95;
    pub const RENAMEAT: usize = C | 465;
    pub const FSTAT: usize = C | 339; // fstat64 (64-bit inode struct stat)
    pub const STAT64: usize = C | 338;
    pub const LSTAT64: usize = C | 340;
    pub const GETDIRENTRIES64: usize = C | 344;
    pub const CHDIR: usize = C | 12;
    pub const FCNTL: usize = C | 92;
    pub const FORK: usize = C | 2;
    pub const EXECVE: usize = C | 59;
    pub const WAIT4: usize = C | 7;
    pub const DUP2: usize = C | 90;
    pub const PIPE: usize = C | 42;
    pub const SYSCTL: usize = C | 202;
    pub const THREAD_SELFID: usize = C | 372;
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
    let ret: usize;
    let err: u8;
    asm!(
        "syscall",
        "setc {err}",            // err = 1 on carry (error)
        inlateout("rax") n => ret,
        in("rdi") a0,
        in("rsi") a1,
        in("rdx") a2,
        in("r10") a3,
        in("r8") a4,
        in("r9") a5,
        err = lateout(reg_byte) err,
        lateout("rcx") _,        // `syscall` clobbers rcx and r11
        lateout("r11") _,
        options(nostack),
    );
    if err != 0 {
        // rax holds positive errno on error; mirror Linux's -errno encoding.
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
