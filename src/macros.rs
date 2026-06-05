//! The print family and the `entry!` macro.

/// Print to standard output. Drop-in for `std::print!`.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::io::_print(::core::format_args!($($arg)*)) };
}

/// Print to standard output with a trailing newline. Drop-in for `std::println!`.
#[macro_export]
macro_rules! println {
    () => { $crate::io::_print(::core::format_args!("\n")) };
    ($($arg:tt)*) => {
        $crate::io::_print(::core::format_args!("{}\n", ::core::format_args!($($arg)*)))
    };
}

/// Print to standard error. Drop-in for `std::eprint!`.
#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => { $crate::io::_eprint(::core::format_args!($($arg)*)) };
}

/// Print to standard error with a trailing newline. Drop-in for `std::eprintln!`.
#[macro_export]
macro_rules! eprintln {
    () => { $crate::io::_eprint(::core::format_args!("\n")) };
    ($($arg:tt)*) => {
        $crate::io::_eprint(::core::format_args!("{}\n", ::core::format_args!($($arg)*)))
    };
}

/// Define the program entry point.
///
/// Generates the standard C `main(argc, argv, envp)` symbol, so the program is
/// started the ordinary way — by the C runtime's `_start` (crt0) in a normal
/// build, or by the fullrust toolchain's equivalent in a libc-free build. Either
/// caller passes the argument/environment vectors, which are recorded for
/// [`crate::env`] before your `main` runs.
///
/// Your `fn main` may return `()`, `i32`, or `Result<_, E: Debug>` — exactly as
/// with real `std`.
///
/// ```ignore
/// #![no_std]
/// #![no_main]
/// use purestd::prelude::*;
///
/// fn main() {
///     println!("hello from purestd");
/// }
/// purestd::entry!(main);
/// ```
#[macro_export]
macro_rules! entry {
    ($main:path) => {
        // `export_name` (not a `fn main` item) so this doesn't collide with the
        // user's `fn main`, while still emitting the C `main` symbol crt0 wants.
        #[export_name = "main"]
        pub extern "C" fn __purestd_main(
            argc: ::core::ffi::c_int,
            argv: *const *const u8,
            envp: *const *const u8,
        ) -> ::core::ffi::c_int {
            unsafe { $crate::rt::__entry(argc as usize, argv, envp, $main) as ::core::ffi::c_int }
        }
    };
}
