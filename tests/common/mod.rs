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
//! - `GET /named/report-final.bin` — `/named`, under a URL naming the same file.
//! - `GET /redirect`  — 307 to `/file`.
//! - `GET /missing`   — 404.
//! - `GET /ranged`    — payload, full+correct `Range`/`If-Range` support.
//! - `GET /slow`      — like `/ranged` but streamed slowly (a kill/pause window).
//! - `GET /named-slow` — `/slow` plus `/named`'s `Content-Disposition`.
//! - `GET /crawl`     — `/slow` over a [`CRAWL_LEN`] payload (~150 chunks), so
//!   it outlasts a multi-step UI flow at the E2E delay.
//! - `GET /gated`     — no `Accept-Ranges`, no length: headers at once, then a
//!   1 KiB chunk every [`GATED_TRICKLE`] until the test releases it through
//!   [`TestServer::gate`] (keyed by arrival order), then ends.
//! - `GET /norange`   — always 200, full body, never honours `Range`.
//! - `GET /liar`      — advertises `Accept-Ranges` but ignores `Range` (200).
//! - `GET /flaky`     — supports ranges; drops the connection mid-body for the
//!   first `FLAKY_FAILURES` requests, then serves fully.
//! - `GET /mutable`   — serves the current version with `Range`/`If-Range`.
//! - `GET /mutable/bump` — flip `/mutable` to v2 (new bytes, length, ETag).
//!
//! Multi-segment routes (M3), all over [`seg_payload`] (4 MiB by default,
//! streamed in 32 KiB chunks) with a strong ETag + `Last-Modified` and closed
//! (`bytes=s-e`) or open (`bytes=s-`) `Range` support, unless stated:
//! - `GET /seg`        — per-chunk delay [`FixtureConfig::seg_chunk_delay`].
//! - `GET /seg-flaky`  — the first ranged request aborts mid-body.
//! - `GET /seg-flaky-tail` — the first ranged request for the final byte
//!   aborts mid-body once it is the only response still streaming (so it is
//!   the last worker standing).
//! - `GET /seg-noaccept` — no `Accept-Ranges`; ignores `Range` (always 200).
//! - `GET /seg-short`  — every ranged request gets a correctly-labelled 206 of
//!   at most 256 KiB.
//! - `GET /seg-skewed` — requests starting at byte 0 stream at 50 ms/chunk,
//!   others at full speed (forces a work steal).
//! - `GET /seg-redirect` — 307 to `/seg`.
//! - `GET /seg-weak-etag` — weak ETag + `Last-Modified`.
//! - `GET /seg-capped` — 503 for any request beyond 2 in flight (counted in
//!   [`RouteStats::refused`]).
//! - `GET /seg-capped-flaky` — `/seg-capped`, and the first ranged response it
//!   lets in aborts mid-body; an unranged response holds after its first chunk
//!   until released through [`TestServer::gate`].
//! - `GET /seg-capped-drop` — `/seg-capped`, and every ranged response it lets
//!   in aborts after [`FixtureConfig::seg_abort_after`] chunks.
//! - `GET /seg-drop`   — every ranged response aborts after
//!   [`FixtureConfig::seg_abort_after`] chunks.
//! - `GET /seg-503`    — every ranged request gets 503.
//! - `GET /seg-403`    — every ranged request gets 403 after [`FORBIDDEN_DELAY`].
//! - `GET /liar-etag`  — `/liar` over [`seg_payload`], with a strong ETag.
//! - `GET /seg-identity` — `/seg` plus `Content-Encoding: identity`.
//! - `GET /seg-expiring` — 307 to a freshly minted `/seg-signed?sig=<n>`.
//! - `GET /seg-signed?sig=<n>` — `/seg`, but each `sig` is good for
//!   [`SIGNED_URL_USES`] requests, then 403 (counted in
//!   [`RouteStats::refused`]), like an expiring signed URL.
//! - `GET /seg-ramp`   — [`SEG_RAMP_CHUNK`]s paced by deadline (no burst) at
//!   [`SEG_RAMP_RATE`] per connection and [`SEG_RAMP_CAP`] across all of
//!   them, like a host whose link saturates at four connections.
//! - `GET /seg-429`    — the first [`FixtureConfig::seg_refusals`] ranged
//!   requests get 429 (with `Retry-After` from
//!   [`FixtureConfig::seg_retry_after`]), like a per-IP rate limit.
//! - `GET /seg-429-drop` — ranged requests alternate: the even ones abort after
//!   [`FixtureConfig::seg_abort_after`] chunks, the odd ones get 429, so every
//!   attempt makes progress and then is rate-limited.
//! - `GET /seg-429-then-drop` — the first ranged request gets 429 (with
//!   `Retry-After` from [`FixtureConfig::seg_retry_after`]), the second aborts
//!   after [`FixtureConfig::seg_abort_after`] chunks, the rest are served.
//! - `GET /limited-once` — [`payload`], but the first request gets 429 (with
//!   `Retry-After` from [`FixtureConfig::seg_retry_after`]).
//! - `GET /stall`      — no length: one 1 KiB chunk, then the body stalls
//!   forever (a connection whose packets are dropped).
//! - `GET /hang`       — never answers, not even with headers.
//!
//! Aborting bodies wait [`ABORT_GRACE`] after their prefix before erroring, so
//! the prefix really reaches the client. Every `/seg*` route, `/liar-etag`,
//! `/norange`, `/liar`, `/gated` and `/stall` keeps [`RouteStats`] (see
//! [`TestServer::stats`]); arrivals and refusals are timestamped in
//! [`RouteStats::arrived_at`] and [`RouteStats::refused_at`].
//!
//! The same routes back the GUI's webview E2E suite through the
//! `fixture_server` example, which serves them on a fixed port with a slower
//! `/slow` so a human-speed UI click can land mid-stream.

#![allow(dead_code)] // Not every test binary uses every helper.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, MethodRouter},
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

/// Default per-chunk delay for `/slow`, opening a window to kill/pause
/// mid-stream.
pub const SLOW_CHUNK_DELAY: Duration = Duration::from_millis(15);

/// `/crawl`: payload length, ~150 [`STREAM_CHUNK`]s (~30 s at the E2E delay).
pub const CRAWL_LEN: usize = 150 * STREAM_CHUNK;

/// `/gated`: delay between trickled chunks while held.
pub const GATED_TRICKLE: Duration = Duration::from_millis(20);

/// `/gated`: bytes per trickled chunk.
const GATED_CHUNK: usize = 1024;

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

/// Default [`seg_payload`] length: big enough to go multi-segment with the
/// engine's default 1 MiB minimum segment.
pub const SEG_LEN: usize = 4 * 1024 * 1024;

/// Bytes streamed per chunk by the `/seg*` routes.
pub const SEG_CHUNK: usize = 32 * 1024;

/// `/seg-short`: the most a single ranged response serves.
pub const SEG_SHORT_MAX: usize = 256 * 1024;

/// `/seg-skewed`: chunk delay for requests that start at byte 0.
const SKEWED_DELAY: Duration = Duration::from_millis(50);

/// `/seg-capped`: requests allowed in flight before 503s.
pub const SEG_CAP: usize = 2;

/// `/seg-signed`: requests each minted `sig` serves before it "expires".
pub const SIGNED_URL_USES: usize = 2;

/// `/seg-ramp`: bytes per paced chunk.
pub const SEG_RAMP_CHUNK: usize = 8 * 1024;

/// `/seg-ramp`: the most one connection is served, in bytes/s.
pub const SEG_RAMP_RATE: u64 = 256 * 1024;

/// `/seg-ramp`: the most all connections together are served, in bytes/s.
pub const SEG_RAMP_CAP: u64 = 4 * SEG_RAMP_RATE;

/// How long an aborting body waits after its prefix before erroring.
pub const ABORT_GRACE: Duration = Duration::from_millis(25);

/// `/seg-403`: how long a ranged request waits before its 403.
pub const FORBIDDEN_DELAY: Duration = Duration::from_millis(200);

/// Strong ETag of the `/seg*` resources.
pub const SEG_ETAG: &str = "\"seg-v1\"";

/// Weak ETag served by `/seg-weak-etag`.
pub const SEG_WEAK_ETAG: &str = "W/\"seg-v1\"";

/// Deterministic, non-periodic payload of `len` bytes for the `/seg*` routes
/// (non-periodic so a range written at the wrong offset can't go unnoticed).
pub fn seg_payload_of(len: usize) -> Vec<u8> {
    (0..len as u64)
        .map(|i| {
            let mut x = i.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5DEE_CE66;
            x ^= x >> 29;
            x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            (x >> 56) as u8
        })
        .collect()
}

/// The default-length [`seg_payload_of`].
pub fn seg_payload() -> Vec<u8> {
    seg_payload_of(SEG_LEN)
}

/// Tunables for a fixture server; [`spawn`] uses the defaults.
#[derive(Debug, Clone)]
pub struct FixtureConfig {
    /// `/slow`, `/named-slow`, `/crawl`: delay between streamed chunks.
    pub slow_chunk_delay: Duration,
    /// Length of [`seg_payload_of`] served by the `/seg*` routes.
    pub seg_len: usize,
    /// `/seg` (and most `/seg*` routes): delay before each 32 KiB chunk.
    pub seg_chunk_delay: Duration,
    /// `/seg-drop`, `/seg-429-drop`: chunks served before a ranged response
    /// aborts.
    pub seg_abort_after: usize,
    /// `/seg-429`: how many leading ranged requests are refused.
    pub seg_refusals: usize,
    /// `/seg-429`: the `Retry-After` seconds sent with a refusal, if any.
    pub seg_retry_after: Option<u64>,
}

impl Default for FixtureConfig {
    fn default() -> Self {
        Self {
            slow_chunk_delay: SLOW_CHUNK_DELAY,
            seg_len: SEG_LEN,
            seg_chunk_delay: Duration::ZERO,
            seg_abort_after: 1,
            seg_refusals: 3,
            seg_retry_after: None,
        }
    }
}

/// Per-route request accounting, for asserting what the engine asked for.
#[derive(Default)]
pub struct RouteStats {
    requests: AtomicUsize,
    unranged: AtomicUsize,
    ranges: Mutex<Vec<String>>,
    if_ranges: Mutex<Vec<String>>,
    body_bytes: AtomicU64,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
    refused: AtomicUsize,
    refused_at: Mutex<Vec<Instant>>,
    arrived_at: Mutex<Vec<Instant>>,
    alongside: Mutex<Vec<usize>>,
}

impl RouteStats {
    /// Requests received.
    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    /// Requests received without a `Range` header.
    pub fn unranged(&self) -> usize {
        self.unranged.load(Ordering::SeqCst)
    }

    /// Every `Range` header received, in arrival order.
    pub fn ranges(&self) -> Vec<String> {
        self.ranges.lock().unwrap().clone()
    }

    /// Every closed `Range: bytes=s-e` received, as sorted `[s, e + 1)` pairs.
    pub fn closed_ranges(&self) -> Vec<(u64, u64)> {
        let mut ranges: Vec<_> = self
            .ranges()
            .iter()
            .map(|r| {
                let (s, e) = r
                    .strip_prefix("bytes=")
                    .and_then(|r| r.split_once('-'))
                    .unwrap_or_else(|| panic!("unexpected Range header {r}"));
                let end: u64 = e.parse().unwrap_or_else(|_| panic!("open Range {r}"));
                (s.parse::<u64>().unwrap(), end + 1)
            })
            .collect();
        ranges.sort();
        ranges
    }

    /// Every `If-Range` header received, in arrival order.
    pub fn if_ranges(&self) -> Vec<String> {
        self.if_ranges.lock().unwrap().clone()
    }

    /// Body bytes the server has yielded to the connection.
    pub fn body_bytes(&self) -> u64 {
        self.body_bytes.load(Ordering::SeqCst)
    }

    /// Responses whose body is still streaming.
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Highest [`in_flight`](Self::in_flight) seen since the last reset.
    pub fn max_in_flight(&self) -> usize {
        self.max_in_flight.load(Ordering::SeqCst)
    }

    /// Requests turned away (connection cap reached, or an expired `sig`).
    pub fn refused(&self) -> usize {
        self.refused.load(Ordering::SeqCst)
    }

    /// When each [`refused`](Self::refused) request arrived, in order.
    pub fn refused_at(&self) -> Vec<Instant> {
        self.refused_at.lock().unwrap().clone()
    }

    /// For each response let in, how many others were streaming as it arrived.
    pub fn alongside(&self) -> Vec<usize> {
        self.alongside.lock().unwrap().clone()
    }

    /// When each request arrived, in order.
    pub fn arrived_at(&self) -> Vec<Instant> {
        self.arrived_at.lock().unwrap().clone()
    }

    /// Zero the counters (the in-flight gauge itself is live, not reset).
    pub fn reset(&self) {
        self.refused.store(0, Ordering::SeqCst);
        self.refused_at.lock().unwrap().clear();
        self.arrived_at.lock().unwrap().clear();
        self.alongside.lock().unwrap().clear();
        self.requests.store(0, Ordering::SeqCst);
        self.unranged.store(0, Ordering::SeqCst);
        self.ranges.lock().unwrap().clear();
        self.if_ranges.lock().unwrap().clear();
        self.body_bytes.store(0, Ordering::SeqCst);
        self.max_in_flight.store(self.in_flight(), Ordering::SeqCst);
    }

    /// Count a request in; returns its 0-based arrival index among the ranged
    /// ones, if it is ranged.
    fn record(&self, headers: &HeaderMap) -> Option<usize> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.arrived_at.lock().unwrap().push(Instant::now());
        let ranged = match headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
            Some(range) => {
                let mut ranges = self.ranges.lock().unwrap();
                ranges.push(range.to_string());
                Some(ranges.len() - 1)
            }
            None => {
                self.unranged.fetch_add(1, Ordering::SeqCst);
                None
            }
        };
        if let Some(v) = headers.get(header::IF_RANGE).and_then(|v| v.to_str().ok()) {
            self.if_ranges.lock().unwrap().push(v.to_string());
        }
        ranged
    }

    fn refuse(&self) {
        self.refused_at.lock().unwrap().push(Instant::now());
        self.refused.fetch_add(1, Ordering::SeqCst);
    }
}

/// Holds one slot of a route's in-flight gauge; moved into the body stream so
/// it is released only when the body finishes or the connection drops it.
struct InFlight(Arc<RouteStats>);

impl InFlight {
    /// Take a slot, or `None` if `cap` slots are already taken.
    fn enter(stats: &Arc<RouteStats>, cap: Option<usize>) -> Option<Self> {
        let prev = stats.in_flight.fetch_add(1, Ordering::SeqCst);
        if cap.is_some_and(|cap| prev >= cap) {
            stats.in_flight.fetch_sub(1, Ordering::SeqCst);
            stats.refuse();
            return None;
        }
        stats.max_in_flight.fetch_max(prev + 1, Ordering::SeqCst);
        stats.alongside.lock().unwrap().push(prev);
        Some(Self(stats.clone()))
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Every route that keeps [`RouteStats`].
const COUNTED_ROUTES: &[&str] = &[
    "/seg",
    "/seg-flaky",
    "/seg-flaky-tail",
    "/seg-noaccept",
    "/seg-short",
    "/seg-skewed",
    "/seg-redirect",
    "/seg-weak-etag",
    "/seg-capped",
    "/seg-capped-flaky",
    "/seg-capped-drop",
    "/seg-drop",
    "/seg-503",
    "/seg-403",
    "/liar-etag",
    "/seg-identity",
    "/seg-expiring",
    "/seg-signed",
    "/seg-ramp",
    "/seg-429",
    "/seg-429-drop",
    "/seg-429-then-drop",
    "/limited-once",
    "/norange",
    "/liar",
    "/gated",
    "/stall",
    "/hang",
];

/// The `/gated` hold: each request gets an arrival sequence number and
/// trickles until its number is released.
#[derive(Default)]
pub struct Gate(Mutex<GateState>);

#[derive(Default)]
struct GateState {
    next_seq: usize,
    /// Requests still connected.
    live: BTreeSet<usize>,
    released: HashSet<usize>,
    release_all: bool,
}

impl Gate {
    /// Release the oldest still-connected, held request; returns its sequence
    /// number, or `None` if none is held.
    pub fn release_next(&self) -> Option<usize> {
        let mut st = self.0.lock().unwrap();
        let seq = st
            .live
            .iter()
            .copied()
            .find(|seq| !st.released.contains(seq))?;
        st.released.insert(seq);
        Some(seq)
    }

    /// Release every current and future request.
    pub fn release_all(&self) {
        self.0.lock().unwrap().release_all = true;
    }

    /// Release request `seq` (0-based arrival order).
    pub fn release(&self, seq: usize) {
        self.0.lock().unwrap().released.insert(seq);
    }

    fn enter(self: &Arc<Self>) -> GateTicket {
        let mut st = self.0.lock().unwrap();
        let seq = st.next_seq;
        st.next_seq += 1;
        st.live.insert(seq);
        GateTicket {
            gate: self.clone(),
            seq,
        }
    }
}

/// One `/gated` request's place in the [`Gate`]; dropped with the body, so a
/// disconnected request can never swallow a release.
struct GateTicket {
    gate: Arc<Gate>,
    seq: usize,
}

impl GateTicket {
    fn released(&self) -> bool {
        let st = self.gate.0.lock().unwrap();
        st.release_all || st.released.contains(&self.seq)
    }
}

impl Drop for GateTicket {
    fn drop(&mut self) {
        self.gate.0.lock().unwrap().live.remove(&self.seq);
    }
}

/// Per-server state: route counters and the `/mutable` version.
struct AppState {
    cfg: FixtureConfig,
    /// The `/seg*` resource.
    seg: Bytes,
    /// `/flaky`: how many requests have been served so far.
    flaky_hits: AtomicUsize,
    /// `/seg-flaky`: whether the one aborted ranged request has happened.
    seg_flaky_tripped: AtomicBool,
    /// `/seg-flaky-tail`: likewise, for the final-segment request.
    seg_flaky_tail_tripped: AtomicBool,
    /// `/mutable`: `false` = v1 (`payload`), `true` = v2 (`payload_v2`).
    mutable_bumped: AtomicBool,
    /// `/seg-expiring`: the next `sig` to mint.
    next_sig: AtomicU64,
    /// `/seg-signed`: requests served per `sig`.
    sig_uses: Mutex<HashMap<u64, usize>>,
    /// `/gated`: which held requests may finish.
    gate: Arc<Gate>,
    /// `/seg-ramp`: when the shared link can send its next chunk.
    ramp_next_free: Arc<Mutex<tokio::time::Instant>>,
    stats: HashMap<&'static str, Arc<RouteStats>>,
}

impl AppState {
    fn new(cfg: FixtureConfig) -> Self {
        Self {
            seg: Bytes::from(seg_payload_of(cfg.seg_len)),
            cfg,
            flaky_hits: AtomicUsize::new(0),
            seg_flaky_tripped: AtomicBool::new(false),
            seg_flaky_tail_tripped: AtomicBool::new(false),
            mutable_bumped: AtomicBool::new(false),
            next_sig: AtomicU64::new(0),
            sig_uses: Mutex::new(HashMap::new()),
            gate: Arc::default(),
            ramp_next_free: Arc::new(Mutex::new(tokio::time::Instant::now())),
            stats: COUNTED_ROUTES
                .iter()
                .map(|route| (*route, Arc::new(RouteStats::default())))
                .collect(),
        }
    }

    fn stats(&self, route: &str) -> &Arc<RouteStats> {
        &self.stats[route]
    }
}

async fn file_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/octet-stream")],
        payload(),
    )
}

/// `Content-Disposition` served by the named routes.
const NAMED_DISPOSITION: &str = "attachment; filename=\"report-final.bin\"";

async fn named_handler() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CONTENT_DISPOSITION, NAMED_DISPOSITION),
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
async fn slow_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    slow_response(&headers, None, state.cfg.slow_chunk_delay, payload())
}

/// `/crawl`: `/slow` over a [`CRAWL_LEN`] payload.
async fn crawl_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let body = (0..CRAWL_LEN).map(|i| (i % 251) as u8).collect();
    slow_response(&headers, None, state.cfg.slow_chunk_delay, body)
}

/// `/named-slow`: `/slow` with a `Content-Disposition` filename.
async fn named_slow_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    slow_response(
        &headers,
        Some(NAMED_DISPOSITION),
        state.cfg.slow_chunk_delay,
        payload(),
    )
}

fn slow_response(
    headers: &HeaderMap,
    disposition: Option<&str>,
    delay: Duration,
    body: Vec<u8>,
) -> Response {
    let etag = "\"v1\"";
    let total = body.len();
    let range_start = parse_range_start(headers);
    let validator_ok = if_range_matches(headers, etag);

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
    let stream_body = slow_stream(slice, delay);

    let mut builder = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, etag)
        .header(header::LAST_MODIFIED, LAST_MODIFIED)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, slice_len);
    if let Some(cd) = disposition {
        builder = builder.header(header::CONTENT_DISPOSITION, cd);
    }
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
fn slow_stream(slice: Vec<u8>, delay: Duration) -> Body {
    // Hand-rolled stream without an extra dependency: chunk the buffer and
    // sleep between yields.
    let chunks: Vec<Vec<u8>> = slice.chunks(STREAM_CHUNK).map(|c| c.to_vec()).collect();
    let s = futures_util::stream::unfold(chunks.into_iter(), move |mut it| async move {
        match it.next() {
            Some(chunk) => {
                tokio::time::sleep(delay).await;
                Some((Ok::<_, std::io::Error>(axum::body::Bytes::from(chunk)), it))
            }
            None => None,
        }
    });
    Body::from_stream(s)
}

/// `/norange`: never supports ranges. Always 200 with the full body and no
/// `Accept-Ranges`; any `Range` header is ignored.
async fn norange_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    state.stats("/norange").record(&headers);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, payload().len())
        .body(Body::from(payload()))
        .unwrap()
}

/// `/liar`: advertises `Accept-Ranges: bytes` but ignores `Range`, always
/// returning 200 with the full body (a server lying about range support).
async fn liar_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    state.stats("/liar").record(&headers);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, payload().len())
        .body(Body::from(payload()))
        .unwrap()
}

/// `/gated`: trickle until this request's arrival number is released.
async fn gated_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    state.stats("/gated").record(&headers);
    let ticket = state.gate.enter();
    let chunk = Bytes::from(vec![0x47; GATED_CHUNK]);
    let s = futures_util::stream::unfold(ticket, move |ticket| {
        let chunk = chunk.clone();
        async move {
            tokio::time::sleep(GATED_TRICKLE).await;
            (!ticket.released()).then(|| (Ok::<_, std::io::Error>(chunk), ticket))
        }
    });
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from_stream(s))
        .unwrap()
}

/// A 429, with `Retry-After: <secs>` if given.
fn too_many_requests(retry_after: Option<u64>) -> Response {
    let mut resp = StatusCode::TOO_MANY_REQUESTS.into_response();
    if let Some(secs) = retry_after {
        resp.headers_mut()
            .insert(header::RETRY_AFTER, secs.to_string().parse().unwrap());
    }
    resp
}

/// `/limited-once`: 429 for the first request, then [`payload`].
async fn limited_once_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let stats = state.stats("/limited-once");
    stats.record(&headers);
    if stats.requests() == 1 {
        stats.refuse();
        return too_many_requests(state.cfg.seg_retry_after);
    }
    file_handler().await.into_response()
}

/// `/hang`: never answers, not even with headers.
async fn hang_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    state.stats("/hang").record(&headers);
    std::future::pending().await
}

/// `/stall`: one chunk, then a body that never yields again.
async fn stall_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    state.stats("/stall").record(&headers);
    let first = futures_util::stream::once(async {
        Ok::<_, std::io::Error>(Bytes::from(vec![0x53; GATED_CHUNK]))
    });
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from_stream(futures_util::StreamExt::chain(
            first,
            futures_util::stream::pending(),
        )))
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

/// The multi-segment route variants (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seg {
    Plain,
    Flaky,
    FlakyTail,
    NoAccept,
    Short,
    Skewed,
    WeakEtag,
    Capped,
    CappedFlaky,
    CappedDrop,
    Drop,
    Unavailable,
    Forbidden,
    LiarEtag,
    Identity,
    Signed,
    Ramp,
    RateLimited,
    RateLimitedDrop,
    RateLimitedThenDrop,
}

impl Seg {
    fn route(self) -> &'static str {
        match self {
            Seg::Plain => "/seg",
            Seg::Flaky => "/seg-flaky",
            Seg::FlakyTail => "/seg-flaky-tail",
            Seg::NoAccept => "/seg-noaccept",
            Seg::Short => "/seg-short",
            Seg::Skewed => "/seg-skewed",
            Seg::WeakEtag => "/seg-weak-etag",
            Seg::Capped => "/seg-capped",
            Seg::CappedFlaky => "/seg-capped-flaky",
            Seg::CappedDrop => "/seg-capped-drop",
            Seg::Drop => "/seg-drop",
            Seg::Unavailable => "/seg-503",
            Seg::Forbidden => "/seg-403",
            Seg::LiarEtag => "/liar-etag",
            Seg::Identity => "/seg-identity",
            Seg::Signed => "/seg-signed",
            Seg::Ramp => "/seg-ramp",
            Seg::RateLimited => "/seg-429",
            Seg::RateLimitedDrop => "/seg-429-drop",
            Seg::RateLimitedThenDrop => "/seg-429-then-drop",
        }
    }
}

fn seg_route(kind: Seg) -> MethodRouter<Arc<AppState>> {
    get(
        move |State(state): State<Arc<AppState>>, headers: HeaderMap| async move {
            if kind == Seg::Forbidden && headers.contains_key(header::RANGE) {
                state.stats(kind.route()).record(&headers);
                tokio::time::sleep(FORBIDDEN_DELAY).await;
                return StatusCode::FORBIDDEN.into_response();
            }
            seg_response(&state, &headers, kind)
        },
    )
}

/// Parse `Range: bytes=s-e` / `bytes=s-` into `(start, inclusive end)`.
fn parse_range(headers: &HeaderMap) -> Option<(usize, Option<usize>)> {
    let raw = headers.get(header::RANGE)?.to_str().ok()?;
    let (start, end) = raw.strip_prefix("bytes=")?.split_once('-')?;
    let end = match end.trim() {
        "" => None,
        e => Some(e.parse().ok()?),
    };
    Some((start.trim().parse().ok()?, end))
}

fn seg_response(state: &AppState, headers: &HeaderMap, kind: Seg) -> Response {
    let stats = state.stats(kind.route());
    let ranged_index = stats.record(headers);
    let rate_limited = match (kind, ranged_index) {
        (Seg::RateLimited, Some(i)) => i < state.cfg.seg_refusals,
        (Seg::RateLimitedDrop, Some(i)) => i % 2 == 1,
        (Seg::RateLimitedThenDrop, Some(i)) => i == 0,
        _ => false,
    };
    if rate_limited {
        stats.refuse();
        let retry_after = state
            .cfg
            .seg_retry_after
            .filter(|_| kind != Seg::RateLimitedDrop);
        return too_many_requests(retry_after);
    }
    let cap = matches!(kind, Seg::Capped | Seg::CappedFlaky | Seg::CappedDrop).then_some(SEG_CAP);
    let Some(guard) = InFlight::enter(stats, cap) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let range = parse_range(headers);
    if kind == Seg::Unavailable && range.is_some() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    let data = state.seg.clone();
    let total = data.len();
    let etag = if kind == Seg::WeakEtag {
        SEG_WEAK_ETAG
    } else {
        SEG_ETAG
    };
    let validator_ok = match headers.get(header::IF_RANGE).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(v) => (kind != Seg::WeakEtag && v == SEG_ETAG) || v == LAST_MODIFIED,
    };
    let honour_range = !matches!(kind, Seg::LiarEtag | Seg::NoAccept) && validator_ok;

    let (status, start, end) = match range {
        Some((s, e)) if honour_range && s < total => {
            let mut end = e.map_or(total, |e| e + 1).min(total);
            if kind == Seg::Short {
                end = end.min(s + SEG_SHORT_MAX);
            }
            (StatusCode::PARTIAL_CONTENT, s, end)
        }
        Some(_) if honour_range => {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{total}"))
                .body(Body::empty())
                .unwrap();
        }
        _ => (StatusCode::OK, 0, total),
    };
    let ranged = status == StatusCode::PARTIAL_CONTENT;

    let delay = match kind {
        Seg::Skewed if start == 0 => SKEWED_DELAY,
        Seg::Skewed | Seg::LiarEtag => Duration::ZERO,
        _ => state.cfg.seg_chunk_delay,
    };
    let abort_after = match kind {
        Seg::Drop | Seg::RateLimitedDrop if ranged => Some(state.cfg.seg_abort_after),
        Seg::RateLimitedThenDrop if ranged_index == Some(1) => Some(state.cfg.seg_abort_after),
        Seg::Flaky | Seg::CappedFlaky
            if ranged && !state.seg_flaky_tripped.swap(true, Ordering::SeqCst) =>
        {
            Some(2)
        }
        Seg::CappedDrop if ranged => Some(state.cfg.seg_abort_after),
        Seg::FlakyTail
            if ranged
                && end == total
                && !state.seg_flaky_tail_tripped.swap(true, Ordering::SeqCst) =>
        {
            Some(2)
        }
        _ => None,
    };
    let abort_alone = kind == Seg::FlakyTail;
    let hold = (kind == Seg::CappedFlaky && !ranged).then(|| state.gate.enter());

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, end - start)
        .header(header::ETAG, etag);
    if kind != Seg::NoAccept {
        builder = builder.header(header::ACCEPT_RANGES, "bytes");
    }
    if kind != Seg::LiarEtag {
        builder = builder.header(header::LAST_MODIFIED, LAST_MODIFIED);
    }
    if kind == Seg::Identity {
        builder = builder.header(header::CONTENT_ENCODING, "identity");
    }
    if ranged {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{}/{total}", end - 1),
        );
    }
    let body = if kind == Seg::Ramp {
        paced_stream(data.slice(start..end), state.ramp_next_free.clone(), guard)
    } else {
        seg_stream(
            data.slice(start..end),
            delay,
            abort_after,
            abort_alone,
            hold,
            guard,
        )
    };
    builder.body(body).unwrap()
}

/// Stream `data` in [`SEG_RAMP_CHUNK`]s, the `n`th no earlier than `n` chunk
/// times at [`SEG_RAMP_RATE`] after the response started, and each in its own
/// slot of the link shared through `next_free` (one chunk per
/// [`SEG_RAMP_CAP`] chunk time). Deadlines, not sleeps, so a late timer
/// (Windows' ~15.6 ms tick) only jitters the pace and never banks a burst.
fn paced_stream(data: Bytes, next_free: Arc<Mutex<tokio::time::Instant>>, guard: InFlight) -> Body {
    let own_gap = Duration::from_secs_f64(SEG_RAMP_CHUNK as f64 / SEG_RAMP_RATE as f64);
    let link_gap = Duration::from_secs_f64(SEG_RAMP_CHUNK as f64 / SEG_RAMP_CAP as f64);
    let started = tokio::time::Instant::now();
    let s = futures_util::stream::unfold(
        (data, 0usize, 1u32, guard),
        move |(data, offset, n, guard)| {
            let next_free = next_free.clone();
            async move {
                if offset >= data.len() {
                    return None;
                }
                tokio::time::sleep_until(started + own_gap * n).await;
                let slot = {
                    let mut next = next_free.lock().unwrap();
                    let slot = (*next).max(tokio::time::Instant::now());
                    *next = slot + link_gap;
                    slot
                };
                tokio::time::sleep_until(slot).await;
                let end = (offset + SEG_RAMP_CHUNK).min(data.len());
                let chunk = data.slice(offset..end);
                guard
                    .0
                    .body_bytes
                    .fetch_add(chunk.len() as u64, Ordering::SeqCst);
                Some((Ok::<_, std::io::Error>(chunk), (data, end, n + 1, guard)))
            }
        },
    );
    Body::from_stream(s)
}

/// Stream `data` in [`SEG_CHUNK`]s, sleeping `delay` before each; with
/// `abort_after`, error out after that many chunks (following [`ABORT_GRACE`],
/// and with `abort_alone` only once no other response on the route streams);
/// with `hold`, pause after the first chunk until that ticket is released.
/// Counts yielded bytes and holds the in-flight slot until dropped.
fn seg_stream(
    data: Bytes,
    delay: Duration,
    abort_after: Option<usize>,
    abort_alone: bool,
    hold: Option<GateTicket>,
    guard: InFlight,
) -> Body {
    let s = futures_util::stream::unfold(
        (data, 0usize, 0usize, Some(guard), hold),
        move |(data, offset, sent, guard, mut hold)| async move {
            let guard = guard?;
            if sent >= 1 {
                if let Some(ticket) = hold.take() {
                    while !ticket.released() {
                        tokio::time::sleep(GATED_TRICKLE).await;
                    }
                }
            }
            if abort_after.is_some_and(|limit| sent >= limit) && offset < data.len() {
                if abort_alone {
                    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
                    while guard.0.in_flight() > 1 && tokio::time::Instant::now() < deadline {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                }
                tokio::time::sleep(ABORT_GRACE).await;
                let err = std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "simulated mid-segment disconnect",
                );
                return Some((Err(err), (data, offset, sent, None, None)));
            }
            if offset >= data.len() {
                return None;
            }
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            let end = (offset + SEG_CHUNK).min(data.len());
            let chunk = data.slice(offset..end);
            guard
                .0
                .body_bytes
                .fetch_add(chunk.len() as u64, Ordering::SeqCst);
            Some((Ok(chunk), (data, end, sent + 1, Some(guard), hold)))
        },
    );
    Body::from_stream(s)
}

/// `/seg-redirect`: 307 to `/seg`.
async fn seg_redirect_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    state.stats("/seg-redirect").record(&headers);
    Redirect::temporary("/seg").into_response()
}

/// `/seg-expiring`: 307 to a newly minted `/seg-signed?sig=<n>`.
async fn seg_expiring_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    state.stats("/seg-expiring").record(&headers);
    let sig = state.next_sig.fetch_add(1, Ordering::SeqCst);
    Redirect::temporary(&format!("/seg-signed?sig={sig}")).into_response()
}

/// `/seg-signed`: `/seg` while the `sig` has uses left, then 403.
async fn seg_signed_handler(
    State(state): State<Arc<AppState>>,
    uri: axum::http::Uri,
    headers: HeaderMap,
) -> Response {
    let sig = uri
        .query()
        .and_then(|q| q.strip_prefix("sig="))
        .and_then(|s| s.parse::<u64>().ok());
    let live = sig.is_some_and(|sig| {
        let mut uses = state.sig_uses.lock().unwrap();
        let used = uses.entry(sig).or_default();
        *used += 1;
        *used <= SIGNED_URL_USES
    });
    if !live {
        let stats = state.stats("/seg-signed");
        stats.record(&headers);
        stats.refuse();
        return StatusCode::FORBIDDEN.into_response();
    }
    seg_response(&state, &headers, Seg::Signed)
}

/// A running test server. The background task is aborted on drop.
pub struct TestServer {
    /// Base URL, e.g. `http://127.0.0.1:52344`.
    pub base: String,
    state: Arc<AppState>,
    handle: JoinHandle<()>,
}

impl TestServer {
    /// Full URL for a path on this server, e.g. `url("/file")`.
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// Request accounting for a counted route, e.g. `stats("/seg")`.
    pub fn stats(&self, route: &str) -> Arc<RouteStats> {
        self.state.stats(route).clone()
    }

    /// The `/gated` hold, to release its requests.
    pub fn gate(&self) -> &Gate {
        &self.state.gate
    }

    /// The bytes the `/seg*` routes serve on this server.
    pub fn seg_payload(&self) -> Vec<u8> {
        self.state.seg.to_vec()
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
    spawn_with(FixtureConfig::default()).await
}

/// [`spawn`] with non-default tunables.
pub async fn spawn_with(cfg: FixtureConfig) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let state = Arc::new(AppState::new(cfg));
    let handle = tokio::spawn(serve_state(listener, state.clone()));

    TestServer {
        base: format!("http://{addr}"),
        state,
        handle,
    }
}

/// Serve the fixture routes on `listener` with `cfg` until the task is dropped.
pub async fn serve(listener: TcpListener, cfg: FixtureConfig) {
    serve_state(listener, Arc::new(AppState::new(cfg))).await;
}

async fn serve_state(listener: TcpListener, state: Arc<AppState>) {
    let app = Router::new()
        .route("/file", get(file_handler))
        .route("/named", get(named_handler))
        .route("/named/report-final.bin", get(named_handler))
        .route("/named-slow", get(named_slow_handler))
        .route("/redirect", get(redirect_handler))
        .route("/missing", get(missing_handler))
        .route("/ranged", get(ranged_handler))
        .route("/slow", get(slow_handler))
        .route("/crawl", get(crawl_handler))
        .route("/gated", get(gated_handler))
        .route("/norange", get(norange_handler))
        .route("/liar", get(liar_handler))
        .route("/flaky", get(flaky_handler))
        .route("/mutable", get(mutable_handler))
        .route("/mutable/bump", get(mutable_bump_handler))
        .route("/seg", seg_route(Seg::Plain))
        .route("/seg-flaky", seg_route(Seg::Flaky))
        .route("/seg-flaky-tail", seg_route(Seg::FlakyTail))
        .route("/seg-noaccept", seg_route(Seg::NoAccept))
        .route("/seg-short", seg_route(Seg::Short))
        .route("/seg-skewed", seg_route(Seg::Skewed))
        .route("/seg-redirect", get(seg_redirect_handler))
        .route("/seg-weak-etag", seg_route(Seg::WeakEtag))
        .route("/seg-capped", seg_route(Seg::Capped))
        .route("/seg-capped-flaky", seg_route(Seg::CappedFlaky))
        .route("/seg-capped-drop", seg_route(Seg::CappedDrop))
        .route("/seg-drop", seg_route(Seg::Drop))
        .route("/seg-503", seg_route(Seg::Unavailable))
        .route("/seg-403", seg_route(Seg::Forbidden))
        .route("/liar-etag", seg_route(Seg::LiarEtag))
        .route("/seg-identity", seg_route(Seg::Identity))
        .route("/seg-expiring", get(seg_expiring_handler))
        .route("/seg-signed", get(seg_signed_handler))
        .route("/seg-ramp", seg_route(Seg::Ramp))
        .route("/seg-429", seg_route(Seg::RateLimited))
        .route("/seg-429-drop", seg_route(Seg::RateLimitedDrop))
        .route("/seg-429-then-drop", seg_route(Seg::RateLimitedThenDrop))
        .route("/limited-once", get(limited_once_handler))
        .route("/stall", get(stall_handler))
        .route("/hang", get(hang_handler))
        .with_state(state);
    axum::serve(listener, app).await.expect("serve");
}
