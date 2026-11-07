//! Multi-segment parallel downloads.
//!
//! A known-size, range-capable resource is split into byte ranges that are
//! fetched concurrently into a pre-allocated `.part`. Each worker writes its
//! range at the right offset; per-segment progress is checkpointed into a v2
//! sidecar so a killed download re-fetches only the missing ranges. An idle
//! worker steals the back half of the largest remaining segment.
//!
//! The pool starts on one worker and ramps (see [`ramp`](super::ramp)): each
//! measurement window the coordinator may add a worker, or retire the last
//! trial one, which leaves at its next chunk boundary and hands its segment
//! back.
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
use crate::engine::{
    is_transient, progress, status_error, throttle_wait, Control, DownloadState, Error, Flow,
    Progress, SpeedMeter,
};

/// Hard cap on parallel connections per download.
pub(crate) const MAX_CONNECTIONS: u8 = 16;

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
/// the back half as a new segment and returning its index. Segments with less
/// than `2 * min` left are not worth a second connection.
pub(crate) fn steal(segments: &mut Vec<Segment>, min: u64) -> Option<usize> {
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
    Some(segments.len() - 1)
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
    pub ramp: Ramping,
    pub first: Option<reqwest::Response>,
}

/// The shared work table: segments plus which ones a worker currently owns.
struct Table {
    segments: Vec<Segment>,
    owned: Vec<bool>,
}

impl Table {
    /// Hand out an unowned unfinished segment, else steal one.
    fn claim(&mut self, min: u64) -> Option<usize> {
        let queued =
            (0..self.segments.len()).find(|&i| !self.owned[i] && !self.segments[i].is_complete());
        let idx = match queued {
            Some(i) => i,
            None => {
                let i = steal(&mut self.segments, min)?;
                self.owned.push(false);
                i
            }
        };
        self.owned[idx] = true;
        Some(idx)
    }

    fn done(&self) -> u64 {
        self.segments.iter().map(|s| s.done).sum()
    }

    /// Whether some unfinished segment has no worker (e.g. one handed back).
    fn has_unowned_work(&self) -> bool {
        self.segments
            .iter()
            .zip(&self.owned)
            .any(|(seg, &owned)| !owned && !seg.is_complete())
    }
}

/// Live-worker accounting: how many workers run, how many the ramp wants, and
/// whether the pool changed since the last ramp window.
struct Pool {
    live: AtomicU8,
    target: AtomicU8,
    changed: AtomicBool,
}

impl Pool {
    fn new(target: u8) -> Self {
        Self {
            live: AtomicU8::new(0),
            target: AtomicU8::new(target),
            // The first window holds the initial spawn.
            changed: AtomicBool::new(true),
        }
    }

    /// Count a new worker in; it is counted out when the slot drops.
    fn join(&self) -> Slot<'_> {
        self.live.fetch_add(1, Ordering::SeqCst);
        self.changed.store(true, Ordering::SeqCst);
        Slot {
            pool: self,
            held: true,
        }
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

/// One worker's place in the [`Pool`].
struct Slot<'a> {
    pool: &'a Pool,
    held: bool,
}

impl Slot<'_> {
    /// Leave the pool if it is over target. The check and the decrement are
    /// one CAS, so a pending retire lapses if the pool shrank some other way.
    fn try_retire(&mut self) -> bool {
        let pool = self.pool;
        let left = pool
            .live
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |live| {
                (live > pool.target()).then(|| live - 1)
            })
            .is_ok();
        if left {
            self.held = false;
            pool.changed.store(true, Ordering::SeqCst);
        }
        left
    }
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        if self.held {
            self.pool.live.fetch_sub(1, Ordering::SeqCst);
            self.pool.changed.store(true, Ordering::SeqCst);
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
    Retired,
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
    part: &'a Path,
    table: Mutex<Table>,
    progressed: Notify,
    stop: watch::Sender<bool>,
    pool: Pool,
    /// Nanoseconds workers have spent waiting on rate limits.
    waited: AtomicU64,
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
    let mut owned = vec![false; segments.len()];
    if job.first.is_some() {
        owned[0] = true;
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
        part: job.part,
        table: Mutex::new(Table { segments, owned }),
        progressed: Notify::new(),
        stop,
        pool: Pool::new(ramp.target()),
        waited: AtomicU64::new(0),
    };

    let mut meter = SpeedMeter::new();
    let started = Instant::now();
    let report = |meter: &mut SpeedMeter, on_progress: &mut dyn FnMut(Progress)| {
        // One lock, so the readout and the segments always agree.
        let (downloaded, segments) = {
            let table = shared.table.lock().unwrap();
            (table.done().min(total), table.segments.clone())
        };
        let bytes_per_sec = meter.observe(started.elapsed(), downloaded);
        on_progress(Progress {
            downloaded,
            total: Some(total),
            bytes_per_sec,
            eta: progress::eta(downloaded, Some(total), bytes_per_sec),
            connections: shared.pool.live(),
            segments,
        });
    };

    let mut first = job.first;
    let mut workers = FuturesUnordered::new();
    for _ in 0..ramp.target() {
        workers.push(worker(&shared, first.take(), shared.pool.join()));
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
            Some(result) = workers.next() => {
                if let Err(failure) = result {
                    match failure {
                        Failure::Engine(ref e) if is_transient(e) => {
                            tracing::debug!(error = %crate::engine::error_detail(e), "segment worker failed; segment re-queued");
                            let severity = Severity::of(e, shared.pool.live() > 0);
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
                                shared.pool.set_target(capped(ramp.target(), cap));
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
                            if !rate_limited && orphaned && workers.len() < wanted && replacements_left > 0 && !shared.is_stopping() {
                                replacements_left -= 1;
                                workers.push(worker(&shared, None, shared.pool.join()));
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
                tracing::debug!(?measured, ?decision, target = ramp.target(), "ramp window");
                match decision {
                    Decision::Add if !shared.is_stopping() && job.control.poll() == Flow::Continue => {
                        workers.push(worker(&shared, None, shared.pool.join()));
                        report(&mut meter, on_progress);
                    }
                    Decision::Add | Decision::Hold | Decision::Retire => {}
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
            tracing::info!("server does not honour ranges; falling back to one connection");
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

/// A worker: repeatedly claim a segment (queued, else stolen) and fetch it,
/// until there is no work left, the stop signal is raised, or the pool is over
/// target. A failed or retired-from segment is handed back to the queue for
/// the remaining workers.
async fn worker(
    shared: &Shared<'_>,
    first: Option<reqwest::Response>,
    mut slot: Slot<'_>,
) -> Result<(), Failure> {
    let mut stop = shared.stop.subscribe();
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(shared.part)
        .await?;
    let mut first = first.map(|resp| (0, resp));
    loop {
        if shared.is_stopping() {
            return Ok(());
        }
        let (idx, resp) = match first.take() {
            Some((idx, resp)) => (idx, Some(resp)),
            None => {
                if slot.try_retire() {
                    return Ok(());
                }
                let claimed = shared.table.lock().unwrap().claim(shared.min);
                match claimed {
                    Some(idx) => (idx, None),
                    None => return Ok(()),
                }
            }
        };
        match fetch_segment(shared, &mut file, idx, resp, &mut stop, &mut slot).await {
            Ok(Outcome::Done) => {}
            Ok(Outcome::Stopped) => return Ok(()),
            Ok(Outcome::Retired) => {
                shared.table.lock().unwrap().owned[idx] = false;
                return Ok(());
            }
            Err(failure) => {
                shared.table.lock().unwrap().owned[idx] = false;
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
                    _ = stop.wait_for(|stopped| *stopped) => return Ok(Outcome::Stopped),
                };
                check_ranged(&resp, pos, end, shared.total, shared.redirected)?;
                resp
            }
        };

        file.seek(std::io::SeekFrom::Start(pos)).await?;
        let mut body = resp.bytes_stream();
        let mut progressed = false;
        loop {
            let next = tokio::select! {
                next = body.next() => next,
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
            } else if slot.try_retire() {
                Some(Outcome::Retired)
            } else {
                None
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
        Ok(Err(e)) => tracing::warn!(error = %crate::engine::error_detail(&e), "checkpoint failed"),
        Err(e) => tracing::warn!(error = %e, "checkpoint task failed"),
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
            tracing::warn!(error = %crate::engine::error_detail(&e), "final checkpoint not saved");
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
        let idx = steal(&mut segs, 10).unwrap();
        assert_eq!(idx, 3);
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
        assert_eq!(steal(&mut segs, 10), Some(1));
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
            owned: vec![true, false, false],
        };
        // Segment 1 is complete, so the queued pick is segment 2.
        assert_eq!(table.claim(10), Some(2));
        // Nothing queued: steal from the largest remaining (segment 0 wins ties).
        assert_eq!(table.claim(10), Some(3));
        assert_eq!(table.segments[0], seg(0, 50, 0));
        assert_eq!(table.owned, vec![true, false, true, true]);
    }

    #[test]
    fn retire_leaves_only_while_over_target_and_counts_once() {
        let pool = Pool::new(3);
        assert!(pool.take_changed(), "the initial spawn is a pool change");
        let mut slots: Vec<_> = (0..3).map(|_| pool.join()).collect();
        assert_eq!(pool.live(), 3);
        assert!(pool.take_changed());
        assert!(!slots[0].try_retire(), "at target: nobody leaves");

        pool.set_target(2);
        assert!(slots[0].try_retire());
        assert!(!slots[1].try_retire(), "only one leaves per step");
        assert_eq!(pool.live(), pool.target());
        assert!(pool.take_changed());

        // The retired slot was already counted out when it left.
        slots.remove(0);
        assert_eq!(pool.live(), 2);
        assert!(!pool.take_changed());

        // A pending retire lapses if the pool shrinks some other way.
        pool.set_target(1);
        slots.remove(0);
        assert!(!slots[0].try_retire());
        drop(slots);
        assert_eq!(pool.live(), 0);
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
