//! The download engine: the crate's core, frontend-agnostic logic.
//!
//! [`download`] streams a single HTTP(S) GET to disk. The chunk loop consults a
//! [`Control`] before each chunk and runs a throttle hook after each write, so
//! pause/cancel (M1) and rate limiting (M4) slot in without reshaping it.

pub mod control;
pub mod filename;
pub mod progress;

pub use control::{Control, Flow};
pub use progress::{Progress, SpeedMeter};

use std::path::PathBuf;
use std::time::Instant;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

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
}

impl DownloadRequest {
    /// A request that saves into `dir` under a filename derived from the
    /// response.
    pub fn new(url: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            dir: dir.into(),
            out: None,
        }
    }

    /// A request saved to an explicit path.
    pub fn to_path(url: impl Into<String>, out: impl Into<PathBuf>) -> Self {
        let out = out.into();
        Self {
            url: url.into(),
            dir: out.parent().map(PathBuf::from).unwrap_or_default(),
            out: Some(out),
        }
    }
}

/// Outcome of a successful download.
#[derive(Debug, Clone)]
pub struct Summary {
    /// Where the file was written.
    pub path: PathBuf,
    /// Number of bytes written.
    pub bytes: u64,
    /// Final HTTP status code (after redirects).
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
    /// The download was cancelled via its [`Control`].
    #[error("download cancelled")]
    Cancelled,
}

/// Engine result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Download `req.url` to disk, streaming the body chunk-by-chunk (never
/// buffering the whole file in memory).
///
/// `on_progress` is invoked once before the first byte and after every chunk,
/// so a frontend can render a live readout. `control` steers the in-flight
/// download (cancellation in M0).
pub async fn download(
    req: &DownloadRequest,
    control: &Control,
    mut on_progress: impl FnMut(Progress),
) -> Result<Summary> {
    let client = reqwest::Client::builder().user_agent(USER_AGENT).build()?;

    let resp = client.get(&req.url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Status(status.as_u16()));
    }

    // Resolve the destination from the *final* URL (post-redirect) and headers.
    let final_url = resp.url().clone();
    let content_disposition = resp
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let total = resp.content_length();

    let path = match &req.out {
        Some(out) => out.clone(),
        None => {
            let name = filename::resolve(None, final_url.as_str(), content_disposition.as_deref());
            req.dir.join(name)
        }
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let mut file = tokio::fs::File::create(&path).await?;
    let mut downloaded: u64 = 0;
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
        // Control point: honoured once per chunk, before the write. Cancel is
        // live today; pause slots in here in M1.
        if let Flow::Cancel = control.poll() {
            drop(file);
            let _ = tokio::fs::remove_file(&path).await;
            return Err(Error::Cancelled);
        }

        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        // Throttle point: M4's rate limiter sleeps here. No-op in M0.
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
    Ok(Summary {
        path,
        bytes: downloaded,
        status: status.as_u16(),
    })
}
