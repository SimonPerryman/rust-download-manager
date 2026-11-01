//! Offline integration tests: drive the engine against the in-process fixture.

mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rdm::{download, Control, DownloadRequest, Error};

/// The engine writes the served bytes to disk, byte-for-byte, and reports them.
#[tokio::test]
async fn downloads_bytes_identical() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let req = DownloadRequest::new(server.url("/file"), dir.path());
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    let expected = common::payload();
    assert_eq!(summary.status, 200);
    assert_eq!(summary.bytes, expected.len() as u64);
    // Filename derived from the URL's last segment.
    assert_eq!(summary.path.file_name().unwrap(), "file");

    let on_disk = std::fs::read(&summary.path).unwrap();
    assert_eq!(
        on_disk, expected,
        "bytes on disk must match what was served"
    );
}

/// Progress is reported and ends exactly at the total size.
#[tokio::test]
async fn progress_reaches_total() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let last = Arc::new(AtomicU64::new(0));
    let total = Arc::new(AtomicU64::new(0));
    let (l, t) = (last.clone(), total.clone());

    let req = DownloadRequest::new(server.url("/file"), dir.path());
    let summary = download(&req, &Control::new(), move |p| {
        l.store(p.downloaded, Ordering::SeqCst);
        if let Some(tot) = p.total {
            t.store(tot, Ordering::SeqCst);
        }
    })
    .await
    .unwrap();

    assert_eq!(last.load(Ordering::SeqCst), summary.bytes);
    assert_eq!(total.load(Ordering::SeqCst), summary.bytes);
}

/// `Content-Disposition` supplies the filename when there's no explicit output.
#[tokio::test]
async fn content_disposition_sets_filename() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let req = DownloadRequest::new(server.url("/named"), dir.path());
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.path.file_name().unwrap(), "report-final.bin");
    assert_eq!(summary.path.parent().unwrap(), dir.path());
}

/// An explicit output path overrides any derived name.
#[tokio::test]
async fn explicit_output_path_is_honoured() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("chosen.dat");

    let req = DownloadRequest::to_path(server.url("/named"), &out);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.path, out);
    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
}

/// A claim hook receives the `Content-Disposition`-derived target and the
/// engine writes to whatever path it returns instead.
#[tokio::test]
async fn claim_target_hook_chooses_derived_path() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let claimed = dir.path().join("claimed.bin");

    let proposals = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (seen, chosen) = (proposals.clone(), claimed.clone());
    let req = DownloadRequest::new(server.url("/named"), dir.path()).with_claim_target(
        move |proposed: &std::path::Path| {
            seen.lock().unwrap().push(proposed.to_path_buf());
            chosen.clone()
        },
    );
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(
        *proposals.lock().unwrap(),
        vec![dir.path().join("report-final.bin")]
    );
    assert_eq!(summary.path, claimed);
    assert_eq!(std::fs::read(&claimed).unwrap(), common::payload());
    assert!(!dir.path().join("report-final.bin").exists());
}

/// With an explicit output path there is nothing to claim: the hook never runs.
#[tokio::test]
async fn explicit_output_path_skips_claim_hook() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("chosen.dat");

    let calls = Arc::new(AtomicU64::new(0));
    let counter = calls.clone();
    let req = DownloadRequest::to_path(server.url("/named"), &out).with_claim_target(move |p| {
        counter.fetch_add(1, Ordering::SeqCst);
        p.to_path_buf()
    });
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.path, out);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// Redirects are followed to completion.
#[tokio::test]
async fn follows_redirects() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let req = DownloadRequest::new(server.url("/redirect"), dir.path());
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.bytes, common::payload().len() as u64);
    // Name comes from the final URL after the redirect.
    assert_eq!(summary.path.file_name().unwrap(), "file");
}

/// A non-success status is surfaced as `Error::Status`.
#[tokio::test]
async fn non_success_status_errors() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let req = DownloadRequest::new(server.url("/missing"), dir.path());
    let err = download(&req, &Control::new(), |_| {}).await.unwrap_err();

    assert!(matches!(err, Error::Status(404)), "got {err:?}");
}

/// Cancelling before the download starts aborts it and leaves no file behind.
#[tokio::test]
async fn cancel_aborts_and_cleans_up() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("cancelled.bin");

    let control = Control::new();
    control.cancel();

    let req = DownloadRequest::to_path(server.url("/file"), &out);
    let err = download(&req, &control, |_| {}).await.unwrap_err();

    assert!(matches!(err, Error::Cancelled), "got {err:?}");
    assert!(!out.exists(), "partial file must be removed on cancel");
}
