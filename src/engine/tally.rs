//! Running totals behind a download's finish summary. One [`Tally`] spans every
//! run of a download in this process (the Manager keeps one per download, so a
//! pause and resume still sum up), while [`download`](crate::download) feeds it.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::human;
use crate::engine::lock::lock;

/// The span the peak speed is measured over. Shorter readings swing wildly:
/// chunks often arrive back to back from a buffer, or in a burst after a stall.
const PEAK_WINDOW: Duration = Duration::from_secs(1);

/// Totals across a download's runs; see the module docs.
#[derive(Debug, Default)]
pub(crate) struct Tally(Mutex<Totals>);

#[derive(Debug, Default)]
struct Totals {
    /// When the first run in this process started.
    first_started: Option<Instant>,
    /// When the current run started; `None` between runs.
    run_started: Option<Instant>,
    /// Active time before the current run: the caller's record from before
    /// this process, plus every run here since.
    earlier: Duration,
    /// Whether active time from before this process counts, which makes this
    /// process's wall clock too short to stand for the whole download.
    inherited: bool,
    /// Whether this process has finished a run of this download yet.
    ran_before: bool,
    /// Whether the current run has said how it started (`Started`/`Resumed`).
    announced: bool,
    /// Bytes already on disk that no run here fetched, left out of the average.
    baseline: u64,
    /// Where the current peak window started: when, and the bytes on disk.
    window: Option<(Instant, u64)>,
    peak: f64,
    retries: u32,
    redownloaded: u64,
}

impl Tally {
    /// A run starts. `active_before` is the caller's record of active time
    /// from before this process; only the first run takes it, since later
    /// runs add up their own.
    pub(crate) fn begin_run(&self, active_before: Duration) {
        self.begin_run_at(active_before, Instant::now());
    }

    fn begin_run_at(&self, active_before: Duration, now: Instant) {
        let mut t = lock(&self.0);
        if t.first_started.is_none() {
            t.first_started = Some(now);
            t.earlier = active_before;
            t.inherited = !active_before.is_zero();
        }
        t.run_started = Some(now);
        t.announced = false;
        t.window = None;
    }

    /// The current run has ended, however it ended.
    pub(crate) fn end_run(&self) {
        self.end_run_at(Instant::now());
    }

    fn end_run_at(&self, now: Instant) {
        let mut t = lock(&self.0);
        if let Some(started) = t.run_started.take() {
            t.earlier += now.saturating_duration_since(started);
            t.ran_before = true;
        }
    }

    /// True the first time it's asked in a run, so a run announces how it
    /// started once, not again on each retry.
    pub(crate) fn announce(&self) -> bool {
        !std::mem::replace(&mut lock(&self.0).announced, true)
    }

    /// A run picked up `on_disk` bytes left by an earlier one. Bytes no run in
    /// this tally fetched (and no earlier active time covers) stay out of the
    /// average speed.
    pub(crate) fn resumed_with(&self, on_disk: u64) {
        let mut t = lock(&self.0);
        if t.earlier.is_zero() && !t.ran_before {
            t.baseline = on_disk;
        }
    }

    /// Progress: `downloaded` bytes are on disk. Feeds the peak speed, taken
    /// over windows of at least [`PEAK_WINDOW`].
    pub(crate) fn observe_progress(&self, downloaded: u64) {
        self.observe_progress_at(downloaded, Instant::now());
    }

    fn observe_progress_at(&self, downloaded: u64, now: Instant) {
        let mut t = lock(&self.0);
        match t.window {
            // A restart threw bytes away: measure afresh.
            Some((_, from)) if downloaded < from => t.window = Some((now, downloaded)),
            Some((since, from)) => {
                let span = now.saturating_duration_since(since);
                if span >= PEAK_WINDOW {
                    let speed = (downloaded - from) as f64 / span.as_secs_f64();
                    t.peak = t.peak.max(speed);
                    t.window = Some((now, downloaded));
                }
            }
            None => t.window = Some((now, downloaded)),
        }
    }

    pub(crate) fn retried(&self) {
        lock(&self.0).retries += 1;
    }

    /// `bytes` already fetched are being thrown away and fetched again.
    pub(crate) fn wasted(&self, bytes: u64) {
        let mut t = lock(&self.0);
        t.redownloaded = t.redownloaded.saturating_add(bytes);
        // Everything is fetched again now, so all of it counts.
        t.baseline = 0;
    }

    /// The finish line for a `bytes`-long file, e.g. `Completed · 6.25 GB in 9m 59s
    /// (active 7m 14s) · avg 14.7 MB/s · peak 38.0 MB/s · 2 retries`.
    pub(crate) fn done_line(&self, bytes: u64) -> String {
        self.done_line_at(bytes, Instant::now())
    }

    fn done_line_at(&self, bytes: u64, now: Instant) -> String {
        let t = lock(&self.0);
        let since =
            |s: Option<Instant>| s.map_or(Duration::ZERO, |s| now.saturating_duration_since(s));
        let active = t.earlier + since(t.run_started);
        let wall = since(t.first_started);
        // Wall time is shown only when it spans the whole download, and only
        // when it says more than the active time does.
        let took = if !t.inherited && wall >= active + Duration::from_secs(1) {
            format!(
                "{} (active {})",
                human::duration(wall),
                human::duration(active)
            )
        } else {
            human::duration(active)
        };
        let fetched = bytes.saturating_sub(t.baseline);
        let avg = fetched as f64 / active.as_secs_f64().max(0.001);
        let retries = match t.retries {
            0 => "no retries".to_owned(),
            n => human::count(n, "retry", "retries"),
        };
        let mut line = format!(
            "Completed · {} in {took} · avg {} · peak {} · {retries}",
            human::bytes(bytes),
            human::speed(avg),
            human::speed(t.peak.max(avg)),
        );
        if t.redownloaded > 0 {
            line.push_str(&format!(
                " · {} re-downloaded",
                human::bytes(t.redownloaded)
            ));
        }
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn announces_once_per_run() {
        let tally = Tally::default();
        tally.begin_run(Duration::ZERO);
        assert!(tally.announce());
        assert!(!tally.announce());
        tally.begin_run(Duration::ZERO);
        assert!(tally.announce());
    }

    /// Two runs with a pause between: active time is the runs, the wall time
    /// includes the pause, and retries and waste add up across both.
    #[test]
    fn the_summary_counts_every_run() {
        let tally = Tally::default();
        let t0 = Instant::now();
        tally.begin_run_at(Duration::ZERO, t0);
        tally.retried();
        tally.end_run_at(t0 + secs(4));
        // Paused for a minute. A later run's `active_before` is ignored: the
        // tally already counts the first run itself.
        tally.begin_run_at(secs(999), t0 + secs(64));
        tally.retried();
        tally.wasted(MIB);
        let line = tally.done_line_at(60 * MIB, t0 + secs(70));
        assert_eq!(
            line,
            "Completed · 60.0 MB in 1m 10s (active 10s) · avg 6.00 MB/s · peak 6.00 MB/s · \
             2 retries · 1.00 MB re-downloaded"
        );
    }

    /// Earlier active time from the caller (a run before this process) counts
    /// once, and the wall clock can't show less than the active time.
    #[test]
    fn earlier_active_time_counts_once() {
        let tally = Tally::default();
        let t0 = Instant::now();
        tally.begin_run_at(secs(20), t0);
        let line = tally.done_line_at(30 * MIB, t0 + secs(10));
        assert!(
            line.starts_with("Completed · 30.0 MB in 30s · avg 1.00 MB/s"),
            "{line}"
        );
    }

    /// With active time from before this process, this process's wall clock
    /// misses the earlier launches, so even a long pause here doesn't make
    /// it the download's elapsed time.
    #[test]
    fn a_download_from_an_earlier_launch_shows_only_active_time() {
        let tally = Tally::default();
        let t0 = Instant::now();
        tally.begin_run_at(secs(60), t0);
        tally.end_run_at(t0 + secs(60));
        tally.begin_run_at(Duration::ZERO, t0 + secs(30 * 60 + 60));
        let line = tally.done_line_at(180 * MIB, t0 + secs(30 * 60 + 120));
        assert!(
            line.starts_with("Completed · 180.0 MB in 3m 0s · avg 1.00 MB/s"),
            "{line}"
        );
    }

    /// The peak is the best whole window: a burst within one (buffered
    /// chunks, or the catch-up after a stall) can't inflate it.
    #[test]
    fn the_peak_is_measured_over_whole_windows() {
        let tally = Tally::default();
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        tally.begin_run_at(Duration::ZERO, t0);
        tally.observe_progress_at(0, t0);
        tally.observe_progress_at(MIB, t0 + ms(1));
        tally.observe_progress_at(3 * MIB, t0 + ms(1000));
        // A stall, then a burst.
        tally.observe_progress_at(3 * MIB + 1, t0 + secs(10));
        tally.observe_progress_at(4 * MIB, t0 + secs(10) + ms(2));
        let line = tally.done_line_at(4 * MIB, t0 + secs(11));
        assert!(line.contains(" · peak 3.00 MB/s · "), "{line}");
    }

    /// A restart (fewer bytes on disk than before) starts a fresh window, and
    /// the bytes thrown away are counted.
    #[test]
    fn a_restart_starts_a_fresh_window_and_counts_the_waste() {
        let tally = Tally::default();
        let t0 = Instant::now();
        tally.begin_run_at(Duration::ZERO, t0);
        tally.resumed_with(MIB);
        tally.observe_progress_at(2 * MIB, t0);
        tally.wasted(2 * MIB);
        tally.observe_progress_at(0, t0 + secs(1));
        tally.observe_progress_at(2 * MIB, t0 + secs(2));
        let line = tally.done_line_at(2 * MIB, t0 + secs(2));
        // Everything was fetched here after all, so it all counts.
        assert!(
            line.contains(" · avg 1.00 MB/s · peak 2.00 MB/s · "),
            "{line}"
        );
        assert!(line.ends_with(" · 2.00 MB re-downloaded"), "{line}");
    }

    #[test]
    fn a_quiet_summary_leaves_out_what_did_not_happen() {
        let tally = Tally::default();
        let t0 = Instant::now();
        tally.begin_run_at(Duration::ZERO, t0);
        let line = tally.done_line_at(512, t0 + secs(1));
        assert_eq!(
            line,
            "Completed · 512 B in 1.0s · avg 512 B/s · peak 512 B/s · no retries"
        );
    }

    /// Bytes a first run here found on disk weren't fetched in the time
    /// counted, so they stay out of the average, unless earlier active time
    /// covers them.
    #[test]
    fn bytes_found_on_disk_stay_out_of_the_average() {
        let t0 = Instant::now();
        let covered = Tally::default();
        covered.begin_run_at(secs(2), t0);
        covered.resumed_with(2 * MIB);
        let line = covered.done_line_at(4 * MIB, t0 + secs(2));
        assert!(line.contains("avg 1.00 MB/s"), "{line}");

        let fresh = Tally::default();
        fresh.begin_run_at(Duration::ZERO, t0);
        fresh.resumed_with(2 * MIB);
        let line = fresh.done_line_at(4 * MIB, t0 + secs(2));
        assert!(line.contains("avg 1.00 MB/s"), "{line}");
        let all = Tally::default();
        all.begin_run_at(Duration::ZERO, t0);
        let line = all.done_line_at(4 * MIB, t0 + secs(2));
        assert!(line.contains("avg 2.00 MB/s"), "{line}");
    }
}
