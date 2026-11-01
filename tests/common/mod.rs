//! Shared, in-process HTTP server for the offline integration tests.
//!
//! A small `axum` app bound to an ephemeral loopback port. It gives us full
//! control over status codes, `Content-Disposition`, and redirects — and it is
//! the deterministic alternative to hitting the real network. Picking a tiny
//! hand-rolled server over a mocking crate (wiremock/httptest) keeps the
//! fixture flexible for later milestones (e.g. range requests in M1).

#![allow(dead_code)] // Not every test binary uses every helper.

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Deterministic payload served by the fixture (just under 100 KB, enough to
/// span many chunks). Reproduced byte-for-byte on every call.
pub fn payload() -> Vec<u8> {
    (0..99_991u32).map(|i| (i % 251) as u8).collect()
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

/// Spawn the fixture on a random free loopback port.
///
/// Routes:
/// - `GET /file` — the payload with `Content-Length`.
/// - `GET /named` — the payload with a `Content-Disposition` filename.
/// - `GET /redirect` — 307 to `/file`.
/// - `GET /missing` — 404.
pub async fn spawn() -> TestServer {
    let app = Router::new()
        .route("/file", get(file_handler))
        .route("/named", get(named_handler))
        .route("/redirect", get(redirect_handler))
        .route("/missing", get(missing_handler));

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
