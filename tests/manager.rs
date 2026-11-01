//! Headless tests for the engine-side [`Manager`]: the full add/pause/resume/
//! cancel/remove lifecycle and restart persistence, driven against the
//! in-process fixture with no UI involved. These assert the M2 acceptance
//! criteria at the engine layer (the GUI is a thin frontend over exactly this).

mod common;

use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use rdm::engine::state;
use rdm::{Checksum, DownloadSnapshot, JsonStore, Manager, Status, Store};

/// Build a manager over a fresh JSON store inside `dir`.
fn manager_in(dir: &std::path::Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

/// SHA-256 of a buffer, hex-encoded (mirrors the engine's own encoding).
fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let digest = Sha256::digest(bytes);
    digest.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Poll snapshots for download `id` until `pred` holds or `timeout` elapses.
async fn wait_for(
    mgr: &Manager,
    id: u64,
    timeout: Duration,
    pred: impl Fn(&DownloadSnapshot) -> bool,
) -> Option<DownloadSnapshot> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(s) = mgr.snapshot().into_iter().find(|s| s.id == id) {
            if pred(&s) {
                return Some(s);
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_status(mgr: &Manager, id: u64, want: Status, timeout: Duration) -> bool {
    wait_for(mgr, id, timeout, |s| s.status == want)
        .await
        .is_some()
}

const T: Duration = Duration::from_secs(10);

/// A plain download started through the manager runs to completion and writes
/// the served bytes to disk.
#[tokio::test]
async fn add_completes_and_writes_file() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/file"), dir.path(), None, None);
    assert!(
        wait_status(&mgr, id, Status::Completed, T).await,
        "never completed"
    );

    let snap = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    let final_path = snap
        .final_path
        .expect("completed download has a final path");
    assert_eq!(std::fs::read(&final_path).unwrap(), common::payload());
    assert_eq!(snap.downloaded, common::payload().len() as u64);
}

/// AC #1: start, pause, and resume a download entirely through the manager; the
/// resumed file is byte-identical.
#[tokio::test]
async fn pause_then_resume_completes() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/slow"), dir.path(), None, None);

    // Let it make partial progress, then pause.
    let partial = wait_for(&mgr, id, T, |s| {
        s.downloaded > 0 && s.total.map(|t| s.downloaded < t).unwrap_or(true)
    })
    .await;
    assert!(partial.is_some(), "never reached partial progress");
    let target = partial.unwrap().target;

    mgr.pause(id);
    assert!(
        wait_status(&mgr, id, Status::Paused, T).await,
        "never paused"
    );
    assert!(
        state::part_path(&target).exists(),
        "`.part` must be kept across a pause"
    );

    mgr.resume(id);
    assert!(
        wait_status(&mgr, id, Status::Completed, T).await,
        "never resumed to completion"
    );

    let snap = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    let final_path = snap.final_path.unwrap();
    assert_eq!(std::fs::read(&final_path).unwrap(), common::payload());
}

/// Cancelling a running download discards its partial data.
#[tokio::test]
async fn cancel_discards_partial() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/slow"), dir.path(), None, None);
    let snap = wait_for(&mgr, id, T, |s| s.downloaded > 0).await.unwrap();
    let target = snap.target;

    mgr.cancel(id);
    assert!(
        wait_status(&mgr, id, Status::Cancelled, T).await,
        "never cancelled"
    );
    assert!(!state::part_path(&target).exists(), "`.part` must be gone");
    assert!(
        !state::sidecar_path(&target).exists(),
        "sidecar must be gone"
    );
    assert!(!target.exists(), "no target file on cancel");
}

/// AC #2: the download list and history survive an app restart — a new manager
/// over the same store restores both a completed download and a paused one, and
/// the paused one can still be resumed to completion.
#[tokio::test]
async fn list_and_history_survive_restart() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let done_id;
    let paused_id;
    {
        let mgr = manager_in(dir.path());

        done_id = mgr.add(server.url("/file"), dir.path(), None, None);
        assert!(wait_status(&mgr, done_id, Status::Completed, T).await);

        paused_id = mgr.add(server.url("/slow"), dir.path(), None, None);
        assert!(wait_for(&mgr, paused_id, T, |s| s.downloaded > 0)
            .await
            .is_some());
        mgr.pause(paused_id);
        assert!(wait_status(&mgr, paused_id, Status::Paused, T).await);

        drop(mgr); // simulate app shutdown
    }

    // Restart: a brand-new manager over the same store.
    let mgr = manager_in(dir.path());
    let restored = mgr.snapshot();
    assert_eq!(restored.len(), 2, "both downloads restored");

    let done = restored.iter().find(|s| s.id == done_id).unwrap();
    assert_eq!(done.status, Status::Completed);
    assert!(done.final_path.as_ref().unwrap().exists());

    let paused = restored.iter().find(|s| s.id == paused_id).unwrap();
    assert_eq!(paused.status, Status::Paused);

    // The restored paused download still resumes to a byte-identical file.
    mgr.resume(paused_id);
    assert!(wait_status(&mgr, paused_id, Status::Completed, T).await);
    let snap = mgr
        .snapshot()
        .into_iter()
        .find(|s| s.id == paused_id)
        .unwrap();
    assert_eq!(
        std::fs::read(snap.final_path.unwrap()).unwrap(),
        common::payload()
    );
}

/// `subscribe` delivers live snapshots as state changes.
#[tokio::test]
async fn subscribe_delivers_live_updates() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let mut rx = mgr.subscribe();
    assert!(rx.borrow().is_empty(), "starts empty");

    let id = mgr.add(server.url("/file"), dir.path(), None, None);

    // The watch should reflect the new download, and eventually its completion.
    let saw_completion = tokio::time::timeout(T, async {
        loop {
            if rx.changed().await.is_err() {
                return false;
            }
            let snaps = rx.borrow();
            if snaps
                .iter()
                .any(|s| s.id == id && s.status == Status::Completed)
            {
                return true;
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(saw_completion, "never observed completion via subscribe");
}

/// Removing a download drops it from the list and cleans up any partial data.
#[tokio::test]
async fn remove_drops_entry_and_partial() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/slow"), dir.path(), None, None);
    let snap = wait_for(&mgr, id, T, |s| s.downloaded > 0).await.unwrap();
    let target = snap.target;

    mgr.remove(id);
    // Give the aborted task a moment to unwind.
    assert!(
        wait_for(&mgr, id, Duration::from_millis(500), |_| true)
            .await
            .is_none(),
        "entry must be gone from the list"
    );
    assert!(mgr.snapshot().is_empty());
    assert!(!state::part_path(&target).exists());
}

/// A wrong checksum surfaces as a failed download with an error message.
#[tokio::test]
async fn bad_checksum_marks_failed() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let wrong: Checksum = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        .parse()
        .unwrap();
    let id = mgr.add(server.url("/ranged"), dir.path(), None, Some(wrong));

    assert!(
        wait_status(&mgr, id, Status::Failed, T).await,
        "should fail"
    );
    let snap = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert!(snap.error.is_some(), "failure carries a reason");
}

/// A correct checksum completes normally (checksum plumbs through the manager).
#[tokio::test]
async fn good_checksum_completes() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let sum: Checksum = format!("sha256:{}", sha256_hex(&common::payload()))
        .parse()
        .unwrap();
    let id = mgr.add(server.url("/ranged"), dir.path(), None, Some(sum));

    assert!(
        wait_status(&mgr, id, Status::Completed, T).await,
        "should complete"
    );
}

/// Regression: the manager must drive downloads when `add` is called from a
/// *synchronous* context with no ambient tokio runtime — exactly how a GUI's
/// sync command handler calls it. It spawns onto the handle it was given, not
/// an ambient one. (Before the explicit-handle fix this panicked with "there is
/// no reactor running".) This is a plain `#[test]`, deliberately not
/// `#[tokio::test]`, so there is no current runtime on the calling thread.
#[test]
fn add_from_sync_context_without_ambient_runtime() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    // Start the fixture server on the runtime.
    let server = rt.block_on(common::spawn());
    let dir = tempfile::tempdir().unwrap();
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.path().join("state.json")));
    let mgr = Manager::load(store, rt.handle().clone()).unwrap();

    // The critical call: on the main thread, outside any async context.
    let id = mgr.add(server.url("/file"), dir.path(), None, None);

    // Drive to completion on the runtime.
    let completed = rt.block_on(async {
        let deadline = Instant::now() + T;
        while Instant::now() < deadline {
            if let Some(s) = mgr.snapshot().into_iter().find(|s| s.id == id) {
                if s.status == Status::Completed {
                    return true;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    });
    assert!(
        completed,
        "download added from a sync context should complete"
    );
}

/// The final snapshot of download `id`, panicking if it never completes.
async fn completed(mgr: &Manager, id: u64) -> DownloadSnapshot {
    wait_for(mgr, id, T, |s| s.status == Status::Completed)
        .await
        .unwrap_or_else(|| panic!("download {id} never completed"))
}

/// Without an explicit `out`, the manager honours the server's
/// `Content-Disposition` filename (the URL itself names `named`).
#[tokio::test]
async fn add_uses_content_disposition_name() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/named"), dir.path(), None, None);
    let snap = completed(&mgr, id).await;

    let expected = dir.path().join("report-final.bin");
    assert_eq!(snap.filename, "report-final.bin");
    assert_eq!(snap.target, expected);
    assert_eq!(snap.final_path.as_deref(), Some(expected.as_path()));
    assert_eq!(std::fs::read(&expected).unwrap(), common::payload());
    assert!(!dir.path().join("named").exists());
}

/// A download's own URL-derived placeholder never counts as "taken" when it
/// claims the same name from `Content-Disposition`.
#[tokio::test]
async fn claim_does_not_collide_with_own_placeholder() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(
        server.url("/named/report-final.bin"),
        dir.path(),
        None,
        None,
    );
    let snap = completed(&mgr, id).await;
    assert_eq!(snap.filename, "report-final.bin");
}

/// Concurrent downloads claiming the same server name get distinct targets, so
/// their `.part` files never collide, and both finish byte-identical.
#[tokio::test]
async fn concurrent_same_name_downloads_are_deduplicated() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let a = mgr.add(server.url("/named-slow"), dir.path(), None, None);
    let b = mgr.add(server.url("/named-slow"), dir.path(), None, None);
    let (a, b) = (completed(&mgr, a).await, completed(&mgr, b).await);

    let mut names = vec![a.filename.clone(), b.filename.clone()];
    names.sort();
    assert_eq!(names, ["report-final (1).bin", "report-final.bin"]);
    for snap in [a, b] {
        let path = snap.final_path.unwrap();
        assert_eq!(path, snap.target, "no finalize-time rename was needed");
        assert_eq!(std::fs::read(path).unwrap(), common::payload());
    }
}

/// An existing file on disk is not clobbered by a claimed name.
#[tokio::test]
async fn claim_avoids_existing_file() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("report-final.bin");
    std::fs::write(&existing, b"keep me").unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/named"), dir.path(), None, None);
    let snap = completed(&mgr, id).await;

    assert_eq!(snap.filename, "report-final (1).bin");
    assert_eq!(std::fs::read(&existing).unwrap(), b"keep me");
    assert_eq!(
        std::fs::read(snap.final_path.unwrap()).unwrap(),
        common::payload()
    );
}

/// A claimed name survives an app restart: a download paused after claiming its
/// `Content-Disposition` name resumes under that name in a fresh manager.
#[tokio::test]
async fn claimed_name_survives_restart_and_resumes() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let expected = dir.path().join("report-final.bin");

    let id;
    {
        let mgr = manager_in(dir.path());
        id = mgr.add(server.url("/named-slow"), dir.path(), None, None);
        let placeholder = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
        assert_eq!(
            placeholder.filename, "named-slow",
            "URL-derived until claimed"
        );
        let partial = wait_for(&mgr, id, T, |s| s.downloaded > 0).await.unwrap();
        assert_eq!(partial.target, expected, "claimed before bytes stream");
        mgr.pause(id);
        assert!(wait_status(&mgr, id, Status::Paused, T).await);
        assert!(state::part_path(&expected).exists());
    }

    let mgr = manager_in(dir.path());
    let restored = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(restored.status, Status::Paused);
    assert_eq!(restored.filename, "report-final.bin");
    assert!(
        restored.downloaded > 0,
        "progress restored from the `.part`"
    );

    mgr.resume(id);
    let snap = completed(&mgr, id).await;
    assert_eq!(snap.final_path.as_deref(), Some(expected.as_path()));
    assert_eq!(std::fs::read(&expected).unwrap(), common::payload());
}

/// An explicit `out` is used as-is; the server's name is never claimed.
#[tokio::test]
async fn explicit_out_ignores_content_disposition() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let out = dir.path().join("chosen.dat");

    let id = mgr.add(server.url("/named"), dir.path(), Some(out.clone()), None);
    let snap = completed(&mgr, id).await;
    assert_eq!(snap.final_path, Some(out));
    assert!(!dir.path().join("report-final.bin").exists());
}

/// A download restored before it ever claimed a name owns nothing on disk:
/// cancelling it must not delete a same-named `.part` belonging to another.
#[tokio::test]
async fn cancelling_unclaimed_download_leaves_other_partials() {
    let dir = tempfile::tempdir().unwrap();
    let placeholder = dir.path().join("shared.bin");
    let other_part = state::part_path(&placeholder);
    std::fs::write(&other_part, b"someone else's bytes").unwrap();
    let doc = serde_json::json!({
        "next_id": 2,
        "downloads": [{
            "id": 1,
            "url": "http://127.0.0.1:9/shared.bin",
            "target": placeholder,
            "target_provisional": true,
            "status": "downloading",
        }],
    });
    std::fs::write(dir.path().join("state.json"), doc.to_string()).unwrap();

    let mgr = manager_in(dir.path());
    let restored = mgr.snapshot().into_iter().find(|s| s.id == 1).unwrap();
    assert_eq!(restored.status, Status::Paused);
    assert_eq!(
        restored.downloaded, 0,
        "another download's `.part` is not ours"
    );

    mgr.cancel(1);
    assert!(wait_status(&mgr, 1, Status::Cancelled, T).await);
    assert!(other_part.exists());
}
