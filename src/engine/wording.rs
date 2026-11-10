//! What the log says about a download as a whole: how it starts, resumes and retries.

use reqwest::header;

use super::http::header_str;
use super::{human, DownloadRequest, Error};

/// A size for a log line, or `size unknown`.
pub(super) fn size_text(total: Option<u64>) -> String {
    total.map_or_else(|| "size unknown".to_owned(), human::bytes)
}

/// Say how a fresh download starts (once per run), e.g. `Started · 6.25 GB ·
/// resumable · 4 pieces, up to 4 lanes`. Resumable means the server takes
/// byte ranges.
pub(super) fn announce_start(req: &DownloadRequest, resp: &reqwest::Response, lanes: &str) {
    if !req.tally.announce() {
        return;
    }
    let size = size_text(resp.content_length());
    let ranges = header_str(resp.headers(), header::ACCEPT_RANGES)
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("bytes"));
    let resumable = if ranges { "resumable" } else { "not resumable" };
    tracing::info!("Started · {size} · {resumable} · {lanes}");
}

/// Say a run picked up the `on_disk` bytes an earlier one left (once per run).
pub(super) fn announce_resume(req: &DownloadRequest, on_disk: u64) {
    if req.tally.announce() {
        req.tally.resumed_with(on_disk);
        if on_disk == 0 {
            tracing::info!("Resumed from the start");
        } else {
            tracing::info!("Resumed at {}", human::bytes(on_disk));
        }
    }
}

/// Say a resume throws its `offset` bytes away, and `why`.
pub(super) fn restarting(req: &DownloadRequest, offset: u64, why: &str) {
    req.tally.announce();
    req.tally.wasted(offset);
    tracing::warn!("Couldn't resume ({why}) — starting over");
}

/// A transient failure in a few words, for a retry line.
pub(super) fn problem(e: &Error) -> String {
    match e {
        Error::TimedOut => "Server stopped responding".to_owned(),
        Error::Http(e) if e.is_timeout() => "Server stopped responding".to_owned(),
        Error::Http(e) if e.is_connect() => "Couldn't connect".to_owned(),
        Error::Status(code) | Error::Throttled { status: code, .. } => {
            format!("Server returned HTTP {code}")
        }
        Error::RedirectExpired(_) => "Download link expired".to_owned(),
        _ => "Connection lost".to_owned(),
    }
}
