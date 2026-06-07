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

/// WASI startup: `argv` is supplied by the command crt; the environment is
/// fetched via WASI `environ_get` into leaked buffers (process-lifetime).
#[cfg(target_family = "wasm")]
pub(crate) unsafe fn init_wasm(argc: usize, argv: *const *const u8) {
    use crate::alloc::vec::Vec;
    use crate::arch::wasi;

    ARGC.store(argc, Ordering::Relaxed);
    ARGV.store(argv as *mut *const u8, Ordering::Relaxed);

    let mut count: usize = 0;
    let mut buf_size: usize = 0;
    if wasi::environ_sizes_get(&mut count, &mut buf_size) == 0 && count > 0 {
        let mut ptrs: Vec<*mut u8> = Vec::with_capacity(count + 1);
        ptrs.resize(count + 1, ptr::null_mut());
        let mut buf: Vec<u8> = Vec::with_capacity(buf_size.max(1));
        buf.resize(buf_size.max(1), 0);
        if wasi::environ_get(ptrs.as_mut_ptr(), buf.as_mut_ptr()) == 0 {
            ptrs[count] = ptr::null_mut(); // NULL-terminate the char** array
            let leaked = ptrs.leak();
            core::mem::forget(buf); // the strings the pointers reference
            ENVP.store(leaked.as_mut_ptr() as *mut *const u8, Ordering::Relaxed);
        }
    }
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

// The mutable environment: lazily seeded from the loader's `envp`, then owned by
// us so `set_var`/`remove_var` work (std keeps its own copy of `environ` too).
use crate::collections::BTreeMap;
use crate::sync::{Mutex, OnceLock};

static ENV_MAP: OnceLock<Mutex<BTreeMap<String, String>>> = OnceLock::new();

fn env_map() -> &'static Mutex<BTreeMap<String, String>> {
    ENV_MAP.get_or_init(|| {
        let mut m = BTreeMap::new();
        let envp = ENVP.load(Ordering::Relaxed) as *const *const u8;
        if !envp.is_null() {
            let mut i = 0;
            loop {
                let p = unsafe { *envp.add(i) };
                if p.is_null() {
                    break;
                }
                i += 1;
                let bytes = unsafe { CStr::from_ptr(p as *const c_char).to_bytes() };
                if let Some(eq) = bytes.iter().position(|&b| b == b'=') {
                    let k = String::from_utf8_lossy(&bytes[..eq]).into_owned();
                    let v = String::from_utf8_lossy(&bytes[eq + 1..]).into_owned();
                    m.insert(k, v);
                }
            }
        }
        Mutex::new(m)
    })
}

/// Iterator over `(key, value)` environment pairs.
pub struct Vars {
    inner: crate::alloc::vec::IntoIter<(String, String)>,
}
impl Iterator for Vars {
    type Item = (String, String);
    fn next(&mut self) -> Option<(String, String)> {
        self.inner.next()
    }
}

/// Returns a snapshot iterator over the environment variables.
pub fn vars() -> Vars {
    let snapshot: crate::alloc::vec::Vec<(String, String)> = env_map()
        .lock()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Vars {
        inner: snapshot.into_iter(),
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

impl core::error::Error for VarError {}

/// Fetch the environment variable `key`. Mirrors `std::env::var`.
pub fn var(key: &str) -> Result<String, VarError> {
    env_map()
        .lock()
        .unwrap()
        .get(key)
        .cloned()
        .ok_or(VarError::NotPresent)
}

/// Set the environment variable `key` to `value`.
pub fn set_var<K: AsRef<str>, V: AsRef<str>>(key: K, value: V) {
    env_map()
        .lock()
        .unwrap()
        .insert(key.as_ref().into(), value.as_ref().into());
}

/// Remove the environment variable `key`.
pub fn remove_var<K: AsRef<str>>(key: K) {
    env_map().lock().unwrap().remove(key.as_ref());
}

/// Returns the current working directory.
pub fn current_dir() -> crate::io::Result<crate::path::PathBuf> {
    let mut buf = [0u8; 4096];
    crate::syscall::getcwd(&mut buf).map_err(crate::io::Error::from)?;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Ok(crate::path::PathBuf::from(
        String::from_utf8_lossy(&buf[..end]).into_owned(),
    ))
}

/// Change the current working directory.
pub fn set_current_dir<P: AsRef<crate::path::Path>>(path: P) -> crate::io::Result<()> {
    let c = crate::ffi::CString::new(path.as_ref().as_str().as_bytes())
        .map_err(|_| crate::io::Error::from(crate::io::ErrorKind::InvalidInput))?;
    crate::syscall::chdir(&c).map_err(crate::io::Error::from)
}

/// Returns the OS temporary-files directory (`$TMPDIR`, else `/tmp`).
pub fn temp_dir() -> crate::path::PathBuf {
    match var("TMPDIR") {
        Ok(d) if !d.is_empty() => crate::path::PathBuf::from(d),
        _ => crate::path::PathBuf::from("/tmp"),
    }
}
