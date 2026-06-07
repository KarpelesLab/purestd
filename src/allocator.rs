//! Global allocator: an mmap-backed segregated free-list.
//!
//! Small requests (≤ 4 KiB) are rounded to a power-of-two size class and served
//! from a per-class intrusive free list, refilled from 1 MiB `mmap` arenas.
//! Larger requests get their own page-rounded `mmap`, released with `munmap` on
//! free (the size returns to us in the `Layout`, so no header is needed). A
//! spinlock makes it `Sync`. Small blocks are never returned to the OS — they
//! stay on the free list for reuse.
//!
//! The [`Allocator`] type is always available; the `#[global_allocator]` static
//! is registered only under the `rt` feature, so a host runtime can supply its
//! own.

#[cfg(not(target_family = "wasm"))]
use crate::arch::{PROT_READ, PROT_WRITE};
use crate::syscall;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

/// Granularity for arenas and large allocations. A multiple of every supported
/// target's page size (4 KiB on Linux, 16 KiB on macOS/arm64), so the same
/// rounded length is valid for both `mmap` and the matching `munmap`.
const GRAN: usize = 16384;
/// Minimum guaranteed `mmap` alignment across targets (Linux page size). Arena
/// bases are at least this aligned, which bounds the largest small class.
const MIN_MAP_ALIGN: usize = 4096;
const MIN_SHIFT: usize = 4; // 16-byte smallest class
const MAX_SMALL: usize = MIN_MAP_ALIGN; // 4096
const NUM_CLASSES: usize = 12 - MIN_SHIFT + 1; // classes 2^4..2^12 → 9
const ARENA: usize = 1 << 20; // 1 MiB

#[inline]
fn round_up(v: usize, to: usize) -> usize {
    (v + to - 1) & !(to - 1)
}

#[cfg(not(target_family = "wasm"))]
#[inline]
fn map(len: usize) -> *mut u8 {
    match syscall::mmap_anon(len, PROT_READ | PROT_WRITE) {
        Ok(p) => p,
        Err(_) => ptr::null_mut(),
    }
}

/// wasm has no `mmap`; grow the single linear memory and hand back the new
/// region. Freed large blocks are never returned (linear memory can't shrink).
#[cfg(target_family = "wasm")]
#[inline]
fn map(len: usize) -> *mut u8 {
    const PAGE: usize = 65536;
    let pages = (len + PAGE - 1) / PAGE;
    let prev = core::arch::wasm32::memory_grow(0, pages);
    if prev == usize::MAX {
        ptr::null_mut()
    } else {
        (prev * PAGE) as *mut u8
    }
}

#[inline]
fn class_for(size: usize, align: usize) -> Option<usize> {
    let need = size.max(align).max(1 << MIN_SHIFT);
    if need > MAX_SMALL {
        return None;
    }
    let mut shift = MIN_SHIFT;
    while (1usize << shift) < need {
        shift += 1;
    }
    Some(shift - MIN_SHIFT)
}

#[inline]
fn class_size(idx: usize) -> usize {
    1 << (idx + MIN_SHIFT)
}

/// An mmap-backed segregated free-list allocator.
pub struct Allocator {
    lock: AtomicBool,
    free: UnsafeCell<[*mut u8; NUM_CLASSES]>,
}

// Safe: every access to `free` is guarded by the spinlock.
unsafe impl Sync for Allocator {}

impl Allocator {
    pub const fn new() -> Self {
        Allocator {
            lock: AtomicBool::new(false),
            free: UnsafeCell::new([ptr::null_mut(); NUM_CLASSES]),
        }
    }

    #[inline]
    fn lock(&self) {
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    #[inline]
    fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }

    /// Carve a fresh arena into `idx`-class blocks. Caller holds the lock.
    unsafe fn refill(&self, idx: usize) -> bool {
        let block = class_size(idx);
        let base = map(ARENA);
        if base.is_null() {
            return false;
        }
        let count = ARENA / block;
        let heads = &mut *self.free.get();
        let mut next = heads[idx];
        let mut i = count;
        while i > 0 {
            i -= 1;
            let blk = base.add(i * block);
            ptr::write(blk as *mut *mut u8, next);
            next = blk;
        }
        heads[idx] = next;
        true
    }

    unsafe fn alloc_small(&self, idx: usize) -> *mut u8 {
        self.lock();
        let heads = &mut *self.free.get();
        if heads[idx].is_null() && !self.refill(idx) {
            self.unlock();
            return ptr::null_mut();
        }
        let heads = &mut *self.free.get();
        let blk = heads[idx];
        heads[idx] = ptr::read(blk as *const *mut u8);
        self.unlock();
        blk
    }

    unsafe fn free_small(&self, idx: usize, ptr: *mut u8) {
        self.lock();
        let heads = &mut *self.free.get();
        ptr::write(ptr as *mut *mut u8, heads[idx]);
        heads[idx] = ptr;
        self.unlock();
    }
}

impl Default for Allocator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match class_for(layout.size(), layout.align()) {
            Some(idx) => self.alloc_small(idx),
            None => {
                if layout.align() > GRAN {
                    return ptr::null_mut();
                }
                map(round_up(layout.size(), GRAN))
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        match class_for(layout.size(), layout.align()) {
            Some(idx) => self.free_small(idx, ptr),
            None => {
                let _ = syscall::munmap(ptr, round_up(layout.size(), GRAN));
            }
        }
    }
}

#[cfg(feature = "rt")]
#[global_allocator]
static GLOBAL: Allocator = Allocator::new();
