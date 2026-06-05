//! `std::time` subset: `Duration` (from `core`), plus syscall-backed `Instant`
//! and `SystemTime`.
//!
//! On macOS both clocks read `gettimeofday` (wall clock), so `Instant` is not
//! strictly monotonic there. On Linux (M6) `Instant` will use
//! `clock_gettime(CLOCK_MONOTONIC)` for true monotonicity.

pub use core::time::Duration;

use crate::syscall;
use core::fmt;

const UNIX_EPOCH_INNER: Duration = Duration::ZERO;

fn now_since_epoch() -> Duration {
    match syscall::gettimeofday() {
        Ok((secs, usecs)) => Duration::new(secs, (usecs as u32) * 1000),
        Err(_) => Duration::ZERO,
    }
}

/// A measurement of a monotonically nondecreasing clock. Drop-in for
/// `std::time::Instant`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(Duration);

impl Instant {
    pub fn now() -> Instant {
        Instant(now_since_epoch())
    }
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        self.0.checked_sub(earlier.0).unwrap_or(Duration::ZERO)
    }
    pub fn elapsed(&self) -> Duration {
        Instant::now().duration_since(*self)
    }
    pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
        self.0.checked_sub(earlier.0)
    }
}

/// A measurement of the system clock. Drop-in for `std::time::SystemTime`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SystemTime(Duration);

/// The Unix epoch — `1970-01-01 00:00:00 UTC`.
pub const UNIX_EPOCH: SystemTime = SystemTime(UNIX_EPOCH_INNER);

impl SystemTime {
    pub const UNIX_EPOCH: SystemTime = UNIX_EPOCH;

    pub fn now() -> SystemTime {
        SystemTime(now_since_epoch())
    }
    pub fn duration_since(&self, earlier: SystemTime) -> Result<Duration, SystemTimeError> {
        self.0
            .checked_sub(earlier.0)
            .ok_or_else(|| SystemTimeError(earlier.0 - self.0))
    }
    pub fn elapsed(&self) -> Result<Duration, SystemTimeError> {
        SystemTime::now().duration_since(*self)
    }
}

/// Error returned by `SystemTime::duration_since` when the earlier time is
/// actually later. Drop-in for `std::time::SystemTimeError`.
#[derive(Clone, Copy, Debug)]
pub struct SystemTimeError(Duration);

impl SystemTimeError {
    pub fn duration(&self) -> Duration {
        self.0
    }
}

impl fmt::Display for SystemTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "second time provided was later than self")
    }
}

impl core::error::Error for SystemTimeError {}
