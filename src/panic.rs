//! The single `#[panic_handler]` for a `purestd` binary. Gated behind `rt`.
//!
//! Prints the message and location to stderr, then exits with status 101 — the
//! status `std` reports for an unhandled panic — so drop-in programs and their
//! test harnesses see the value they expect. No unwinding happens (we build
//! with `panic = "abort"`), so this never touches `_Unwind_*`.

use crate::io::Write;
use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // `PanicInfo`'s Display already includes both the message and location.
    let mut err = crate::io::Fd(2);
    let _ = writeln!(err, "thread 'main' panicked:\n{}", info);
    crate::rt::exit(101)
}

/// The Rust exception-personality routine. In a hosted build `std` provides this
/// (the C `_Unwind_*` routines come from the unwinder); we build with
/// `panic = "abort"`, so it is referenced by precompiled `alloc` but never
/// called. An empty stub satisfies the linker.
#[unsafe(no_mangle)]
extern "C" fn rust_eh_personality() {}
