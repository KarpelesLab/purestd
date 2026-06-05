//! The prelude, mirroring `std`'s.
//!
//! When `purestd` is aliased as `std` for a freestanding target, the compiler
//! auto-injects `std::prelude::rust_2021` (or `rust_2024`); those must carry the
//! standard macros and common `alloc` types, so they re-export everything in
//! [`v1`].

pub mod v1 {
    pub use crate::{eprint, eprintln, print, println};
    pub use crate::alloc::borrow::ToOwned;
    pub use crate::alloc::boxed::Box;
    pub use crate::alloc::string::{String, ToString};
    pub use crate::alloc::vec::Vec;
    pub use crate::alloc::{format, vec};
    pub use core::prelude::v1::*;
}

pub mod rust_2021 {
    pub use super::v1::*;
    pub use core::prelude::rust_2021::*;
}

pub mod rust_2024 {
    pub use super::v1::*;
    pub use core::prelude::rust_2024::*;
}

pub use v1::*;
