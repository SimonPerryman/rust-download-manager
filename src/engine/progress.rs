//! Progress reporting: a sliding-window speed meter and the [`Progress`]
//! snapshot handed to frontends.
//!
//! The speed meter is written around an explicit elapsed [`Duration`] rather
//! than reading the clock itself, so it can be unit-tested deterministically.

use std::collections::VecDeque;
use std::time::Duration;

/// A point-in-time view of a download, passed to the progress callback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    /// Bytes written to disk so far.
    pub downloaded: u64,
    /// Total expected bytes, if the server sent `Content-Length`.
    pub total: Option<u64>,
    /// Current throughput estimate (bytes/sec) over the sliding window.
    pub bytes_per_sec: f64,
    /// Estimated time remaining, when both total and speed are known.
    pub eta: Option<Duration>,
}

/// Sliding-window throughput estimator.
///
/// Records `(elapsed, cumulative_bytes)` samples and reports the average rate
/// over the most recent `window`. Averaging over a window (rather than since
/// the start) keeps the reading responsive to speed changes.
pub struct SpeedMeter {
    window: Duration,
    samples: VecDeque<(Duration, u64)>,
}

impl SpeedMeter {
    /// Default meter with a 5-second window.
    pub fn new() -> Self {
        Self::with_window(Duration::from_secs(5))
    }

    /// Meter with an explicit averaging window.
    pub fn with_window(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
        }
    }

    /// Record a cumulative-bytes reading taken at `elapsed` since the download
    /// started, and return the current speed estimate in bytes/sec.
    pub fn observe(&mut self, elapsed: Duration, downloaded: u64) -> f64 {
        self.samples.push_back((elapsed, downloaded));
        // Drop samples older than the window, but keep at least one so we can
        // always measure an interval.
        while self.samples.len() > 1 {
            let (oldest, _) = self.samples[0];
            if elapsed.saturating_sub(oldest) > self.window {
                self.samples.pop_front();
            } else {
                break;
            }
        }
        self.speed()
    }

    fn speed(&self) -> f64 {
        let (first_t, first_b) = match self.samples.front() {
            Some(&s) => s,
            None => return 0.0,
        };
        let (last_t, last_b) = *self.samples.back().expect("front implies back");
        let secs = last_t.saturating_sub(first_t).as_secs_f64();
        if secs <= 0.0 {
            return 0.0;
        }
        last_b.saturating_sub(first_b) as f64 / secs
    }
}

impl Default for SpeedMeter {
    fn default() -> Self {
        Self::new()
    }
}

/// Estimate time remaining from a speed and the bytes still to fetch.
pub fn eta(downloaded: u64, total: Option<u64>, bytes_per_sec: f64) -> Option<Duration> {
    let total = total?;
    if bytes_per_sec <= 0.0 || downloaded >= total {
        return None;
    }
    let remaining = (total - downloaded) as f64;
    Some(Duration::from_secs_f64(remaining / bytes_per_sec))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_is_zero_with_a_single_sample() {
        let mut m = SpeedMeter::new();
        assert_eq!(m.observe(Duration::from_secs(0), 0), 0.0);
    }

    #[test]
    fn speed_averages_over_the_interval() {
        let mut m = SpeedMeter::new();
        m.observe(Duration::from_secs(0), 0);
        // 1000 bytes across 2 seconds => 500 B/s.
        let bps = m.observe(Duration::from_secs(2), 1000);
        assert!((bps - 500.0).abs() < 1e-9, "got {bps}");
    }

    #[test]
    fn old_samples_fall_out_of_the_window() {
        let mut m = SpeedMeter::with_window(Duration::from_secs(1));
        m.observe(Duration::from_secs(0), 0);
        m.observe(Duration::from_secs(10), 10_000);
        // Only the latest two samples remain and they are <= window apart,
        // so the rate reflects recent progress, not the whole history.
        let bps = m.observe(Duration::from_secs(11), 11_000);
        assert!((bps - 1000.0).abs() < 1e-9, "got {bps}");
    }

    #[test]
    fn eta_needs_total_and_positive_speed() {
        assert_eq!(eta(0, None, 100.0), None);
        assert_eq!(eta(0, Some(100), 0.0), None);
        assert_eq!(eta(100, Some(100), 100.0), None);
        assert_eq!(eta(0, Some(1000), 500.0), Some(Duration::from_secs(2)));
    }
}
