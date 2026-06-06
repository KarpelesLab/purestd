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

/// Fill `buf` with the NUL-terminated working directory.
///
/// Linux has `getcwd`; macOS has no such syscall, so we open `"."` and use
/// `fcntl(F_GETPATH)`.
#[inline]
pub fn getcwd(buf: &mut [u8]) -> Result<usize, Errno> {
    #[cfg(target_os = "macos")]
    {
        const F_GETPATH: usize = 50;
        let dot = c".";
        let fd = open(dot, O_RDONLY, 0)?;
        let r = from_ret(unsafe {
            arch::syscall3(nr::FCNTL, fd as usize, F_GETPATH, buf.as_mut_ptr() as usize)
        });
        let _ = close(fd);
        r.map(|_| buf.iter().position(|&b| b == 0).unwrap_or(buf.len()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        from_ret(unsafe { arch::syscall2(nr::GETCWD, buf.as_mut_ptr() as usize, buf.len()) })
    }
}

/// `chdir(path)`.
#[inline]
pub fn chdir(path: &core::ffi::CStr) -> Result<(), Errno> {
    from_ret(unsafe { arch::syscall1(nr::CHDIR, path.as_ptr() as usize) }).map(|_| ())
}

// ---- seek / file size ----
pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

/// `lseek(fd, offset, whence)` -> resulting absolute offset.
#[inline]
pub fn lseek(fd: i32, offset: i64, whence: i32) -> Result<u64, Errno> {
    from_ret(unsafe { arch::syscall3(nr::LSEEK, fd as usize, offset as usize, whence as usize) })
        .map(|o| o as u64)
}

/// `ftruncate(fd, len)`.
#[inline]
pub fn ftruncate(fd: i32, len: u64) -> Result<(), Errno> {
    from_ret(unsafe { arch::syscall2(nr::FTRUNCATE, fd as usize, len as usize) }).map(|_| ())
}

/// `fsync(fd)`.
#[inline]
pub fn fsync(fd: i32) -> Result<(), Errno> {
    from_ret(unsafe { arch::syscall1(nr::FSYNC, fd as usize) }).map(|_| ())
}

/// `dup(fd)` -> new fd.
#[inline]
pub fn dup(fd: i32) -> Result<i32, Errno> {
    from_ret(unsafe { arch::syscall1(nr::DUP, fd as usize) }).map(|fd| fd as i32)
}

// ---- stat / directories / rename ----
/// `*at` flag: don't follow a trailing symlink (lstat semantics).
#[cfg(target_os = "macos")]
pub const AT_SYMLINK_NOFOLLOW: usize = 0x20;
#[cfg(not(target_os = "macos"))]
pub const AT_SYMLINK_NOFOLLOW: usize = 0x100;
/// `unlinkat` flag: remove a directory (`rmdir`).
#[cfg(target_os = "macos")]
pub const AT_REMOVEDIR: usize = 0x80;
#[cfg(not(target_os = "macos"))]
pub const AT_REMOVEDIR: usize = 0x200;

/// A raw `stat` buffer. The kernel struct is smaller; the extra room is slack.
pub type StatBuf = [u8; 256];

/// Stat the file at `path` into a raw `stat` buffer (following a final symlink
/// only when `follow`).
///
/// macOS has no 64-bit-inode `fstatat`, so it uses `stat64`/`lstat64`; Linux uses
/// `newfstatat(AT_FDCWD, ...)`.
#[inline]
pub fn statat(path: &core::ffi::CStr, follow: bool) -> Result<StatBuf, Errno> {
    let mut buf: StatBuf = [0; 256];
    #[cfg(target_os = "macos")]
    {
        let n = if follow { nr::STAT64 } else { nr::LSTAT64 };
        from_ret(unsafe {
            arch::syscall2(n, path.as_ptr() as usize, buf.as_mut_ptr() as usize)
        })?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let flags = if follow { 0 } else { AT_SYMLINK_NOFOLLOW };
        from_ret(unsafe {
            arch::syscall4(
                nr::NEWFSTATAT,
                arch::AT_FDCWD as usize,
                path.as_ptr() as usize,
                buf.as_mut_ptr() as usize,
                flags,
            )
        })?;
    }
    Ok(buf)
}

/// `fstat(fd, &buf)`.
#[inline]
pub fn fstat(fd: i32) -> Result<StatBuf, Errno> {
    let mut buf: StatBuf = [0; 256];
    from_ret(unsafe { arch::syscall2(nr::FSTAT, fd as usize, buf.as_mut_ptr() as usize) })?;
    Ok(buf)
}

/// `renameat(AT_FDCWD, old, AT_FDCWD, new)`.
#[inline]
pub fn rename(old: &core::ffi::CStr, new: &core::ffi::CStr) -> Result<(), Errno> {
    from_ret(unsafe {
        arch::syscall4(
            nr::RENAMEAT,
            arch::AT_FDCWD as usize,
            old.as_ptr() as usize,
            arch::AT_FDCWD as usize,
            new.as_ptr() as usize,
        )
    })
    .map(|_| ())
}

/// `unlinkat(AT_FDCWD, path, AT_REMOVEDIR)` — remove an empty directory.
#[inline]
pub fn rmdir(path: &core::ffi::CStr) -> Result<(), Errno> {
    from_ret(unsafe {
        arch::syscall3(
            nr::UNLINKAT,
            arch::AT_FDCWD as usize,
            path.as_ptr() as usize,
            AT_REMOVEDIR,
        )
    })
    .map(|_| ())
}

/// Read a chunk of directory entries into `buf`. Returns bytes filled (0 = end).
#[inline]
pub fn getdirentries(fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
    #[cfg(target_os = "macos")]
    {
        let mut basep: i64 = 0;
        from_ret(unsafe {
            arch::syscall4(
                nr::GETDIRENTRIES64,
                fd as usize,
                buf.as_mut_ptr() as usize,
                buf.len(),
                &mut basep as *mut i64 as usize,
            )
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        from_ret(unsafe {
            arch::syscall3(nr::GETDENTS64, fd as usize, buf.as_mut_ptr() as usize, buf.len())
        })
    }
}

/// Monotonic clock as `(seconds, nanoseconds)`.
///
/// On Linux this is `clock_gettime(CLOCK_MONOTONIC)`. On macOS/arm64 it reads
/// the architectural counter (`CNTVCT_EL0`/`CNTFRQ_EL0`) directly — the same
/// source `mach_absolute_time` uses — since there is no stable `clock_gettime`
/// syscall there.
#[inline]
pub fn monotonic() -> (u64, u32) {
    #[cfg(target_os = "linux")]
    {
        let mut ts = [0i64; 2]; // struct timespec { i64 tv_sec; i64 tv_nsec; }
        let _ = unsafe { arch::syscall2(nr::CLOCK_GETTIME, 1, ts.as_mut_ptr() as usize) };
        (ts[0] as u64, ts[1] as u32)
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let (cnt, freq): (u64, u64);
        unsafe {
            core::arch::asm!("mrs {}, cntvct_el0", out(reg) cnt, options(nomem, nostack));
            core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq, options(nomem, nostack));
        }
        // nanos = cnt * 1e9 / freq, computed as (whole seconds, remainder ns).
        let secs = cnt / freq;
        let rem = cnt % freq;
        let nanos = (rem as u128 * 1_000_000_000 / freq as u128) as u32;
        (secs, nanos)
    }
}

/// Fill `buf` with cryptographically-secure random bytes from the kernel.
///
/// Uses `getrandom` on Linux and `getentropy` on macOS (which caps each call at
/// 256 bytes and returns 0 on success rather than a count).
#[inline]
pub fn getrandom(buf: &mut [u8]) -> Result<(), Errno> {
    #[cfg(target_os = "macos")]
    {
        // getentropy(ptr, len) — at most 256 bytes per call, returns 0/-errno.
        let mut off = 0;
        while off < buf.len() {
            let n = core::cmp::min(256, buf.len() - off);
            from_ret(unsafe {
                arch::syscall2(nr::GETENTROPY, buf[off..].as_mut_ptr() as usize, n)
            })?;
            off += n;
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        // getrandom(ptr, len, flags) — returns the number of bytes filled.
        let mut off = 0;
        while off < buf.len() {
            let n = from_ret(unsafe {
                arch::syscall3(nr::GETRANDOM, buf[off..].as_mut_ptr() as usize, buf.len() - off, 0)
            })?;
            if n == 0 {
                return Err(Errno(5)); // EIO: made no progress
            }
            off += n;
        }
        Ok(())
    }
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

// ---- sockets ----

/// `socket(domain, type, protocol)` -> fd.
#[inline]
pub fn socket(domain: i32, ty: i32, protocol: i32) -> Result<i32, Errno> {
    from_ret(unsafe { arch::syscall3(nr::SOCKET, domain as usize, ty as usize, protocol as usize) })
        .map(|fd| fd as i32)
}

/// `connect(fd, addr, addrlen)`.
#[inline]
pub fn connect(fd: i32, addr: *const u8, addrlen: u32) -> Result<(), Errno> {
    from_ret(unsafe { arch::syscall3(nr::CONNECT, fd as usize, addr as usize, addrlen as usize) })
        .map(|_| ())
}

/// `bind(fd, addr, addrlen)`.
#[inline]
pub fn bind(fd: i32, addr: *const u8, addrlen: u32) -> Result<(), Errno> {
    from_ret(unsafe { arch::syscall3(nr::BIND, fd as usize, addr as usize, addrlen as usize) })
        .map(|_| ())
}

/// `listen(fd, backlog)`.
#[inline]
pub fn listen(fd: i32, backlog: i32) -> Result<(), Errno> {
    from_ret(unsafe { arch::syscall2(nr::LISTEN, fd as usize, backlog as usize) }).map(|_| ())
}

/// `accept(fd, addr, addrlen)` -> new fd.
#[inline]
pub fn accept(fd: i32, addr: *mut u8, addrlen: *mut u32) -> Result<i32, Errno> {
    from_ret(unsafe { arch::syscall3(nr::ACCEPT, fd as usize, addr as usize, addrlen as usize) })
        .map(|fd| fd as i32)
}

/// `sendto(fd, buf, flags, addr, addrlen)` -> bytes sent.
#[inline]
pub fn sendto(
    fd: i32,
    buf: &[u8],
    flags: i32,
    addr: *const u8,
    addrlen: u32,
) -> Result<usize, Errno> {
    from_ret(unsafe {
        arch::syscall6(
            nr::SENDTO,
            fd as usize,
            buf.as_ptr() as usize,
            buf.len(),
            flags as usize,
            addr as usize,
            addrlen as usize,
        )
    })
}

/// `recvfrom(fd, buf, flags, addr, addrlen)` -> bytes received.
#[inline]
pub fn recvfrom(
    fd: i32,
    buf: &mut [u8],
    flags: i32,
    addr: *mut u8,
    addrlen: *mut u32,
) -> Result<usize, Errno> {
    from_ret(unsafe {
        arch::syscall6(
            nr::RECVFROM,
            fd as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
            flags as usize,
            addr as usize,
            addrlen as usize,
        )
    })
}

/// `setsockopt(fd, level, name, val, len)`.
#[inline]
pub fn setsockopt(fd: i32, level: i32, name: i32, val: *const u8, len: u32) -> Result<(), Errno> {
    from_ret(unsafe {
        arch::syscall5(
            nr::SETSOCKOPT,
            fd as usize,
            level as usize,
            name as usize,
            val as usize,
            len as usize,
        )
    })
    .map(|_| ())
}

/// `getsockname(fd, addr, addrlen)`.
#[inline]
pub fn getsockname(fd: i32, addr: *mut u8, addrlen: *mut u32) -> Result<(), Errno> {
    from_ret(unsafe {
        arch::syscall3(nr::GETSOCKNAME, fd as usize, addr as usize, addrlen as usize)
    })
    .map(|_| ())
}

/// `getpeername(fd, addr, addrlen)`.
#[inline]
pub fn getpeername(fd: i32, addr: *mut u8, addrlen: *mut u32) -> Result<(), Errno> {
    from_ret(unsafe {
        arch::syscall3(nr::GETPEERNAME, fd as usize, addr as usize, addrlen as usize)
    })
    .map(|_| ())
}

/// `shutdown(fd, how)`.
#[inline]
pub fn shutdown(fd: i32, how: i32) -> Result<(), Errno> {
    from_ret(unsafe { arch::syscall2(nr::SHUTDOWN, fd as usize, how as usize) }).map(|_| ())
}

// ---- process creation ----

/// `fork()` -> 0 in the child, child pid in the parent.
///
/// On macOS the syscall reports child-vs-parent in `x1` (the classic Darwin fork
/// quirk), so we read it directly.
#[inline]
pub fn fork() -> Result<i32, Errno> {
    #[cfg(target_os = "macos")]
    {
        let (x0, x1, err): (usize, usize, usize);
        unsafe {
            core::arch::asm!(
                "svc #0x80",
                "cset {e}, cs",
                inout("x16") (0x2000000usize | 2) => _,
                lateout("x17") _,
                out("x0") x0,
                out("x1") x1,
                e = out(reg) err,
                options(nostack),
            );
        }
        if err != 0 {
            return Err(Errno(x0 as i32));
        }
        // x1 == 1 marks the child; its x0 holds the parent's pid, not 0.
        if x1 == 1 {
            Ok(0)
        } else {
            Ok(x0 as i32)
        }
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        from_ret(unsafe { arch::syscall0(nr::FORK) }).map(|p| p as i32)
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        // aarch64 Linux has no `fork`; clone with SIGCHLD and no new stack.
        const SIGCHLD: usize = 17;
        from_ret(unsafe { arch::syscall5(nr::CLONE, SIGCHLD, 0, 0, 0, 0) }).map(|p| p as i32)
    }
}

/// `execve(path, argv, envp)` — only returns (an error) if exec fails.
#[inline]
pub fn execve(path: &core::ffi::CStr, argv: *const *const u8, envp: *const *const u8) -> Errno {
    let r = unsafe {
        arch::syscall3(nr::EXECVE, path.as_ptr() as usize, argv as usize, envp as usize)
    };
    Errno(-(r as isize) as i32)
}

/// `wait4(pid, &status, options, 0)` -> the reaped pid.
#[inline]
pub fn wait4(pid: i32, status: &mut i32, options: i32) -> Result<i32, Errno> {
    from_ret(unsafe {
        arch::syscall4(
            nr::WAIT4,
            pid as usize,
            status as *mut i32 as usize,
            options as usize,
            0,
        )
    })
    .map(|p| p as i32)
}

/// `pipe()` -> (read_fd, write_fd).
#[inline]
pub fn pipe() -> Result<(i32, i32), Errno> {
    #[cfg(target_os = "macos")]
    {
        // Darwin returns the two fds in x0/x1.
        let (r, w, err): (usize, usize, usize);
        unsafe {
            core::arch::asm!(
                "svc #0x80",
                "cset {e}, cs",
                inout("x16") (0x2000000usize | 42) => _,
                lateout("x17") _,
                out("x0") r,
                out("x1") w,
                e = out(reg) err,
                options(nostack),
            );
        }
        if err != 0 {
            return Err(Errno(r as i32));
        }
        Ok((r as i32, w as i32))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut fds = [0i32; 2];
        from_ret(unsafe { arch::syscall2(nr::PIPE2, fds.as_mut_ptr() as usize, 0) })?;
        Ok((fds[0], fds[1]))
    }
}

/// `dup2(old, new)` — make `new` refer to `old`.
#[inline]
pub fn dup2(old: i32, new: i32) -> Result<(), Errno> {
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        from_ret(unsafe { arch::syscall3(nr::DUP3, old as usize, new as usize, 0) }).map(|_| ())
    }
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    {
        from_ret(unsafe { arch::syscall2(nr::DUP2, old as usize, new as usize) }).map(|_| ())
    }
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
