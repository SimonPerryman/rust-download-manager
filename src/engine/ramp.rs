//! Throughput-driven connection ramping: a multi-segment download starts on
//! one connection and adds one per measurement window while the aggregate
//! speed keeps beating its best so far, then settles.
//!
//! [`Ramp`] is the pure decision; the segment coordinator measures each
//! window and acts on the [`Decision`].

use std::time::Duration;

/// How long each ramp measurement window lasts.
pub(crate) const RAMP_WINDOW: Duration = Duration::from_secs(1);

/// How much faster than the best window so far a window must be to count as
/// an improvement worth another connection.
const MARGIN: f64 = 0.10;

/// A window that spent at least this share of its connection-time waiting on
/// rate-limit tokens says nothing about the server, so it is not judged.
const FREEZE_FRACTION: f64 = 0.5;

/// How a multi-segment download grows its connection pool (a test seam: the
/// CLI and GUI always use the default window).
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ramping {
    /// Start on one connection and judge the speed every window.
    Window(Duration),
    /// Open every connection up front.
    Off,
}

impl Default for Ramping {
    fn default() -> Self {
        Self::Window(RAMP_WINDOW)
    }
}

/// What one measurement window looked like.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Window {
    /// Aggregate bytes/s written during the window.
    pub bytes_per_sec: f64,
    /// Share of the window's connection-time spent waiting on rate limits.
    pub waited_fraction: f64,
    /// Whether any worker started or exited during the window.
    pub pool_changed: bool,
}

/// What the coordinator should do after a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Open one more connection.
    Add,
    /// Close one connection (the trial that didn't pay off).
    Retire,
    /// Keep the pool as it is.
    Hold,
}

/// The ramp state for one segmented attempt.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ramp {
    ceiling: u8,
    target: u8,
    best: f64,
    settled: bool,
}

impl Ramp {
    /// Start at one connection, ramping up to `ceiling`.
    pub fn new(ceiling: u8) -> Self {
        let ceiling = ceiling.max(1);
        Self {
            ceiling,
            target: 1,
            best: 0.0,
            settled: ceiling <= 1,
        }
    }

    /// Every connection up front, never ramping.
    pub fn fixed(ceiling: u8) -> Self {
        let ceiling = ceiling.max(1);
        Self {
            ceiling,
            target: ceiling,
            best: 0.0,
            settled: true,
        }
    }

    /// Judge one window and say how the pool should change.
    pub fn observe(&mut self, w: Window) -> Decision {
        // A pool change means some connection spent part of the window on
        // setup (incl. the first window's request latency and the limiter's
        // free burst): under-measuring it would make the next step over-ramp.
        if self.settled
            || w.pool_changed
            || w.waited_fraction >= FREEZE_FRACTION
            || w.bytes_per_sec <= 0.0
        {
            return Decision::Hold;
        }
        if self.best == 0.0 || w.bytes_per_sec > self.best * (1.0 + MARGIN) {
            self.best = w.bytes_per_sec;
            if self.target < self.ceiling {
                self.target += 1;
                return Decision::Add;
            }
            self.settled = true;
            return Decision::Hold;
        }
        self.settled = true;
        if self.target > 1 {
            self.target -= 1;
            Decision::Retire
        } else {
            Decision::Hold
        }
    }

    /// The host refused more than `n` connections (429/503): stay at `n`.
    /// With none live, a refusal says nothing about the host's cap.
    pub fn settle_at(&mut self, n: u8) {
        if !self.settled && n > 0 && n < self.target {
            self.target = n;
            self.settled = true;
        }
    }

    /// How many connections the pool should have.
    pub fn target(&self) -> u8 {
        self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn judged(bytes_per_sec: f64) -> Window {
        Window {
            bytes_per_sec,
            waited_fraction: 0.0,
            pool_changed: false,
        }
    }

    fn skipped() -> Window {
        Window {
            pool_changed: true,
            ..judged(1000.0)
        }
    }

    #[test]
    fn new_starts_at_one_and_fixed_at_the_ceiling() {
        assert_eq!(Ramp::new(4).target(), 1);
        assert_eq!(Ramp::fixed(4).target(), 4);
        assert_eq!(Ramp::new(0).target(), 1, "ceiling is at least 1");
        assert_eq!(Ramp::fixed(0).target(), 1);
    }

    #[test]
    fn single_connection_ceiling_never_ramps() {
        let mut r = Ramp::new(1);
        assert_eq!(r.observe(judged(1000.0)), Decision::Hold);
        assert_eq!(r.target(), 1);
    }

    #[test]
    fn fixed_never_changes() {
        let mut r = Ramp::fixed(4);
        assert_eq!(r.observe(judged(1000.0)), Decision::Hold);
        assert_eq!(r.observe(judged(1.0)), Decision::Hold);
        r.settle_at(2);
        assert_eq!(r.target(), 4, "settle_at is a no-op once settled");
    }

    #[test]
    fn first_judged_window_adds() {
        let mut r = Ramp::new(4);
        assert_eq!(r.observe(judged(100.0)), Decision::Add);
        assert_eq!(r.target(), 2);
    }

    #[test]
    fn pool_change_skips_the_window() {
        let mut r = Ramp::new(4);
        assert_eq!(r.observe(skipped()), Decision::Hold);
        assert_eq!(r.target(), 1);
        // Not counted as a judged window: the next one is still the first.
        assert_eq!(r.observe(judged(100.0)), Decision::Add);
    }

    #[test]
    fn waiting_on_a_limit_freezes_the_ramp() {
        let mut r = Ramp::new(4);
        let throttled = |f| Window {
            waited_fraction: f,
            ..judged(100.0)
        };
        assert_eq!(r.observe(throttled(0.5)), Decision::Hold);
        assert_eq!(r.observe(throttled(0.9)), Decision::Hold);
        assert_eq!(r.target(), 1);
        // Just under the freeze threshold is judged.
        assert_eq!(r.observe(throttled(0.49)), Decision::Add);
        // Frozen windows leave `best` alone: 105 isn't > 100 × 1.1 ...
        assert_eq!(r.observe(throttled(0.8)), Decision::Hold);
        let mut probe = r.clone();
        assert_eq!(probe.observe(judged(105.0)), Decision::Retire);
        // ... while 111 is.
        assert_eq!(r.observe(judged(111.0)), Decision::Add);
    }

    #[test]
    fn stalled_window_is_no_evidence() {
        let mut r = Ramp::new(4);
        assert_eq!(r.observe(judged(0.0)), Decision::Hold);
        assert_eq!(r.observe(judged(100.0)), Decision::Add);
        assert_eq!(r.observe(judged(0.0)), Decision::Hold);
        assert_eq!(r.target(), 2, "a stall neither adds nor retires");
    }

    #[test]
    fn improvement_must_beat_the_margin() {
        let mut r = Ramp::new(8);
        assert_eq!(r.observe(judged(100.0)), Decision::Add);
        // Exactly 10% better is not more than the margin.
        let mut edge = r.clone();
        assert_eq!(edge.observe(judged(110.0)), Decision::Retire);
        assert_eq!(edge.target(), 1);
        assert_eq!(r.observe(judged(110.5)), Decision::Add);
        assert_eq!(r.target(), 3);
    }

    #[test]
    fn no_improvement_retires_the_trial_and_settles() {
        let mut r = Ramp::new(8);
        r.observe(judged(100.0));
        assert_eq!(r.observe(judged(100.0)), Decision::Retire);
        assert_eq!(r.target(), 1);
        // Settled: even a big jump changes nothing.
        assert_eq!(r.observe(judged(1000.0)), Decision::Hold);
        assert_eq!(r.target(), 1);
    }

    #[test]
    fn hitting_the_ceiling_settles_without_retiring() {
        let mut r = Ramp::new(2);
        assert_eq!(r.observe(judged(100.0)), Decision::Add);
        assert_eq!(r.observe(judged(200.0)), Decision::Hold);
        assert_eq!(r.target(), 2);
        assert_eq!(r.observe(judged(50.0)), Decision::Hold, "settled");
        assert_eq!(r.target(), 2);
    }

    #[test]
    fn settle_at_caps_the_target_once() {
        let mut r = Ramp::new(8);
        r.observe(judged(100.0));
        r.observe(judged(200.0));
        assert_eq!(r.target(), 3);
        r.settle_at(5);
        assert_eq!(r.target(), 3, "above the target: ignored");
        r.settle_at(2);
        assert_eq!(r.target(), 2);
        assert_eq!(r.observe(judged(1000.0)), Decision::Hold, "settled");
        r.settle_at(1);
        assert_eq!(r.target(), 2, "already settled");

        let mut none_live = Ramp::new(4);
        none_live.observe(judged(100.0));
        none_live.settle_at(0);
        assert_eq!(none_live.target(), 2, "a refusal with none live is no cap");
        assert_eq!(
            none_live.observe(judged(300.0)),
            Decision::Add,
            "still ramping"
        );
    }

    /// A host whose aggregate caps at 4R: 1 → 5 then back to 4, with each
    /// step's setup window skipped.
    #[test]
    fn scripted_ramp_settles_at_the_aggregate_cap() {
        let r_conn = 256.0;
        let mut r = Ramp::new(8);
        let mut decisions = Vec::new();
        for live in 1..=5u8 {
            decisions.push(r.observe(skipped()));
            let speed = r_conn * f64::from(live.min(4));
            decisions.push(r.observe(judged(speed)));
        }
        use Decision::*;
        assert_eq!(
            decisions,
            [Hold, Add, Hold, Add, Hold, Add, Hold, Add, Hold, Retire]
        );
        assert_eq!(r.target(), 4);
    }
}
