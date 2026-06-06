//! `std::sync`: `Mutex` (futex-backed), `RwLock`, `Once`, `OnceLock`, `Condvar`,
//! `Barrier`, `LazyLock`, and the `mpsc` channel — plus `Arc`/`Weak` from
//! `alloc` and the `atomic` module from `core`.
//!
//! `Mutex`, `RwLock`, and `Condvar` all block in the kernel via the futex
//! (`futex` on Linux, `__ulock` on macOS) — no busy-spinning under contention.

pub use crate::alloc::sync::{Arc, Weak};
pub use core::sync::atomic;

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

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

/// A futex-backed mutual-exclusion lock. Drop-in for `std::sync::Mutex`.
///
/// State: 0 = unlocked, 1 = locked (no waiters), 2 = locked with waiters. The
/// uncontended path is a single CAS; contention blocks in the kernel via the
/// futex (`futex` on Linux, `__ulock` on macOS) rather than spinning.
pub struct Mutex<T: ?Sized> {
    state: AtomicU32,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Mutex<T> {
        Mutex {
            state: AtomicU32::new(0),
            data: UnsafeCell::new(value),
        }
    }
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> Mutex<T> {
    fn raw_lock(&self) {
        if self
            .state
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            return;
        }
        // Contended: mark as "locked with waiters" and block until released.
        loop {
            if self.state.swap(2, Ordering::Acquire) == 0 {
                return;
            }
            crate::sys_thread::futex_wait(&self.state, 2);
        }
    }

    fn raw_unlock(&self) {
        if self.state.swap(0, Ordering::Release) == 2 {
            crate::sys_thread::futex_wake(&self.state);
        }
    }

    pub fn lock(&self) -> LockResult<MutexGuard<'_, T>> {
        self.raw_lock();
        Ok(MutexGuard { lock: self })
    }

    pub fn try_lock(&self) -> TryLockResult<MutexGuard<'_, T>> {
        if self
            .state
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
        self.lock.raw_unlock();
    }
}

// ---------------------------------------------------------------------------
// RwLock (futex-backed; reader-preferring)
//
// `state` is a 32-bit word: the high bit (`RW_WRITER`) means write-locked, and
// the low 31 bits count active readers. Contended readers and writers block in
// the kernel on `state` via the futex, exactly like `Mutex`. When the last
// reader or the writer releases, it wakes everyone so a waiting writer (or a
// herd of readers) can retry. Reader-preferring: a steady stream of readers can
// starve a writer — acceptable, and what a simple `std`-shaped lock needs.
// ---------------------------------------------------------------------------

const RW_WRITER: u32 = 1 << 31;

/// A futex-backed reader-writer lock. Drop-in for `std::sync::RwLock`.
pub struct RwLock<T: ?Sized> {
    state: AtomicU32,
    data: UnsafeCell<T>,
}
unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    pub const fn new(value: T) -> RwLock<T> {
        RwLock {
            state: AtomicU32::new(0),
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
            let s = self.state.load(Ordering::Acquire);
            if s & RW_WRITER == 0 {
                // No writer: try to bump the reader count.
                if self
                    .state
                    .compare_exchange_weak(s, s + 1, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
                {
                    return Ok(RwLockReadGuard { lock: self });
                }
                // CAS lost a race; retry without sleeping.
            } else {
                // Writer holds it: sleep until `state` changes.
                crate::sys_thread::futex_wait(&self.state, s);
            }
        }
    }
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        let s = self.state.load(Ordering::Acquire);
        if s & RW_WRITER == 0
            && self
                .state
                .compare_exchange(s, s + 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        {
            Some(RwLockReadGuard { lock: self })
        } else {
            None
        }
    }
    pub fn write(&self) -> LockResult<RwLockWriteGuard<'_, T>> {
        loop {
            // Acquire only when fully unlocked (no readers, no writer).
            if self
                .state
                .compare_exchange(0, RW_WRITER, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(RwLockWriteGuard { lock: self });
            }
            let s = self.state.load(Ordering::Relaxed);
            if s != 0 {
                crate::sys_thread::futex_wait(&self.state, s);
            }
        }
    }
    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
        if self
            .state
            .compare_exchange(0, RW_WRITER, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(RwLockWriteGuard { lock: self })
        } else {
            None
        }
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
        // If we were the last reader, a writer may be waiting — wake the herd.
        if self.lock.state.fetch_sub(1, Ordering::Release) == 1 {
            crate::sys_thread::futex_wake_all(&self.lock.state);
        }
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
        // Wake every waiter: readers and/or a writer are all blocked on `state`.
        crate::sys_thread::futex_wake_all(&self.lock.state);
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

// ---------------------------------------------------------------------------
// Condvar
// ---------------------------------------------------------------------------

/// A condition variable. Drop-in for `std::sync::Condvar`.
///
/// Futex-backed via a sequence counter: `wait` reads the sequence under the
/// mutex, drops the mutex, and blocks until the sequence changes; `notify_*`
/// bumps the sequence and wakes waiters. Spurious wakeups are possible, so use
/// it in the usual `while !condition { guard = cv.wait(guard)? }` loop.
pub struct Condvar {
    seq: AtomicU32,
}

impl Condvar {
    pub const fn new() -> Condvar {
        Condvar {
            seq: AtomicU32::new(0),
        }
    }

    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> LockResult<MutexGuard<'a, T>> {
        let seq = self.seq.load(Ordering::Acquire);
        let mutex: &Mutex<T> = guard.lock;
        // Release the mutex without running the guard's Drop (we re-lock below).
        mutex.raw_unlock();
        core::mem::forget(guard);
        crate::sys_thread::futex_wait(&self.seq, seq);
        mutex.raw_lock();
        Ok(MutexGuard { lock: mutex })
    }

    pub fn wait_while<'a, T, F>(
        &self,
        mut guard: MutexGuard<'a, T>,
        mut condition: F,
    ) -> LockResult<MutexGuard<'a, T>>
    where
        F: FnMut(&mut T) -> bool,
    {
        while condition(&mut guard) {
            guard = self.wait(guard)?;
        }
        Ok(guard)
    }

    pub fn notify_one(&self) {
        self.seq.fetch_add(1, Ordering::Release);
        crate::sys_thread::futex_wake(&self.seq);
    }

    pub fn notify_all(&self) {
        self.seq.fetch_add(1, Ordering::Release);
        crate::sys_thread::futex_wake_all(&self.seq);
    }
}

impl Default for Condvar {
    fn default() -> Self {
        Condvar::new()
    }
}

// ---------------------------------------------------------------------------
// Barrier
// ---------------------------------------------------------------------------

/// A synchronization point for a fixed number of threads. Drop-in for
/// `std::sync::Barrier`.
pub struct Barrier {
    state: Mutex<BarrierState>,
    cv: Condvar,
    n: usize,
}
struct BarrierState {
    count: usize,
    generation: usize,
}

/// Returned by [`Barrier::wait`].
pub struct BarrierWaitResult(bool);
impl BarrierWaitResult {
    pub fn is_leader(&self) -> bool {
        self.0
    }
}

impl Barrier {
    pub fn new(n: usize) -> Barrier {
        Barrier {
            state: Mutex::new(BarrierState {
                count: 0,
                generation: 0,
            }),
            cv: Condvar::new(),
            n,
        }
    }

    pub fn wait(&self) -> BarrierWaitResult {
        let mut state = self.state.lock().unwrap();
        let gen = state.generation;
        state.count += 1;
        if state.count < self.n {
            while gen == state.generation {
                state = self.cv.wait(state).unwrap();
            }
            BarrierWaitResult(false)
        } else {
            state.count = 0;
            state.generation = state.generation.wrapping_add(1);
            self.cv.notify_all();
            BarrierWaitResult(true)
        }
    }
}

// ---------------------------------------------------------------------------
// LazyLock
// ---------------------------------------------------------------------------

/// A value initialized on first access. Drop-in for `std::sync::LazyLock`.
pub struct LazyLock<T, F = fn() -> T> {
    once: Once,
    value: UnsafeCell<Option<T>>,
    init: UnsafeCell<Option<F>>,
}
unsafe impl<T: Sync + Send, F: Send> Sync for LazyLock<T, F> {}

impl<T, F: FnOnce() -> T> LazyLock<T, F> {
    pub const fn new(f: F) -> LazyLock<T, F> {
        LazyLock {
            once: Once::new(),
            value: UnsafeCell::new(None),
            init: UnsafeCell::new(Some(f)),
        }
    }
    pub fn force(this: &LazyLock<T, F>) -> &T {
        this.once.call_once(|| {
            let f = unsafe { (*this.init.get()).take().unwrap() };
            unsafe { *this.value.get() = Some(f()) };
        });
        unsafe { (*this.value.get()).as_ref().unwrap() }
    }
}

impl<T, F: FnOnce() -> T> Deref for LazyLock<T, F> {
    type Target = T;
    fn deref(&self) -> &T {
        LazyLock::force(self)
    }
}

// ---------------------------------------------------------------------------
// mpsc — multi-producer, single-consumer channels
// ---------------------------------------------------------------------------

/// `std::sync::mpsc` — a futex/condvar-backed unbounded channel.
pub mod mpsc {
    use super::{Condvar, Mutex};
    use crate::alloc::collections::VecDeque;
    use crate::alloc::sync::Arc;
    use core::fmt;
    use core::sync::atomic::{AtomicUsize, Ordering};

    struct Shared<T> {
        queue: Mutex<VecDeque<T>>,
        cv: Condvar,
        senders: AtomicUsize,
        receiver_gone: core::sync::atomic::AtomicBool,
    }

    /// The sending half of a channel.
    pub struct Sender<T> {
        shared: Arc<Shared<T>>,
    }
    /// The receiving half of a channel.
    pub struct Receiver<T> {
        shared: Arc<Shared<T>>,
    }

    /// Error returned by [`Sender::send`] when the receiver is gone.
    pub struct SendError<T>(pub T);
    impl<T> fmt::Debug for SendError<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("SendError(..)")
        }
    }
    /// Error returned by [`Receiver::recv`] when all senders are gone.
    #[derive(Debug)]
    pub struct RecvError;
    /// Error returned by [`Receiver::try_recv`].
    #[derive(Debug, PartialEq, Eq)]
    pub enum TryRecvError {
        Empty,
        Disconnected,
    }

    /// Create an unbounded channel.
    pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
            senders: AtomicUsize::new(1),
            receiver_gone: core::sync::atomic::AtomicBool::new(false),
        });
        (
            Sender {
                shared: shared.clone(),
            },
            Receiver { shared },
        )
    }

    impl<T> Sender<T> {
        pub fn send(&self, t: T) -> Result<(), SendError<T>> {
            if self.shared.receiver_gone.load(Ordering::Acquire) {
                return Err(SendError(t));
            }
            self.shared.queue.lock().unwrap().push_back(t);
            self.shared.cv.notify_one();
            Ok(())
        }
    }
    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Sender<T> {
            self.shared.senders.fetch_add(1, Ordering::Relaxed);
            Sender {
                shared: self.shared.clone(),
            }
        }
    }
    impl<T> Drop for Sender<T> {
        fn drop(&mut self) {
            if self.shared.senders.fetch_sub(1, Ordering::AcqRel) == 1 {
                // Last sender gone — wake any blocked receiver.
                self.shared.cv.notify_all();
            }
        }
    }

    impl<T> Receiver<T> {
        pub fn try_recv(&self) -> Result<T, TryRecvError> {
            let mut q = self.shared.queue.lock().unwrap();
            if let Some(v) = q.pop_front() {
                Ok(v)
            } else if self.shared.senders.load(Ordering::Acquire) == 0 {
                Err(TryRecvError::Disconnected)
            } else {
                Err(TryRecvError::Empty)
            }
        }

        pub fn recv(&self) -> Result<T, RecvError> {
            let mut q = self.shared.queue.lock().unwrap();
            loop {
                if let Some(v) = q.pop_front() {
                    return Ok(v);
                }
                if self.shared.senders.load(Ordering::Acquire) == 0 {
                    return Err(RecvError);
                }
                q = self.shared.cv.wait(q).unwrap();
            }
        }

        pub fn iter(&self) -> Iter<'_, T> {
            Iter { rx: self }
        }
    }
    impl<T> Drop for Receiver<T> {
        fn drop(&mut self) {
            self.shared.receiver_gone.store(true, Ordering::Release);
        }
    }

    pub struct Iter<'a, T> {
        rx: &'a Receiver<T>,
    }
    impl<T> Iterator for Iter<'_, T> {
        type Item = T;
        fn next(&mut self) -> Option<T> {
            self.rx.recv().ok()
        }
    }
}
