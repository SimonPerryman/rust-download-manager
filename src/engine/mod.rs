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

mod attempt;
pub mod control;
mod download_log;
#[cfg(feature = "fault-injection")]
mod fault;
pub mod filename;
mod finish;
mod http;
mod human;
pub mod integrity;
pub mod limit;
mod lock;
pub mod manager;
pub mod progress;
mod ramp;
mod redact;
pub mod retry;
mod segment;
mod single;
pub mod state;
pub mod store;
pub mod tags;
mod tally;
mod wording;

pub use control::{Control, Flow};
pub use download_log::{LogLevel, LogLine, LOG_LINES};
pub(crate) use http::{status_error, throttle_wait};
pub use integrity::{Algo, Checksum};
pub use limit::{parse_rate, RateLimiter};
pub use manager::{AddOptions, DownloadSnapshot, Manager};
pub use progress::{Progress, SpeedMeter};
#[doc(hidden)]
pub use ramp::Ramping;
pub use redact::{map_urls_in, redact_url, redact_urls_in};
pub use retry::RetryPolicy;
pub use segment::Segment;
pub(crate) use single::single_segment;
pub use state::DownloadState;
pub use store::{ChecksumState, DownloadRecord, JsonStore, Settings, Status, Store, StoreData};

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use attempt::attempt_download;
use finish::{discard_partial, finalize};
use http::{backoff, interruption, is_transient, next_chunk};
use tally::Tally;
use wording::problem;

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
    shed_grace: Duration,
    read_timeout: Duration,
    tally: Arc<Tally>,
    active_before: Duration,
}

/// Default for [`DownloadRequest::connections`].
pub(crate) const DEFAULT_CONNECTIONS: u8 = 4;

/// Default for [`DownloadRequest::min_segment_size`].
const DEFAULT_MIN_SEGMENT_SIZE: u64 = 1024 * 1024;

/// Until the response headers arrive, a deadline from the request's start
/// (connect, TLS, redirects, time to headers); then, while the body is read, an
/// idle limit between reads (our own throttle sleeps don't count against it).
/// Never a total deadline, since downloads run long. Enforced by the engine
/// ([`send`](http::send), [`next_chunk`]) rather than reqwest's `read_timeout`, whose body
/// clock keeps running while we sleep between reads.
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
            shed_grace: segment::SHED_GRACE,
            read_timeout: READ_TIMEOUT,
            tally: Arc::default(),
            active_before: Duration::ZERO,
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
            shed_grace: segment::SHED_GRACE,
            read_timeout: READ_TIMEOUT,
            tally: Arc::default(),
            active_before: Duration::ZERO,
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

    /// Override how long the highest-numbered lane gets to close when the
    /// ramp sheds one, before another may (test seam; frontends never call this).
    #[doc(hidden)]
    pub fn with_shed_grace(mut self, grace: Duration) -> Self {
        self.shed_grace = grace;
        self
    }

    /// Override the per-read idle timeout (test seam; frontends never call this).
    #[doc(hidden)]
    pub fn with_read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = timeout;
        self
    }

    /// Share `tally` with this download's other runs, so its finish summary
    /// covers them all; `active_before` is their active time.
    pub(crate) fn with_history(mut self, tally: Arc<Tally>, active_before: Duration) -> Self {
        self.tally = tally;
        self.active_before = active_before;
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
    /// Built via `From`, which strips the URL so it can't leak into messages.
    #[error("request failed: {0}")]
    Http(reqwest::Error),
    /// The server sent nothing for the read timeout: no response headers in
    /// time, or a stalled body (transient: retried).
    #[error("timed out waiting for the server")]
    TimedOut,
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
    /// The finished file failed its size check.
    #[error("integrity check failed: {0}")]
    Integrity(String),
    /// The finished file's digest doesn't match the expected checksum.
    #[error("integrity check failed: {0}")]
    ChecksumMismatch(String),
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
    /// The download list (`state.json`) has a schema version newer than this
    /// build understands; it's left untouched rather than loaded and saved
    /// back without the fields this build doesn't know.
    #[error("download list was saved by a newer rdm (state version {0}); upgrade rdm to open it")]
    UnsupportedStore(u32),
    /// The caller asked for something the target download can't do now (e.g.
    /// verify one that isn't completed).
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// A bug in rdm: the download's task panicked (the panic message is
    /// carried). The download fails instead of taking the app down with it.
    #[error("internal error, please report it: {0}")]
    Internal(String),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e.without_url())
    }
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
///
/// Each event worth telling the user is logged as one short line (`Started ·
/// …`, `Paused`, `Completed · …`). Running the same request again, e.g. after a
/// pause, continues its finish summary: a request and its clones share it.
// Error level so the identity survives any filter that still shows errors.
#[tracing::instrument(name = "download", level = "error", skip_all, fields(url = %redact_url(&req.url)))]
pub async fn download(
    req: &DownloadRequest,
    control: &Control,
    mut on_progress: impl FnMut(Progress),
) -> Result<Summary> {
    #[cfg(feature = "fault-injection")]
    fault::panic_on_request(&req.url);
    install_crypto_provider();
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
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

    req.tally.begin_run(req.active_before);
    // Ends the run however `download` returns (or is dropped).
    let _run = RunGuard(&req.tally);
    let tally = req.tally.clone();
    let mut on_progress = move |p: Progress| {
        tally.observe_progress(p.downloaded);
        on_progress(p);
    };
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
                let finished = finalize(req, &final_path, bytes, status).await;
                match &finished {
                    Ok(summary) => {
                        tracing::info!("Saved to {}", summary.path.display());
                        tracing::info!("{}", req.tally.done_line(summary.bytes));
                    }
                    // `verify` has already said what didn't match.
                    Err(Error::ChecksumMismatch(_)) => {}
                    Err(e) => tracing::error!("Failed: {}", error_detail(e)),
                }
                return finished;
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
            match e {
                Error::Paused => tracing::info!("Paused"),
                Error::Cancelled => {
                    discard_partial(&known_target).await;
                    tracing::info!("Cancelled")
                }
                _ if attempt > 1 => {
                    tracing::error!("Failed after {attempt} attempts: {}", error_detail(&e))
                }
                _ => tracing::error!("Failed: {}", error_detail(&e)),
            }
            return Err(e);
        };
        req.tally.retried();
        tracing::debug!("Attempt {attempt} failed: {}", error_detail(&e));
        if throttle.is_some() {
            tracing::warn!("Server rate-limited — waiting {}", human::duration(delay));
        } else {
            tracing::warn!(
                "{} — retrying in {} (attempt {})",
                problem(&e),
                human::duration(delay),
                attempt + 1
            );
        }
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

/// Ends its run in the tally when dropped.
struct RunGuard<'a>(&'a Tally);

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.end_run();
    }
}

/// Make ring the process-wide rustls crypto provider, which reqwest needs
/// before it builds a client. Idempotent: a provider already installed (by an
/// earlier call or by the embedding app) is kept.
fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
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

/// The text a panic was raised with (`panic!` with a literal or a format
/// string); a panic with any other payload has none to give.
pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic with no message".to_owned())
}

/// The error for a blocking-pool task that didn't return: a panic in it is a
/// bug ([`Error::Internal`]); a cancelled one, an I/O failure.
pub(crate) fn join_error(err: tokio::task::JoinError) -> Error {
    if err.is_panic() {
        Error::Internal(panic_message(&*err.into_panic()))
    } else {
        Error::Io(std::io::Error::other(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_panicking_blocking_task_is_an_internal_error() {
        let joined = tokio::task::spawn_blocking(|| panic!("boom")).await;
        let err = join_error(joined.unwrap_err());
        assert!(matches!(&err, Error::Internal(m) if m == "boom"), "{err:?}");
    }

    #[test]
    fn crypto_provider_install_is_idempotent_and_lets_a_client_build() {
        install_crypto_provider();
        install_crypto_provider();
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
        reqwest::Client::builder()
            .build()
            .expect("a TLS-capable client builds once the provider is installed");
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
