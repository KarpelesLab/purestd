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
/// Generates the C-ABI symbol `__purestd_main` that the runtime `_start` calls.
/// The function may return `()`, `i32`, or `Result<_, E: Debug>` — exactly as
/// `fn main` may with real `std`.
///
/// ```ignore
/// #![no_std]
/// #![no_main]
/// use purestd::prelude::*;
///
/// fn main() {
///     println!("hello, libc-free world");
/// }
/// purestd::entry!(main);
/// ```
#[macro_export]
macro_rules! entry {
    ($main:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn __purestd_main() -> i32 {
            $crate::rt::Termination::report($main())
        }
    };
}
