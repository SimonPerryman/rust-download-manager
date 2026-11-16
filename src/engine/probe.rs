//! The size probe: a small request that reads a file's size and name without
//! downloading it, so a download still waiting in the queue can show them.

use std::time::Duration;

use reqwest::{header, StatusCode};

use super::filename;
use super::http::header_str;

/// How many probes the [`Manager`](crate::engine::Manager) runs at once,
/// whatever the queue holds: separate from the download cap, so a probe never
/// takes a download's slot.
pub const PROBE_CONCURRENCY: usize = 4;

/// Longest one probe request may take before it is given up on.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// What a probe learned about a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Probed {
    /// The file's size in bytes, when the server said (never zero).
    pub size: Option<u64>,
    /// The file name the server or the final URL gives it.
    pub filename: String,
}

/// Ask the server about `url`: a `HEAD`, then, if that is refused or sends no
/// size, a one-byte range request (`Range: bytes=0-0`, size from its
/// `Content-Range`). Follows redirects like a download does. Neither body is
/// read. `None` when neither request got a successful answer; this never
/// reports a failure, because a probe is a courtesy.
pub(crate) async fn probe(client: &reqwest::Client, url: &str) -> Option<Probed> {
    let mut found: Option<Probed> = None;
    if let Ok(resp) = client.head(url).timeout(PROBE_TIMEOUT).send().await {
        if resp.status().is_success() {
            // `content_length()` is the (empty) body's size for a HEAD, so
            // read the header itself.
            let size = header_str(resp.headers(), header::CONTENT_LENGTH)
                .and_then(|v| v.trim().parse::<u64>().ok())
                .filter(|&n| n > 0);
            let probed = Probed {
                size,
                filename: name_of(&resp),
            };
            if size.is_some() {
                return Some(probed);
            }
            found = Some(probed);
        }
    }
    let ranged = client
        .get(url)
        .header(header::RANGE, "bytes=0-0")
        .timeout(PROBE_TIMEOUT)
        .send()
        .await;
    if let Ok(resp) = ranged {
        let size = match resp.status() {
            StatusCode::PARTIAL_CONTENT => header_str(resp.headers(), header::CONTENT_RANGE)
                .and_then(|v| total_of_content_range(&v)),
            // The server ignores ranges and would send the whole file.
            StatusCode::OK => resp.content_length(),
            _ => return found,
        }
        .filter(|&n| n > 0);
        return Some(Probed {
            size,
            filename: name_of(&resp),
        });
    }
    found
}

/// The name a response gives its file: `Content-Disposition`, else the final
/// URL after redirects.
fn name_of(resp: &reqwest::Response) -> String {
    let disposition = header_str(resp.headers(), header::CONTENT_DISPOSITION);
    filename::resolve(None, resp.url().as_str(), disposition.as_deref())
}

/// The total of a `Content-Range` such as `bytes 0-0/4321`; `None` for an
/// unknown total (`bytes 0-0/*`) or a value that isn't one.
fn total_of_content_range(value: &str) -> Option<u64> {
    let (_, total) = value.trim().strip_prefix("bytes")?.trim().split_once('/')?;
    total.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_total() {
        assert_eq!(total_of_content_range("bytes 0-0/4321"), Some(4321));
        assert_eq!(total_of_content_range("bytes 0-0/*"), None);
        assert_eq!(total_of_content_range("bytes */10"), Some(10));
        assert_eq!(total_of_content_range("items 0-0/9"), None);
        assert_eq!(total_of_content_range("junk"), None);
    }
}
