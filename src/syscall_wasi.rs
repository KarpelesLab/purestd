//! WASI (wasm) implementation of the [`crate::syscall`] surface.
//!
//! WebAssembly has no syscalls, so this module mirrors the public API the rest
//! of purestd is written against, backed by WASI preview1 imports
//! ([`crate::arch::wasi`]). The supported surface is reduced: standard I/O,
//! random, clocks, and process exit work; the filesystem, sockets, processes,
//! and threads are **not** wired and return `ENOSYS` (so the corresponding
//! `std` APIs report `Unsupported`). The allocator grows linear memory directly
//! rather than calling `mmap`.

use crate::arch::wasi;

/// A raw OS error number, mirroring [`crate::syscall::Errno`] on other targets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Errno(pub i32);

const ENOSYS: Errno = Errno(38);

#[inline]
pub fn from_ret(ret: usize) -> Result<usize, Errno> {
    let s = ret as isize;
    if (-4095..0).contains(&s) {
        Err(Errno(-s as i32))
    } else {
        Ok(ret)
    }
}

// ---- open flags / seek / at-flags (Linux values; fs is unsupported but the
// constants are still referenced by fs.rs) ----
pub const O_RDONLY: usize = 0o0;
pub const O_WRONLY: usize = 0o1;
pub const O_RDWR: usize = 0o2;
pub const O_CREAT: usize = 0o100;
pub const O_TRUNC: usize = 0o1000;
pub const O_APPEND: usize = 0o2000;
pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;
pub const AT_SYMLINK_NOFOLLOW: usize = 0x100;
pub const AT_REMOVEDIR: usize = 0x200;

/// A kernel `struct iovec`: a (base, len) pair.
#[repr(C)]
pub struct IoVec {
    pub base: *const u8,
    pub len: usize,
}

/// Raw stat buffer (unused on wasm — `statat`/`fstat` return `ENOSYS`).
pub type StatBuf = [u8; 256];

// ---------------------------------------------------------------------------
// Working: standard I/O, random, clocks, exit
// ---------------------------------------------------------------------------

#[inline]
pub fn read(fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
    wasi::read(fd, buf)
}

#[inline]
pub fn write(fd: i32, buf: &[u8]) -> Result<usize, Errno> {
    wasi::write(fd, buf)
}

/// # Safety
/// `iov` must point to `cnt` valid `IoVec`s for the call's duration.
#[inline]
pub unsafe fn readv(fd: i32, iov: *const IoVec, cnt: usize) -> Result<usize, Errno> {
    let slice = core::slice::from_raw_parts(iov, cnt);
    let mut total = 0;
    for v in slice {
        let b = core::slice::from_raw_parts_mut(v.base as *mut u8, v.len);
        match wasi::read(fd, b) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if n < v.len {
                    break;
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}

/// # Safety
/// `iov` must point to `cnt` valid `IoVec`s for the call's duration.
#[inline]
pub unsafe fn writev(fd: i32, iov: *const IoVec, cnt: usize) -> Result<usize, Errno> {
    let slice = core::slice::from_raw_parts(iov, cnt);
    let mut total = 0;
    for v in slice {
        let b = core::slice::from_raw_parts(v.base, v.len);
        match wasi::write(fd, b) {
            Ok(n) => {
                total += n;
                if n < v.len {
                    break;
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}

#[inline]
pub fn close(fd: i32) -> Result<(), Errno> {
    wasi::close(fd)
}

#[inline]
pub fn getrandom(buf: &mut [u8]) -> Result<(), Errno> {
    wasi::getrandom(buf)
}

#[inline]
pub fn monotonic() -> (u64, u32) {
    wasi::clock(wasi::CLOCKID_MONOTONIC)
}

#[inline]
pub fn gettimeofday() -> Result<(u64, u64), Errno> {
    let (secs, nanos) = wasi::clock(wasi::CLOCKID_REALTIME);
    Ok((secs, (nanos / 1000) as u64))
}

#[inline]
pub fn exit_group(code: i32) -> ! {
    unsafe { wasi::proc_exit(code as u32) }
}

// ---------------------------------------------------------------------------
// Allocator hooks: no mmap on wasm (allocator.rs grows linear memory directly)
// ---------------------------------------------------------------------------

#[inline]
pub fn mmap_anon(_len: usize, _prot: usize) -> Result<*mut u8, Errno> {
    Err(ENOSYS)
}

/// # Safety
/// No-op on wasm; linear memory cannot be unmapped.
#[inline]
pub unsafe fn munmap(_addr: *mut u8, _len: usize) -> Result<(), Errno> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Process / identity
// ---------------------------------------------------------------------------

#[inline]
pub fn getpid() -> u32 {
    1
}
#[inline]
pub fn gettid() -> u64 {
    0
}
#[inline]
pub fn num_cpus() -> usize {
    1
}

// ---------------------------------------------------------------------------
// Unsupported surface: filesystem, sockets, processes (return ENOSYS)
// ---------------------------------------------------------------------------

#[inline]
pub fn open(_path: &core::ffi::CStr, _flags: usize, _mode: u32) -> Result<i32, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn getcwd(_buf: &mut [u8]) -> Result<usize, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn chdir(_path: &core::ffi::CStr) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn lseek(_fd: i32, _offset: i64, _whence: i32) -> Result<u64, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn ftruncate(_fd: i32, _len: u64) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn fsync(_fd: i32) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn dup(_fd: i32) -> Result<i32, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn statat(_path: &core::ffi::CStr, _follow: bool) -> Result<StatBuf, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn fstat(_fd: i32) -> Result<StatBuf, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn rename(_old: &core::ffi::CStr, _new: &core::ffi::CStr) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn rmdir(_path: &core::ffi::CStr) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn getdirentries(_fd: i32, _buf: &mut [u8]) -> Result<usize, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn unlink(_path: &core::ffi::CStr) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn mkdir(_path: &core::ffi::CStr, _mode: u32) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn socket(_domain: i32, _ty: i32, _protocol: i32) -> Result<i32, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn connect(_fd: i32, _addr: *const u8, _addrlen: u32) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn bind(_fd: i32, _addr: *const u8, _addrlen: u32) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn listen(_fd: i32, _backlog: i32) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn accept(_fd: i32, _addr: *mut u8, _addrlen: *mut u32) -> Result<i32, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn sendto(
    _fd: i32,
    _buf: &[u8],
    _flags: i32,
    _addr: *const u8,
    _addrlen: u32,
) -> Result<usize, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn recvfrom(
    _fd: i32,
    _buf: &mut [u8],
    _flags: i32,
    _addr: *mut u8,
    _addrlen: *mut u32,
) -> Result<usize, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn setsockopt(
    _fd: i32,
    _level: i32,
    _name: i32,
    _val: *const u8,
    _len: u32,
) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn getsockname(_fd: i32, _addr: *mut u8, _addrlen: *mut u32) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn getpeername(_fd: i32, _addr: *mut u8, _addrlen: *mut u32) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn shutdown(_fd: i32, _how: i32) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn fork() -> Result<i32, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn execve(_path: &core::ffi::CStr, _argv: *const *const u8, _envp: *const *const u8) -> Errno {
    ENOSYS
}
#[inline]
pub fn wait4(_pid: i32, _status: &mut i32, _options: i32) -> Result<i32, Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn pipe() -> Result<(i32, i32), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn dup2(_old: i32, _new: i32) -> Result<(), Errno> {
    Err(ENOSYS)
}
#[inline]
pub fn fcntl(_fd: i32, _cmd: i32, _arg: i32) -> Result<i32, Errno> {
    Err(ENOSYS)
}
