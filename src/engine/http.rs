//! Requests and their failures: sending with timeouts, reading chunks, and
//! classifying errors for the retry loop.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header;

use super::{segment, Control, Error, Flow, Result};

/// Read a header value as an owned `String`, if present and valid UTF-8.
pub(super) fn header_str(headers: &header::HeaderMap, name: header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// Whether an error is worth retrying: transport hiccups and 5xx/408/429.
pub(super) fn is_transient(err: &Error) -> bool {
    match err {
        Error::Http(e) => {
            e.is_timeout() || e.is_connect() || e.is_request() || e.is_body() || e.is_decode()
        }
        Error::Status(code) => *code >= 500 || *code == 408 || *code == 429,
        Error::Throttled { .. } | Error::TimedOut => true,
        Error::Incomplete(_) | Error::RedirectExpired(_) => true,
        Error::Io(_)
        | Error::Cancelled
        | Error::Paused
        | Error::Integrity(_)
        | Error::ChecksumMismatch(_)
        | Error::UnsupportedState(_)
        | Error::UnsupportedStore(_)
        | Error::InvalidRequest(_) => false,
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
pub(super) fn interruption(control: &Control) -> Option<Error> {
    match control.poll() {
        Flow::Cancel => Some(Error::Cancelled),
        Flow::Pause => Some(Error::Paused),
        Flow::Continue => None,
    }
}

/// Send a request, cut short by pause/cancel: a host that never answers would
/// otherwise hold the download until the read timeout. [`Error::TimedOut`] if
/// the response headers don't arrive within `read_timeout`.
pub(super) async fn send(
    control: &Control,
    request: reqwest::RequestBuilder,
    read_timeout: Duration,
) -> Result<reqwest::Response> {
    let response = request.send();
    tokio::pin!(response);
    let deadline = tokio::time::sleep(read_timeout);
    tokio::pin!(deadline);
    let mut poll = tokio::time::interval(segment::CONTROL_POLL_INTERVAL);
    loop {
        tokio::select! {
            resp = &mut response => return Ok(resp?),
            _ = &mut deadline => return Err(Error::TimedOut),
            _ = poll.tick() => {
                if let Some(e) = interruption(control) {
                    return Err(e);
                }
            }
        }
    }
}

/// The next body chunk, or [`Error::TimedOut`] if none arrives within
/// `read_timeout`. The clock only runs while we wait here, so throttle sleeps
/// between reads don't count against it.
pub(super) async fn next_chunk<S, T>(body: &mut S, read_timeout: Duration) -> Option<Result<T>>
where
    S: futures_util::Stream<Item = reqwest::Result<T>> + Unpin,
{
    match tokio::time::timeout(read_timeout, body.next()).await {
        Ok(next) => next.map(|chunk| chunk.map_err(Error::from)),
        Err(_) => Some(Err(Error::TimedOut)),
    }
}

/// Sleep off a retry delay, waking early on pause/cancel.
pub(super) async fn backoff(control: &Control, delay: Duration) {
    let until = tokio::time::Instant::now() + delay;
    while control.poll() == Flow::Continue {
        let now = tokio::time::Instant::now();
        if now >= until {
            break;
        }
        tokio::time::sleep_until(until.min(now + segment::CONTROL_POLL_INTERVAL)).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Time spent away from the stream (a throttle sleep) doesn't count; only
    /// waiting on it does.
    #[tokio::test(start_paused = true)]
    async fn next_chunk_times_out_only_while_waiting() {
        let timeout = Duration::from_millis(100);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<reqwest::Result<u8>>();
        let mut body = receiver_stream(rx);
        tx.send(Ok(1)).unwrap();
        assert!(matches!(next_chunk(&mut body, timeout).await, Some(Ok(1))));
        tokio::time::sleep(timeout * 5).await;
        let late = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(timeout / 2).await;
            late.send(Ok(2)).unwrap();
        });
        assert!(matches!(next_chunk(&mut body, timeout).await, Some(Ok(2))));
        assert!(matches!(
            next_chunk(&mut body, timeout).await,
            Some(Err(Error::TimedOut))
        ));
    }

    fn receiver_stream<T>(
        rx: tokio::sync::mpsc::UnboundedReceiver<T>,
    ) -> impl futures_util::Stream<Item = T> + Unpin {
        Box::pin(futures_util::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        }))
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
        assert!(is_transient(&Error::TimedOut));
        assert!(is_transient(&Error::RedirectExpired(403)));
        assert!(!is_transient(&Error::Status(404)));
        assert!(!is_transient(&Error::Cancelled));
        assert!(!is_transient(&Error::Paused));
        assert!(!is_transient(&Error::Integrity("x".into())));
        assert!(!is_transient(&Error::ChecksumMismatch("x".into())));
        assert!(!is_transient(&Error::UnsupportedState(3)));
        assert!(!is_transient(&Error::InvalidRequest("x".into())));
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
}
