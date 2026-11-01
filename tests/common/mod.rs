//! Shared, in-process HTTP server for the offline integration tests.
//!
//! A small `axum` app bound to an ephemeral loopback port. It gives us full
//! control over status codes, `Content-Disposition`, redirects, and — for M1 —
//! HTTP range semantics, validators, mid-stream disconnects, and a mutable
//! resource. It is the deterministic alternative to hitting the real network.
//! Picking a tiny hand-rolled server over a mocking crate keeps the fixture
//! flexible as the engine grows.
//!
//! # Fixture route contract
//! - `GET /file`      — payload with `Content-Length`; no range support.
//! - `GET /named`     — payload with a `Content-Disposition` filename.
//! - `GET /redirect`  — 307 to `/file`.
//! - `GET /missing`   — 404.
//! - `GET /ranged`    — payload, full+correct `Range`/`If-Range` support.
//! - `GET /slow`      — like `/ranged` but streamed slowly (a kill/pause window).
//! - `GET /norange`   — always 200, full body, never honours `Range`.
//! - `GET /liar`      — advertises `Accept-Ranges` but ignores `Range` (200).
//! - `GET /flaky`     — supports ranges; drops the connection mid-body for the
//!   first `FLAKY_FAILURES` requests, then serves fully.
//! - `GET /mutable`   — serves the current version with `Range`/`If-Range`.
//! - `GET /mutable/bump` — flip `/mutable` to v2 (new bytes, length, ETag).

#![allow(dead_code)] // Not every test binary uses every helper.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// A stable `Last-Modified` for the range-capable routes.
pub const LAST_MODIFIED: &str = "Wed, 01 Jan 2020 00:00:00 GMT";

/// How many leading `/flaky` requests abort mid-body before one succeeds.
const FLAKY_FAILURES: usize = 2;

/// Bytes streamed per chunk by the slow/flaky streaming bodies.
const STREAM_CHUNK: usize = 4 * 1024;

/// Per-chunk delay for `/slow`, opening a window to kill/pause mid-stream.
const SLOW_CHUNK_DELAY: Duration = Duration::from_millis(15);

/// Deterministic payload served by the fixture (just under 100 KB, enough to
/// span many chunks). Reproduced byte-for-byte on every call.
pub fn payload() -> Vec<u8> {
    (0..99_991u32).map(|i| (i % 251) as u8).collect()
}

/// A second, distinct payload: different bytes AND a different length from
/// [`payload`]. Used to prove a clean restart when the resource changes.
pub fn payload_v2() -> Vec<u8> {
    (0..61_237u32).map(|i| ((i % 241) as u8) ^ 0x5a).collect()
}

/// Mutable-resource version state behind `/mutable`.
#[derive(Default)]
struct AppState {
    /// `/flaky`: how many requests have been served so far.
    flaky_hits: AtomicUsize,
    /// `/mutable`: `false` = v1 (`payload`), `true` = v2 (`payload_v2`).
    mutable_bumped: AtomicBool,
}

async fn file_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/octet-stream")],
        payload(),
    )
}

async fn named_handler() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"report-final.bin\"",
            ),
        ],
        payload(),
    )
}

async fn redirect_handler() -> impl IntoResponse {
    Redirect::temporary("/file")
}

async fn missing_handler() -> impl IntoResponse {
    StatusCode::NOT_FOUND
}

/// Parse a `Range: bytes=N-` header into the start offset. Only the open-ended
/// suffix form the engine emits is supported.
fn parse_range_start(headers: &HeaderMap) -> Option<u64> {
    let raw = headers.get(header::RANGE)?.to_str().ok()?;
    let spec = raw.strip_prefix("bytes=")?;
    let start = spec.split('-').next()?;
    start.parse::<u64>().ok()
}

/// Does the `If-Range` validator (if any) still match the current `etag`?
/// Absent header means "unconditional" → treated as a match.
fn if_range_matches(headers: &HeaderMap, etag: &str) -> bool {
    match headers.get(header::IF_RANGE).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(v) => v == etag || v == LAST_MODIFIED,
    }
}

/// Common validator + length headers for a range-capable resource.
fn range_headers(etag: &str, total: usize) -> [(header::HeaderName, String); 4] {
    [
        (header::ACCEPT_RANGES, "bytes".to_string()),
        (header::ETAG, etag.to_string()),
        (header::LAST_MODIFIED, LAST_MODIFIED.to_string()),
        (header::CONTENT_LENGTH, total.to_string()),
    ]
}

/// Build a 200/206/416 response for `body` honouring `Range`/`If-Range`,
/// returning the sliced bytes and the correct `Content-Range`/`Content-Length`.
fn ranged_response(headers: &HeaderMap, etag: &str, body: Vec<u8>) -> Response {
    let total = body.len();
    let range_start = parse_range_start(headers);
    let validator_ok = if_range_matches(headers, etag);

    match range_start {
        // A genuine, in-bounds range against an unchanged resource → 206.
        Some(start) if validator_ok && (start as usize) < total => {
            let slice = body[start as usize..].to_vec();
            let last = total as u64 - 1;
            let mut resp = Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::ETAG, etag)
                .header(header::LAST_MODIFIED, LAST_MODIFIED)
                .header(header::CONTENT_LENGTH, slice.len())
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {start}-{last}/{total}"),
                )
                .body(Body::from(slice))
                .unwrap();
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                "application/octet-stream".parse().unwrap(),
            );
            resp
        }
        // Start past the end → 416.
        Some(start) if validator_ok && (start as usize) >= total => Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{total}"))
            .body(Body::empty())
            .unwrap(),
        // No range, or validator no longer matches → full 200 body.
        _ => {
            let mut builder = Response::builder().status(StatusCode::OK);
            for (name, value) in range_headers(etag, total) {
                builder = builder.header(name, value);
            }
            builder
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(Body::from(body))
                .unwrap()
        }
    }
}

/// `/ranged`: full, correct range support over [`payload`] with ETag `"v1"`.
async fn ranged_handler(headers: HeaderMap) -> Response {
    ranged_response(&headers, "\"v1\"", payload())
}

/// `/slow`: same semantics as `/ranged`, but the body is streamed in small
/// chunks with a short delay — a window to kill or pause mid-stream. Range and
/// If-Range are honoured so a resume finishes quickly.
async fn slow_handler(headers: HeaderMap) -> Response {
    let etag = "\"v1\"";
    let body = payload();
    let total = body.len();
    let range_start = parse_range_start(&headers);
    let validator_ok = if_range_matches(&headers, etag);

    let (status, start, slice): (StatusCode, u64, Vec<u8>) = match range_start {
        Some(s) if validator_ok && (s as usize) < total => {
            (StatusCode::PARTIAL_CONTENT, s, body[s as usize..].to_vec())
        }
        Some(s) if validator_ok && (s as usize) >= total => {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{total}"))
                .body(Body::empty())
                .unwrap();
        }
        _ => (StatusCode::OK, 0, body),
    };

    let slice_len = slice.len();
    let stream_body = slow_stream(slice);

    let mut builder = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, etag)
        .header(header::LAST_MODIFIED, LAST_MODIFIED)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, slice_len);
    if status == StatusCode::PARTIAL_CONTENT {
        let last = total as u64 - 1;
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{last}/{total}"),
        );
    }
    builder.body(stream_body).unwrap()
}

/// A body that yields `slice` a few KB at a time with a per-chunk delay.
fn slow_stream(slice: Vec<u8>) -> Body {
    // Hand-rolled stream without an extra dependency: chunk the buffer and
    // sleep between yields.
    let chunks: Vec<Vec<u8>> = slice.chunks(STREAM_CHUNK).map(|c| c.to_vec()).collect();
    let s = futures_util::stream::unfold(chunks.into_iter(), |mut it| async move {
        match it.next() {
            Some(chunk) => {
                tokio::time::sleep(SLOW_CHUNK_DELAY).await;
                Some((Ok::<_, std::io::Error>(axum::body::Bytes::from(chunk)), it))
            }
            None => None,
        }
    });
    Body::from_stream(s)
}

/// `/norange`: never supports ranges. Always 200 with the full body and no
/// `Accept-Ranges`; any `Range` header is ignored.
async fn norange_handler() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, payload().len())
        .body(Body::from(payload()))
        .unwrap()
}

/// `/liar`: advertises `Accept-Ranges: bytes` but ignores `Range`, always
/// returning 200 with the full body (a server lying about range support).
async fn liar_handler() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, payload().len())
        .body(Body::from(payload()))
        .unwrap()
}

/// `/flaky`: supports ranges, but aborts the connection mid-body for the first
/// `FLAKY_FAILURES` requests; the next request serves the requested range to
/// completion. Each retry resumes from the current `.part` offset.
async fn flaky_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let hit = state.flaky_hits.fetch_add(1, Ordering::SeqCst);
    let etag = "\"v1\"";
    let body = payload();
    let total = body.len();

    let start = match parse_range_start(&headers) {
        Some(s) if if_range_matches(&headers, etag) && (s as usize) <= total => s,
        _ => 0,
    };
    let slice = body[start as usize..].to_vec();
    let last = total as u64 - 1;

    let status = if start > 0 {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };

    let mut builder = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, etag)
        .header(header::LAST_MODIFIED, LAST_MODIFIED)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, slice.len());
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{last}/{total}"),
        );
    }

    if hit < FLAKY_FAILURES {
        // Yield a prefix, then abort with a body error so reqwest sees a broken
        // stream and the engine's retry kicks in.
        builder.body(aborting_stream(slice)).unwrap()
    } else {
        builder.body(Body::from(slice)).unwrap()
    }
}

/// A body that yields one small chunk then errors, aborting the connection.
fn aborting_stream(slice: Vec<u8>) -> Body {
    let prefix_len = STREAM_CHUNK.min(slice.len() / 2).max(1);
    let prefix = slice[..prefix_len.min(slice.len())].to_vec();
    let s = futures_util::stream::unfold(0u8, move |step| {
        let prefix = prefix.clone();
        async move {
            match step {
                0 => Some((
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(prefix)),
                    1u8,
                )),
                1 => Some((
                    Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        "simulated mid-stream disconnect",
                    )),
                    2u8,
                )),
                _ => None,
            }
        }
    });
    Body::from_stream(s)
}

/// `/mutable`: serves the current version with full `Range`/`If-Range` support.
/// v1 = `payload` / ETag `"v1"`; after a bump, v2 = `payload_v2` / ETag `"v2"`.
async fn mutable_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let (etag, body) = if state.mutable_bumped.load(Ordering::SeqCst) {
        ("\"v2\"", payload_v2())
    } else {
        ("\"v1\"", payload())
    };
    ranged_response(&headers, etag, body)
}

/// `/mutable/bump`: flip `/mutable` to v2.
async fn mutable_bump_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.mutable_bumped.store(true, Ordering::SeqCst);
    StatusCode::NO_CONTENT
}

/// A running test server. The background task is aborted on drop.
pub struct TestServer {
    /// Base URL, e.g. `http://127.0.0.1:52344`.
    pub base: String,
    handle: JoinHandle<()>,
}

impl TestServer {
    /// Full URL for a path on this server, e.g. `url("/file")`.
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Spawn the fixture on a random free loopback port. See the module-level
/// contract for the full route list.
pub async fn spawn() -> TestServer {
    let state = Arc::new(AppState::default());
    let app = Router::new()
        .route("/file", get(file_handler))
        .route("/named", get(named_handler))
        .route("/redirect", get(redirect_handler))
        .route("/missing", get(missing_handler))
        .route("/ranged", get(ranged_handler))
        .route("/slow", get(slow_handler))
        .route("/norange", get(norange_handler))
        .route("/liar", get(liar_handler))
        .route("/flaky", get(flaky_handler))
        .route("/mutable", get(mutable_handler))
        .route("/mutable/bump", get(mutable_bump_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    TestServer {
        base: format!("http://{addr}"),
        handle,
    }
}
