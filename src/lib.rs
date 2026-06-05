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
//! ## `rt` feature (default)
//!
//! Gates the binary-level *policy* symbols — `_start`, the `#[panic_handler]`,
//! the `#[global_allocator]` static, and the `mem*`/unwind intrinsics. Disable
//! it (`default-features = false`) when another crate in the final binary
//! supplies those. The *mechanisms* (syscalls, [`allocator::Allocator`], the
//! `std` surface, [`rt::Termination`]) are always available.

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
pub mod env;
pub mod error;
pub mod ffi;
pub mod fs;
pub mod io;
pub mod net;
pub mod path;
pub mod process;
pub mod sync;
pub mod thread;
pub mod time;

pub use syscall::Errno;

// Binary-level policy symbols, behind the default `rt` feature.
#[cfg(feature = "rt")]
mod intrinsics;
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
    any, arch as core_arch, ascii, cell, char, clone, cmp, convert, default, future, hash, hint,
    iter, marker, mem, num, ops, option, panic as core_panic, pin, primitive, ptr, result, slice,
    str, task,
};
pub use core::{
    assert, assert_eq, assert_ne, debug_assert, debug_assert_eq, debug_assert_ne, format_args,
    matches, todo, unimplemented, unreachable, write, writeln,
};

pub use alloc::{borrow, boxed, fmt, format, rc, string, vec};

/// Collections: ordered ones from `alloc`, hash ones from `hashbrown` (so
/// `HashMap`/`HashSet` need no type params, matching `std`).
pub mod collections {
    pub use crate::alloc::collections::{
        btree_map, btree_set, BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque,
    };
    pub use hashbrown::{hash_map, hash_set, HashMap, HashSet};
}

/// `std::panic` subset. Under `panic = "abort"` there is no unwinding, so
/// `catch_unwind` simply runs the closure (a panic aborts the process).
pub mod panicking {
    pub use core::panic::{Location, PanicInfo};
}
