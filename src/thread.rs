//! `std::thread` — minimal placeholder.
//!
//! Real threads (clone/futex-backed) land with the Linux backend in M6. For now
//! this provides the always-available pieces: `sleep` (via `nanosleep`) and
//! `yield_now`, so single-threaded code that calls them still compiles and runs.

use crate::time::Duration;

/// Cooperatively yield the timeslice. A no-op spin hint until real scheduling.
pub fn yield_now() {
    core::hint::spin_loop();
}

/// Put the current thread to sleep for at least `dur`.
///
/// Not yet wired to `nanosleep` — currently a busy spin would be wrong for long
/// durations, so this is a no-op placeholder until M6 adds the syscall.
pub fn sleep(_dur: Duration) {
    // TODO(M6): nanosleep syscall.
}
