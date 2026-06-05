//! Arch-neutral, `Result`-returning syscall wrappers.
//!
//! Built on [`crate::arch`]. Every backend normalizes kernel errors to the
//! Linux `-errno` convention (a small negative return), so [`from_ret`] is
//! shared. This is the boundary the rest of `purestd` is written against.

use crate::arch::{self, nr};

/// A raw OS error number (e.g. `2` = `ENOENT`, `9` = `EBADF`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Errno(pub i32);

/// Convert a raw syscall return into a `Result`. Returns in `[-4095, -1]` are
/// interpreted as `-errno`; anything else is a success value.
#[inline]
pub fn from_ret(ret: usize) -> Result<usize, Errno> {
    let s = ret as isize;
    if (-4095..0).contains(&s) {
        Err(Errno(-s as i32))
    } else {
        Ok(ret)
    }
}

// ---- open flags (subset; values match Linux & Darwin where they agree) ----
pub const O_RDONLY: usize = 0o0;
pub const O_WRONLY: usize = 0o1;
pub const O_RDWR: usize = 0o2;
#[cfg(target_os = "macos")]
pub const O_CREAT: usize = 0x0200;
#[cfg(target_os = "macos")]
pub const O_TRUNC: usize = 0x0400;
#[cfg(target_os = "macos")]
pub const O_APPEND: usize = 0x0008;
#[cfg(not(target_os = "macos"))]
pub const O_CREAT: usize = 0o100;
#[cfg(not(target_os = "macos"))]
pub const O_TRUNC: usize = 0o1000;
#[cfg(not(target_os = "macos"))]
pub const O_APPEND: usize = 0o2000;

/// `read(fd, buf)` — number of bytes read (0 at EOF).
#[inline]
pub fn read(fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
    from_ret(unsafe { arch::syscall3(nr::READ, fd as usize, buf.as_mut_ptr() as usize, buf.len()) })
}

/// `write(fd, buf)` — number of bytes written.
#[inline]
pub fn write(fd: i32, buf: &[u8]) -> Result<usize, Errno> {
    from_ret(unsafe { arch::syscall3(nr::WRITE, fd as usize, buf.as_ptr() as usize, buf.len()) })
}

/// `close(fd)`.
#[inline]
pub fn close(fd: i32) -> Result<(), Errno> {
    from_ret(unsafe { arch::syscall1(nr::CLOSE, fd as usize) }).map(|_| ())
}

/// `openat(AT_FDCWD, path, flags, mode)`. `path` must be NUL-terminated.
#[inline]
pub fn open(path: &core::ffi::CStr, flags: usize, mode: u32) -> Result<i32, Errno> {
    let r = unsafe {
        arch::syscall4(
            nr::OPENAT,
            arch::AT_FDCWD as usize,
            path.as_ptr() as usize,
            flags,
            mode as usize,
        )
    };
    from_ret(r).map(|fd| fd as i32)
}

/// Anonymous private `mmap` of `len` bytes with the given protection.
#[inline]
pub fn mmap_anon(len: usize, prot: usize) -> Result<*mut u8, Errno> {
    let r = unsafe {
        arch::syscall6(
            nr::MMAP,
            0,
            len,
            prot,
            arch::MAP_PRIVATE | arch::MAP_ANONYMOUS,
            usize::MAX, // fd = -1
            0,
        )
    };
    from_ret(r).map(|p| p as *mut u8)
}

/// `munmap(addr, len)`.
///
/// # Safety
/// `addr`/`len` must describe a mapping previously returned by [`mmap_anon`].
#[inline]
pub unsafe fn munmap(addr: *mut u8, len: usize) -> Result<(), Errno> {
    from_ret(arch::syscall2(nr::MUNMAP, addr as usize, len)).map(|_| ())
}

/// `getpid()` — the current process id.
#[inline]
pub fn getpid() -> u32 {
    unsafe { arch::syscall0(nr::GETPID) as u32 }
}

/// `unlinkat(AT_FDCWD, path, 0)` — remove a file. `path` must be NUL-terminated.
/// (aarch64 Linux has no bare `unlink`, so we always use the `*at` form.)
#[inline]
pub fn unlink(path: &core::ffi::CStr) -> Result<(), Errno> {
    from_ret(unsafe {
        arch::syscall3(nr::UNLINKAT, arch::AT_FDCWD as usize, path.as_ptr() as usize, 0)
    })
    .map(|_| ())
}

/// `mkdirat(AT_FDCWD, path, mode)`.
#[inline]
pub fn mkdir(path: &core::ffi::CStr, mode: u32) -> Result<(), Errno> {
    from_ret(unsafe {
        arch::syscall3(
            nr::MKDIRAT,
            arch::AT_FDCWD as usize,
            path.as_ptr() as usize,
            mode as usize,
        )
    })
    .map(|_| ())
}

/// Wall-clock time as `(seconds, microseconds)` since the Unix epoch, via
/// `gettimeofday`.
#[inline]
pub fn gettimeofday() -> Result<(u64, u64), Errno> {
    // struct timeval { i64 tv_sec; i64 tv_usec; } — 16 bytes.
    let mut tv = [0u64; 2];
    from_ret(unsafe { arch::syscall2(nr::GETTIMEOFDAY, tv.as_mut_ptr() as usize, 0) })?;
    Ok((tv[0], tv[1]))
}

/// Terminate the whole process with `code`. Never returns.
#[inline]
pub fn exit_group(code: i32) -> ! {
    unsafe {
        arch::syscall1(nr::EXIT_GROUP, code as usize);
        // If that ever returns (or on Darwin where it's plain `exit`), make sure
        // this thread dies regardless.
        arch::syscall1(nr::EXIT, code as usize);
    }
    loop {
        core::hint::spin_loop();
    }
}
