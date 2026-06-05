//! Process lifetime: exit, abort, and `main`'s return-value conversion.
//!
//! [`Termination`] mirrors `std::process::Termination`, so `fn main` may return
//! `()`, `i32`, or `Result<_, E: Debug>` exactly as it does with real `std`.

use crate::syscall;

/// The entry shim called by the [`entry!`](crate::entry) macro's `main`.
///
/// Records the loader-provided argument/environment vectors, runs the user
/// `main`, and converts its return value into an exit code. The analogue of
/// `std`'s `lang_start`.
#[doc(hidden)]
pub unsafe fn __entry<T: Termination>(
    argc: usize,
    argv: *const *const u8,
    envp: *const *const u8,
    main: fn() -> T,
) -> i32 {
    crate::env::init(argc, argv, envp);
    main().report()
}

/// Exit the process with `code`. Never returns.
#[inline]
pub fn exit(code: i32) -> ! {
    syscall::exit_group(code)
}

/// Abort the process (status 134 = 128 + SIGABRT). Never returns.
#[inline]
pub fn abort() -> ! {
    syscall::exit_group(134)
}

/// Conversion from a `main` return value into a process exit code.
///
/// The drop-in analogue of `std::process::Termination`.
pub trait Termination {
    /// Consume the value and produce an exit code.
    fn report(self) -> i32;
}

impl Termination for () {
    #[inline]
    fn report(self) -> i32 {
        0
    }
}

impl Termination for i32 {
    #[inline]
    fn report(self) -> i32 {
        self
    }
}

impl<T: Termination, E: core::fmt::Debug> Termination for Result<T, E> {
    #[inline]
    fn report(self) -> i32 {
        match self {
            Ok(v) => v.report(),
            Err(e) => {
                crate::eprintln!("Error: {:?}", e);
                1
            }
        }
    }
}
