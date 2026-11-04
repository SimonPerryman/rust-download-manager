//! The download engine: the crate's core, frontend-agnostic logic.
//!
//! [`download`] streams an HTTP(S) GET to a `.part` file with a JSON sidecar
//! for resume state, then verifies and renames it into place. The chunk loop
//! consults a [`Control`] before each chunk (cancel/pause) and runs a throttle
//! hook after each write (rate limiting). Transient failures are retried
//! with backoff, and retry and resume share one mechanism: each attempt picks
//! up from the on-disk state.
//!
//! A large file on a range-capable server with a stable validator is fetched as
//! several parallel byte ranges (see [`DownloadRequest::connections`]) written
//! into a pre-allocated `.part`, starting on one connection and adding more
//! while the measured speed keeps rising. A single-connection download resumes from the
//! `.part` size (v1 sidecar); a multi-segment one from the per-segment progress
//! checkpointed in its v2 sidecar, re-fetching only the missing ranges.

pub mod control;
pub mod filename;
pub mod integrity;
pub mod limit;
pub mod manager;
pub mod progress;
mod ramp;
pub mod retry;
mod segment;
pub mod state;
pub mod store;

pub use control::{Control, Flow};
pub use integrity::{Algo, Checksum};
pub use limit::{parse_rate, RateLimiter};
pub use manager::{AddOptions, DownloadSnapshot, Manager};
pub use progress::{Progress, SpeedMeter};
#[doc(hidden)]
pub use ramp::Ramping;
pub use retry::RetryPolicy;
pub use segment::Segment;
pub use state::DownloadState;
pub use store::{DownloadRecord, JsonStore, Settings, Status, Store, StoreData};

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header;
use reqwest::StatusCode;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use segment::Failure;

/// User-Agent sent with every request, e.g. `rdm/0.1.0`.
const USER_AGENT: &str = concat!("rdm/", env!("CARGO_PKG_VERSION"));

/// What to download and where to put it.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// The URL to fetch.
    pub url: String,
    /// Directory the file lands in when the name is derived from the response.
    pub dir: PathBuf,
    /// Explicit output path; when set it overrides both `dir` and any derived
    /// filename.
    pub out: Option<PathBuf>,
    /// Backoff policy applied to transient failures.
    pub retry: RetryPolicy,
    /// Optional checksum to verify the finished file against.
    pub checksum: Option<Checksum>,
    /// Optional hook to claim the derived target before anything is written;
    /// ignored when `out` is set.
    pub claim_target: Option<ClaimTarget>,
    /// Max parallel connections, added one at a time while speed keeps
    /// rising; 1 forces single-connection. Default 4; clamped to 1..=16 at use.
    pub connections: u8,
    /// Smallest segment worth its own connection (files under 2× this stay
    /// single). Default 1 MiB; a tuning knob, mainly for tests.
    pub min_segment_size: u64,
    ramp: Ramping,
    read_timeout: Duration,
}

/// Default for [`DownloadRequest::connections`].
pub(crate) const DEFAULT_CONNECTIONS: u8 = 4;

/// Default for [`DownloadRequest::min_segment_size`].
const DEFAULT_MIN_SEGMENT_SIZE: u64 = 1024 * 1024;

/// Until the response headers arrive, a deadline from the request's start
/// (connect, TLS, redirects, time to headers); then, while the body is read, an
/// idle limit between reads (our own throttle sleeps don't count against it).
/// Never a total deadline, since downloads run long.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Longest a connect may take; a host dropping our packets (e.g. a firewall
/// ban) otherwise hangs each attempt for minutes.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// A hook handed the target the engine derived from the first response
/// (`dir` + server/URL filename); its returned path is used instead. Lets a
/// caller that owns many downloads dedup paths while still honouring the
/// server's `Content-Disposition` name. Called at most once per [`download`].
#[derive(Clone)]
pub struct ClaimTarget(Arc<dyn Fn(&Path) -> PathBuf + Send + Sync>);

impl ClaimTarget {
    /// Wrap a function mapping the proposed target to the path to use.
    pub fn new(f: impl Fn(&Path) -> PathBuf + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    fn claim(&self, proposed: &Path) -> PathBuf {
        (self.0)(proposed)
    }
}

impl fmt::Debug for ClaimTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClaimTarget(..)")
    }
}

impl DownloadRequest {
    /// A request that saves into `dir` under a filename derived from the
    /// response.
    pub fn new(url: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            dir: dir.into(),
            out: None,
            retry: RetryPolicy::default(),
            checksum: None,
            claim_target: None,
            connections: DEFAULT_CONNECTIONS,
            min_segment_size: DEFAULT_MIN_SEGMENT_SIZE,
            ramp: Ramping::default(),
            read_timeout: READ_TIMEOUT,
        }
    }

    /// A request saved to an explicit path.
    pub fn to_path(url: impl Into<String>, out: impl Into<PathBuf>) -> Self {
        let out = out.into();
        Self {
            url: url.into(),
            dir: out.parent().map(PathBuf::from).unwrap_or_default(),
            out: Some(out),
            retry: RetryPolicy::default(),
            checksum: None,
            claim_target: None,
            connections: DEFAULT_CONNECTIONS,
            min_segment_size: DEFAULT_MIN_SEGMENT_SIZE,
            ramp: Ramping::default(),
            read_timeout: READ_TIMEOUT,
        }
    }

    /// Set the retry/backoff policy (builder).
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Require the finished file to match `checksum` (builder).
    pub fn with_checksum(mut self, checksum: Checksum) -> Self {
        self.checksum = Some(checksum);
        self
    }

    /// Let `claim` choose the final target from the derived one (builder);
    /// see [`ClaimTarget`].
    pub fn with_claim_target(
        mut self,
        claim: impl Fn(&Path) -> PathBuf + Send + Sync + 'static,
    ) -> Self {
        self.claim_target = Some(ClaimTarget::new(claim));
        self
    }

    /// Set the max parallel connections; 1 forces single-connection (builder).
    pub fn with_connections(mut self, n: u8) -> Self {
        self.connections = n;
        self
    }

    /// Set the smallest segment worth its own connection (builder).
    pub fn with_min_segment_size(mut self, bytes: u64) -> Self {
        self.min_segment_size = bytes;
        self
    }

    /// Override connection ramping (test seam; frontends never call this).
    #[doc(hidden)]
    pub fn with_ramp(mut self, ramp: Ramping) -> Self {
        self.ramp = ramp;
        self
    }

    /// Override the per-read idle timeout (test seam; frontends never call this).
    #[doc(hidden)]
    pub fn with_read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = timeout;
        self
    }

    fn effective_connections(&self) -> u8 {
        self.connections.clamp(1, segment::MAX_CONNECTIONS)
    }
}

/// Outcome of a successful download.
#[derive(Debug, Clone)]
pub struct Summary {
    /// Where the file was written (the actual final path after any
    /// collision-avoiding rename).
    pub path: PathBuf,
    /// Number of bytes written.
    pub bytes: u64,
    /// Final HTTP status code of the last attempt (206 for a multi-segment
    /// download).
    pub status: u16,
}

/// Errors the engine can return.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request itself failed (DNS, connection, TLS, mid-stream read, ...).
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// The server answered with a non-success status.
    #[error("server returned HTTP {0}")]
    Status(u16),
    /// The server refused with 429/503 and a `Retry-After` (seconds, capped at
    /// 60 s); transient, but retried no sooner than that.
    #[error("server returned HTTP {status} (rate limited; Retry-After {}s)", .retry_after.as_secs())]
    Throttled {
        /// The refusing status (429 or 503).
        status: u16,
        /// How long the server asked us to wait.
        retry_after: Duration,
    },
    /// Writing to disk failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The download was cancelled via its [`Control`]; partial state removed.
    #[error("download cancelled")]
    Cancelled,
    /// The download was paused via its [`Control`]; `.part` + sidecar kept.
    #[error("download paused")]
    Paused,
    /// The finished file failed a size or checksum check.
    #[error("integrity check failed: {0}")]
    Integrity(String),
    /// A response body ended before its byte range did (transient: retried).
    #[error("incomplete response: {0}")]
    Incomplete(String),
    /// A redirect target refused a ranged request, e.g. an expired signed URL
    /// (transient: retried via the original URL, which re-resolves it).
    #[error("redirect target refused a range request with HTTP {0}")]
    RedirectExpired(u16),
    /// The partial download's sidecar has a schema version newer than this
    /// build understands; its `.part` + sidecar are kept untouched.
    #[error(
        "partial download was saved by a newer rdm (sidecar version {0}); upgrade rdm to resume it"
    )]
    UnsupportedState(u32),
}

/// Engine result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Download `req.url` to disk, streaming the body chunk-by-chunk (never
/// buffering the whole file in memory). Resumes automatically when a `.part`
/// and sidecar already exist for the target, retries transient failures with
/// backoff, and verifies integrity before finalizing.
///
/// `on_progress` is invoked before the first byte and as bytes land (coalesced
/// across parallel connections), so a frontend can render a live readout. `control` steers the in-flight
/// download (cancel/pause).
pub async fn download(
    req: &DownloadRequest,
    control: &Control,
    mut on_progress: impl FnMut(Progress),
) -> Result<Summary> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(req.read_timeout)
        .build()?;

    // The target may not be known until the first response (derived filename),
    // but when it is known up front we can detect an existing resume. The
    // target is remembered once known so retries resume it instead of
    // re-claiming or re-fetching from scratch.
    let mut known_target = req.out.clone();
    if let Some(target) = &known_target {
        state::remove_stale_temps(&state::sidecar_path(target));
    }
    // Set once the server proves it can't serve ranges; later attempts stay
    // single-connection.
    let mut force_single = false;

    tracing::info!(url = %req.url, "download started");
    let mut attempt: u32 = 0;
    let mut single_attempts: u32 = 0;
    // Segmented attempts only count against the budget while they make no
    // progress, so a long download over a flaky link isn't capped by
    // `max_attempts` disconnects in total.
    let mut stalled_attempts: u32 = 0;
    let mut throttled_attempts: u32 = 0;
    loop {
        attempt += 1;
        let before = downloaded_so_far(&known_target);
        let mut segmented = false;
        let result = match interruption(control) {
            Some(e) => Err(e),
            None => {
                attempt_download(
                    &client,
                    req,
                    control,
                    &mut known_target,
                    &mut force_single,
                    &mut segmented,
                    &mut on_progress,
                )
                .await
            }
        };
        let e = match result {
            Ok((status, final_path, bytes)) => {
                return finalize(req, &final_path, bytes, status).await;
            }
            Err(e) => e,
        };
        let max_attempts = req.retry.max_attempts;
        let throttle = throttle_wait(&e);
        let delay = if !is_transient(&e) || max_attempts <= 1 {
            None
        } else if segmented {
            if downloaded_so_far(&known_target) > before {
                stalled_attempts = 0;
            } else {
                stalled_attempts += 1;
            }
            // Progress doesn't reset a rate limit's backoff, or the limit
            // turns into a request flood (and an IP ban).
            throttled_attempts = if throttle.is_some() {
                throttled_attempts + 1
            } else {
                0
            };
            let step = stalled_attempts.max(throttled_attempts).max(1);
            (stalled_attempts < max_attempts).then(|| req.retry.delay_for(step))
        } else {
            single_attempts += 1;
            (single_attempts < max_attempts).then(|| req.retry.delay_for(single_attempts))
        };
        let delay = delay.map(|d| d.max(throttle.unwrap_or_default()));
        let Some(delay) = delay else {
            if matches!(e, Error::Cancelled) {
                discard_partial(&known_target).await;
            }
            tracing::error!(attempts = attempt, error = %error_detail(&e), "download failed");
            return Err(e);
        };
        tracing::warn!(
            attempt,
            max_attempts,
            segmented,
            delay_ms = delay.as_millis() as u64,
            error = %error_detail(&e),
            "download attempt failed; retrying"
        );
        // A pause/cancel cuts this short; the next iteration reports it.
        backoff(control, delay).await;
    }
}

/// Bytes on disk for the target so far (0 while it is still unknown).
fn downloaded_so_far(known_target: &Option<PathBuf>) -> u64 {
    known_target
        .as_deref()
        .and_then(state::downloaded_bytes)
        .unwrap_or(0)
}

/// One attempt, re-run single-connection (without consuming a retry) if a
/// segmented run finds the server doesn't really honour ranges.
async fn attempt_download(
    client: &reqwest::Client,
    req: &DownloadRequest,
    control: &Control,
    known_target: &mut Option<PathBuf>,
    force_single: &mut bool,
    segmented: &mut bool,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(u16, PathBuf, u64)> {
    loop {
        let result = attempt_once(
            client,
            req,
            control,
            known_target,
            *force_single,
            segmented,
            on_progress,
        )
        .await;
        match result {
            Ok(done) => return Ok(done),
            Err(Failure::Engine(e)) => return Err(e),
            // The segmented run already discarded its sidecar and `.part`.
            Err(Failure::RangeUnsupported) => {
                *force_single = true;
                *segmented = false;
            }
        }
    }
}

/// What a previous run left on disk for a target.
enum Existing {
    /// A v1 sidecar: resume from the `.part` size.
    Single(DownloadState),
    /// A v2 sidecar with a full-size `.part`: resume the missing ranges.
    Segmented(DownloadState),
    /// Nothing usable.
    Absent,
}

/// Inspect `.part` + sidecar. A v2 `.part` is always pre-allocated to the full
/// size, so any other length means the pair is inconsistent and is discarded,
/// as is a `.part` whose sidecar is invalid (its contents can't be trusted).
/// A sidecar from a newer rdm is [`Error::UnsupportedState`] and kept.
fn existing_state(part: &Path, sidecar: &Path) -> Result<Existing> {
    if !(part.exists() && sidecar.exists()) {
        return Ok(Existing::Absent);
    }
    let st = match DownloadState::load(sidecar) {
        Ok(st) => st,
        Err(e @ Error::UnsupportedState(_)) => return Err(e),
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::InvalidData => {
            let _ = std::fs::remove_file(part);
            let _ = std::fs::remove_file(sidecar);
            return Ok(Existing::Absent);
        }
        Err(_) => return Ok(Existing::Absent),
    };
    if st.segments.is_none() {
        return Ok(Existing::Single(st));
    }
    let part_len = std::fs::metadata(part).ok().map(|m| m.len());
    if part_len.is_some() && part_len == st.total {
        Ok(Existing::Segmented(st))
    } else {
        let _ = std::fs::remove_file(part);
        let _ = std::fs::remove_file(sidecar);
        Ok(Existing::Absent)
    }
}

/// The total size, if this fresh response can be fetched as parallel ranges:
/// a plain 200 of known size, ranges advertised, a validator to pin the bytes
/// with `If-Range`, and big enough for at least two segments.
fn segmentable(req: &DownloadRequest, resp: &reqwest::Response) -> Option<u64> {
    let headers = resp.headers();
    let accepts_ranges = header_str(headers, header::ACCEPT_RANGES)
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("bytes"));
    let strong_etag =
        header_str(headers, header::ETAG).is_some_and(|e| segment::is_strong_etag(&e));
    let validated = strong_etag || headers.contains_key(header::LAST_MODIFIED);
    let total = resp.content_length()?;
    let eligible = req.effective_connections() > 1
        && resp.status() == StatusCode::OK
        && !headers.contains_key(header::CONTENT_ENCODING)
        && accepts_ranges
        && validated
        && total >= req.min_segment_size.saturating_mul(2);
    eligible.then_some(total)
}

/// One connect+stream attempt. Returns `(status, part_path, part_len)` on a
/// completed stream (body fully read), or an error to be classified by the
/// retry loop. `segmented` is set when the attempt ran multi-segment.
async fn attempt_once(
    client: &reqwest::Client,
    req: &DownloadRequest,
    control: &Control,
    known_target: &mut Option<PathBuf>,
    force_single: bool,
    segmented: &mut bool,
    on_progress: &mut impl FnMut(Progress),
) -> std::result::Result<(u16, PathBuf, u64), Failure> {
    // If we already know the target and a prior `.part`+sidecar exist, resume.
    if let Some(target) = known_target.as_deref() {
        let part = state::part_path(target);
        let sidecar = state::sidecar_path(target);
        match existing_state(&part, &sidecar)? {
            Existing::Single(st) => {
                return Ok(resume(
                    client,
                    req,
                    control,
                    target,
                    &part,
                    &sidecar,
                    st,
                    on_progress,
                )
                .await?);
            }
            Existing::Segmented(st) => {
                *segmented = true;
                return resume_segmented(client, req, control, &part, &sidecar, st, on_progress)
                    .await;
            }
            Existing::Absent => {}
        }
    }

    // FRESH: issue a plain GET so we can learn the (post-redirect) URL and the
    // derived filename before committing to a target path.
    let resp = send(control, client.get(&req.url)).await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(status_error(&resp).into());
    }

    let final_url = resp.url().clone();
    let content_disposition = header_str(resp.headers(), header::CONTENT_DISPOSITION);
    let total = resp.content_length();
    let etag = header_str(resp.headers(), header::ETAG);
    let last_modified = header_str(resp.headers(), header::LAST_MODIFIED);

    let target = match known_target.as_ref() {
        Some(known) => known.clone(),
        None => {
            let name = filename::resolve(None, final_url.as_str(), content_disposition.as_deref());
            let derived = req.dir.join(name);
            let chosen = match &req.claim_target {
                Some(claim) => claim.claim(&derived),
                None => derived,
            };
            state::remove_stale_temps(&state::sidecar_path(&chosen));
            *known_target = Some(chosen.clone());
            chosen
        }
    };
    ensure_parent(&target).await?;

    let part = state::part_path(&target);
    let sidecar = state::sidecar_path(&target);

    // A resumable sidecar may exist even when the target wasn't known up front
    // (derived filename). If so, and the resource is unchanged, resume rather
    // than restart. Otherwise write a fresh sidecar BEFORE any bytes stream.
    match existing_state(&part, &sidecar)? {
        Existing::Single(st) => {
            return Ok(resume(
                client,
                req,
                control,
                &target,
                &part,
                &sidecar,
                st,
                on_progress,
            )
            .await?);
        }
        Existing::Segmented(st) => {
            *segmented = true;
            return resume_segmented(client, req, control, &part, &sidecar, st, on_progress).await;
        }
        Existing::Absent => {}
    }

    if !force_single {
        if let Some(total) = segmentable(req, &resp) {
            *segmented = true;
            let segments = segment::plan(total, req.effective_connections(), req.min_segment_size);
            let workers = segments.len();
            let st = DownloadState::segmented(
                &req.url,
                &target,
                total,
                etag,
                last_modified,
                segments,
                state::new_nonce(),
            );
            st.save(&sidecar)?;
            let file = tokio::fs::File::create(&part).await?;
            file.set_len(total).await?;
            drop(file);

            let job = segment::Job {
                client,
                control,
                url: final_url.to_string(),
                redirected: reqwest::Url::parse(&req.url).ok().as_ref() != Some(&final_url),
                part: &part,
                sidecar: &sidecar,
                state: st,
                workers,
                min_segment_size: req.min_segment_size,
                ramp: req.ramp,
                first: Some(resp),
            };
            let bytes = segment::run(job, on_progress).await?;
            return Ok((StatusCode::PARTIAL_CONTENT.as_u16(), part, bytes));
        }
    }

    // Truncate any stale `.part` before the v1 sidecar lands: a kill in between
    // must not leave a sidecar that resumes from leftover bytes.
    let file = tokio::fs::File::create(&part).await?;
    let st = DownloadState::new(&req.url, &target, total, etag, last_modified);
    st.save(&sidecar)?;

    let bytes = stream_to_part(resp, file, control, &part, &sidecar, 0, total, on_progress).await?;
    Ok((status.as_u16(), part, bytes))
}

/// Resume a multi-segment download from its v2 sidecar: fetch only the
/// missing ranges, with up to `connections` workers (redirects re-followed
/// from the original URL).
async fn resume_segmented(
    client: &reqwest::Client,
    req: &DownloadRequest,
    control: &Control,
    part: &Path,
    sidecar: &Path,
    st: DownloadState,
    on_progress: &mut impl FnMut(Progress),
) -> std::result::Result<(u16, PathBuf, u64), Failure> {
    let job = segment::Job {
        client,
        control,
        url: st.url.clone(),
        redirected: false,
        part,
        sidecar,
        state: st,
        workers: usize::from(req.effective_connections()),
        min_segment_size: req.min_segment_size,
        ramp: req.ramp,
        first: None,
    };
    let bytes = segment::run(job, on_progress).await?;
    Ok((
        StatusCode::PARTIAL_CONTENT.as_u16(),
        part.to_path_buf(),
        bytes,
    ))
}

/// Resume an existing `.part`. Offset is the live `.part` size. A genuine 206
/// appends; a 200 means restart (changed resource or a range-ignoring/lying
/// server); a 416 means already-complete-or-restart.
#[allow(clippy::too_many_arguments)]
async fn resume(
    client: &reqwest::Client,
    req: &DownloadRequest,
    control: &Control,
    target: &Path,
    part: &Path,
    sidecar: &Path,
    st: DownloadState,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(u16, PathBuf, u64)> {
    let offset = tokio::fs::metadata(part).await?.len();

    let mut builder = client
        .get(&st.url)
        .header(header::RANGE, format!("bytes={offset}-"));
    if let Some(validator) = st.if_range() {
        builder = builder.header(header::IF_RANGE, validator);
    }
    let resp = send(control, builder).await?;
    let status = resp.status();

    match status {
        StatusCode::PARTIAL_CONTENT => {
            // Resource unchanged: append from the current offset.
            let total = st.total;
            let mut file = tokio::fs::OpenOptions::new()
                .append(true)
                .open(part)
                .await?;
            file.seek(std::io::SeekFrom::End(0)).await?;
            let bytes = stream_to_part(
                resp,
                file,
                control,
                part,
                sidecar,
                offset,
                total,
                on_progress,
            )
            .await?;
            Ok((status.as_u16(), part.to_path_buf(), bytes))
        }
        StatusCode::OK => {
            // 200 to a Range request: the resource changed, or the server
            // ignores/lies about ranges. Only a true 206 is safe to append, so
            // truncate and restart with fresh validators.
            let total = resp.content_length();
            let etag = header_str(resp.headers(), header::ETAG);
            let last_modified = header_str(resp.headers(), header::LAST_MODIFIED);
            // Truncate before the new validators land, as on the fresh path.
            let file = tokio::fs::File::create(part).await?;
            let fresh = DownloadState::new(&st.url, target, total, etag, last_modified);
            fresh.save(sidecar)?;

            let bytes =
                stream_to_part(resp, file, control, part, sidecar, 0, total, on_progress).await?;
            Ok((status.as_u16(), part.to_path_buf(), bytes))
        }
        StatusCode::RANGE_NOT_SATISFIABLE => {
            // Offset past the end. If we already have the whole thing, we're
            // done; otherwise the `.part` is inconsistent — restart.
            if let Some(total) = st.total {
                if offset >= total {
                    on_progress(Progress {
                        downloaded: offset,
                        total: Some(total),
                        bytes_per_sec: 0.0,
                        eta: None,
                        connections: 0,
                    });
                    return Ok((status.as_u16(), part.to_path_buf(), offset));
                }
            }
            tokio::fs::File::create(part).await?; // truncate
            restart(client, req, control, target, part, sidecar, on_progress).await
        }
        _ => Err(status_error(&resp)),
    }
}

/// Fresh GET + stream after a 416-triggered restart (target already resolved).
#[allow(clippy::too_many_arguments)]
async fn restart(
    client: &reqwest::Client,
    _req: &DownloadRequest,
    control: &Control,
    target: &Path,
    part: &Path,
    sidecar: &Path,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(u16, PathBuf, u64)> {
    let url = {
        // Re-read the URL from the sidecar (the request's url matches it).
        let st = DownloadState::load(sidecar)?;
        st.url
    };
    let resp = send(control, client.get(&url)).await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(status_error(&resp));
    }
    let total = resp.content_length();
    let etag = header_str(resp.headers(), header::ETAG);
    let last_modified = header_str(resp.headers(), header::LAST_MODIFIED);
    let fresh = DownloadState::new(&url, target, total, etag, last_modified);
    fresh.save(sidecar)?;

    let file = tokio::fs::File::create(part).await?;
    let bytes = stream_to_part(resp, file, control, part, sidecar, 0, total, on_progress).await?;
    Ok((status.as_u16(), part.to_path_buf(), bytes))
}

/// Stream a response body into an open `.part` file, honouring cancel/pause and
/// the throttle hook. `start_offset` is how many bytes are already on disk (for
/// progress accounting); the returned count is the total `.part` length.
///
/// On cancel the `.part` + `sidecar` are removed (download discarded); on pause
/// the `.part` is flushed and both are kept so it can be resumed.
#[allow(clippy::too_many_arguments)]
async fn stream_to_part(
    resp: reqwest::Response,
    mut file: tokio::fs::File,
    control: &Control,
    part: &Path,
    sidecar: &Path,
    start_offset: u64,
    total: Option<u64>,
    on_progress: &mut impl FnMut(Progress),
) -> Result<u64> {
    let mut downloaded = start_offset;
    let mut meter = SpeedMeter::new();
    let start = Instant::now();

    on_progress(Progress {
        downloaded,
        total,
        bytes_per_sec: 0.0,
        eta: None,
        connections: 1,
    });

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        // Control point: honoured once per chunk, before the write.
        match control.poll() {
            Flow::Cancel => {
                drop(file);
                let _ = tokio::fs::remove_file(part).await;
                let _ = tokio::fs::remove_file(sidecar).await;
                state::remove_stale_temps(sidecar);
                return Err(Error::Cancelled);
            }
            Flow::Pause => {
                file.flush().await?;
                return Err(Error::Paused);
            }
            Flow::Continue => {}
        }

        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        // Throttle point: sleeps off rate-limit debt; pause/cancel cut it short.
        control.throttle(chunk.len(), None).await;

        let bytes_per_sec = meter.observe(start.elapsed(), downloaded);
        on_progress(Progress {
            downloaded,
            total,
            bytes_per_sec,
            eta: progress::eta(downloaded, total, bytes_per_sec),
            connections: 1,
        });
    }

    file.flush().await?;
    Ok(downloaded)
}

/// Verify the finished `.part` (size, then checksum), rename it to a
/// collision-free final path, and delete the sidecar.
async fn finalize(req: &DownloadRequest, part: &Path, bytes: u64, status: u16) -> Result<Summary> {
    // The sidecar carries the authoritative expected total (survives restarts).
    let sidecar = sidecar_for_part(part);
    let expected_total = DownloadState::load(&sidecar).ok().and_then(|s| s.total);

    integrity::verify_size(bytes, expected_total)?;
    if let Some(checksum) = &req.checksum {
        checksum.verify(part).await?;
    }

    let target = target_for_part(part);
    let final_path = filename::unique_path(&target);
    tokio::fs::rename(part, &final_path).await?;
    let _ = tokio::fs::remove_file(&sidecar).await;
    state::remove_stale_temps(&sidecar);

    Ok(Summary {
        path: final_path,
        bytes,
        status,
    })
}

/// Strip the trailing `.part` to recover the target path.
fn target_for_part(part: &Path) -> PathBuf {
    let s = part.to_string_lossy();
    match s.strip_suffix(".part") {
        Some(base) => PathBuf::from(base),
        None => part.to_path_buf(),
    }
}

/// The sidecar path matching a `.part` path.
fn sidecar_for_part(part: &Path) -> PathBuf {
    state::sidecar_path(&target_for_part(part))
}

/// Read a header value as an owned `String`, if present and valid UTF-8.
fn header_str(headers: &header::HeaderMap, name: header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// Create the parent directory of `path` if it isn't the cwd.
async fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    Ok(())
}

/// Whether an error is worth retrying: transport hiccups and 5xx/408/429.
fn is_transient(err: &Error) -> bool {
    match err {
        Error::Http(e) => {
            e.is_timeout() || e.is_connect() || e.is_request() || e.is_body() || e.is_decode()
        }
        Error::Status(code) => *code >= 500 || *code == 408 || *code == 429,
        Error::Throttled { .. } => true,
        Error::Incomplete(_) | Error::RedirectExpired(_) => true,
        Error::Io(_)
        | Error::Cancelled
        | Error::Paused
        | Error::Integrity(_)
        | Error::UnsupportedState(_) => false,
    }
}

/// Longest `Retry-After` honoured, so a confused host can't park a download.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

/// The error for a non-success response; see [`status_error_for`].
pub(crate) fn status_error(resp: &reqwest::Response) -> Error {
    status_error_for(resp.status().as_u16(), resp.headers())
}

/// [`Error::Throttled`] for a 429/503 with a non-zero `Retry-After` in seconds
/// (capped at [`MAX_RETRY_AFTER`]; the HTTP-date form is ignored), else
/// [`Error::Status`].
fn status_error_for(status: u16, headers: &header::HeaderMap) -> Error {
    let retry_after = header_str(headers, header::RETRY_AFTER)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&secs| secs > 0)
        .map(|secs| Duration::from_secs(secs).min(MAX_RETRY_AFTER));
    match retry_after {
        Some(retry_after) if matches!(status, 429 | 503) => Error::Throttled {
            status,
            retry_after,
        },
        _ => Error::Status(status),
    }
}

/// Pause/cancel as the error the engine returns for it, if one is pending.
fn interruption(control: &Control) -> Option<Error> {
    match control.poll() {
        Flow::Cancel => Some(Error::Cancelled),
        Flow::Pause => Some(Error::Paused),
        Flow::Continue => None,
    }
}

/// Send a request, cut short by pause/cancel: a host that never answers would
/// otherwise hold the download until the read timeout.
async fn send(control: &Control, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
    let response = request.send();
    tokio::pin!(response);
    let mut poll = tokio::time::interval(segment::CONTROL_POLL_INTERVAL);
    loop {
        tokio::select! {
            resp = &mut response => return Ok(resp?),
            _ = poll.tick() => {
                if let Some(e) = interruption(control) {
                    return Err(e);
                }
            }
        }
    }
}

/// Sleep off a retry delay, waking early on pause/cancel.
async fn backoff(control: &Control, delay: Duration) {
    let until = tokio::time::Instant::now() + delay;
    while control.poll() == Flow::Continue {
        let now = tokio::time::Instant::now();
        if now >= until {
            break;
        }
        tokio::time::sleep_until(until.min(now + segment::CONTROL_POLL_INTERVAL)).await;
    }
}

/// Remove a cancelled download's `.part`, sidecar and temps, once its target
/// is known.
async fn discard_partial(known_target: &Option<PathBuf>) {
    if let Some(target) = known_target {
        let sidecar = state::sidecar_path(target);
        let _ = tokio::fs::remove_file(state::part_path(target)).await;
        let _ = tokio::fs::remove_file(&sidecar).await;
        state::remove_stale_temps(&sidecar);
    }
}

/// For a 429/503 (the host is shedding load), how long it asked us to wait.
pub(crate) fn throttle_wait(err: &Error) -> Option<Duration> {
    match err {
        Error::Status(429 | 503) => Some(Duration::ZERO),
        Error::Throttled { retry_after, .. } => Some(*retry_after),
        _ => None,
    }
}

/// Format an error with its full `source()` chain, so a frontend surfaces the
/// real underlying cause (e.g. a DNS or connection failure) rather than just
/// the top-level message. De-duplicates the leading segment that `thiserror`'s
/// `{0}` already embeds in the outer `Display`.
pub(crate) fn error_detail(err: &dyn std::error::Error) -> String {
    let mut msg = err.to_string();
    let mut source = err.source();
    while let Some(e) = source {
        let s = e.to_string();
        if !msg.ends_with(&s) {
            msg.push_str(": ");
            msg.push_str(&s);
        }
        source = e.source();
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_and_sidecar_derive_from_part() {
        let part = Path::new("/tmp/archive.zip.part");
        assert_eq!(target_for_part(part), Path::new("/tmp/archive.zip"));
        assert_eq!(sidecar_for_part(part), Path::new("/tmp/archive.zip.rdm"));
    }

    #[test]
    fn transient_classification() {
        assert!(is_transient(&Error::Status(503)));
        assert!(is_transient(&Error::Status(429)));
        assert!(is_transient(&Error::Status(408)));
        assert!(is_transient(&Error::Throttled {
            status: 429,
            retry_after: Duration::ZERO,
        }));
        assert!(is_transient(&Error::Incomplete("x".into())));
        assert!(is_transient(&Error::RedirectExpired(403)));
        assert!(!is_transient(&Error::Status(404)));
        assert!(!is_transient(&Error::Cancelled));
        assert!(!is_transient(&Error::Paused));
        assert!(!is_transient(&Error::Integrity("x".into())));
        assert!(!is_transient(&Error::UnsupportedState(3)));
    }

    #[test]
    fn retry_after_seconds_are_honoured_and_capped() {
        let with = |status, retry_after: &str| {
            let mut headers = header::HeaderMap::new();
            headers.insert(header::RETRY_AFTER, retry_after.parse().unwrap());
            status_error_for(status, &headers)
        };
        let throttled = |secs| Error::Throttled {
            status: 503,
            retry_after: Duration::from_secs(secs),
        };
        assert_eq!(with(503, "5").to_string(), throttled(5).to_string());
        assert!(matches!(
            with(429, "3600"),
            Error::Throttled { status: 429, retry_after } if retry_after == MAX_RETRY_AFTER
        ));
        assert!(matches!(
            with(429, "Wed, 21 Oct 2015 07:28:00 GMT"),
            Error::Status(429)
        ));
        assert!(matches!(with(429, "0"), Error::Status(429)));
        assert!(matches!(with(404, "5"), Error::Status(404)));
        assert!(matches!(
            status_error_for(503, &header::HeaderMap::new()),
            Error::Status(503)
        ));
        assert_eq!(
            throttled(60).to_string(),
            "server returned HTTP 503 (rate limited; Retry-After 60s)"
        );
    }

    #[test]
    fn error_detail_walks_and_dedups_chain() {
        use std::fmt;

        #[derive(Debug)]
        struct Dns;
        impl fmt::Display for Dns {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "dns error")
            }
        }
        impl std::error::Error for Dns {}

        #[derive(Debug)]
        struct Connect(Dns);
        impl fmt::Display for Connect {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "connect failed")
            }
        }
        impl std::error::Error for Connect {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        // `Outer` embeds its source's `Display` (as `thiserror`'s `{0}` does)
        // and also exposes it via `source()`.
        #[derive(Debug)]
        struct Outer(Connect);
        impl fmt::Display for Outer {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "request failed: {}", self.0)
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        // The embedded "connect failed" is not repeated; the deeper "dns error"
        // cause is appended.
        assert_eq!(
            error_detail(&Outer(Connect(Dns))),
            "request failed: connect failed: dns error"
        );
    }
}
