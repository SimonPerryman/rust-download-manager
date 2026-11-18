//! The injected clock: the only way the schedule learns the time.

use super::time::LocalDateTime;
use crate::engine::lock::lock;
use std::sync::Mutex;

/// A source of the machine's local wall-clock time. Injected so tests run on a
/// fake and never wait for real.
pub trait Clock: Send + Sync {
    /// The current local time, to the minute.
    fn now(&self) -> LocalDateTime;
}

/// A clock tests move by hand.
#[derive(Debug)]
pub struct FakeClock(Mutex<LocalDateTime>);

impl FakeClock {
    /// A clock standing at `at`.
    pub fn new(at: LocalDateTime) -> Self {
        Self(Mutex::new(at))
    }

    /// Jump to `at`.
    pub fn set(&self, at: LocalDateTime) {
        *lock(&self.0) = at;
    }

    /// Move the wall clock by `minutes`. A real minute is `advance(1)`; a
    /// clock change is an extra `advance(60)` (spring forward) or
    /// `advance(-60)` (fall back).
    pub fn advance(&self, minutes: i64) {
        let mut now = lock(&self.0);
        *now = now.add_minutes(minutes);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> LocalDateTime {
        *lock(&self.0)
    }
}
