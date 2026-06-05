//! `std::sync` subset: `Mutex`, `RwLock`, `Once`, `OnceLock`, plus `Arc`/`Weak`
//! re-exported from `alloc` and the `atomic` module from `core`.
//!
//! The locks are spin-based. With no real threads yet (M6 brings clone/futex)
//! this is correct and `Sync`; it will be upgraded to futex-backed blocking when
//! threads land.

pub use crate::alloc::sync::{Arc, Weak};
pub use core::sync::atomic;

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicIsize, AtomicU8, Ordering};

/// Sentinel poison error. These locks never poison (panics abort the process),
/// so this is constructed only to satisfy the `std` return-type shape.
pub struct PoisonError<T> {
    guard: T,
}
impl<T> core::fmt::Debug for PoisonError<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PoisonError { .. }")
    }
}
impl<T> core::fmt::Display for PoisonError<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("poisoned lock: another task failed inside")
    }
}

impl<T> PoisonError<T> {
    pub fn into_inner(self) -> T {
        self.guard
    }
    pub fn get_ref(&self) -> &T {
        &self.guard
    }
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

/// Result of a lock operation. Drop-in for `std::sync::LockResult`.
pub type LockResult<G> = Result<G, PoisonError<G>>;

/// Returned by `try_lock`. Drop-in for `std::sync::TryLockError`/`TryLockResult`.
pub enum TryLockError<T> {
    Poisoned(PoisonError<T>),
    WouldBlock,
}
pub type TryLockResult<G> = Result<G, TryLockError<G>>;

// ---------------------------------------------------------------------------
// Mutex
// ---------------------------------------------------------------------------

/// A spin-based mutual-exclusion lock. Drop-in for `std::sync::Mutex`.
pub struct Mutex<T: ?Sized> {
    locked: AtomicU8,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Mutex<T> {
        Mutex {
            locked: AtomicU8::new(0),
            data: UnsafeCell::new(value),
        }
    }
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> LockResult<MutexGuard<'_, T>> {
        while self
            .locked
            .compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        Ok(MutexGuard { lock: self })
    }

    pub fn try_lock(&self) -> TryLockResult<MutexGuard<'_, T>> {
        if self
            .locked
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Ok(MutexGuard { lock: self })
        } else {
            Err(TryLockError::WouldBlock)
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

/// RAII guard for [`Mutex`].
pub struct MutexGuard<'a, T: ?Sized> {
    lock: &'a Mutex<T>,
}
impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}
impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}
impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(0, Ordering::Release);
    }
}

// ---------------------------------------------------------------------------
// RwLock (spin; -1 = write-locked, >0 = reader count)
// ---------------------------------------------------------------------------

/// A spin-based reader-writer lock. Drop-in for `std::sync::RwLock`.
pub struct RwLock<T: ?Sized> {
    state: AtomicIsize,
    data: UnsafeCell<T>,
}
unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    pub const fn new(value: T) -> RwLock<T> {
        RwLock {
            state: AtomicIsize::new(0),
            data: UnsafeCell::new(value),
        }
    }
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> RwLock<T> {
    pub fn read(&self) -> LockResult<RwLockReadGuard<'_, T>> {
        loop {
            let s = self.state.load(Ordering::Relaxed);
            if s >= 0
                && self
                    .state
                    .compare_exchange_weak(s, s + 1, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
            {
                return Ok(RwLockReadGuard { lock: self });
            }
            core::hint::spin_loop();
        }
    }
    pub fn write(&self) -> LockResult<RwLockWriteGuard<'_, T>> {
        while self
            .state
            .compare_exchange_weak(0, -1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        Ok(RwLockWriteGuard { lock: self })
    }
}

pub struct RwLockReadGuard<'a, T: ?Sized> {
    lock: &'a RwLock<T>,
}
impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}
impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.fetch_sub(1, Ordering::Release);
    }
}

pub struct RwLockWriteGuard<'a, T: ?Sized> {
    lock: &'a RwLock<T>,
}
impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}
impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}
impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.store(0, Ordering::Release);
    }
}

// ---------------------------------------------------------------------------
// Once / OnceLock
// ---------------------------------------------------------------------------

const INCOMPLETE: u8 = 0;
const RUNNING: u8 = 1;
const COMPLETE: u8 = 2;

/// A synchronization primitive for one-time global initialization. Drop-in for
/// `std::sync::Once`.
pub struct Once {
    state: AtomicU8,
}
unsafe impl Sync for Once {}

impl Once {
    pub const fn new() -> Once {
        Once {
            state: AtomicU8::new(INCOMPLETE),
        }
    }
    pub fn is_completed(&self) -> bool {
        self.state.load(Ordering::Acquire) == COMPLETE
    }
    pub fn call_once<F: FnOnce()>(&self, f: F) {
        if self.state.load(Ordering::Acquire) == COMPLETE {
            return;
        }
        match self
            .state
            .compare_exchange(INCOMPLETE, RUNNING, Ordering::Acquire, Ordering::Acquire)
        {
            Ok(_) => {
                f();
                self.state.store(COMPLETE, Ordering::Release);
            }
            Err(_) => {
                // Another caller is running or finished; spin until complete.
                while self.state.load(Ordering::Acquire) != COMPLETE {
                    core::hint::spin_loop();
                }
            }
        }
    }
}

impl Default for Once {
    fn default() -> Self {
        Once::new()
    }
}

/// A thread-safe cell written at most once. Drop-in for `std::sync::OnceLock`.
pub struct OnceLock<T> {
    once: Once,
    value: UnsafeCell<Option<T>>,
}
unsafe impl<T: Send + Sync> Sync for OnceLock<T> {}
unsafe impl<T: Send> Send for OnceLock<T> {}

impl<T> OnceLock<T> {
    pub const fn new() -> OnceLock<T> {
        OnceLock {
            once: Once::new(),
            value: UnsafeCell::new(None),
        }
    }
    pub fn get(&self) -> Option<&T> {
        if self.once.is_completed() {
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }
    pub fn set(&self, value: T) -> Result<(), T> {
        let mut value = Some(value);
        self.once.call_once(|| {
            unsafe { *self.value.get() = value.take() };
        });
        match value {
            None => Ok(()),
            Some(v) => Err(v),
        }
    }
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        self.once.call_once(|| {
            unsafe { *self.value.get() = Some(f()) };
        });
        unsafe { (*self.value.get()).as_ref().unwrap() }
    }
}

impl<T> Default for OnceLock<T> {
    fn default() -> Self {
        OnceLock::new()
    }
}
