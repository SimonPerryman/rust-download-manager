//! M1 acceptance tests, in-process: pause/resume, resume-after-restart against
//! range-capable / range-less / lying / changed-resource servers, retry with
//! backoff, integrity, and filename collisions. Each test proves byte-identical
//! results against `common::payload()` (or `payload_v2()`).

mod common;

use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use rdm::engine::state::{self, DownloadState};
use rdm::{download, Checksum, Control, DownloadRequest, Error, RetryPolicy};

/// Lowercase hex of a byte slice (mirrors `live_smoke.rs`).
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// SHA-256 of a buffer, hex-encoded.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex(&h.finalize())
}

/// A fast retry policy so backoff never slows the suite.
fn fast_retry() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 10,
        base_delay: Duration::from_millis(1),
        factor: 1.0,
        max_delay: Duration::from_millis(1),
    }
}

/// Pre-seed a partial `.part` (first `len` payload bytes) plus a matching
/// sidecar carrying `etag`/`total`, as if an earlier run was interrupted.
fn seed_partial(target: &std::path::Path, url: &str, prefix: &[u8], etag: &str, total: u64) {
    let part = state::part_path(target);
    let sidecar = state::sidecar_path(target);
    std::fs::write(&part, prefix).unwrap();
    let st = DownloadState::new(
        url,
        target,
        Some(total),
        Some(etag.to_string()),
        Some(common::LAST_MODIFIED.to_string()),
    );
    st.save(&sidecar).unwrap();
}

/// Poll until `cond()` holds or `timeout` elapses; returns whether it held.
async fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    cond()
}

/// AC: Pausing then resuming produces a byte-identical file.
#[tokio::test]
async fn pause_then_resume_is_byte_identical() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("paused.bin");
    let part = state::part_path(&out);

    let req = DownloadRequest::to_path(server.url("/slow"), &out);
    let control = Control::new();

    // First leg: start the slow download, pause once the `.part` is partial.
    let total = common::payload().len() as u64;
    let c = control.clone();
    let r = req.clone();
    let task = tokio::spawn(async move { download(&r, &c, |_| {}).await });

    let partial = wait_until(Duration::from_secs(10), || {
        std::fs::metadata(&part)
            .map(|m| m.len() > 0 && m.len() < total)
            .unwrap_or(false)
    })
    .await;
    assert!(partial, "`.part` never reached a partial size");
    control.pause();

    let first = task.await.unwrap();
    assert!(
        matches!(first, Err(Error::Paused)),
        "first leg should pause, got {first:?}"
    );
    assert!(part.exists(), "`.part` must be kept across a pause");

    // Second leg: clear the pause and resume the same request to completion.
    control.resume();
    let summary = download(&req, &control, |_| {}).await.unwrap();

    assert_eq!(summary.path, out);
    let on_disk = std::fs::read(&out).unwrap();
    assert_eq!(
        on_disk,
        common::payload(),
        "resumed file must match payload"
    );
    assert_eq!(summary.bytes, total);
}

/// AC: A server that doesn't support ranges falls back to a clean restart.
#[tokio::test]
async fn norange_server_restarts_cleanly() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("norange.bin");

    let full = common::payload();
    seed_partial(
        &out,
        &server.url("/norange"),
        &full[..10_000],
        "\"v1\"",
        full.len() as u64,
    );

    let req = DownloadRequest::to_path(server.url("/norange"), &out);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(
        summary.status, 200,
        "no-range server replies 200 to a Range"
    );
    let on_disk = std::fs::read(&out).unwrap();
    assert_eq!(
        on_disk, full,
        "must be the full body, not partial+full concatenated"
    );
    assert_eq!(on_disk.len(), full.len());
}

/// AC: A server that lies about range support is detected; no corruption.
#[tokio::test]
async fn range_liar_does_not_corrupt() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("liar.bin");

    let full = common::payload();
    seed_partial(
        &out,
        &server.url("/liar"),
        &full[..25_000],
        "\"v1\"",
        full.len() as u64,
    );

    let req = DownloadRequest::to_path(server.url("/liar"), &out);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.status, 200);
    let on_disk = std::fs::read(&out).unwrap();
    assert_eq!(
        on_disk, full,
        "lying server's 200 must truncate+restart, not append"
    );
    assert_eq!(sha256_hex(&on_disk), sha256_hex(&full));
}

/// AC: Resource changed between runs → If-Range no longer matches → clean
/// restart onto the new version (no mixing of v1/v2 bytes).
#[tokio::test]
async fn changed_resource_restarts_onto_v2() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("mutable.bin");

    let v1 = common::payload();
    let v2 = common::payload_v2();
    // Seed a partial of v1 with the v1 ETag, as if we'd started before the bump.
    seed_partial(
        &out,
        &server.url("/mutable"),
        &v1[..15_000],
        "\"v1\"",
        v1.len() as u64,
    );

    // Flip the server to v2.
    server.bump_mutable();

    let req = DownloadRequest::to_path(server.url("/mutable"), &out);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(
        summary.status, 200,
        "stale If-Range must force a 200 restart"
    );
    let on_disk = std::fs::read(&out).unwrap();
    assert_eq!(on_disk, v2, "must be exactly v2, no v1 bytes mixed in");
    assert_ne!(on_disk, v1);
    assert_eq!(summary.bytes, v2.len() as u64);
}

/// AC: Transient failures are retried with backoff and the download completes.
#[tokio::test]
async fn flaky_server_retries_to_completion() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("flaky.bin");

    let req = DownloadRequest::to_path(server.url("/flaky"), &out).with_retry(fast_retry());
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    let full = common::payload();
    let on_disk = std::fs::read(&out).unwrap();
    assert_eq!(on_disk, full, "retried download must be byte-identical");
    assert_eq!(summary.bytes, full.len() as u64);
}

/// A claimed target is kept across retries: the hook runs once and the retried
/// attempts resume the claimed `.part`.
#[tokio::test]
async fn claim_hook_runs_once_across_retries() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let claimed = dir.path().join("claimed.bin");

    let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let (counter, chosen) = (calls.clone(), claimed.clone());
    let req = DownloadRequest::new(server.url("/flaky"), dir.path())
        .with_retry(fast_retry())
        .with_claim_target(move |_| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            chosen.clone()
        });
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(summary.path, claimed);
    assert_eq!(std::fs::read(&claimed).unwrap(), common::payload());
}

/// AC: Integrity — a correct checksum succeeds.
#[tokio::test]
async fn checksum_match_succeeds() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("sum-ok.bin");

    let digest = sha256_hex(&common::payload());
    let checksum: Checksum = format!("sha256:{digest}").parse().unwrap();

    let req = DownloadRequest::to_path(server.url("/ranged"), &out).with_checksum(checksum);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
    assert_eq!(summary.bytes, common::payload().len() as u64);
}

/// AC: Integrity — a wrong checksum fails and the target is NOT created (the
/// bad `.part` is left behind, never promoted into place).
#[tokio::test]
async fn checksum_mismatch_errors_and_leaves_no_target() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("sum-bad.bin");

    let wrong: Checksum = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        .parse()
        .unwrap();

    let req = DownloadRequest::to_path(server.url("/ranged"), &out).with_checksum(wrong);
    let err = download(&req, &Control::new(), |_| {}).await.unwrap_err();

    assert!(matches!(err, Error::ChecksumMismatch(_)), "got {err:?}");
    assert!(
        !out.exists(),
        "a checksum failure must not create the target"
    );
    assert!(
        state::part_path(&out).exists(),
        "the failed `.part` is left behind, not renamed into place"
    );
}

/// AC: Filename collision — downloading the same derived name twice into one
/// directory lands the second at `file (1)`; both hold the payload.
#[tokio::test]
async fn filename_collision_gets_suffixed() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let req = DownloadRequest::new(server.url("/file"), dir.path());

    let first = download(&req, &Control::new(), |_| {}).await.unwrap();
    assert_eq!(first.path.file_name().unwrap(), "file");

    let second = download(&req, &Control::new(), |_| {}).await.unwrap();
    assert_eq!(second.path.file_name().unwrap(), "file (1)");

    assert_eq!(std::fs::read(&first.path).unwrap(), common::payload());
    assert_eq!(std::fs::read(&second.path).unwrap(), common::payload());
}
