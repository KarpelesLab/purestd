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
use core::sync::atomic::{AtomicU32, Ordering};

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
        stack_base,
        stack_size,
    } = payload;

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

        let payload = Box::new(Payload {
            f,
            packet: packet.clone(),
            stack_base: base,
            stack_size,
        });
        let arg = Box::into_raw(payload) as *mut u8;

        let entry: sys_thread::ThreadEntry = thread_start::<F, T>;
        match unsafe { sys_thread::spawn_os(entry, arg, stack_top) } {
            Ok(()) => Ok(JoinHandle {
                packet,
                thread: Thread {
                    name: self.name,
                    id: ThreadId(0),
                },
            }),
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

/// An opaque thread identifier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ThreadId(u64);

/// A handle to a thread (minimal). Drop-in-ish for `std::thread::Thread`.
#[derive(Clone)]
pub struct Thread {
    name: Option<String>,
    id: ThreadId,
}

impl Thread {
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn id(&self) -> ThreadId {
        self.id
    }
}

/// Returns a handle to the current thread.
pub fn current() -> Thread {
    Thread {
        name: None,
        id: ThreadId(0),
    }
}

/// Returns an estimate of the number of hardware threads available.
pub fn available_parallelism() -> crate::io::Result<core::num::NonZeroUsize> {
    let n = crate::syscall::num_cpus();
    core::num::NonZeroUsize::new(n)
        .ok_or_else(|| crate::io::Error::from(crate::io::ErrorKind::Other))
}
