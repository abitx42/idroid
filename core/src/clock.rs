//! Clock boundary for lifecycle measurements.
//!
//! Production macOS adapters must implement [`Clock`] with Darwin's
//! `clock_gettime(CLOCK_MONOTONIC, ...)` (or an equivalent sleep-inclusive
//! source). `CLOCK_UPTIME_RAW`, `mach_absolute_time()`, and Rust's usual
//! `Instant` implementation are not suitable for sleep/wake reconnect metrics
//! on macOS because their uptime clock excludes system sleep.

use crate::session::Timestamp;

pub trait Clock {
    fn now(&self) -> Timestamp;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManualClock {
    now: Timestamp,
}

impl ManualClock {
    pub const fn new(now: Timestamp) -> Self {
        Self { now }
    }

    pub const fn now(&self) -> Timestamp {
        self.now
    }

    pub fn set(&mut self, now: Timestamp) {
        self.now = now;
    }

    pub fn advance(&mut self, milliseconds: u64) {
        self.now = Timestamp::from_millis(self.now.as_millis().saturating_add(milliseconds));
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        self.now()
    }
}
