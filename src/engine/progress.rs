//! Progress reporting: a sliding-window speed meter and the [`Progress`]
//! snapshot handed to frontends. The meter takes an explicit elapsed
//! [`Duration`] (rather than reading the clock) so it is deterministically
//! testable.

use std::collections::VecDeque;
use std::time::Duration;

use crate::engine::Segment;

/// A point-in-time view of a download, passed to the progress callback.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    /// Bytes written to disk so far.
    pub downloaded: u64,
    /// Total expected bytes, if the server sent `Content-Length`.
    pub total: Option<u64>,
    /// Current throughput estimate (bytes/sec) over the sliding window.
    pub bytes_per_sec: f64,
    /// Estimated time remaining, when both total and speed are known.
    pub eta: Option<Duration>,
    /// Connections open, including ones still connecting (they ramp up on a
    /// multi-segment download); 1 on the single-connection path.
    pub connections: u8,
    /// The byte ranges this attempt is fetching and how far each has got: the
    /// whole tiling of `[0, total)` (one range on a single connection; none
    /// while the size is unknown). Work-stealing shows as a range's `end`
    /// shrinking plus a new range appended.
    pub segments: Vec<Segment>,
    /// For each of `segments`, the lane fetching it now (numbered from 1, as
    /// the log names it), if any; lets a frontend draw one row per lane.
    pub lanes: Vec<Option<u8>>,
}

/// Sliding-window throughput estimator.
///
/// Records `(elapsed, cumulative_bytes)` samples and reports the average rate
/// over the most recent `window`. Averaging over a window (rather than since
/// the start) keeps the reading responsive to speed changes. The last two
/// samples are always kept, so samples further apart than the window (a slow
/// throttled stream) still give a reading.
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
        // Older than the window goes, but two samples always stay: after a
        // gap the previous one is all there is to measure from.
        while self.samples.len() > 2 && elapsed.saturating_sub(self.samples[0].0) > self.window {
            self.samples.pop_front();
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
    fn a_gap_longer_than_the_window_still_reads_the_rate() {
        let mut m = SpeedMeter::new();
        m.observe(Duration::from_secs(0), 0);
        // One chunk every 8 s (a 2 KB/s limit on 16 KB chunks): the previous
        // sample is the only one to measure from, so it must be kept.
        let bps = m.observe(Duration::from_secs(8), 16_000);
        assert!((bps - 2000.0).abs() < 1e-9, "got {bps}");
        let bps = m.observe(Duration::from_secs(16), 32_000);
        assert!((bps - 2000.0).abs() < 1e-9, "got {bps}");
    }

    #[test]
    fn the_reading_recovers_on_the_next_chunk_after_a_stall() {
        let mut m = SpeedMeter::new();
        for s in 0..=4 {
            m.observe(Duration::from_secs(s), s * 1_000_000);
        }
        // A 30 s stall, then two chunks a tenth of a second apart at 1 MB/s.
        m.observe(Duration::from_secs(34), 5_000_000);
        let bps = m.observe(Duration::from_millis(34_100), 5_100_000);
        assert!((bps - 1_000_000.0).abs() < 1e-3, "got {bps}");
    }

    #[test]
    fn eta_needs_total_and_positive_speed() {
        assert_eq!(eta(0, None, 100.0), None);
        assert_eq!(eta(0, Some(100), 0.0), None);
        assert_eq!(eta(100, Some(100), 100.0), None);
        assert_eq!(eta(0, Some(1000), 500.0), Some(Duration::from_secs(2)));
    }
}
