//! i686 (32-bit x86) Linux: `int 0x80` syscall wrappers and syscall numbers.
//!
//! ## Syscall ABI
//! Number in `eax`; arguments in `ebx, ecx, edx, esi, edi, ebp`; result in
//! `eax`. Errors come back as `-errno` in `eax`, which
//! [`crate::syscall::from_ret`] handles.
//!
//! ## The reserved-register problem
//! LLVM reserves `ebx` (PIC/GOT base) and `ebp` (frame pointer), so inline
//! `asm!` cannot bind them as operands. We sidestep it by passing **all** args
//! through a small stack array: only `eax` is an operand (holding the array
//! pointer); the asm loads every argument register from memory, then loads the
//! syscall number into `eax` last, right before `int 0x80`. `ebx`/`ebp` are
//! saved/restored with push/pop around the call.
//!
//! This is a 32-bit arch: 64-bit file offsets/lengths use the `*64` syscall
//! variants (`_llseek`, `ftruncate64`, `mmap2`, `*stat64`), and sockets
//! multiplex through `socketcall` — handled in [`crate::syscall`].

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
    // [ebx, ecx, edx, esi, edi, ebp, eax(number)] — number stored last so the
    // asm can use eax as the base pointer for all the loads, then overwrite it.
    let args = [a0, a1, a2, a3, a4, a5, n];
    // LLVM reserves ebx (PIC), ebp (frame ptr), and esi (base ptr under stack
    // realignment) — none can be bound as operands. So save/restore all four
    // around the call and load every arg from the array via eax; only ecx/edx
    // are declared clobbers.
    asm!(
        "push ebp",
        "push ebx",
        "push esi",
        "push edi",
        "mov ebx, [eax + 0]",
        "mov ecx, [eax + 4]",
        "mov edx, [eax + 8]",
        "mov esi, [eax + 12]",
        "mov edi, [eax + 16]",
        "mov ebp, [eax + 20]",
        "mov eax, [eax + 24]",
        "int 0x80",
        "pop edi",
        "pop esi",
        "pop ebx",
        "pop ebp",
        inout("eax") args.as_ptr() => ret,
        lateout("ecx") _,
        lateout("edx") _,
        // We push/pop, so the asm touches the stack — no `nostack`.
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

/// Linux/i386 syscall numbers (legacy `unistd_32` table).
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
    pub const SOCKETCALL: usize = 102;
    pub const WAIT4: usize = 114;
    pub const FSYNC: usize = 118;
    pub const CLONE: usize = 120;
    pub const LLSEEK: usize = 140;
    pub const READV: usize = 145;
    pub const WRITEV: usize = 146;
    pub const SCHED_YIELD: usize = 158;
    pub const NANOSLEEP: usize = 162;
    pub const GETCWD: usize = 183;
    pub const MMAP: usize = 192; // mmap2 (offset in pages; mmap_anon passes 0)
    pub const FTRUNCATE: usize = 194; // ftruncate64
    pub const GETDENTS64: usize = 220;
    pub const FCNTL: usize = 221; // fcntl64
    pub const GETTID: usize = 224;
    pub const FUTEX: usize = 240;
    pub const SCHED_GETAFFINITY: usize = 242;
    pub const EXIT_GROUP: usize = 252;
    pub const CLOCK_GETTIME: usize = 265;
    pub const OPENAT: usize = 295;
    pub const MKDIRAT: usize = 296;
    pub const FSTATAT64: usize = 300;
    pub const NEWFSTATAT: usize = 300; // alias: 32-bit uses fstatat64
    pub const FSTAT: usize = 197; // fstat64
    pub const UNLINKAT: usize = 301;
    pub const RENAMEAT: usize = 302;
    pub const DUP3: usize = 330;
    pub const PIPE2: usize = 331;
    pub const RENAMEAT2: usize = 353;
    pub const GETRANDOM: usize = 355;
    pub const GETENTROPY: usize = GETRANDOM;

    // `socketcall` sub-call selectors (first arg to SYS_socketcall).
    pub const SC_SOCKET: usize = 1;
    pub const SC_BIND: usize = 2;
    pub const SC_CONNECT: usize = 3;
    pub const SC_LISTEN: usize = 4;
    pub const SC_ACCEPT: usize = 5;
    pub const SC_GETSOCKNAME: usize = 6;
    pub const SC_GETPEERNAME: usize = 7;
    pub const SC_SENDTO: usize = 11;
    pub const SC_RECVFROM: usize = 12;
    pub const SC_SHUTDOWN: usize = 13;
    pub const SC_SETSOCKOPT: usize = 14;
    pub const SC_GETSOCKOPT: usize = 15;
}

pub const AT_FDCWD: isize = -100;

pub const PROT_READ: usize = 0x1;
pub const PROT_WRITE: usize = 0x2;
pub const MAP_PRIVATE: usize = 0x2;
pub const MAP_ANONYMOUS: usize = 0x20;
