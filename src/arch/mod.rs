//! The only architecture/OS-specific code: raw syscall instruction wrappers,
//! the syscall number table (`nr`), and a handful of ABI constants.
//!
//! Every backend exposes the same surface — `syscall0..syscall6`, `nr::*`,
//! `AT_FDCWD`, and the `PROT_*`/`MAP_*` constants — so the layers above
//! ([`crate::syscall`] and up) are completely OS-neutral. Adding a target means
//! adding one file here.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "macos_aarch64.rs"]
mod imp;

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
#[path = "macos_x86_64.rs"]
mod imp;

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[path = "linux_aarch64.rs"]
mod imp;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[path = "linux_x86_64.rs"]
mod imp;

#[cfg(all(target_os = "linux", target_arch = "riscv64"))]
#[path = "linux_riscv64.rs"]
mod imp;

#[cfg(all(target_os = "linux", target_arch = "x86"))]
#[path = "i686_linux.rs"]
mod imp;

#[cfg(all(target_os = "linux", target_arch = "arm"))]
#[path = "arm_linux.rs"]
mod imp;

#[cfg(not(target_family = "wasm"))]
pub use imp::*;

// wasm has no syscall instruction; the WASI backend is exposed as a module and
// driven by `syscall_wasi.rs` (there is no `syscall0..6` surface here).
#[cfg(target_family = "wasm")]
pub mod wasi;
