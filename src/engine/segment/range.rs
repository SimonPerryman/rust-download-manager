//! Ranged-request checks: parsing `Content-Range` and validating a segment's response.

use reqwest::header;
use reqwest::StatusCode;

use super::Failure;
use crate::engine::{status_error, Error};

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

/// Validate a response to `Range: bytes=pos-(end-1)`. A full 200 or a 206 for
/// some other range means ranges can't be trusted; a short 206 is fine.
pub(super) fn check_ranged(
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
