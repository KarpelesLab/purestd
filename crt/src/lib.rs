//! # purert — the freestanding runtime companion to [`purestd`].
//!
//! `purestd` provides exactly what a real `std` provides. The pieces a libc-free
//! binary *also* needs — but which in a normal hosted build come from **crt0**,
//! **compiler_builtins**, and the **unwinder**, not from std — live here:
//!
//! * the process entry point `_start` (crt0's job), which decodes
//!   `argc`/`argv`/`envp` and calls `purestd`'s runtime start symbol;
//! * the `mem*` intrinsics, `strlen`, and (on aarch64) `getauxval`
//!   (compiler_builtins / libc's job);
//! * the unwind abort-stubs `rust_eh_personality` / `_Unwind_Resume`
//!   (the unwinder's job).
//!
//! A program links `purestd` (the std) **and** `purert` (this runtime):
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//! extern crate purert; // pull in _start + the toolchain symbols
//! use purestd::prelude::*;
//!
//! fn main() { println!("hello"); }
//! purestd::entry!(main);
//! ```

#![no_std]
#![allow(internal_features)]

mod entry;
mod intrinsics;
