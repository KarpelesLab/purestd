//! Thread-local storage.
//!
//! A key-based `thread_local!` implementation: each `thread_local!` static is a
//! [`LocalKey`], identified by its own address; per-thread values live in a
//! global registry keyed by `(thread id, key)`. This needs no ELF-TLS / thread
//! pointer setup, so it works on our raw Mach/clone threads uniformly.
//!
//! Caveat: destructors do not yet run at thread exit (values are leaked when a
//! thread ends). The common `FOO.with(|v| …)` access pattern — including
//! `Cell`/`RefCell` interior mutability — works as with `std`.

use crate::alloc::boxed::Box;
use crate::alloc::collections::BTreeMap;
use crate::sync::{Mutex, OnceLock};

static REGISTRY: OnceLock<Mutex<BTreeMap<(u64, usize), usize>>> = OnceLock::new();

fn registry() -> &'static Mutex<BTreeMap<(u64, usize), usize>> {
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// A thread-local storage key. Drop-in for `std::thread::LocalKey`.
pub struct LocalKey<T: 'static> {
    init: fn() -> T,
}

impl<T: 'static> LocalKey<T> {
    #[doc(hidden)]
    pub const fn new(init: fn() -> T) -> LocalKey<T> {
        LocalKey { init }
    }

    /// Access this thread's copy of the value, initializing it on first use.
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let key = self as *const LocalKey<T> as usize;
        let tid = crate::syscall::gettid();
        let reg = registry();

        let existing = reg.lock().unwrap().get(&(tid, key)).copied();
        let ptr = match existing {
            Some(p) => p,
            None => {
                // Box gives the value a stable address independent of the map.
                let p = Box::into_raw(Box::new((self.init)())) as usize;
                reg.lock().unwrap().insert((tid, key), p);
                p
            }
        };
        // SAFETY: the boxed value is owned solely by this (tid, key) slot, so
        // this thread is its only accessor; the pointer stays valid (Box, not
        // an interior map pointer).
        let value: &T = unsafe { &*(ptr as *const T) };
        f(value)
    }

    /// Like [`with`](Self::with) but never fails here (kept for `std` parity).
    pub fn try_with<F, R>(&'static self, f: F) -> Result<R, AccessError>
    where
        F: FnOnce(&T) -> R,
    {
        Ok(self.with(f))
    }
}

/// Error returned by `LocalKey::try_with` in `std` (never produced here).
#[derive(Clone, Copy, Debug)]
pub struct AccessError;

/// Declare new thread-local storage keys. Drop-in for `std::thread_local!`.
#[macro_export]
macro_rules! thread_local {
    () => {};
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr; $($rest:tt)*) => {
        $(#[$attr])* $vis static $name: $crate::tls::LocalKey<$t> =
            $crate::tls::LocalKey::new(|| $init);
        $crate::thread_local!($($rest)*);
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr) => {
        $(#[$attr])* $vis static $name: $crate::tls::LocalKey<$t> =
            $crate::tls::LocalKey::new(|| $init);
    };
}
