//! A drop-in subset of `std::thread`: `spawn`, `JoinHandle`/`join`, `Builder`,
//! `sleep`, `yield_now`, and a minimal `current()`/`Thread`/`ThreadId`.
//!
//! Threads are real OS threads (Mach `thread_create_running` on macOS, `clone`
//! on Linux) sharing the address space. Synchronization for join uses a futex
//! (`__ulock`/`futex`). Built with `panic = "abort"`, so a panic in a thread
//! aborts the whole process — `join` therefore always yields `Ok`.

use crate::alloc::boxed::Box;
use crate::alloc::string::String;
use crate::alloc::sync::Arc;
use crate::io;
use crate::sys_thread;
use crate::time::Duration;
use core::any::Any;
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

const DEFAULT_STACK: usize = 2 * 1024 * 1024;

/// The result of a joined thread. With `panic = "abort"` the `Err` arm never
/// occurs, but the type mirrors `std::thread::Result`.
pub type Result<T> = core::result::Result<T, Box<dyn Any + Send + 'static>>;

struct Packet<T> {
    state: AtomicU32, // 0 = running, 1 = finished
    result: UnsafeCell<Option<T>>,
}
// Access to `result` is published through `state` (Release/Acquire).
unsafe impl<T: Send> Send for Packet<T> {}
unsafe impl<T: Send> Sync for Packet<T> {}

struct Payload<F, T> {
    f: F,
    packet: Arc<Packet<T>>,
    thread: Thread,
    stack_base: usize,
    stack_size: usize,
}

extern "C" fn thread_start<F, T>(arg: *mut u8) -> !
where
    F: FnOnce() -> T,
{
    // Move the payload off the heap onto this thread's stack.
    let payload = unsafe { *Box::from_raw(arg as *mut Payload<F, T>) };
    let Payload {
        f,
        packet,
        thread,
        stack_base,
        stack_size,
    } = payload;

    // Install our handle so `current()`/`park()` find this thread's parker.
    CURRENT.with(|c| *c.borrow_mut() = Some(thread));

    let value = f(); // a panic here aborts the process (panic = "abort")

    unsafe {
        *packet.result.get() = Some(value);
    }
    packet.state.store(1, Ordering::Release);
    sys_thread::futex_wake(&packet.state);
    drop(packet); // release this thread's reference

    // Frees this thread's stack (macOS) and terminates. Never returns.
    unsafe { sys_thread::thread_exit(stack_base, stack_size) }
}

/// An owned handle to a thread; dropping it detaches the thread.
pub struct JoinHandle<T> {
    packet: Arc<Packet<T>>,
    thread: Thread,
}

impl<T> JoinHandle<T> {
    /// Wait for the associated thread to finish and return its result.
    pub fn join(self) -> Result<T> {
        sys_thread::futex_wait(&self.packet.state, 0);
        // SAFETY: state == 1 (Acquire) means the result was published.
        let value = unsafe { (*self.packet.result.get()).take() };
        Ok(value.expect("thread finished without producing a result"))
    }

    /// Whether the thread has finished.
    pub fn is_finished(&self) -> bool {
        self.packet.state.load(Ordering::Acquire) == 1
    }

    /// The underlying thread handle.
    pub fn thread(&self) -> &Thread {
        &self.thread
    }
}

/// Thread configuration. Drop-in for `std::thread::Builder`.
#[derive(Default)]
pub struct Builder {
    stack_size: Option<usize>,
    name: Option<String>,
}

impl Builder {
    pub fn new() -> Builder {
        Builder::default()
    }
    pub fn stack_size(mut self, size: usize) -> Builder {
        self.stack_size = Some(size);
        self
    }
    pub fn name(mut self, name: String) -> Builder {
        self.name = Some(name);
        self
    }

    pub fn spawn<F, T>(self, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let stack_size = self.stack_size.unwrap_or(DEFAULT_STACK);
        // Round up to a page (16 KiB covers both targets).
        let stack_size = (stack_size + 0x3fff) & !0x3fff;

        let base = crate::syscall::mmap_anon(
            stack_size,
            crate::arch::PROT_READ | crate::arch::PROT_WRITE,
        )
        .map_err(io::Error::from)? as usize;

        let stack_top = (base + stack_size) & !15;

        let packet = Arc::new(Packet {
            state: AtomicU32::new(0),
            result: UnsafeCell::new(None),
        });

        // The handle is built by the parent and shared: the child installs it as
        // its `current()` (so `park()` reaches this parker), and the caller keeps
        // a clone in the `JoinHandle` (so `unpark()` reaches the same parker).
        let thread = Thread::new(self.name);

        let payload = Box::new(Payload {
            f,
            packet: packet.clone(),
            thread: thread.clone(),
            stack_base: base,
            stack_size,
        });
        let arg = Box::into_raw(payload) as *mut u8;

        let entry: sys_thread::ThreadEntry = thread_start::<F, T>;
        match unsafe { sys_thread::spawn_os(entry, arg, stack_top) } {
            Ok(()) => Ok(JoinHandle { packet, thread }),
            Err(()) => {
                // Reclaim everything we allocated.
                unsafe {
                    drop(Box::from_raw(arg as *mut Payload<F, T>));
                    let _ = crate::syscall::munmap(base as *mut u8, stack_size);
                }
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "failed to spawn OS thread",
                ))
            }
        }
    }
}

/// Spawn a new thread, returning a [`JoinHandle`] for it.
pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Builder::new()
        .spawn(f)
        .expect("failed to spawn thread")
}

/// Put the current thread to sleep for at least `dur`.
pub fn sleep(dur: Duration) {
    sys_thread::sleep(dur);
}

/// Cooperatively yield the current timeslice.
pub fn yield_now() {
    sys_thread::yield_now();
}

/// An opaque, process-unique thread identifier. (Like `std`, this is *not* the
/// OS tid — it's a monotonic token assigned at handle creation.)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct ThreadId(u64);

impl ThreadId {
    fn next() -> ThreadId {
        static NEXT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);
        ThreadId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// A blocking token for `park`/`unpark`. `state` is 0 (empty) or 1 (a token is
/// available); both transitions wake any futex waiter.
struct Parker {
    state: AtomicU32,
}

impl Parker {
    fn new() -> Parker {
        Parker { state: AtomicU32::new(0) }
    }

    /// Block until a token is available, then consume it. A prior `unpark`
    /// makes this return immediately. Only returns once genuinely notified.
    fn park(&self) {
        if self.state.swap(0, Ordering::Acquire) == 1 {
            return; // token was already waiting
        }
        loop {
            sys_thread::futex_wait(&self.state, 0);
            if self.state.swap(0, Ordering::Acquire) == 1 {
                return;
            }
            // Spurious wake: keep waiting.
        }
    }

    /// Like [`park`](Self::park) but gives up after `dur`. May also return
    /// spuriously (as `std::thread::park_timeout` is permitted to).
    fn park_timeout(&self, dur: Duration) {
        if self.state.swap(0, Ordering::Acquire) == 1 {
            return;
        }
        sys_thread::futex_wait_timeout(&self.state, 0, dur);
        self.state.swap(0, Ordering::Acquire); // consume a token if one arrived
    }

    /// Make the next (or current) `park` return. Idempotent until consumed.
    fn unpark(&self) {
        if self.state.swap(1, Ordering::Release) == 0 {
            sys_thread::futex_wake(&self.state);
        }
    }
}

struct Inner {
    id: ThreadId,
    name: Option<String>,
    parker: Parker,
}

/// A handle to a thread. Drop-in-ish for `std::thread::Thread`. Cloning shares
/// the same underlying thread (and parker), as in `std`.
#[derive(Clone)]
pub struct Thread(Arc<Inner>);

impl Thread {
    fn new(name: Option<String>) -> Thread {
        Thread(Arc::new(Inner {
            id: ThreadId::next(),
            name,
            parker: Parker::new(),
        }))
    }
    pub fn name(&self) -> Option<&str> {
        self.0.name.as_deref()
    }
    pub fn id(&self) -> ThreadId {
        self.0.id
    }
    /// Atomically make the thread's next `park` call return immediately.
    pub fn unpark(&self) {
        self.0.parker.unpark();
    }
}

crate::thread_local! {
    /// This thread's handle, lazily created for threads we didn't spawn.
    static CURRENT: core::cell::RefCell<Option<Thread>> = core::cell::RefCell::new(None);
}

/// Returns a handle to the current thread.
pub fn current() -> Thread {
    if let Some(t) = CURRENT.with(|c| c.borrow().clone()) {
        return t;
    }
    // A thread we didn't spawn (main or foreign): mint a handle and remember it.
    let t = Thread::new(None);
    CURRENT.with(|c| *c.borrow_mut() = Some(t.clone()));
    t
}

/// Block the current thread until another thread calls `unpark` on its handle.
/// May wake spuriously, but never before at least one `unpark` (tokens are not
/// lost: an `unpark` before `park` makes the `park` return at once).
pub fn park() {
    current().0.parker.park();
}

/// Like [`park`] but returns after at most `dur` even without an `unpark`.
pub fn park_timeout(dur: Duration) {
    current().0.parker.park_timeout(dur);
}

/// Returns an estimate of the number of hardware threads available.
pub fn available_parallelism() -> crate::io::Result<core::num::NonZeroUsize> {
    let n = crate::syscall::num_cpus();
    core::num::NonZeroUsize::new(n)
        .ok_or_else(|| crate::io::Error::from(crate::io::ErrorKind::Other))
}

// ---------------------------------------------------------------------------
// Scoped threads
// ---------------------------------------------------------------------------

struct ScopeData {
    running: AtomicUsize, // count of scoped threads not yet finished
    wake: AtomicU32,      // futex word the parent waits on
}

/// A scope for spawning threads that may borrow from the enclosing stack.
/// Drop-in for `std::thread::Scope`.
pub struct Scope<'scope, 'env: 'scope> {
    data: ScopeData,
    _scope: PhantomData<&'scope mut &'scope ()>,
    _env: PhantomData<&'env mut &'env ()>,
}

/// Handle to a scoped thread. Drop-in for `std::thread::ScopedJoinHandle`.
pub struct ScopedJoinHandle<'scope, T> {
    handle: JoinHandle<()>,
    slot: *mut Option<T>,
    _p: PhantomData<&'scope ()>,
}
impl<'scope, T> ScopedJoinHandle<'scope, T> {
    pub fn join(self) -> Result<T> {
        // Move the inner handle out without running this handle's (absent) Drop.
        let handle = unsafe { core::ptr::read(&self.handle) };
        let slot = self.slot;
        core::mem::forget(self);
        handle.join()?;
        // SAFETY: the thread finished, so the slot is written and unaliased.
        let value = unsafe { (*slot).take() };
        unsafe { drop(Box::from_raw(slot)) };
        Ok(value.expect("scoped thread finished without a result"))
    }
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }
}

impl<'scope, 'env> Scope<'scope, 'env> {
    pub fn spawn<F, T>(&'scope self, f: F) -> ScopedJoinHandle<'scope, T>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        self.data.running.fetch_add(1, Ordering::Relaxed);
        let data = &self.data as *const ScopeData as usize;
        let slot: *mut Option<T> = Box::into_raw(Box::new(None));
        let slot_usize = slot as usize;

        // Returns `()`, so `T` never crosses the `'static` spawn boundary.
        let wrapped = move || {
            let r = f();
            // SAFETY: this thread owns the slot until it writes the result.
            unsafe { *(slot_usize as *mut Option<T>) = Some(r) };
            // SAFETY: `scope()` outlives all its threads, so `data` is valid.
            let data = unsafe { &*(data as *const ScopeData) };
            if data.running.fetch_sub(1, Ordering::AcqRel) == 1 {
                data.wake.store(1, Ordering::Release);
                sys_thread::futex_wake(&data.wake);
            }
        };

        // Erase the 'scope lifetime to 'static; sound because `scope()` joins
        // every spawned thread before returning (so before borrowed data dies).
        let boxed: Box<dyn FnOnce() + Send + 'scope> = Box::new(wrapped);
        let boxed: Box<dyn FnOnce() + Send + 'static> = unsafe { core::mem::transmute(boxed) };

        let handle = Builder::new()
            .spawn(move || boxed())
            .expect("failed to spawn scoped thread");
        ScopedJoinHandle {
            handle,
            slot,
            _p: PhantomData,
        }
    }
}

/// Create a scope for spawning scoped threads. Drop-in for `std::thread::scope`.
pub fn scope<'env, F, T>(f: F) -> T
where
    F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
{
    let scope = Scope {
        data: ScopeData {
            running: AtomicUsize::new(0),
            wake: AtomicU32::new(0),
        },
        _scope: PhantomData,
        _env: PhantomData,
    };
    let result = f(&scope);
    // Wait until every scoped thread has finished.
    while scope.data.running.load(Ordering::Acquire) != 0 {
        sys_thread::futex_wait(&scope.data.wake, 0);
        scope.data.wake.store(0, Ordering::Relaxed);
    }
    result
}
