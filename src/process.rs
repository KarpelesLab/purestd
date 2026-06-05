//! `std::process` subset: `exit`, `abort`, `id`, and `ExitCode`.
//!
//! `Command` (subprocess spawning) is not implemented yet — it needs
//! `fork`/`posix_spawn`/`execve` wiring and lands later.

use crate::syscall;

/// Terminate the current process with the given exit code. Drop-in for
/// `std::process::exit`.
#[inline]
pub fn exit(code: i32) -> ! {
    crate::rt::exit(code)
}

/// Terminate abnormally (status 134). Drop-in for `std::process::abort`.
#[inline]
pub fn abort() -> ! {
    crate::rt::abort()
}

/// The id of the current process. Drop-in for `std::process::id`.
#[inline]
pub fn id() -> u32 {
    syscall::getpid()
}

/// A process exit code. Drop-in for `std::process::ExitCode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitCode(u8);

impl ExitCode {
    pub const SUCCESS: ExitCode = ExitCode(0);
    pub const FAILURE: ExitCode = ExitCode(1);

    pub fn exit_process(self) -> ! {
        exit(self.0 as i32)
    }
}

impl From<u8> for ExitCode {
    fn from(n: u8) -> ExitCode {
        ExitCode(n)
    }
}

impl crate::rt::Termination for ExitCode {
    fn report(self) -> i32 {
        self.0 as i32
    }
}
