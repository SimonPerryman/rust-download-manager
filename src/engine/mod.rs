//! The download engine: the crate's core, frontend-agnostic logic.
//!
//! [`download`] streams a single HTTP(S) GET to a `.part` file with a JSON
//! sidecar for resume state, then verifies and renames it into place. The chunk
//! loop consults a [`Control`] before each chunk (cancel/pause) and runs a
//! throttle hook after each write (rate limiting, M4). Transient failures are
//! retried with backoff; each attempt re-derives its offset from the `.part`
//! size, so retry and resume share one mechanism.

pub mod control;
pub mod filename;
pub mod integrity;
pub mod manager;
pub mod progress;
pub mod retry;
pub mod state;
pub mod store;

pub use control::{Control, Flow};
pub use integrity::{Algo, Checksum};
pub use manager::{DownloadSnapshot, Manager};
pub use progress::{Progress, SpeedMeter};
pub use retry::RetryPolicy;
pub use state::DownloadState;
pub use store::{DownloadRecord, JsonStore, Status, Store, StoreData};

use std::path::{Path, PathBuf};
use std::time::Instant;

use futures_util::StreamExt;
use reqwest::header;
use reqwest::StatusCode;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

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
}

/// Outcome of a successful download.
#[derive(Debug, Clone)]
pub struct Summary {
    /// Where the file was written (the actual final path after any
    /// collision-avoiding rename).
    pub path: PathBuf,
    /// Number of bytes written.
    pub bytes: u64,
    /// Final HTTP status code of the last attempt.
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
}

/// Engine result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Download `req.url` to disk, streaming the body chunk-by-chunk (never
/// buffering the whole file in memory). Resumes automatically when a `.part`
/// and sidecar already exist for the target, retries transient failures with
/// backoff, and verifies integrity before finalizing.
///
/// `on_progress` is invoked once before the first byte and after every chunk,
/// so a frontend can render a live readout. `control` steers the in-flight
/// download (cancel/pause).
pub async fn download(
    req: &DownloadRequest,
    control: &Control,
    mut on_progress: impl FnMut(Progress),
) -> Result<Summary> {
    let client = reqwest::Client::builder().user_agent(USER_AGENT).build()?;

    // The target may not be known until the first response (derived filename),
    // but when it is known up front we can detect an existing resume.
    let known_target = req.out.clone();

    tracing::info!(url = %req.url, "download started");
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        match attempt_download(&client, req, control, &known_target, &mut on_progress).await {
            Ok((status, final_path, bytes)) => {
                return finalize(req, &final_path, bytes, status).await;
            }
            Err(e) => {
                if is_transient(&e) && attempt < req.retry.max_attempts {
                    let delay = req.retry.delay_for(attempt);
                    tracing::warn!(
                        attempt,
                        max_attempts = req.retry.max_attempts,
                        delay_ms = delay.as_millis() as u64,
                        error = %error_detail(&e),
                        "download attempt failed; retrying"
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }
                tracing::error!(attempts = attempt, error = %error_detail(&e), "download failed");
                return Err(e);
            }
        }
    }
}

/// One connect+stream attempt. Returns `(status, part_path, part_len)` on a
/// completed stream (body fully read), or an error to be classified by the
/// retry loop.
async fn attempt_download(
    client: &reqwest::Client,
    req: &DownloadRequest,
    control: &Control,
    known_target: &Option<PathBuf>,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(u16, PathBuf, u64)> {
    // If we already know the target and a prior `.part`+sidecar exist, resume.
    if let Some(target) = known_target {
        let part = state::part_path(target);
        let sidecar = state::sidecar_path(target);
        if part.exists() && sidecar.exists() {
            if let Ok(st) = DownloadState::load(&sidecar) {
                return resume(
                    client,
                    req,
                    control,
                    target,
                    &part,
                    &sidecar,
                    st,
                    on_progress,
                )
                .await;
            }
        }
    }

    // FRESH: issue a plain GET so we can learn the (post-redirect) URL and the
    // derived filename before committing to a target path.
    let resp = client.get(&req.url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Status(status.as_u16()));
    }

    let final_url = resp.url().clone();
    let content_disposition = header_str(resp.headers(), header::CONTENT_DISPOSITION);
    let total = resp.content_length();
    let etag = header_str(resp.headers(), header::ETAG);
    let last_modified = header_str(resp.headers(), header::LAST_MODIFIED);

    let target = match &req.out {
        Some(out) => out.clone(),
        None => {
            let name = filename::resolve(None, final_url.as_str(), content_disposition.as_deref());
            req.dir.join(name)
        }
    };
    ensure_parent(&target).await?;

    let part = state::part_path(&target);
    let sidecar = state::sidecar_path(&target);

    // A resumable sidecar may exist even when the target wasn't known up front
    // (derived filename). If so, and the resource is unchanged, resume rather
    // than restart. Otherwise write a fresh sidecar BEFORE any bytes stream.
    if part.exists() && sidecar.exists() {
        if let Ok(st) = DownloadState::load(&sidecar) {
            return resume(
                client,
                req,
                control,
                &target,
                &part,
                &sidecar,
                st,
                on_progress,
            )
            .await;
        }
    }

    let st = DownloadState::new(&req.url, &target, total, etag, last_modified);
    st.save(&sidecar)?;

    let file = tokio::fs::File::create(&part).await?;
    let bytes = stream_to_part(resp, file, control, &part, &sidecar, 0, total, on_progress).await?;
    Ok((status.as_u16(), part, bytes))
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
    let resp = builder.send().await?;
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
            let fresh = DownloadState::new(&st.url, target, total, etag, last_modified);
            fresh.save(sidecar)?;

            let file = tokio::fs::File::create(part).await?; // truncates to 0
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
                    });
                    return Ok((status.as_u16(), part.to_path_buf(), offset));
                }
            }
            tokio::fs::File::create(part).await?; // truncate
            restart(client, req, control, target, part, sidecar, on_progress).await
        }
        other => Err(Error::Status(other.as_u16())),
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
    let resp = client.get(&url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Status(status.as_u16()));
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
    });

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        // Control point: honoured once per chunk, before the write.
        match control.poll() {
            Flow::Cancel => {
                drop(file);
                let _ = tokio::fs::remove_file(part).await;
                let _ = tokio::fs::remove_file(sidecar).await;
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

        // Throttle point: M4's rate limiter sleeps here. No-op today.
        control.throttle(chunk.len()).await;

        let bytes_per_sec = meter.observe(start.elapsed(), downloaded);
        on_progress(Progress {
            downloaded,
            total,
            bytes_per_sec,
            eta: progress::eta(downloaded, total, bytes_per_sec),
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
        Error::Io(_) | Error::Cancelled | Error::Paused | Error::Integrity(_) => false,
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
        assert!(!is_transient(&Error::Status(404)));
        assert!(!is_transient(&Error::Cancelled));
        assert!(!is_transient(&Error::Paused));
        assert!(!is_transient(&Error::Integrity("x".into())));
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
