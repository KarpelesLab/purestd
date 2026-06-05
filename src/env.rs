//! Process arguments and environment — the `std::env` subset, read from the
//! values the loader hands to `_start`.
//!
//! `argv`/`envp` are the classic C `char**` arrays (NUL-terminated C strings,
//! the arrays themselves NULL-terminated). We capture the pointers once at
//! startup and decode lazily, lossily as UTF-8 (matching how `std` treats
//! non-Unicode on a best-effort basis here).

use crate::alloc::string::String;
use core::ffi::{c_char, CStr};
use core::fmt;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

static ARGC: AtomicUsize = AtomicUsize::new(0);
static ARGV: AtomicPtr<*const u8> = AtomicPtr::new(ptr::null_mut());
static ENVP: AtomicPtr<*const u8> = AtomicPtr::new(ptr::null_mut());

/// Record the loader-provided argument and environment vectors. Called once by
/// the [`entry!`](crate::entry) glue before user `main`.
pub(crate) unsafe fn init(argc: usize, argv: *const *const u8, envp: *const *const u8) {
    ARGC.store(argc, Ordering::Relaxed);
    ARGV.store(argv as *mut *const u8, Ordering::Relaxed);
    ENVP.store(envp as *mut *const u8, Ordering::Relaxed);
}

#[inline]
unsafe fn cstr_to_string(p: *const u8) -> String {
    let bytes = CStr::from_ptr(p as *const c_char).to_bytes();
    String::from_utf8_lossy(bytes).into_owned()
}

/// Iterator over the program's command-line arguments. `args().next()` is the
/// program path, mirroring `std::env::args()`.
pub struct Args {
    idx: usize,
    argc: usize,
    argv: *const *const u8,
}

impl Iterator for Args {
    type Item = String;
    fn next(&mut self) -> Option<String> {
        if self.idx >= self.argc || self.argv.is_null() {
            return None;
        }
        let p = unsafe { *self.argv.add(self.idx) };
        self.idx += 1;
        if p.is_null() {
            return None;
        }
        Some(unsafe { cstr_to_string(p) })
    }
}

impl ExactSizeIterator for Args {
    fn len(&self) -> usize {
        self.argc.saturating_sub(self.idx)
    }
}

/// Returns the command-line arguments, the first being the program path.
pub fn args() -> Args {
    Args {
        idx: 0,
        argc: ARGC.load(Ordering::Relaxed),
        argv: ARGV.load(Ordering::Relaxed) as *const *const u8,
    }
}

/// Iterator over `(key, value)` environment pairs.
pub struct Vars {
    idx: usize,
    envp: *const *const u8,
}

impl Iterator for Vars {
    type Item = (String, String);
    fn next(&mut self) -> Option<(String, String)> {
        if self.envp.is_null() {
            return None;
        }
        loop {
            let p = unsafe { *self.envp.add(self.idx) };
            if p.is_null() {
                return None;
            }
            self.idx += 1;
            let bytes = unsafe { CStr::from_ptr(p as *const c_char).to_bytes() };
            if let Some(eq) = bytes.iter().position(|&b| b == b'=') {
                let k = String::from_utf8_lossy(&bytes[..eq]).into_owned();
                let v = String::from_utf8_lossy(&bytes[eq + 1..]).into_owned();
                return Some((k, v));
            }
            // No '=' — skip malformed entry and continue.
        }
    }
}

/// Returns an iterator over the environment variables.
pub fn vars() -> Vars {
    Vars {
        idx: 0,
        envp: ENVP.load(Ordering::Relaxed) as *const *const u8,
    }
}

/// The error type for [`var`].
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VarError {
    /// The variable was not set.
    NotPresent,
}

impl fmt::Display for VarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VarError::NotPresent => f.write_str("environment variable not found"),
        }
    }
}

/// Fetch the environment variable `key`. Mirrors `std::env::var`.
pub fn var(key: &str) -> Result<String, VarError> {
    let envp = ENVP.load(Ordering::Relaxed) as *const *const u8;
    if envp.is_null() {
        return Err(VarError::NotPresent);
    }
    let mut i = 0;
    loop {
        let p = unsafe { *envp.add(i) };
        if p.is_null() {
            return Err(VarError::NotPresent);
        }
        i += 1;
        let bytes = unsafe { CStr::from_ptr(p as *const c_char).to_bytes() };
        if let Some(eq) = bytes.iter().position(|&b| b == b'=') {
            if &bytes[..eq] == key.as_bytes() {
                return Ok(String::from_utf8_lossy(&bytes[eq + 1..]).into_owned());
            }
        }
    }
}
