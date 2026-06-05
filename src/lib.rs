//! # purestd
//!
//! A **drop-in replacement for `std` that does not depend on libc.** Every
//! operation is a direct kernel syscall (`svc`/`syscall`), with no C library and
//! no C runtime. Aliased as `std` for a freestanding target, ordinary programs —
//! `use std::io::Write;`, `std::fs::read(..)`, `println!`, `Vec`, `String`,
//! `HashMap` — compile and run unchanged, linking zero libc.
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//! use purestd::prelude::*;
//!
//! fn main() {
//!     println!("hello from purestd — no libc");
//! }
//! purestd::entry!(main);
//! ```
//!
//! ## Layout
//!
//! * [`arch`] — the only OS/arch-specific code: raw syscall wrappers + number
//!   table. One file per target.
//! * [`syscall`] — arch-neutral, `Result`-returning syscall wrappers ([`Errno`]).
//! * [`io`], [`fs`], [`env`], [`process`], [`time`], [`sync`], [`path`], [`ffi`]
//!   — the `std`-shaped surface, all backed by syscalls.
//! * `core`/`alloc` are re-exported under `std`-shaped paths so existing imports
//!   resolve here.
//!
//! ## Scope: purestd is *only* `std`
//!
//! purestd provides exactly what a real `std` provides. The lower-level pieces a
//! libc-free binary also needs — the process entry point `_start` and the
//! `mem*`/unwind/`getauxval` symbols — come from **crt0** and
//! **compiler_builtins** in a hosted build, not from std, so they live in the
//! separate [`purert`](https://docs.rs/purert) runtime crate. A program links
//! both (`extern crate purert;`).
//!
//! ## `rt` feature (default)
//!
//! Gates the std-provided *policy* symbols — the `#[panic_handler]`, the
//! `#[global_allocator]` static, and the `lang_start`-equivalent runtime glue
//! (`__purestd_start`). Disable it (`default-features = false`) when a host
//! runtime supplies those. The *mechanisms* (syscalls, [`allocator::Allocator`],
//! the `std` surface, [`rt::Termination`]) are always available.

#![no_std]
#![allow(clippy::missing_safety_doc)]
#![allow(internal_features)]

/// The `alloc` crate, re-exported so downstream `#![no_std]` programs reach
/// `Vec`, `String`, `Box`, `format!`, etc. through `purestd`.
pub extern crate alloc;

// ---- purestd's own machinery ----
pub mod allocator;
pub mod arch;
pub mod macros;
pub mod prelude;
pub mod rt;
pub mod syscall;

// ---- the std-shaped surface ----
pub mod collections;
pub mod env;
pub mod error;
pub mod ffi;
pub mod fs;
pub mod hash;
pub mod io;
pub mod net;
pub mod path;
pub mod process;
pub mod sync;
pub mod thread;
pub mod time;

mod sys_thread;

pub use syscall::Errno;

// The std-provided policy symbols (panic handler, global allocator, and the
// `lang_start`-equivalent runtime glue), behind the default `rt` feature. The
// process entry point and the mem*/unwind intrinsics are NOT here — like crt0
// and compiler_builtins in a hosted build, they live in the `purert` crate.
#[cfg(feature = "rt")]
mod panic;
#[cfg(feature = "rt")]
mod start;

// ---------------------------------------------------------------------------
// `std`-shaped re-exports of `core` + `alloc`, so the many `std::mem`,
// `std::cmp`, `std::fmt`, `std::vec`, `std::sync::atomic`, … paths resolve here
// when this crate is aliased as `std`.
// ---------------------------------------------------------------------------

pub use core::{
    any, arch as core_arch, ascii, cell, char, clone, cmp, convert, default, future, hint, iter,
    marker, mem, num, ops, option, panic as core_panic, pin, primitive, ptr, result, slice, str,
    task,
};
pub use core::{
    assert, assert_eq, assert_ne, debug_assert, debug_assert_eq, debug_assert_ne, format_args,
    matches, todo, unimplemented, unreachable, write, writeln,
};

pub use alloc::{borrow, boxed, fmt, format, rc, string, vec};

/// `std::panic` subset. Under `panic = "abort"` there is no unwinding, so
/// `catch_unwind` simply runs the closure (a panic aborts the process).
pub mod panicking {
    pub use core::panic::{Location, PanicInfo};
}
