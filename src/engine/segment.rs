//! Multi-segment parallel downloads.
//!
//! A known-size, range-capable resource is split into byte ranges that are
//! fetched concurrently into a pre-allocated `.part`. Each worker writes its
//! range at the right offset; per-segment progress is checkpointed into a v2
//! sidecar so a killed download re-fetches only the missing ranges. An idle
//! worker steals the back half of the largest remaining segment.
//!
//! The pool starts on one worker and ramps (see [`ramp`](super::ramp)): each
//! measurement window the coordinator may add a worker, or retire one. The
//! highest-numbered lane leaves at its next chunk boundary and hands its
//! segment back; if it is stuck for [`SHED_GRACE`], the next lane to reach a
//! boundary leaves instead.
//!
//! Workers are futures polled by the coordinator inside the caller's task (no
//! spawned tasks), so the whole [`download`](crate::download) future stays
//! `Send` and every worker is drained before the coordinator returns.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::stream::FuturesUnordered;
use futures_util::StreamExt;
use reqwest::header;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;

use crate::engine::ramp::{Decision, Ramp, Ramping, Window};
use crate::engine::tally::Tally;
use crate::engine::{
    human, is_transient, progress, status_error, throttle_wait, Control, DownloadState, Error,
    Flow, Progress, SpeedMeter,
};

/// Hard cap on parallel connections per download.
pub(crate) const MAX_CONNECTIONS: u8 = 16;

/// Lane numbers a pool can hand out; above [`MAX_CONNECTIONS`], since a
/// retiring worker holds its lane until it leaves.
const LANES: usize = 32;

/// How long the highest-numbered lane gets to leave an over-target pool
/// before any lane may. Three ramp windows: a lane silent that long is stuck
/// on the server, and a shed shouldn't wait on it much longer than that.
pub(crate) const SHED_GRACE: Duration = Duration::from_secs(3);

/// How often per-segment progress is made durable in the sidecar.
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(1);

/// How often the coordinator polls [`Control`] itself, so a pause/cancel is
/// noticed even while every worker is blocked waiting on the network.
pub(crate) const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// One byte range `[start, end)` of a segmented download, of which the first
/// `done` bytes are written to the `.part`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    /// First byte of the range.
    pub start: u64,
    /// One past the last byte of the range.
    pub end: u64,
    /// Bytes written from `start`; never claims more than is on disk.
    pub done: u64,
}

impl Segment {
    pub(crate) fn len(&self) -> u64 {
        self.end - self.start
    }

    pub(crate) fn remaining(&self) -> u64 {
        self.len().saturating_sub(self.done)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.done >= self.len()
    }
}

/// Split `total` bytes into equal ranges, one per connection, but none smaller
/// than `min` (the last absorbs the remainder) and at most [`MAX_CONNECTIONS`].
pub(crate) fn plan(total: u64, connections: u8, min: u64) -> Vec<Segment> {
    let n = u64::from(connections)
        .min(total / min.max(1))
        .clamp(1, u64::from(MAX_CONNECTIONS));
    let size = total / n;
    (0..n)
        .map(|i| Segment {
            start: i * size,
            end: if i == n - 1 { total } else { (i + 1) * size },
            done: 0,
        })
        .collect()
}

/// Work stealing: split the segment with the most bytes left in half, pushing
/// the back half as a new segment; returns `(victim, tail)` indices. Segments
/// with less than `2 * min` left are not worth a second connection.
pub(crate) fn steal(segments: &mut Vec<Segment>, min: u64) -> Option<(usize, usize)> {
    let threshold = min.max(1).saturating_mul(2);
    let (victim, remaining) = segments
        .iter()
        .map(Segment::remaining)
        .enumerate()
        .filter(|&(_, remaining)| remaining >= threshold)
        .max_by_key(|&(i, remaining)| (remaining, std::cmp::Reverse(i)))?;
    let seg = &mut segments[victim];
    let mid = seg.start + seg.done + remaining / 2;
    let tail = Segment {
        start: mid,
        end: seg.end,
        done: 0,
    };
    seg.end = mid;
    segments.push(tail);
    Some((victim, segments.len() - 1))
}

/// Parse a `Content-Range: bytes <first>-<last>/<total>` value. Unsatisfied
/// (`*`) or unknown-length forms yield `None`.
pub(crate) fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let rest = value.trim().strip_prefix("bytes ")?;
    let (range, total) = rest.split_once('/')?;
    let (first, last) = range.split_once('-')?;
    let first: u64 = first.trim().parse().ok()?;
    let last: u64 = last.trim().parse().ok()?;
    let total: u64 = total.trim().parse().ok()?;
    (first <= last && last < total).then_some((first, last, total))
}

/// Whether an ETag is strong (weak ones are never sent in `If-Range`).
pub(crate) fn is_strong_etag(etag: &str) -> bool {
    !etag.trim_start().starts_with("W/")
}

/// Why a segmented attempt failed.
#[derive(Debug)]
pub(crate) enum Failure {
    /// An ordinary engine error, classified by the retry loop.
    Engine(Error),
    /// The server answered a `Range` request with a full body or a mismatched
    /// range, so the download must fall back to a single connection.
    RangeUnsupported,
}

impl From<Error> for Failure {
    fn from(e: Error) -> Self {
        Self::Engine(e)
    }
}

impl From<reqwest::Error> for Failure {
    fn from(e: reqwest::Error) -> Self {
        Self::Engine(e.into())
    }
}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Self::Engine(e.into())
    }
}

/// Everything a segmented attempt needs. `state` is the v2 sidecar content
/// (its `segments` seed the work table); `first` is the already-open 200
/// response of a fresh download, which serves segment 0. `redirected` says
/// `url` is a redirect target rather than the URL the user asked for.
pub(crate) struct Job<'a> {
    pub client: &'a reqwest::Client,
    pub control: &'a Control,
    pub url: String,
    pub redirected: bool,
    pub part: &'a Path,
    pub sidecar: &'a Path,
    pub state: DownloadState,
    pub workers: usize,
    pub min_segment_size: u64,
    pub read_timeout: Duration,
    pub ramp: Ramping,
    pub shed_grace: Duration,
    pub first: Option<reqwest::Response>,
    pub tally: &'a Tally,
}

/// The shared work table: segments plus which lane, if any, is on each.
struct Table {
    segments: Vec<Segment>,
    /// The lane (zero-based) fetching each segment.
    owner: Vec<Option<u8>>,
    /// Set once the host has refused a request as a rate limit this attempt.
    rate_limited: bool,
}

/// How a lane came by the segment it claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claim {
    /// One nobody has started.
    Fresh,
    /// One another lane started and handed back.
    HandedBack,
    /// The back half of segment `victim`, split off while lane `from` was on it.
    Split { victim: usize, from: Option<u8> },
}

impl Table {
    /// [`Table::claim`] for `lane`, which first lets go of what it held; one
    /// that hasn't sent a request yet gets nothing once the host has
    /// rate-limited us, since its first request would be a new connection
    /// (the outer backoff resumes that work).
    fn claim_for(&mut self, min: u64, fresh: bool, lane: u8) -> Option<(usize, Claim)> {
        self.release(lane);
        if fresh && self.rate_limited {
            return None;
        }
        self.claim(min, lane)
    }

    /// Hand `lane` an unowned unfinished segment, else steal one. Releasing
    /// and claiming under one lock means a lane is never seen holding nothing
    /// between two segments.
    fn claim(&mut self, min: u64, lane: u8) -> Option<(usize, Claim)> {
        let queued = (0..self.segments.len())
            .find(|&i| self.owner[i].is_none() && !self.segments[i].is_complete());
        let (idx, how) = match queued {
            Some(i) if self.segments[i].done > 0 => (i, Claim::HandedBack),
            Some(i) => (i, Claim::Fresh),
            None => {
                let (victim, tail) = steal(&mut self.segments, min)?;
                self.owner.push(None);
                let from = self.owner[victim];
                (tail, Claim::Split { victim, from })
            }
        };
        self.owner[idx] = Some(lane);
        Some((idx, how))
    }

    /// Let go of whatever `lane` holds.
    fn release(&mut self, lane: u8) {
        for owner in &mut self.owner {
            if *owner == Some(lane) {
                *owner = None;
            }
        }
    }

    /// Each segment's lane, numbered from 1 as the log names it.
    fn lanes(&self) -> Vec<Option<u8>> {
        self.owner.iter().map(|o| o.map(|lane| lane + 1)).collect()
    }

    fn done(&self) -> u64 {
        self.segments.iter().map(|s| s.done).sum()
    }

    /// Whether some unfinished segment has no worker (e.g. one handed back).
    fn has_unowned_work(&self) -> bool {
        self.segments
            .iter()
            .zip(&self.owner)
            .any(|(seg, owner)| owner.is_none() && !seg.is_complete())
    }
}

/// Live-worker accounting: how many workers run, how many the ramp wants, and
/// whether the pool changed since the last ramp window.
struct Pool {
    live: AtomicU8,
    target: AtomicU8,
    changed: AtomicBool,
    /// Lane numbers in use and when the pool went over target. `live` only
    /// changes under this lock, so a leave is judged against a stable pool.
    lanes: Mutex<LaneSet>,
    /// How long the bottom lane gets to leave before any lane may.
    grace: Duration,
}

#[derive(Debug, Default)]
struct LaneSet {
    /// One bit per lane number in use; a lane's number is what the log calls it by.
    used: u32,
    /// When a leave first fell due, while one is pending.
    over_since: Option<Instant>,
}

impl LaneSet {
    /// The highest lane number in use (zero-based).
    fn bottom(&self) -> Option<u8> {
        (self.used != 0).then(|| (31 - self.used.leading_zeros()) as u8)
    }
}

impl Pool {
    fn new(target: u8, grace: Duration) -> Self {
        Self {
            live: AtomicU8::new(0),
            target: AtomicU8::new(target),
            // The first window holds the initial spawn.
            changed: AtomicBool::new(true),
            lanes: Mutex::new(LaneSet::default()),
            grace,
        }
    }

    /// Count a new worker in, on lane `prefer` if it's free, else the lowest
    /// free one; it is counted out when the slot drops.
    fn join(&self, prefer: Option<u8>) -> Slot<'_> {
        let mut lanes = self.lanes.lock().unwrap();
        let used = lanes.used;
        let free = |lane: &u8| usize::from(*lane) < LANES && used & (1 << lane) == 0;
        let lane = prefer
            .filter(free)
            .or_else(|| (0..LANES as u8).find(free))
            .unwrap_or(LANES as u8 - 1);
        lanes.used |= 1 << lane;
        self.live.fetch_add(1, Ordering::SeqCst);
        self.changed.store(true, Ordering::SeqCst);
        Slot {
            pool: self,
            held: true,
            lane,
        }
    }

    /// Count `lane`'s worker out.
    fn leave(&self, lanes: &mut LaneSet, lane: u8) {
        lanes.used &= !(1 << lane);
        self.live.fetch_sub(1, Ordering::SeqCst);
        self.changed.store(true, Ordering::SeqCst);
    }

    /// The highest lane number in use (zero-based): the next to be shed.
    fn bottom(&self) -> Option<u8> {
        self.lanes.lock().unwrap().bottom()
    }

    fn live(&self) -> u8 {
        self.live.load(Ordering::SeqCst)
    }

    fn target(&self) -> u8 {
        self.target.load(Ordering::SeqCst)
    }

    fn set_target(&self, n: u8) {
        self.target.store(n, Ordering::SeqCst);
    }

    fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::SeqCst)
    }
}

/// Whether a worker left an over-target pool, and in whose place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leave {
    /// It stays: the pool isn't over target, or it isn't this lane's turn.
    Stay,
    /// It left as the highest-numbered lane.
    Bottom,
    /// It left because the bottom lane (zero-based) didn't within the grace.
    InsteadOf(u8),
}

/// One worker's place in the [`Pool`].
struct Slot<'a> {
    pool: &'a Pool,
    held: bool,
    /// Zero-based; the log shows it one-based.
    lane: u8,
}

impl Slot<'_> {
    /// Leave the pool if it is over target and it is this lane's turn.
    fn try_retire(&mut self) -> Leave {
        self.try_retire_at(Instant::now())
    }

    /// [`Slot::try_retire`] at `now`. The bottom lane leaves first, so Lane 1
    /// stays; once a leave has been due for the pool's grace, another lane may
    /// go, so a bottom lane stuck on the server can't hold the shed up. Lane 1
    /// goes instead only when it is the one other lane.
    fn try_retire_at(&mut self, now: Instant) -> Leave {
        let pool = self.pool;
        let mut lanes = pool.lanes.lock().unwrap();
        let live = pool.live();
        if !self.held || live <= pool.target() {
            lanes.over_since = None;
            return Leave::Stay;
        }
        let since = *lanes.over_since.get_or_insert(now);
        let bottom = lanes.bottom().unwrap_or(self.lane);
        let leave = if self.lane == bottom {
            Leave::Bottom
        } else if now.saturating_duration_since(since) >= pool.grace
            && (self.lane != 0 || lanes.used.count_ones() <= 2)
        {
            Leave::InsteadOf(bottom)
        } else {
            return Leave::Stay;
        };
        pool.leave(&mut lanes, self.lane);
        lanes.over_since = (live - 1 > pool.target()).then_some(now);
        self.held = false;
        leave
    }

    /// The lane's number as the log shows it, from 1.
    fn number(&self) -> u8 {
        self.lane + 1
    }
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        if self.held {
            let mut lanes = self.pool.lanes.lock().unwrap();
            self.pool.leave(&mut lanes, self.lane);
        }
    }
}

/// How a worker's fetch of one segment ended (short of failing).
enum Outcome {
    /// The segment is fully written.
    Done,
    /// The stop signal was raised.
    Stopped,
    /// The worker left the pool mid-segment; the segment needs a new owner.
    Retired(Leave),
}

/// State shared by the coordinator and its workers (borrowed, never spawned).
struct Shared<'a> {
    client: &'a reqwest::Client,
    control: &'a Control,
    url: String,
    redirected: bool,
    validator: Option<String>,
    total: u64,
    min: u64,
    read_timeout: Duration,
    part: &'a Path,
    table: Mutex<Table>,
    progressed: Notify,
    stop: watch::Sender<bool>,
    pool: Pool,
    /// Nanoseconds workers have spent waiting on rate limits.
    waited: AtomicU64,
    tally: &'a Tally,
    /// Each lane's failures since it last got a response; non-zero means its
    /// next response is a reconnect.
    lane_failures: Mutex<[u32; LANES]>,
}

impl Shared<'_> {
    fn raise_stop(&self) {
        self.stop.send_replace(true);
    }

    fn is_stopping(&self) -> bool {
        *self.stop.borrow()
    }

    fn snapshot(&self) -> Vec<Segment> {
        self.table.lock().unwrap().segments.clone()
    }

    fn downloaded(&self) -> u64 {
        self.table.lock().unwrap().done().min(self.total)
    }
}

/// Run one segmented attempt to completion, pause, cancel or failure. Returns
/// the byte count (== total) once every segment is written. On
/// [`Failure::RangeUnsupported`] the sidecar is deleted and the `.part`
/// truncated, ready for a single-connection restart.
pub(crate) async fn run(
    job: Job<'_>,
    on_progress: &mut impl FnMut(Progress),
) -> Result<u64, Failure> {
    let total = job.state.total.unwrap_or(0);
    let segments = job.state.segments.clone().unwrap_or_default();
    let mut owner = vec![None; segments.len()];
    if job.first.is_some() {
        // The first response serves segment 0, on the first lane.
        owner[0] = Some(0);
    }
    let ceiling = u8::try_from(job.workers)
        .unwrap_or(MAX_CONNECTIONS)
        .clamp(1, MAX_CONNECTIONS);
    let (mut ramp, window) = match job.ramp {
        Ramping::Window(w) => (Ramp::new(ceiling), Some(w.max(Duration::from_millis(1)))),
        Ramping::Off => (Ramp::fixed(ceiling), None),
    };
    let (stop, _) = watch::channel(false);
    let shared = Shared {
        client: job.client,
        control: job.control,
        url: job.url,
        redirected: job.redirected,
        validator: job.state.if_range().map(str::to_owned),
        total,
        min: job.min_segment_size,
        read_timeout: job.read_timeout,
        part: job.part,
        table: Mutex::new(Table {
            segments,
            owner,
            rate_limited: false,
        }),
        progressed: Notify::new(),
        stop,
        pool: Pool::new(ramp.target(), job.shed_grace),
        waited: AtomicU64::new(0),
        tally: job.tally,
        lane_failures: Mutex::new([0; LANES]),
    };

    let mut meter = SpeedMeter::new();
    let started = Instant::now();
    let report = |meter: &mut SpeedMeter, on_progress: &mut dyn FnMut(Progress)| {
        // One lock, so the readout and the segments always agree.
        let (downloaded, segments, lanes) = {
            let table = shared.table.lock().unwrap();
            (
                table.done().min(total),
                table.segments.clone(),
                table.lanes(),
            )
        };
        let bytes_per_sec = meter.observe(started.elapsed(), downloaded);
        on_progress(Progress {
            downloaded,
            total: Some(total),
            bytes_per_sec,
            eta: progress::eta(downloaded, Some(total), bytes_per_sec),
            connections: shared.pool.live(),
            segments,
            lanes,
        });
    };

    let mut first = job.first;
    let mut workers = FuturesUnordered::new();
    for _ in 0..ramp.target() {
        workers.push(worker(&shared, first.take(), shared.pool.join(None)));
    }
    report(&mut meter, on_progress);
    // Bounded so a server that keeps failing still ends the attempt.
    let mut replacements_left = 2 * usize::from(ceiling);

    let ramp_period = window.unwrap_or(CONTROL_POLL_INTERVAL);
    let mut ramp_tick =
        tokio::time::interval_at(tokio::time::Instant::now() + ramp_period, ramp_period);
    ramp_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut window_start = (Instant::now(), shared.downloaded(), 0u64);

    let mut control_tick = tokio::time::interval(CONTROL_POLL_INTERVAL);
    control_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut checkpoint_tick = tokio::time::interval_at(
        tokio::time::Instant::now() + CHECKPOINT_INTERVAL,
        CHECKPOINT_INTERVAL,
    );
    checkpoint_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut checkpoint: Option<JoinHandle<crate::engine::Result<()>>> = None;
    let mut fatal: Option<Failure> = None;
    let mut transient: Option<(Severity, Failure)> = None;
    // The most connections a capping host has accepted this attempt; the
    // ramp's own target can sit above it once settled.
    let mut cap: Option<u8> = None;

    while !workers.is_empty() {
        let checkpoint_running = checkpoint.is_some();
        let can_checkpoint = !checkpoint_running && !shared.is_stopping();
        tokio::select! {
            Some((lane, result)) = workers.next() => {
                // The lane's failures in a row: carried over to a replacement
                // below, else dropped, so a later worker on the same number
                // doesn't report a reconnect it never made.
                let failed_before =
                    std::mem::take(&mut shared.lane_failures.lock().unwrap()[usize::from(lane)]);
                if let Err(failure) = result {
                    match failure {
                        Failure::Engine(ref e) if is_transient(e) => {
                            tracing::debug!(
                                "Lane {} failed: {}",
                                lane + 1,
                                crate::engine::error_detail(e)
                            );
                            let severity = Severity::of(e, shared.pool.live() > 0);
                            let what = lane_problem(e);
                            // The host says it won't take more connections.
                            if severity != Severity::Other {
                                let live = shared.pool.live();
                                ramp.settle_at(live);
                                // A fixed (or already settled) ramp ignores
                                // `settle_at`; without the clamp each refusal's
                                // replacement would run straight into the cap.
                                if severity == Severity::Cap {
                                    cap = lower_cap(cap, live);
                                }
                                let before = shared.pool.target();
                                shared.pool.set_target(capped(ramp.target(), cap));
                                let after = shared.pool.target();
                                if severity == Severity::Cap && after < before {
                                    tracing::info!(
                                        "Server limited connections — down to {}",
                                        human::count(after.into(), "lane", "lanes")
                                    );
                                }
                            }
                            // The outer backoff keys off the attempt's error,
                            // so a sibling's later reset mustn't mask a rate limit.
                            let replace = match &transient {
                                None => true,
                                Some((kept, _)) => severity >= *kept,
                            };
                            if replace {
                                transient = Some((severity, failure));
                            }
                            let rate_limited = matches!(transient, Some((Severity::RateLimit(_), _)));
                            // Live workers only re-claim once their own segment
                            // is done; if none are left, the handed-back
                            // segment needs a fresh connection (the settled
                            // target already excludes a capped one). Not once
                            // the host has rate-limited us, though: an instant
                            // retry is a request flood, so it waits for the
                            // outer backoff.
                            let orphaned = shared.table.lock().unwrap().has_unowned_work();
                            let wanted = usize::from(shared.pool.target());
                            let failures = failed_before + 1;
                            if !rate_limited && orphaned && workers.len() < wanted && replacements_left > 0 && !shared.is_stopping() {
                                replacements_left -= 1;
                                let slot = shared.pool.join(Some(lane));
                                // A capped lane isn't lost (the cap line said
                                // why), so it isn't a retry either.
                                if severity == Severity::Other {
                                    shared.tally.retried();
                                    tracing::warn!(
                                        "Lane {} {what} — retrying (attempt {})",
                                        slot.number(),
                                        failures + 1
                                    );
                                    shared.lane_failures.lock().unwrap()[usize::from(slot.lane)] = failures;
                                }
                                workers.push(worker(&shared, None, slot));
                            } else if severity == Severity::Other && !shared.is_stopping() {
                                tracing::warn!("Lane {} {what}", lane + 1);
                            }
                        }
                        other => {
                            shared.raise_stop();
                            fatal.get_or_insert(other);
                        }
                    }
                }
            }
            _ = shared.progressed.notified() => report(&mut meter, on_progress),
            _ = control_tick.tick() => {
                if job.control.poll() != Flow::Continue {
                    shared.raise_stop();
                }
            }
            _ = ramp_tick.tick(), if window.is_some() => {
                let now = Instant::now();
                let (started_at, downloaded_then, waited_then) = window_start;
                let downloaded = shared.downloaded();
                let waited = shared.waited.load(Ordering::Relaxed);
                window_start = (now, downloaded, waited);
                let secs = now.duration_since(started_at).as_secs_f64();
                let live = f64::from(shared.pool.live().max(1));
                let measured = Window {
                    bytes_per_sec: downloaded.saturating_sub(downloaded_then) as f64 / secs,
                    waited_fraction: waited.saturating_sub(waited_then) as f64 / 1e9 / (secs * live),
                    pool_changed: shared.pool.take_changed(),
                };
                let decision = ramp.observe(measured);
                shared.pool.set_target(capped(ramp.target(), cap));
                tracing::trace!(?measured, ?decision, target = ramp.target(), "ramp window");
                match decision {
                    Decision::Add if !shared.is_stopping() && job.control.poll() == Flow::Continue => {
                        let slot = shared.pool.join(None);
                        let number = slot.number();
                        workers.push(worker(&shared, None, slot));
                        tracing::info!(
                            "Added Lane {number} — now {}",
                            human::count(shared.pool.live().into(), "lane", "lanes")
                        );
                        report(&mut meter, on_progress);
                    }
                    Decision::Retire => {
                        let over = shared.pool.live() > shared.pool.target();
                        if let Some(bottom) = shared.pool.bottom().filter(|_| over) {
                            tracing::info!(
                                "Closing Lane {} — more lanes didn't speed this up",
                                bottom + 1
                            );
                        }
                    }
                    Decision::Add | Decision::Hold => {}
                }
            }
            joined = async { checkpoint.as_mut().expect("guarded by precondition").await }, if checkpoint_running => {
                checkpoint = None;
                log_periodic_checkpoint(joined);
            }
            _ = checkpoint_tick.tick(), if can_checkpoint => {
                checkpoint = Some(spawn_checkpoint(&shared, &job.state, job.sidecar));
            }
        }
    }
    drop(workers);

    // Stop invariant: every worker has finished its last write (drained above)
    // and no checkpoint is in flight before the `.part` is touched again.
    if let Some(handle) = checkpoint.take() {
        log_periodic_checkpoint(handle.await);
    }
    report(&mut meter, on_progress);

    let final_checkpoint = |complete: bool| {
        let mut st = job.state.clone();
        st.segments = Some(shared.snapshot());
        let (part, sidecar) = (job.part.to_path_buf(), job.sidecar.to_path_buf());
        // The blocking pool doesn't inherit the download's span; carry it over.
        let span = tracing::Span::current();
        let blocking = tokio::task::spawn_blocking(move || {
            let _entered = span.enter();
            if complete {
                complete_checkpoint_blocking(&part, || {
                    save_unless_removed(&part, &sidecar, &st, || {})
                })
            } else {
                checkpoint_blocking(&part, &sidecar, &st, || {})
            }
        });
        async move {
            blocking
                .await
                .map_err(|e| Error::Io(std::io::Error::other(e)))?
        }
    };

    if job.control.is_cancelled() {
        let _ = tokio::fs::remove_file(job.part).await;
        let _ = tokio::fs::remove_file(job.sidecar).await;
        crate::engine::state::remove_stale_temps(job.sidecar);
        return Err(Error::Cancelled.into());
    }
    let complete = shared.snapshot().iter().all(Segment::is_complete);
    if complete {
        final_checkpoint(true).await?;
        return Ok(shared.downloaded());
    }
    match fatal {
        Some(Failure::RangeUnsupported) => {
            shared.tally.wasted(shared.downloaded());
            tracing::warn!(
                "Server sent the whole file instead of a part — starting over on 1 lane"
            );
            let _ = tokio::fs::remove_file(job.sidecar).await;
            tokio::fs::File::create(job.part).await?;
            Err(Failure::RangeUnsupported)
        }
        Some(failure) => {
            final_checkpoint(false).await?;
            Err(failure)
        }
        None => {
            final_checkpoint(false).await?;
            if job.control.is_paused() {
                return Err(Error::Paused.into());
            }
            // A cap's bare 503 must not reach the outer loop, which would
            // read it as a rate limit and escalate the backoff.
            Err(match transient {
                Some((severity, failure)) if severity != Severity::Cap => failure,
                _ => Error::Incomplete("segments left unfinished".into()).into(),
            })
        }
    }
}

/// How a transient worker failure bears on the rest of its attempt, least to
/// most; the attempt reports its most severe one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Severity {
    /// A bare 503 while other connections are still served: a connection cap.
    Cap,
    /// Any other transient failure (a reset, a timeout, a short body).
    Other,
    /// A 429, any refusal with `Retry-After`, or a bare 503 to the last live
    /// connection; the longest wait ranks highest.
    RateLimit(Duration),
}

impl Severity {
    fn of(e: &Error, others_live: bool) -> Self {
        match e {
            Error::Status(503) if others_live => Self::Cap,
            _ => throttle_wait(e).map_or(Self::Other, Self::RateLimit),
        }
    }
}

fn capped(target: u8, cap: Option<u8>) -> u8 {
    cap.map_or(target, |c| target.min(c))
}

/// Record a connection cap hit with `live` connections still served; a cap
/// only ever tightens within an attempt.
fn lower_cap(cap: Option<u8>, live: u8) -> Option<u8> {
    let live = live.max(1);
    Some(cap.map_or(live, |c| c.min(live)))
}

/// What happened to a lane, e.g. `lost connection`, `got HTTP 500`.
fn lane_problem(e: &Error) -> String {
    match e {
        Error::TimedOut => "timed out".to_owned(),
        Error::Http(e) if e.is_timeout() => "timed out".to_owned(),
        Error::Http(e) if e.is_connect() => "couldn't connect".to_owned(),
        Error::Status(code) | Error::Throttled { status: code, .. } => format!("got HTTP {code}"),
        Error::RedirectExpired(_) => "found the download link expired".to_owned(),
        _ => "lost connection".to_owned(),
    }
}

/// A worker: repeatedly claim a segment (queued, else stolen) and fetch it,
/// until there is no work left, the stop signal is raised, or the pool is over
/// target. A failed or retired-from segment is handed back to the queue for
/// the remaining workers.
async fn worker(
    shared: &Shared<'_>,
    first: Option<reqwest::Response>,
    mut slot: Slot<'_>,
) -> (u8, Result<(), Failure>) {
    let lane = slot.lane;
    let result = work(shared, first, &mut slot).await;
    // However it ended, the lane is no longer on any segment.
    shared.table.lock().unwrap().release(lane);
    (lane, result)
}

/// Say that lane `number` left the pool in another lane's place, if it did.
fn say_left(number: u8, leave: Leave) {
    if let Leave::InsteadOf(bottom) = leave {
        tracing::info!(
            "Lane {} is still busy, so Lane {number} closes instead",
            bottom + 1
        );
    }
}

/// What lane `number` says when it claims segment `idx` of `count` (`seg` as
/// it is now), having last finished segment `finished`, if any.
fn claim_line(
    number: u8,
    finished: Option<usize>,
    idx: usize,
    count: usize,
    seg: Segment,
    how: Claim,
) -> String {
    let piece = idx + 1;
    let range = format!("{} – {}", human::bytes(seg.start), human::bytes(seg.end));
    let lead = match finished {
        Some(f) => format!("Lane {number} finished piece {}, now on", f + 1),
        None => format!("Lane {number} started"),
    };
    match how {
        Claim::Fresh => format!("{lead} piece {piece} of {count} ({range})"),
        Claim::HandedBack => format!(
            "{lead} piece {piece} of {count} from {}",
            human::bytes(seg.start + seg.done)
        ),
        Claim::Split { victim, from } => {
            let whose = match from {
                Some(lane) => format!("Lane {}'s piece", lane + 1),
                None => "piece".to_owned(),
            };
            format!(
                "Lane {number} took over the back half of {whose} {} ({range}) as piece {piece}",
                victim + 1
            )
        }
    }
}

/// [`worker`]'s loop, on `slot`'s lane.
async fn work(
    shared: &Shared<'_>,
    first: Option<reqwest::Response>,
    slot: &mut Slot<'_>,
) -> Result<(), Failure> {
    let mut stop = shared.stop.subscribe();
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(shared.part)
        .await?;
    let mut first = first.map(|resp| (0, resp));
    // Until its first claim, a worker may still be opening the file while a
    // sibling's segment is handed back after a 429.
    let mut fresh = first.is_none();
    let number = slot.number();
    let mut finished = None;
    tracing::debug!("Lane {number} opened");
    loop {
        if shared.is_stopping() {
            return Ok(());
        }
        let (idx, resp) = match first.take() {
            Some((idx, resp)) => {
                // The probe's response already holds this piece, so no claim
                // logs it; say so the same way.
                let (seg, count) = {
                    let table = shared.table.lock().unwrap();
                    (table.segments[idx], table.segments.len())
                };
                tracing::debug!(
                    "{}",
                    claim_line(number, None, idx, count, seg, Claim::Fresh)
                );
                (idx, Some(resp))
            }
            None => {
                let leave = slot.try_retire();
                if leave != Leave::Stay {
                    say_left(number, leave);
                    tracing::debug!("Lane {number} closed");
                    return Ok(());
                }
                let claimed = {
                    let mut table = shared.table.lock().unwrap();
                    table
                        .claim_for(shared.min, fresh, slot.lane)
                        .map(|(idx, how)| (idx, how, table.segments[idx], table.segments.len()))
                };
                let Some((idx, how, seg, count)) = claimed else {
                    tracing::debug!("Lane {number} done — nothing left to fetch");
                    return Ok(());
                };
                tracing::debug!("{}", claim_line(number, finished, idx, count, seg, how));
                (idx, None)
            }
        };
        fresh = false;
        match fetch_segment(shared, &mut file, idx, resp, &mut stop, slot).await {
            Ok(Outcome::Done) => finished = Some(idx),
            Ok(Outcome::Stopped) => return Ok(()),
            Ok(Outcome::Retired(leave)) => {
                let at = {
                    let mut table = shared.table.lock().unwrap();
                    table.owner[idx] = None;
                    let seg = table.segments[idx];
                    seg.start + seg.done
                };
                say_left(number, leave);
                tracing::debug!(
                    "Lane {number} closed; piece {} goes back in the queue at {}",
                    idx + 1,
                    human::bytes(at)
                );
                return Ok(());
            }
            Err(failure) => {
                let mut table = shared.table.lock().unwrap();
                table.owner[idx] = None;
                // Under the same lock as the hand-back, so no fresh worker
                // can claim the segment in between. This worker's slot is
                // still live, hence `> 1` for "others live".
                let others_live = shared.pool.live() > 1;
                if matches!(&failure, Failure::Engine(e)
                    if matches!(Severity::of(e, others_live), Severity::RateLimit(_)))
                {
                    table.rate_limited = true;
                }
                return Err(failure);
            }
        }
    }
}

/// Fetch segment `idx` until complete, stopped or retired. A response that
/// ends early after making progress is re-requested in place; one that made
/// none is [`Error::Incomplete`].
async fn fetch_segment(
    shared: &Shared<'_>,
    file: &mut tokio::fs::File,
    idx: usize,
    mut first: Option<reqwest::Response>,
    stop: &mut watch::Receiver<bool>,
    slot: &mut Slot<'_>,
) -> Result<Outcome, Failure> {
    loop {
        let (pos, end) = {
            let table = shared.table.lock().unwrap();
            let seg = table.segments[idx];
            (seg.start + seg.done, seg.end)
        };
        if pos >= end {
            return Ok(Outcome::Done);
        }
        let resp = match first.take() {
            Some(resp) => resp,
            None => {
                if shared.is_stopping() {
                    return Ok(Outcome::Stopped);
                }
                let mut request = shared
                    .client
                    .get(&shared.url)
                    .header(header::RANGE, format!("bytes={pos}-{}", end - 1));
                if let Some(validator) = &shared.validator {
                    request = request.header(header::IF_RANGE, validator);
                }
                let resp = tokio::select! {
                    resp = request.send() => resp?,
                    _ = tokio::time::sleep(shared.read_timeout) => return Err(Error::TimedOut.into()),
                    _ = stop.wait_for(|stopped| *stopped) => return Ok(Outcome::Stopped),
                };
                tracing::debug!(
                    "Lane {} requested bytes {pos}–{} · HTTP {}",
                    slot.number(),
                    end - 1,
                    resp.status().as_u16()
                );
                check_ranged(&resp, pos, end, shared.total, shared.redirected)?;
                let reconnected = std::mem::take(
                    &mut shared.lane_failures.lock().unwrap()[usize::from(slot.lane)],
                );
                if reconnected > 0 {
                    tracing::info!("Lane {} reconnected", slot.number());
                }
                resp
            }
        };

        file.seek(std::io::SeekFrom::Start(pos)).await?;
        let mut body = resp.bytes_stream();
        let mut progressed = false;
        loop {
            let next = tokio::select! {
                next = super::next_chunk(&mut body, shared.read_timeout) => next,
                _ = stop.wait_for(|stopped| *stopped) => return Ok(Outcome::Stopped),
            };
            let Some(chunk) = next else { break };
            let chunk = chunk?;

            // Control point: honoured once per chunk, before the write.
            if shared.control.poll() != Flow::Continue {
                shared.raise_stop();
                return Ok(Outcome::Stopped);
            }
            // The tail may have been stolen since the request was sent.
            let room = {
                let table = shared.table.lock().unwrap();
                table.segments[idx].remaining()
            };
            let n = (chunk.len() as u64).min(room);
            file.write_all(&chunk[..n as usize]).await?;
            file.flush().await?;
            // `done` only advances once the bytes are written, so a checkpoint
            // snapshot never claims data that isn't in the file.
            let complete = {
                let mut table = shared.table.lock().unwrap();
                let seg = &mut table.segments[idx];
                seg.done = (seg.done + n).min(seg.len());
                seg.is_complete()
            };
            progressed |= n > 0;

            // Throttle point: sleeps off rate-limit debt. A sibling's failure
            // raises `stop` without touching `Control`, so watch it too.
            let stopped = tokio::select! {
                _ = shared.control.throttle(n as usize, Some(&shared.waited)) => false,
                _ = stop.wait_for(|stopped| *stopped) => true,
            };
            // Retire point: only after the chunk is written and `done`
            // advanced, so the handed-back segment resumes exactly there.
            let outcome = if stopped {
                Some(Outcome::Stopped)
            } else if complete {
                Some(Outcome::Done)
            } else {
                match slot.try_retire() {
                    Leave::Stay => None,
                    leave => Some(Outcome::Retired(leave)),
                }
            };
            shared.progressed.notify_one();
            if let Some(outcome) = outcome {
                return Ok(outcome);
            }
        }
        if !progressed {
            return Err(Error::Incomplete(format!("bytes {pos}-{} ended early", end - 1)).into());
        }
    }
}

/// Validate a response to `Range: bytes=pos-(end-1)`. A full 200 or a 206 for
/// some other range means ranges can't be trusted; a short 206 is fine.
fn check_ranged(
    resp: &reqwest::Response,
    pos: u64,
    end: u64,
    total: u64,
    redirected: bool,
) -> Result<(), Failure> {
    let status = resp.status();
    if status == StatusCode::PARTIAL_CONTENT {
        let matches = resp
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_range)
            .is_some_and(|(first, last, len)| first == pos && last < end && len == total);
        return if matches {
            Ok(())
        } else {
            Err(Failure::RangeUnsupported)
        };
    }
    if status.is_success() {
        return Err(Failure::RangeUnsupported);
    }
    // Signed redirect targets (S3, release CDNs) expire mid-download; the
    // original URL mints a fresh one, so this is worth a retry from it.
    let expired = matches!(status.as_u16(), 401 | 403 | 404 | 410);
    if redirected && expired {
        return Err(Error::RedirectExpired(status.as_u16()).into());
    }
    Err(status_error(resp).into())
}

/// Start a periodic checkpoint of the current table on the blocking pool.
fn spawn_checkpoint(
    shared: &Shared<'_>,
    base: &DownloadState,
    sidecar: &Path,
) -> JoinHandle<crate::engine::Result<()>> {
    let mut st = base.clone();
    st.segments = Some(shared.snapshot());
    let part = shared.part.to_path_buf();
    let sidecar = sidecar.to_path_buf();
    tokio::task::spawn_blocking(move || checkpoint_blocking(&part, &sidecar, &st, || {}))
}

/// Periodic checkpoints are best-effort: a failure is logged and the next
/// tick tries again.
fn log_periodic_checkpoint(
    joined: std::result::Result<crate::engine::Result<()>, tokio::task::JoinError>,
) {
    match joined {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!(
            "Couldn't save progress, will try again: {}",
            crate::engine::error_detail(&e)
        ),
        Err(e) => tracing::warn!("Couldn't save progress, will try again: {e}"),
    }
}

/// Make `st` durable: sync the `.part` data first (the snapshot was taken
/// before this, so the sidecar never claims bytes that aren't on disk), then
/// save the sidecar if it still belongs to this download. If the `.part` is
/// gone (download removed) nothing is left behind. `after_save` is a test seam.
pub(crate) fn checkpoint_blocking(
    part: &Path,
    sidecar: &Path,
    st: &DownloadState,
    after_save: impl FnOnce(),
) -> crate::engine::Result<()> {
    if !sync_part(part)? {
        return Ok(());
    }
    save_unless_removed(part, sidecar, st, after_save)
}

/// The checkpoint once every segment is written: the data sync must succeed,
/// but a failed sidecar `save` is only logged, since finalizing needs nothing
/// from it beyond `total`, which the sidecar on disk already holds.
fn complete_checkpoint_blocking(
    part: &Path,
    save: impl FnOnce() -> crate::engine::Result<()>,
) -> crate::engine::Result<()> {
    if sync_part(part)? {
        if let Err(e) = save() {
            tracing::warn!(
                "Couldn't save final progress: {}",
                crate::engine::error_detail(&e)
            );
        }
    }
    Ok(())
}

/// Sync the `.part` data to disk; `false` if it is gone (download removed).
fn sync_part(part: &Path) -> crate::engine::Result<bool> {
    // Writable: on Windows, FlushFileBuffers needs write access.
    let file = match std::fs::OpenOptions::new().write(true).open(part) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    file.sync_data()?;
    Ok(true)
}

/// Save `st` if the sidecar is still this download's, removing the save again
/// if the `.part` vanished meanwhile.
fn save_unless_removed(
    part: &Path,
    sidecar: &Path,
    st: &DownloadState,
    after_save: impl FnOnce(),
) -> crate::engine::Result<()> {
    let wrote = st.save_checkpoint(sidecar)?;
    after_save();
    if wrote && !part.exists() {
        let _ = std::fs::remove_file(sidecar);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// `.part` and sidecar paths for a target inside `dir`.
    fn paths_in(dir: &Path) -> (PathBuf, PathBuf) {
        let target = dir.join("x.bin");
        (
            crate::engine::state::part_path(&target),
            crate::engine::state::sidecar_path(&target),
        )
    }

    fn seg(start: u64, end: u64, done: u64) -> Segment {
        Segment { start, end, done }
    }

    #[test]
    fn plan_splits_equally_with_remainder_in_last() {
        assert_eq!(
            plan(10, 3, 1),
            vec![seg(0, 3, 0), seg(3, 6, 0), seg(6, 10, 0)]
        );
        assert_eq!(
            plan(8, 4, 2),
            vec![seg(0, 2, 0), seg(2, 4, 0), seg(4, 6, 0), seg(6, 8, 0)]
        );
    }

    #[test]
    fn plan_respects_min_size_cap_and_single_connection() {
        // Only 2 segments of at least 4 bytes fit in 9.
        assert_eq!(plan(9, 8, 4), vec![seg(0, 4, 0), seg(4, 9, 0)]);
        // Smaller than one min segment → one segment.
        assert_eq!(plan(3, 4, 4), vec![seg(0, 3, 0)]);
        // Never more than MAX_CONNECTIONS.
        assert_eq!(plan(1000, 200, 1).len(), MAX_CONNECTIONS as usize);
        assert_eq!(plan(1000, 1, 1), vec![seg(0, 1000, 0)]);
    }

    #[test]
    fn steal_halves_the_largest_remaining_segment() {
        let mut segs = vec![seg(0, 100, 90), seg(100, 200, 20), seg(200, 300, 50)];
        assert_eq!(steal(&mut segs, 10), Some((1, 3)));
        // Segment 1 has 80 left from 120: split at 160.
        assert_eq!(segs[1], seg(100, 160, 20));
        assert_eq!(segs[3], seg(160, 200, 0));
    }

    #[test]
    fn steal_refuses_segments_under_twice_min() {
        let mut segs = vec![seg(0, 100, 81), seg(100, 200, 99)];
        assert_eq!(steal(&mut segs, 10), None);
        assert_eq!(segs.len(), 2);
        // Exactly 2×min left is stealable.
        let mut segs = vec![seg(0, 100, 80)];
        assert_eq!(steal(&mut segs, 10), Some((0, 1)));
        assert_eq!(segs, vec![seg(0, 90, 80), seg(90, 100, 0)]);
    }

    #[test]
    fn content_range_parsing() {
        assert_eq!(parse_content_range("bytes 0-99/200"), Some((0, 99, 200)));
        assert_eq!(parse_content_range("bytes 5-5/6"), Some((5, 5, 6)));
        assert_eq!(parse_content_range("bytes */200"), None);
        assert_eq!(parse_content_range("bytes 0-99/*"), None);
        assert_eq!(parse_content_range("bytes 10-5/200"), None);
        assert_eq!(parse_content_range("bytes 0-200/200"), None);
        assert_eq!(parse_content_range("items 0-1/2"), None);
    }

    #[test]
    fn weak_etags_are_detected() {
        assert!(is_strong_etag("\"abc\""));
        assert!(!is_strong_etag("W/\"abc\""));
    }

    #[test]
    fn claim_prefers_queued_then_steals() {
        let mut table = Table {
            segments: vec![seg(0, 100, 0), seg(100, 200, 100), seg(200, 300, 0)],
            owner: vec![Some(0), None, None],
            rate_limited: false,
        };
        // Segment 1 is complete, so the queued pick is segment 2.
        assert_eq!(table.claim(10, 1), Some((2, Claim::Fresh)));
        // Nothing queued: steal from the largest remaining (segment 0 wins ties).
        let split = Claim::Split {
            victim: 0,
            from: Some(0),
        };
        assert_eq!(table.claim(10, 2), Some((3, split)));
        assert_eq!(table.segments[0], seg(0, 50, 0));
        assert_eq!(table.owner, vec![Some(0), None, Some(1), Some(2)]);
        assert_eq!(table.lanes(), vec![Some(1), None, Some(2), Some(3)]);
    }

    /// A lane lets go of the segment it finished in the same step that hands
    /// it the next one, and a started segment nobody holds is handed back.
    #[test]
    fn a_lane_moves_from_one_segment_to_the_next() {
        let mut table = Table {
            segments: vec![seg(0, 100, 100), seg(100, 200, 40), seg(200, 300, 0)],
            owner: vec![Some(0), None, Some(1)],
            rate_limited: false,
        };
        assert_eq!(table.claim_for(10, false, 0), Some((1, Claim::HandedBack)));
        assert_eq!(table.owner, vec![None, Some(0), Some(1)]);
        table.release(1);
        assert_eq!(table.lanes(), vec![None, Some(1), None]);
    }

    #[test]
    fn rate_limit_holds_handed_back_work_from_fresh_workers_only() {
        let mut table = Table {
            segments: vec![seg(0, 100, 100), seg(100, 200, 0)],
            owner: vec![None, None],
            rate_limited: true,
        };
        // A worker still starting up would open a new connection: it waits
        // for the outer backoff instead.
        assert_eq!(table.claim_for(10, true, 1), None);
        assert_eq!(table.owner, vec![None, None]);
        // A live worker that finished its own segment may take it.
        assert_eq!(table.claim_for(10, false, 0), Some((1, Claim::Fresh)));

        table.rate_limited = false;
        table.owner[1] = None;
        assert_eq!(table.claim_for(10, true, 1), Some((1, Claim::Fresh)));
    }

    #[test]
    fn claim_lines_name_the_piece_and_where_it_starts() {
        let mib = 1024 * 1024;
        let piece = seg(2 * mib, 4 * mib, 0);
        assert_eq!(
            claim_line(2, None, 1, 4, piece, Claim::Fresh),
            "Lane 2 started piece 2 of 4 (2.00 MB – 4.00 MB)"
        );
        assert_eq!(
            claim_line(
                1,
                Some(0),
                1,
                4,
                seg(2 * mib, 4 * mib, mib),
                Claim::HandedBack
            ),
            "Lane 1 finished piece 1, now on piece 2 of 4 from 3.00 MB"
        );
        let split = Claim::Split {
            victim: 3,
            from: Some(1),
        };
        assert_eq!(
            claim_line(1, Some(2), 4, 5, seg(7 * mib, 8 * mib, 0), split),
            "Lane 1 took over the back half of Lane 2's piece 4 (7.00 MB – 8.00 MB) as piece 5"
        );
    }

    /// Lanes get the lowest free number, a replacement may keep its
    /// predecessor's, and a number is free again once its worker leaves.
    #[test]
    fn lanes_are_numbered_and_reused() {
        let pool = Pool::new(3, SHED_GRACE);
        let mut slots: Vec<_> = (0..3).map(|_| pool.join(None)).collect();
        assert_eq!(
            slots.iter().map(Slot::number).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(pool.join(Some(1)).number(), 4, "lane 2 is taken");
        drop(slots.remove(1));
        assert_eq!(pool.join(None).number(), 2, "the lowest free lane");
        drop(slots.remove(0));
        let kept = pool.join(Some(0));
        assert_eq!(kept.number(), 1, "a replacement keeps its lane");
        pool.set_target(1);
        assert_eq!(slots[0].try_retire(), Leave::Bottom, "lane 3 is the bottom");
        assert_eq!(
            pool.join(Some(2)).number(),
            3,
            "a retired lane is free again"
        );
    }

    #[test]
    fn retire_leaves_only_while_over_target_and_counts_once() {
        let pool = Pool::new(3, SHED_GRACE);
        assert!(pool.take_changed(), "the initial spawn is a pool change");
        let mut slots: Vec<_> = (0..3).map(|_| pool.join(None)).collect();
        assert_eq!(pool.live(), 3);
        assert!(pool.take_changed());
        assert_eq!(
            slots[2].try_retire(),
            Leave::Stay,
            "at target: nobody leaves"
        );

        pool.set_target(1);
        assert_eq!(slots[2].try_retire(), Leave::Bottom);
        assert_eq!(slots[2].try_retire(), Leave::Stay, "it already left");
        assert_eq!(pool.live(), 2);
        assert!(pool.take_changed());

        // The retired slot was already counted out when it left.
        slots.remove(2);
        assert_eq!(pool.live(), 2);
        assert!(!pool.take_changed());

        // A pending retire lapses if the pool shrinks some other way.
        slots.remove(1);
        assert_eq!(slots[0].try_retire(), Leave::Stay);
        drop(slots);
        assert_eq!(pool.live(), 0);
    }

    /// The bottom lane is the one shed, so Lane 1 always stays; the others
    /// leave only once the bottom one has had the grace to and hasn't.
    #[test]
    fn the_bottom_lane_leaves_first_unless_it_is_stuck() {
        let grace = Duration::from_secs(3);
        let pool = Pool::new(3, grace);
        let mut slots: Vec<_> = (0..3).map(|_| pool.join(None)).collect();
        pool.set_target(2);
        let t0 = Instant::now();

        assert_eq!(slots[0].try_retire_at(t0), Leave::Stay, "Lane 1 stays");
        assert_eq!(slots[1].try_retire_at(t0), Leave::Stay, "Lane 2 stays");
        // Lane 3 is stuck: no turn of its own comes. Within the grace the
        // others still wait for it...
        let early = t0 + grace - Duration::from_millis(1);
        assert_eq!(slots[1].try_retire_at(early), Leave::Stay);
        // ...after it, the next lane to check leaves in its place.
        assert_eq!(slots[1].try_retire_at(t0 + grace), Leave::InsteadOf(2));
        assert_eq!(pool.live(), 2);
        assert_eq!(
            slots[0].try_retire_at(t0 + grace),
            Leave::Stay,
            "one per step"
        );
        assert_eq!(slots[2].try_retire_at(t0 + grace), Leave::Stay);
        assert_eq!(slots[2].number(), 3, "the stuck lane keeps its number");
    }

    /// Past the grace, Lane 1 still waits for a middle lane to go in the stuck
    /// bottom lane's place, and goes itself only when it is the one other lane.
    #[test]
    fn lane_one_stands_in_for_a_stuck_lane_only_as_the_last_choice() {
        let grace = Duration::from_secs(3);
        let pool = Pool::new(3, grace);
        let mut slots: Vec<_> = (0..3).map(|_| pool.join(None)).collect();
        pool.set_target(2);
        let t0 = Instant::now();
        assert_eq!(slots[0].try_retire_at(t0), Leave::Stay);
        let late = t0 + grace;
        assert_eq!(slots[0].try_retire_at(late), Leave::Stay, "Lane 2 can go");
        assert_eq!(slots[1].try_retire_at(late), Leave::InsteadOf(2));

        pool.set_target(1);
        let t1 = late + Duration::from_millis(1);
        assert_eq!(slots[0].try_retire_at(t1), Leave::Stay);
        assert_eq!(
            slots[0].try_retire_at(t1 + grace),
            Leave::InsteadOf(2),
            "only Lane 1 is left to stand in"
        );
        assert_eq!(pool.live(), 1);
    }

    /// Shedding two lanes takes the bottom two, each in turn.
    #[test]
    fn each_shed_gives_the_bottom_lane_its_own_turn() {
        let grace = Duration::from_secs(3);
        let pool = Pool::new(4, grace);
        let mut slots: Vec<_> = (0..4).map(|_| pool.join(None)).collect();
        pool.set_target(2);
        let t0 = Instant::now();
        assert_eq!(slots[3].try_retire_at(t0), Leave::Bottom);
        // Lane 3 is now the bottom, and gets the grace from when Lane 4 left.
        let early = t0 + grace - Duration::from_millis(1);
        assert_eq!(slots[0].try_retire_at(early), Leave::Stay);
        assert_eq!(slots[2].try_retire_at(early), Leave::Bottom);
        assert_eq!(pool.live(), 2);
    }

    #[test]
    fn rate_limits_outrank_other_failures_and_caps_rank_lowest() {
        let of = |e: Error, others_live| Severity::of(&e, others_live);
        let after = |status, secs| Error::Throttled {
            status,
            retry_after: Duration::from_secs(secs),
        };
        let reset = || Error::Incomplete("reset".into());
        // A bare 503 alongside live connections is a cap, below anything else.
        assert_eq!(of(Error::Status(503), true), Severity::Cap);
        assert!(of(reset(), true) > of(Error::Status(503), true));
        // ...but to the last live connection it is a rate limit.
        assert_eq!(
            of(Error::Status(503), false),
            Severity::RateLimit(Duration::ZERO)
        );
        // A 429 or any Retry-After is a rate limit, live siblings or not.
        assert!(of(Error::Status(429), true) > of(reset(), true));
        assert_eq!(
            of(after(503, 5), true),
            Severity::RateLimit(Duration::from_secs(5))
        );
        // Among rate limits the longest wait wins.
        assert!(of(after(429, 30), true) > of(after(429, 5), true));
        assert!(of(after(429, 5), true) > of(Error::Status(429), true));
    }

    #[test]
    fn a_cap_outlasts_later_ramp_targets_and_only_tightens() {
        assert_eq!(capped(4, None), 4, "no cap leaves the ramp alone");
        let cap = lower_cap(None, 2);
        assert_eq!(capped(4, cap), 2, "a later ramp tick can't lift the cap");
        let cap = lower_cap(cap, 3);
        assert_eq!(cap, Some(2), "a later cap at 3 doesn't raise it");
        assert_eq!(capped(4, cap), 2);
        assert_eq!(capped(1, cap), 1, "the ramp can still go lower");
    }

    #[test]
    fn download_future_is_send() {
        fn assert_send<T: Send>(_: &T) {}
        let req = crate::DownloadRequest::new("http://127.0.0.1:1/x", ".");
        let control = Control::new();
        let fut = crate::download(&req, &control, |_| {});
        assert_send(&fut);
    }

    fn v2_state(dir: &Path, nonce: u64) -> DownloadState {
        DownloadState::segmented(
            "u",
            dir.join("x.bin"),
            10,
            Some("\"e\"".into()),
            None,
            vec![seg(0, 10, 4)],
            nonce,
        )
    }

    #[test]
    fn complete_checkpoint_tolerates_save_failure_but_not_sync_failure() {
        let dir = tempfile::tempdir().unwrap();
        let (part, _) = paths_in(dir.path());
        std::fs::write(&part, [0u8; 10]).unwrap();
        let failing_save = || Err(Error::Io(std::io::Error::other("disk full")));
        complete_checkpoint_blocking(&part, failing_save).expect("save failure is not fatal");

        // A `.part` that can't be synced (here: a directory) still fails.
        let unsyncable = dir.path().join("dir.part");
        std::fs::create_dir(&unsyncable).unwrap();
        assert!(complete_checkpoint_blocking(&unsyncable, || Ok(())).is_err());
    }

    #[test]
    fn checkpoint_skips_when_part_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let (part, sidecar) = paths_in(dir.path());
        checkpoint_blocking(&part, &sidecar, &v2_state(dir.path(), 1), || {}).unwrap();
        assert!(!sidecar.exists());
    }

    #[test]
    fn checkpoint_saves_and_deletes_its_save_if_part_vanishes() {
        let dir = tempfile::tempdir().unwrap();
        let (part, sidecar) = paths_in(dir.path());
        std::fs::write(&part, [0u8; 10]).unwrap();
        let st = v2_state(dir.path(), 7);
        st.save(&sidecar).unwrap();

        let mut next = st.clone();
        next.segments = Some(vec![seg(0, 10, 8)]);
        checkpoint_blocking(&part, &sidecar, &next, || {}).unwrap();
        assert_eq!(DownloadState::load(&sidecar).unwrap(), next);

        // The download is removed while the save is in flight.
        checkpoint_blocking(&part, &sidecar, &next, || {
            std::fs::remove_file(&part).unwrap();
            std::fs::remove_file(&sidecar).ok();
        })
        .unwrap();
        assert!(!sidecar.exists(), "no orphan sidecar after a removal race");

        // Variant: the `.part` vanishes but the save landed.
        std::fs::write(&part, [0u8; 10]).unwrap();
        st.save(&sidecar).unwrap();
        checkpoint_blocking(&part, &sidecar, &next, || {
            std::fs::remove_file(&part).unwrap();
        })
        .unwrap();
        assert!(
            !sidecar.exists(),
            "checkpoint removes its own orphaned save"
        );
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files left: {leftovers:?}");
    }
}
