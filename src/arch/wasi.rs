//! WebAssembly (WASI preview1) backend.
//!
//! WebAssembly has **no syscall instruction** — the only system interface is
//! WASI: host functions imported from the `wasi_snapshot_preview1` module. So
//! this backend exposes *named* wrappers (not the `syscall0..6` shape every
//! other arch uses); [`crate::syscall`] dispatches to them under
//! `#[cfg(target_family = "wasm")]`. The supported surface is reduced: there are
//! no threads, no `fork`/`exec`, no sockets, and no `mmap` — those return
//! `ENOSYS` and the corresponding `std` APIs report `Unsupported`.
//!
//! WASI errnos are renumbered, so each wrapper translates them to the Linux
//! `errno` values the rest of purestd (e.g. [`crate::io::Error`]) expects.

use crate::syscall::Errno;

/// WASI `iovec` (read) / `ciovec` (write): a (pointer, length) pair.
#[repr(C)]
pub struct Ciovec {
    pub buf: *const u8,
    pub buf_len: usize,
}
#[repr(C)]
pub struct Iovec {
    pub buf: *mut u8,
    pub buf_len: usize,
}

#[link(wasm_import_module = "wasi_snapshot_preview1")]
unsafe extern "C" {
    pub fn fd_write(fd: u32, iovs: *const Ciovec, iovs_len: usize, nwritten: *mut usize) -> u16;
    pub fn fd_read(fd: u32, iovs: *const Iovec, iovs_len: usize, nread: *mut usize) -> u16;
    pub fn fd_close(fd: u32) -> u16;
    pub fn random_get(buf: *mut u8, len: usize) -> u16;
    pub fn clock_time_get(id: u32, precision: u64, time: *mut u64) -> u16;
    pub fn args_sizes_get(argc: *mut usize, argv_buf_size: *mut usize) -> u16;
    pub fn args_get(argv: *mut *mut u8, argv_buf: *mut u8) -> u16;
    pub fn environ_sizes_get(environ_count: *mut usize, environ_buf_size: *mut usize) -> u16;
    pub fn environ_get(environ: *mut *mut u8, environ_buf: *mut u8) -> u16;
    pub fn proc_exit(rval: u32) -> !;
}

// WASI clock ids.
pub const CLOCKID_REALTIME: u32 = 0;
pub const CLOCKID_MONOTONIC: u32 = 1;

// Linux errno values we translate WASI errnos into.
const EIO: i32 = 5;
const EBADF: i32 = 9;
const EACCES: i32 = 13;
const EEXIST: i32 = 17;
const EINVAL: i32 = 22;
const ENOSYS: i32 = 38;
const ENOENT: i32 = 2;

/// Translate a WASI `errno` (preview1 numbering) into a Linux `errno`.
pub fn wasi_to_linux(e: u16) -> i32 {
    match e {
        0 => 0,
        2 => EACCES,   // WASI EACCES
        8 => EBADF,    // WASI EBADF
        20 => EEXIST,  // WASI EEXIST
        28 => EINVAL,  // WASI EINVAL
        44 => ENOENT,  // WASI ENOENT
        52 => ENOSYS,  // WASI ENOSYS
        _ => EIO,
    }
}

#[inline]
fn check(e: u16) -> Result<(), Errno> {
    if e == 0 {
        Ok(())
    } else {
        Err(Errno(wasi_to_linux(e)))
    }
}

/// `fd_write` of a single buffer → bytes written.
#[inline]
pub fn write(fd: i32, buf: &[u8]) -> Result<usize, Errno> {
    let iov = Ciovec { buf: buf.as_ptr(), buf_len: buf.len() };
    let mut n: usize = 0;
    let e = unsafe { fd_write(fd as u32, &iov, 1, &mut n) };
    check(e).map(|_| n)
}

/// `fd_read` into a single buffer → bytes read.
#[inline]
pub fn read(fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
    let iov = Iovec { buf: buf.as_mut_ptr(), buf_len: buf.len() };
    let mut n: usize = 0;
    let e = unsafe { fd_read(fd as u32, &iov, 1, &mut n) };
    check(e).map(|_| n)
}

/// `fd_write` of several buffers (vectored) → bytes written.
#[inline]
pub fn writev(fd: i32, iovs: &[Ciovec]) -> Result<usize, Errno> {
    let mut n: usize = 0;
    let e = unsafe { fd_write(fd as u32, iovs.as_ptr(), iovs.len(), &mut n) };
    check(e).map(|_| n)
}

/// `fd_read` into several buffers (vectored) → bytes read.
#[inline]
pub fn readv(fd: i32, iovs: &[Iovec]) -> Result<usize, Errno> {
    let mut n: usize = 0;
    let e = unsafe { fd_read(fd as u32, iovs.as_ptr(), iovs.len(), &mut n) };
    check(e).map(|_| n)
}

#[inline]
pub fn close(fd: i32) -> Result<(), Errno> {
    check(unsafe { fd_close(fd as u32) })
}

#[inline]
pub fn getrandom(buf: &mut [u8]) -> Result<(), Errno> {
    check(unsafe { random_get(buf.as_mut_ptr(), buf.len()) })
}

/// Read a clock, returning `(seconds, nanos)`.
#[inline]
pub fn clock(id: u32) -> (u64, u32) {
    let mut t: u64 = 0;
    let _ = unsafe { clock_time_get(id, 1_000, &mut t) };
    (t / 1_000_000_000, (t % 1_000_000_000) as u32)
}

// Dummy constants so the shared upper layers (allocator, syscall) compile.
// (No mmap on wasm — the allocator grows linear memory instead.)
pub const PROT_READ: usize = 0x1;
pub const PROT_WRITE: usize = 0x2;
pub const MAP_PRIVATE: usize = 0x2;
pub const MAP_ANONYMOUS: usize = 0x20;
pub const AT_FDCWD: isize = -2;

/// Empty number table — `crate::syscall`'s `use crate::arch::nr` resolves here,
/// but every `arch::syscallN(nr::…)` call site is `#[cfg]`-d out on wasm.
pub mod nr {}
