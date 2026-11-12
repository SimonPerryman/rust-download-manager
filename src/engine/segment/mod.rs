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
//! Once the host has rate-limited this download, an attempt runs on one lane
//! that fetches everything left in a single request (see
//! [`Job::one_request`]), since every new request may cost another
//! `Retry-After`.
//!
//! Workers are futures polled by the coordinator inside the caller's task (no
//! spawned tasks), so the whole [`download`](crate::download) future stays
//! `Send` and every worker is drained before the coordinator returns.

mod checkpoint;
mod pool;
mod range;
mod severity;
mod table;
mod wording;
mod worker;

pub(crate) use range::is_strong_etag;
pub(crate) use table::plan;
pub use table::Segment;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::stream::FuturesUnordered;
use futures_util::StreamExt;
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;

use crate::engine::lock::lock;
use crate::engine::ramp::{Decision, Ramp, Ramping, Window};
use crate::engine::tally::Tally;
use crate::engine::{
    human, is_transient, progress, Control, DownloadState, Error, Flow, Progress, SpeedMeter,
};
use checkpoint::{
    checkpoint_blocking, complete_checkpoint_blocking, log_periodic_checkpoint,
    save_unless_removed, spawn_checkpoint,
};
use pool::Pool;
use severity::{capped, lower_cap, Severity};
use table::Table;
use wording::lane_problem;
use worker::worker;

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
    /// Fetch everything left in one request, on one lane, reading past (not
    /// rewriting) bytes already on disk: the host has rate-limited this download.
    pub one_request: bool,
    pub tally: &'a Tally,
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
    one_request: bool,
    part: &'a Path,
    table: Mutex<Table>,
    progressed: Notify,
    stop: watch::Sender<bool>,
    pool: Pool,
    /// Nanoseconds workers have spent waiting on rate limits.
    waited: AtomicU64,
    /// Bytes written and past their throttle wait: what the speed reading
    /// counts, since a written chunk still waiting would read as a burst.
    paced: AtomicU64,
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
        lock(&self.table).segments.clone()
    }

    fn downloaded(&self) -> u64 {
        lock(&self.table).done().min(self.total)
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
    let already: u64 = segments.iter().map(|s| s.done).sum();
    let mut owner = vec![None; segments.len()];
    if job.first.is_some() {
        // The first response serves segment 0, on the first lane.
        owner[0] = Some(0);
    }
    let ceiling = u8::try_from(job.workers)
        .unwrap_or(MAX_CONNECTIONS)
        .clamp(1, MAX_CONNECTIONS);
    let (mut ramp, window) = match job.ramp {
        _ if job.one_request => (Ramp::fixed(1), None),
        Ramping::Window(w) => (Ramp::new(ceiling), Some(w.max(Duration::from_millis(1)))),
        Ramping::Off => (Ramp::fixed(ceiling), None),
    };
    if job.one_request {
        tracing::info!("Server limits requests — fetching what's left in one request");
    }
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
        one_request: job.one_request,
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
        paced: AtomicU64::new(already),
        tally: job.tally,
        lane_failures: Mutex::new([0; LANES]),
    };

    let mut meter = SpeedMeter::new();
    job.control.restart_limiters();
    let started = Instant::now();
    let report = |meter: &mut SpeedMeter, on_progress: &mut dyn FnMut(Progress)| {
        // One lock, so the readout and the segments always agree.
        let (downloaded, segments, lanes) = {
            let table = lock(&shared.table);
            (
                table.done().min(total),
                table.segments.clone(),
                table.lanes(),
            )
        };
        let paced = shared.paced.load(Ordering::Relaxed);
        let bytes_per_sec = meter.observe(started.elapsed(), paced);
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
                    std::mem::take(&mut lock(&shared.lane_failures)[usize::from(lane)]);
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
                            let orphaned = lock(&shared.table).has_unowned_work();
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
                                    lock(&shared.lane_failures)[usize::from(slot.lane)] = failures;
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
        async move { blocking.await.map_err(crate::engine::join_error)? }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_future_is_send() {
        fn assert_send<T: Send>(_: &T) {}
        let req = crate::DownloadRequest::new("http://127.0.0.1:1/x", ".");
        let control = Control::new();
        let fut = crate::download(&req, &control, |_| {});
        assert_send(&fut);
    }
}
