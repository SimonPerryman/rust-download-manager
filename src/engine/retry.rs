//! Exponential-backoff retry policy for transient download failures.
//!
//! The schedule is deterministic (no jitter) so it is unit-testable, and the
//! delay is capped. Resume and retry share one mechanism: each attempt
//! re-derives the offset from the `.part` size and issues a `Range` request, so
//! a mid-stream disconnect simply continues where it stopped.

use std::time::Duration;

/// How many times to retry and how long to wait between attempts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryPolicy {
    /// Total attempts including the first (so `1` means no retries).
    pub max_attempts: u32,
    /// Delay before the second attempt; subsequent delays scale by `factor`.
    pub base_delay: Duration,
    /// Multiplier applied to the delay after each attempt.
    pub factor: f64,
    /// Upper bound on any single delay.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_millis(500),
            factor: 2.0,
            max_delay: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// A policy that never waits and never retries — handy for tests.
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            base_delay: Duration::ZERO,
            factor: 2.0,
            max_delay: Duration::ZERO,
        }
    }

    /// Delay before the retry that follows `attempt` (1-based: the delay after
    /// the first attempt is `delay_for(1) == base_delay`), capped at
    /// `max_delay`.
    pub fn delay_for(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }
        let scaled = self.base_delay.as_secs_f64() * self.factor.powi((attempt - 1) as i32);
        let capped = scaled.min(self.max_delay.as_secs_f64());
        Duration::from_secs_f64(capped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_schedule_is_exact_and_capped() {
        let p = RetryPolicy::default();
        assert_eq!(p.delay_for(0), Duration::ZERO);
        assert_eq!(p.delay_for(1), Duration::from_millis(500));
        assert_eq!(p.delay_for(2), Duration::from_millis(1000));
        assert_eq!(p.delay_for(3), Duration::from_millis(2000));
        assert_eq!(p.delay_for(4), Duration::from_millis(4000));
        assert_eq!(p.delay_for(5), Duration::from_millis(8000));
        assert_eq!(p.delay_for(6), Duration::from_millis(16000));
        // 500ms * 2^6 = 32s would exceed the 30s cap.
        assert_eq!(p.delay_for(7), Duration::from_secs(30));
        assert_eq!(p.delay_for(100), Duration::from_secs(30));
    }

    #[test]
    fn none_policy_does_not_wait() {
        let p = RetryPolicy::none();
        assert_eq!(p.max_attempts, 1);
        assert_eq!(p.delay_for(1), Duration::ZERO);
    }
}
