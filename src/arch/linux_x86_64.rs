//! x86-64 Linux: syscall instruction wrappers, entry point, syscall numbers.
//!
//! ## Syscall ABI
//! Number in `rax`; arguments in `rdi, rsi, rdx, r10, r8, r9`; result in `rax`.
//! `syscall` clobbers `rcx`/`r11`. Errors come back as `-errno` in `[-4095, -1]`,
//! which [`crate::syscall::from_ret`] already understands.
//!
//! ## Process entry ABI
//! On `execve` the kernel jumps to `_start` with `rsp` at `argc`, followed by the
//! `argv` pointers, NULL, the `envp` pointers, NULL, then the auxiliary vector.
//! No return address, no C runtime — `_start` must never return.

use core::arch::asm;

macro_rules! syscall_fn {
    ($name:ident; $($arg:ident => $reg:tt),*) => {
        #[inline]
        pub unsafe fn $name(n: usize $(, $arg: usize)*) -> usize {
            let ret;
            asm!(
                "syscall",
                inlateout("rax") n => ret,
                $(in($reg) $arg,)*
                lateout("rcx") _,
                lateout("r11") _,
                options(nostack, preserves_flags),
            );
            ret
        }
    };
}

syscall_fn!(syscall0;);
syscall_fn!(syscall1; a => "rdi");
syscall_fn!(syscall2; a => "rdi", b => "rsi");
syscall_fn!(syscall3; a => "rdi", b => "rsi", c => "rdx");
syscall_fn!(syscall4; a => "rdi", b => "rsi", c => "rdx", d => "r10");
syscall_fn!(syscall5; a => "rdi", b => "rsi", c => "rdx", d => "r10", e => "r8");
syscall_fn!(syscall6; a => "rdi", b => "rsi", c => "rdx", d => "r10", e => "r8", f => "r9");

/// Linux/x86-64 syscall numbers.
pub mod nr {
    pub const READ: usize = 0;
    pub const WRITE: usize = 1;
    pub const READV: usize = 19;
    pub const WRITEV: usize = 20;
    pub const CLOSE: usize = 3;
    pub const LSEEK: usize = 8;
    pub const MMAP: usize = 9;
    pub const MUNMAP: usize = 11;
    pub const GETPID: usize = 39;
    pub const DUP: usize = 32;
    pub const EXIT: usize = 60;
    pub const GETTIMEOFDAY: usize = 96;
    pub const EXIT_GROUP: usize = 231;
    pub const MKDIRAT: usize = 258;
    pub const UNLINKAT: usize = 263;
    pub const OPENAT: usize = 257;
    pub const GETRANDOM: usize = 318;
    pub const GETENTROPY: usize = GETRANDOM;
    pub const SCHED_YIELD: usize = 24;
    pub const NANOSLEEP: usize = 35;
    pub const FUTEX: usize = 202;
    pub const CLONE: usize = 56;
    pub const GETTID: usize = 186;
    // ---- sockets ----
    pub const SOCKET: usize = 41;
    pub const CONNECT: usize = 42;
    pub const ACCEPT: usize = 43;
    pub const BIND: usize = 49;
    pub const LISTEN: usize = 50;
    pub const GETSOCKNAME: usize = 51;
    pub const GETPEERNAME: usize = 52;
    pub const SENDTO: usize = 44;
    pub const RECVFROM: usize = 45;
    pub const SHUTDOWN: usize = 48;
    pub const SETSOCKOPT: usize = 54;
    pub const GETSOCKOPT: usize = 55;
    pub const CLOCK_GETTIME: usize = 228;
    pub const FTRUNCATE: usize = 77;
    pub const FSYNC: usize = 74;
    pub const RENAMEAT: usize = 264;
    pub const FSTAT: usize = 5;
    pub const NEWFSTATAT: usize = 262;
    pub const GETDENTS64: usize = 217;
    pub const CHDIR: usize = 80;
    pub const GETCWD: usize = 79;
    pub const NANOSLEEP_LX: usize = 35;
    pub const FORK: usize = 57;
    pub const EXECVE: usize = 59;
    pub const WAIT4: usize = 61;
    pub const PIPE2: usize = 293;
    pub const DUP2: usize = 33;
    pub const FCNTL: usize = 72;
    pub const SCHED_GETAFFINITY: usize = 204;
}

pub const AT_FDCWD: isize = -100;

pub const PROT_READ: usize = 0x1;
pub const PROT_WRITE: usize = 0x2;
pub const MAP_PRIVATE: usize = 0x2;
pub const MAP_ANONYMOUS: usize = 0x20;
