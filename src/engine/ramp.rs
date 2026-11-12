//! Throughput-driven connection ramping: a multi-segment download starts on
//! one connection and adds one per measurement window while the aggregate
//! speed keeps beating its best so far, then settles. A settled download
//! re-probes: it trials one more connection when its speed limit is raised
//! and every [`REPROBE_INTERVAL`] while below the ceiling. It sheds one when
//! its speed stays well below its best.
//!
//! [`Ramp`] is the pure decision; the segment coordinator measures each
//! window and acts on the [`Decision`].

use std::time::Duration;

/// How long each ramp measurement window lasts.
pub(crate) const RAMP_WINDOW: Duration = Duration::from_secs(1);

/// How long a ramp stays settled below its ceiling before trialling one more
/// connection, in case the host or the network got faster.
pub(crate) const REPROBE_INTERVAL: Duration = Duration::from_secs(30);

/// How much faster than the best window so far a window must be to count as
/// an improvement worth another connection.
const MARGIN: f64 = 0.10;

/// How far each judged settled window moves the smoothed settled speed.
const LEVEL_WEIGHT: f64 = 0.25;

/// A settled window slower than this share of the best is a slow one ...
const SLOW_FRACTION: f64 = 0.70;

/// ... and this many judged ones in a row shed a connection.
const SHED_AFTER: u8 = 3;

/// A slow window that spent more than this share of its connection-time
/// waiting on the speed limit was slowed by the limit, not the host.
const LIMITED_FRACTION: f64 = 0.05;

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
    /// Whether fewer workers ran than the pool wants (the work is running
    /// out), so the window doesn't measure the target.
    pub short: bool,
}

/// What the coordinator should do after a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Open one more connection.
    Add,
    /// Close one connection (the trial that didn't pay off).
    Retire,
    /// Close one connection (the download slowed down).
    Shed,
    /// Keep the pool as it is.
    Hold,
}

/// The ramp state for one segmented attempt.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ramp {
    ceiling: u8,
    /// The most connections the host has accepted before refusing one
    /// (429/503); no re-probe goes past it.
    cap: Option<u8>,
    target: u8,
    best: f64,
    /// The settled speed, smoothed over judged windows so one dip can't let
    /// a useless trial look like a gain: what a re-probe's trial must beat.
    level: f64,
    /// Judged settled windows in a row below [`SLOW_FRACTION`] of the best.
    slow: u8,
    settled: bool,
    /// Whether the steps since the last settle began with a re-probe.
    trial: bool,
    /// Windows since the ramp last settled.
    settled_for: u32,
    /// Settled windows before a re-probe.
    reprobe_after: u32,
    /// A raised limit asked for a re-probe once the current lanes' speed
    /// has been measured afresh.
    reprobe_now: bool,
}

impl Ramp {
    /// Start at one connection, ramping up to `ceiling`; re-probes only when
    /// asked ([`Ramp::reprobe`]) until [`Ramp::reprobing_every`] sets a pace.
    pub fn new(ceiling: u8) -> Self {
        let ceiling = ceiling.max(1);
        Self {
            ceiling,
            cap: None,
            target: 1,
            best: 0.0,
            level: 0.0,
            slow: 0,
            settled: ceiling <= 1,
            trial: false,
            settled_for: 0,
            reprobe_after: u32::MAX,
            reprobe_now: false,
        }
    }

    /// Every connection up front, never ramping.
    pub fn fixed(ceiling: u8) -> Self {
        Self {
            target: ceiling.max(1),
            settled: true,
            ..Self::new(ceiling)
        }
    }

    /// Re-probe once settled for `interval`, measured in `window`s.
    pub fn reprobing_every(self, interval: Duration, window: Duration) -> Self {
        let windows = interval.as_secs_f64() / window.as_secs_f64().max(f64::MIN_POSITIVE);
        Self {
            reprobe_after: windows.ceil().clamp(1.0, f64::from(u32::MAX)) as u32,
            ..self
        }
    }

    /// Judge one window and say how the pool should change.
    pub fn observe(&mut self, w: Window) -> Decision {
        if self.settled {
            self.settled_for = self.settled_for.saturating_add(1);
        }
        // A pool change means some connection spent part of the window on
        // setup (incl. the first window's request latency): under-measuring
        // it would make the next step over-ramp.
        if w.pool_changed || w.short || w.bytes_per_sec <= 0.0 {
            return Decision::Hold;
        }
        if w.waited_fraction >= FREEZE_FRACTION {
            // A trial held back by the limit can't pay off.
            return if self.trial && !self.settled {
                self.retire()
            } else {
                Decision::Hold
            };
        }
        if self.settled {
            self.level = if self.level == 0.0 {
                w.bytes_per_sec
            } else {
                self.level + LEVEL_WEIGHT * (w.bytes_per_sec - self.level)
            };
            if w.bytes_per_sec < self.best * SLOW_FRACTION {
                if w.waited_fraction > LIMITED_FRACTION {
                    // Slowed by rdm's own limit: no evidence about the host.
                    return Decision::Hold;
                }
                self.slow = self.slow.saturating_add(1);
                // A host cap can hold the pool below the target.
                let lanes = self.target.min(self.most());
                if self.slow < SHED_AFTER || lanes <= 1 {
                    return Decision::Hold;
                }
                // Today's speed becomes the best, so a later re-probe judges
                // the connection against it and can win it back.
                self.target = lanes - 1;
                self.best = w.bytes_per_sec;
                self.settle();
                return Decision::Shed;
            }
            self.slow = 0;
            // The smoothed speed, so one fast window can't raise the bar a
            // slowdown is measured against.
            self.best = self.best.max(self.level);
            // A raise's re-probe is spent here even at the ceiling, so it
            // can't fire long after, the moment a lane closes.
            let due =
                std::mem::take(&mut self.reprobe_now) || self.settled_for >= self.reprobe_after;
            return if due && self.reprobe() {
                Decision::Add
            } else {
                Decision::Hold
            };
        }
        if self.best == 0.0 || w.bytes_per_sec > self.best * (1.0 + MARGIN) {
            self.best = w.bytes_per_sec;
            if self.target < self.most() {
                self.target += 1;
                return Decision::Add;
            }
            self.settle();
            return Decision::Hold;
        }
        self.retire()
    }

    /// Give up the last step and settle on the speed before it.
    fn retire(&mut self) -> Decision {
        self.settle();
        if self.target > 1 {
            self.target -= 1;
            Decision::Retire
        } else {
            Decision::Hold
        }
    }

    /// Leave settled to trial one more connection, judged like a ramp-up
    /// step against the settled speed. False while still ramping, or already
    /// at the ceiling or the host's cap.
    pub fn reprobe(&mut self) -> bool {
        if !self.settled || self.target >= self.most() {
            return false;
        }
        self.settled = false;
        self.trial = true;
        self.reprobe_now = false;
        self.best = self.level;
        self.target += 1;
        true
    }

    /// The speed limit was raised or removed: forget the speed measured
    /// under it, and re-probe on the next judged window, which measures the
    /// current lanes under the new limit. A step in progress was judged
    /// against the old limit, so it's given up first.
    pub fn on_limit_raised(&mut self) -> Decision {
        let decision = if self.settled {
            Decision::Hold
        } else {
            self.retire()
        };
        self.level = 0.0;
        self.reprobe_now = true;
        decision
    }

    /// The host refused more than `n` connections (429/503): stay at `n`, and
    /// never re-probe past it. With none live, a refusal says nothing about
    /// the host's cap.
    pub fn settle_at(&mut self, n: u8) {
        if n == 0 {
            return;
        }
        self.cap = Some(self.cap.map_or(n, |c| c.min(n)));
        if n < self.target {
            if !self.settled {
                self.target = n;
                self.settle();
            }
            // Speeds measured on more connections than the host now allows
            // are no baseline: the next judged window is.
            self.best = 0.0;
            self.level = 0.0;
        }
    }

    /// Settle at the target, whose speed is the best so far.
    fn settle(&mut self) {
        self.settled = true;
        self.trial = false;
        self.settled_for = 0;
        self.level = self.best;
        self.reprobe_now = false;
        self.slow = 0;
    }

    /// The most connections the ramp may ask for.
    fn most(&self) -> u8 {
        self.cap.map_or(self.ceiling, |c| c.min(self.ceiling))
    }

    /// How many connections the pool should have.
    pub fn target(&self) -> u8 {
        self.target
    }
}

/// Whether a speed limit went from `before` to a higher one, or was removed
/// (`None` is unlimited): the connections it held back may help again.
pub(crate) fn limit_raised(before: Option<u64>, after: Option<u64>) -> bool {
    match (before, after) {
        (Some(_), None) => true,
        (Some(before), Some(after)) => after > before,
        (None, _) => false,
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
            short: false,
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

    /// Ramp 1 → 2, retire the third: settled at 2 at a speed of 200.
    fn settled_at_two(ceiling: u8) -> Ramp {
        let mut r = Ramp::new(ceiling);
        r.observe(judged(100.0));
        r.observe(judged(200.0));
        assert_eq!(r.observe(judged(200.0)), Decision::Retire);
        assert_eq!(r.target(), 2);
        r
    }

    #[test]
    fn reprobe_trials_one_more_judged_like_a_ramp_step() {
        let mut r = settled_at_two(8);
        assert!(r.reprobe());
        assert_eq!(r.target(), 3);
        assert!(!r.reprobe(), "already trialling");
        assert_eq!(r.observe(skipped()), Decision::Hold, "setup window");
        // Judged against the settled speed (200): a gain
        // keeps the trial and ramps on, as a ramp-up step would.
        assert_eq!(r.observe(judged(300.0)), Decision::Add);
        assert_eq!(r.target(), 4);
        assert_eq!(r.observe(judged(310.0)), Decision::Retire);
        assert_eq!(r.target(), 3);

        // No gain retires the trial and settles again.
        let mut r = settled_at_two(8);
        assert!(r.reprobe());
        assert_eq!(r.observe(judged(210.0)), Decision::Retire);
        assert_eq!(r.target(), 2);
        assert_eq!(r.observe(judged(1000.0)), Decision::Hold, "settled");
    }

    #[test]
    fn reprobe_stays_below_the_ceiling_and_the_host_cap() {
        let mut r = Ramp::new(2);
        r.observe(judged(100.0));
        r.observe(judged(200.0));
        assert_eq!(r.target(), 2);
        assert!(!r.reprobe(), "at the ceiling");
        assert!(!Ramp::fixed(4).reprobe());
        assert!(!Ramp::new(4).reprobe(), "still ramping");

        let mut r = settled_at_two(8);
        r.settle_at(2);
        assert!(!r.reprobe(), "capped at 2");
        // A refusal while settled still caps later re-probes, never raises
        // the cap, and with none live says nothing.
        let mut r = settled_at_two(8);
        assert!(r.reprobe());
        r.settle_at(2);
        assert_eq!(r.target(), 2, "the trial was refused");
        r.settle_at(5);
        r.settle_at(0);
        assert!(!r.reprobe());
    }

    #[test]
    fn a_trial_up_to_the_cap_settles_there() {
        let mut r = settled_at_two(8);
        r.settle_at(3);
        assert!(r.reprobe());
        assert_eq!(r.observe(judged(300.0)), Decision::Hold, "at the cap");
        assert_eq!(r.target(), 3);
        assert!(!r.reprobe());
    }

    #[test]
    fn settled_ramp_reprobes_on_the_first_judged_window_after_the_interval() {
        let window = Duration::from_millis(500);
        let mut r = Ramp::new(8).reprobing_every(Duration::from_secs(2), window);
        r.observe(judged(100.0));
        r.observe(judged(200.0));
        assert_eq!(r.observe(judged(200.0)), Decision::Retire);
        // Four windows after the retire it's due ...
        assert_eq!(r.observe(skipped()), Decision::Hold);
        assert_eq!(r.observe(judged(200.0)), Decision::Hold);
        assert_eq!(r.observe(judged(200.0)), Decision::Hold);
        // ... but a frozen window says nothing ...
        let frozen = Window {
            waited_fraction: 0.9,
            ..judged(200.0)
        };
        assert_eq!(r.observe(frozen), Decision::Hold);
        assert_eq!(r.target(), 2);
        // ... so the next judged one trials.
        assert_eq!(r.observe(judged(200.0)), Decision::Add);
        assert_eq!(r.target(), 3);
        // A failed trial waits the whole interval again.
        assert_eq!(r.observe(judged(200.0)), Decision::Retire);
        for _ in 0..3 {
            assert_eq!(r.observe(judged(200.0)), Decision::Hold);
        }
        assert_eq!(r.observe(judged(200.0)), Decision::Add);
    }

    #[test]
    fn reprobe_interval_is_whole_windows_at_least_one() {
        let ms = Duration::from_millis;
        let after = |interval, window| Ramp::new(4).reprobing_every(interval, window).reprobe_after;
        assert_eq!(after(REPROBE_INTERVAL, RAMP_WINDOW), 30);
        assert_eq!(after(ms(1200), ms(500)), 3, "rounded up");
        assert_eq!(after(Duration::ZERO, ms(500)), 1);
        assert_eq!(Ramp::new(4).reprobe_after, u32::MAX, "never by default");
    }

    #[test]
    fn one_dip_before_a_reprobe_doesnt_let_a_useless_trial_win() {
        let mut r = settled_at_two(8);
        assert_eq!(r.observe(judged(160.0)), Decision::Hold);
        assert!(r.reprobe());
        // Back to normal isn't 10% better than the smoothed 190.
        assert_eq!(r.observe(judged(200.0)), Decision::Retire);
        assert_eq!(r.target(), 2);
    }

    #[test]
    fn a_raised_limit_trials_against_a_fresh_window() {
        // Settled at 2 under a limit that held them to 200.
        let mut r = settled_at_two(8);
        r.on_limit_raised();
        // Two lanes alone now run at 500: that's the baseline ...
        assert_eq!(r.observe(judged(500.0)), Decision::Add);
        assert_eq!(r.target(), 3);
        // ... so a trial that adds nothing retires.
        assert_eq!(r.observe(skipped()), Decision::Hold);
        assert_eq!(r.observe(judged(500.0)), Decision::Retire);
        assert_eq!(r.target(), 2);
    }

    #[test]
    fn a_raise_mid_step_gives_the_step_up_and_retries_it_fresh() {
        // Settled at 2 at 200, trialling a third lane under the old limit.
        let mut r = settled_at_two(8);
        assert!(r.reprobe());
        assert_eq!(r.on_limit_raised(), Decision::Retire);
        assert_eq!(r.target(), 2);
        // Two lanes now run at 500: a third that adds nothing retires,
        // rather than beating the old limit's 200.
        assert_eq!(r.observe(skipped()), Decision::Hold);
        assert_eq!(r.observe(judged(500.0)), Decision::Add);
        assert_eq!(r.observe(skipped()), Decision::Hold);
        assert_eq!(r.observe(judged(500.0)), Decision::Retire);
        assert_eq!(r.target(), 2);

        // On its first lane, a ramp-up just carries on.
        let mut ramping = Ramp::new(4);
        assert_eq!(ramping.on_limit_raised(), Decision::Hold);
        assert_eq!(ramping.observe(judged(100.0)), Decision::Add);
        assert_eq!(ramping.target(), 2);
    }

    #[test]
    fn a_raise_at_the_ceiling_doesnt_reprobe_later() {
        let mut r = Ramp::new(2);
        r.observe(judged(100.0));
        assert_eq!(r.observe(judged(200.0)), Decision::Hold, "settled at 2");
        assert_eq!(r.on_limit_raised(), Decision::Hold);
        assert_eq!(r.observe(judged(500.0)), Decision::Hold, "at the ceiling");
        // Spent, so a lane closing later isn't refilled before the interval.
        assert!(!r.reprobe_now);
    }

    #[test]
    fn a_trial_held_back_by_the_limit_retires() {
        let mut r = settled_at_two(8);
        assert!(r.reprobe());
        assert_eq!(r.observe(skipped()), Decision::Hold);
        let frozen = Window {
            waited_fraction: 0.9,
            ..judged(200.0)
        };
        assert_eq!(r.observe(frozen), Decision::Retire);
        assert_eq!(r.target(), 2);
        assert!(r.reprobe(), "settled again, so the next raise can trial");
    }

    #[test]
    fn a_short_handed_window_is_not_judged() {
        let window = Duration::from_millis(500);
        let mut r = settled_at_two(8).reprobing_every(window, window);
        let short = Window {
            short: true,
            ..judged(200.0)
        };
        assert_eq!(r.observe(short), Decision::Hold, "the work is running out");
        assert_eq!(r.target(), 2);
        assert_eq!(r.observe(judged(200.0)), Decision::Add);
    }

    #[test]
    fn raising_or_removing_a_limit_is_a_raise() {
        assert!(limit_raised(Some(100), Some(200)));
        assert!(limit_raised(Some(100), None));
        assert!(!limit_raised(Some(100), Some(100)));
        assert!(!limit_raised(Some(200), Some(100)), "lowered");
        assert!(!limit_raised(None, None));
        assert!(!limit_raised(None, Some(100)), "a new limit");
    }

    /// Ramp 1 → 4, the ceiling: settled with a best of 400.
    fn settled_at_four() -> Ramp {
        let mut r = Ramp::new(4);
        for speed in [100.0, 200.0, 300.0] {
            assert_eq!(r.observe(judged(speed)), Decision::Add);
        }
        assert_eq!(r.observe(judged(400.0)), Decision::Hold);
        assert_eq!(r.target(), 4);
        r
    }

    #[test]
    fn three_slow_windows_in_a_row_shed_one() {
        let mut r = settled_at_four();
        // Two slow windows, then one that isn't: the count starts over.
        assert_eq!(r.observe(judged(270.0)), Decision::Hold);
        assert_eq!(r.observe(judged(270.0)), Decision::Hold);
        assert_eq!(r.observe(judged(280.0)), Decision::Hold, "70% isn't slow");
        assert_eq!(r.observe(judged(270.0)), Decision::Hold);
        assert_eq!(r.observe(judged(270.0)), Decision::Hold);
        assert_eq!(r.target(), 4);
        assert_eq!(r.observe(judged(270.0)), Decision::Shed);
        assert_eq!(r.target(), 3);
        // The best is now 270, so staying there sheds nothing more.
        for _ in 0..5 {
            assert_eq!(r.observe(judged(270.0)), Decision::Hold);
        }
        assert_eq!(r.target(), 3);
    }

    #[test]
    fn unjudged_windows_neither_count_nor_break_a_slow_run() {
        let mut r = settled_at_four();
        let frozen = Window {
            waited_fraction: 0.9,
            ..judged(100.0)
        };
        assert_eq!(r.observe(judged(200.0)), Decision::Hold);
        assert_eq!(r.observe(frozen), Decision::Hold);
        assert_eq!(r.observe(skipped()), Decision::Hold);
        assert_eq!(r.observe(judged(200.0)), Decision::Hold);
        assert_eq!(r.target(), 4);
        assert_eq!(r.observe(judged(200.0)), Decision::Shed);
    }

    #[test]
    fn a_download_held_down_by_its_limit_never_sheds() {
        let mut r = settled_at_four();
        let throttled = Window {
            waited_fraction: 0.6,
            ..judged(100.0)
        };
        for _ in 0..20 {
            assert_eq!(r.observe(throttled), Decision::Hold);
        }
        assert_eq!(r.target(), 4);
    }

    #[test]
    fn one_fast_window_doesnt_raise_the_bar_for_a_shed() {
        let mut r = settled_at_four();
        assert_eq!(r.observe(judged(600.0)), Decision::Hold);
        // Under 70% of that one window, but not of the smoothed best (450).
        for _ in 0..6 {
            assert_eq!(r.observe(judged(330.0)), Decision::Hold);
        }
        assert_eq!(r.target(), 4);
    }

    #[test]
    fn a_cap_while_settled_is_a_new_baseline_not_a_slowdown() {
        let mut r = settled_at_four();
        r.settle_at(2);
        for _ in 0..6 {
            assert_eq!(r.observe(judged(200.0)), Decision::Hold);
        }
        assert_eq!(r.target(), 4, "the coordinator holds the pool at the cap");
        // A real slowdown sheds from the two lanes the cap allows.
        assert_eq!(r.observe(judged(100.0)), Decision::Hold);
        assert_eq!(r.observe(judged(100.0)), Decision::Hold);
        assert_eq!(r.observe(judged(100.0)), Decision::Shed);
        assert_eq!(r.target(), 1);
    }

    #[test]
    fn a_limit_that_binds_without_freezing_never_sheds() {
        let mut r = settled_at_four();
        let limited = Window {
            waited_fraction: 0.3,
            ..judged(240.0)
        };
        for _ in 0..6 {
            assert_eq!(r.observe(limited), Decision::Hold);
        }
        assert_eq!(r.target(), 4);
    }

    #[test]
    fn shedding_stops_at_one_connection() {
        let mut r = Ramp::new(1);
        assert_eq!(r.observe(judged(100.0)), Decision::Hold);
        for _ in 0..300 {
            assert_eq!(r.observe(judged(10.0)), Decision::Hold);
        }
        assert_eq!(r.target(), 1);
    }

    #[test]
    fn a_reprobe_after_a_shed_can_win_the_connection_back() {
        let window = Duration::from_millis(500);
        let mut r = settled_at_four().reprobing_every(Duration::from_secs(2), window);
        assert_eq!(r.observe(judged(200.0)), Decision::Hold);
        assert_eq!(r.observe(judged(200.0)), Decision::Hold);
        assert_eq!(r.observe(judged(200.0)), Decision::Shed);
        // The interval counts from the shed; meanwhile the speed recovers.
        assert_eq!(r.observe(skipped()), Decision::Hold);
        assert_eq!(r.observe(judged(300.0)), Decision::Hold);
        assert_eq!(r.observe(judged(300.0)), Decision::Hold);
        assert_eq!(r.observe(judged(300.0)), Decision::Add);
        assert_eq!(r.observe(skipped()), Decision::Hold);
        assert_eq!(
            r.observe(judged(400.0)),
            Decision::Hold,
            "kept, at the ceiling"
        );
        assert_eq!(r.target(), 4);
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
