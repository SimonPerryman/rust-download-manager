//! Headless tests for the engine-side [`Manager`]: the full add/pause/resume/
//! cancel/remove lifecycle and restart persistence, driven against the
//! in-process fixture with no UI involved. These assert the M2 acceptance
//! criteria at the engine layer (the GUI is a thin frontend over exactly this),
//! plus M4's queue, concurrency cap and speed-limit settings.

mod common;

use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use common::FixtureConfig;
use rdm::engine::state::{self, DownloadState};
use rdm::{
    AddOptions, Checksum, DownloadSnapshot, JsonStore, Manager, RateLimiter, Settings, Status,
    Store,
};

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

/// Removing a download drops it from the list and cleans up any partial data,
/// including sidecar temp files a hard kill left behind.
#[tokio::test]
async fn remove_drops_entry_and_partial() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/slow"), dir.path(), None, None);
    let snap = wait_for(&mgr, id, T, |s| s.downloaded > 0).await.unwrap();
    let target = snap.target;
    // As if an earlier run was killed mid-checkpoint.
    let mut stale_temp = state::sidecar_path(&target).into_os_string();
    stale_temp.push(".4242.7.tmp");
    std::fs::write(&stale_temp, b"{}").unwrap();

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
    assert!(
        !std::path::Path::new(&stale_temp).exists(),
        "stale sidecar temp left"
    );
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

/// A fixture whose `/seg` streams slowly enough to act mid-flight.
async fn slow_seg_server() -> common::TestServer {
    common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(40),
        ..FixtureConfig::default()
    })
    .await
}

/// M3: a multi-segment download paused through the manager is restored by a
/// fresh manager with its real progress (the checkpointed segment total, not
/// the pre-allocated `.part` size) and resumes to a byte-identical file.
#[tokio::test]
async fn paused_multi_segment_download_restores_progress_and_completes() {
    let server = slow_seg_server().await;
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    let total = common::SEG_LEN as u64;

    let (id, target) = {
        let mgr = manager_in(dir.path());
        let id = mgr.add(server.url("/seg"), &downloads, None, None);
        let partial = wait_for(&mgr, id, T, |s| s.downloaded > 0 && s.downloaded < total)
            .await
            .expect("never reached partial progress");
        mgr.pause(id);
        assert!(
            wait_status(&mgr, id, Status::Paused, T).await,
            "never paused"
        );
        (id, partial.target)
    };

    let st = DownloadState::load(&state::sidecar_path(&target)).unwrap();
    assert_eq!(st.version, 2, "should have run multi-segment");
    let done: u64 = st.segments.iter().flatten().map(|s| s.done).sum();
    assert!(done > 0 && done < total, "checkpointed {done} of {total}");

    let mgr = manager_in(dir.path());
    let restored = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(restored.status, Status::Paused);
    assert_eq!(restored.downloaded, done, "restored readout is Σdone");

    mgr.resume(id);
    assert!(
        wait_status(&mgr, id, Status::Completed, T).await,
        "never resumed to completion"
    );
    let snap = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(
        std::fs::read(snap.final_path.unwrap()).unwrap(),
        server.seg_payload()
    );
}

/// M3: removing a running multi-segment download leaves no `.part`, sidecar or
/// temp file behind — not even from a checkpoint still in flight.
#[tokio::test]
async fn removing_running_multi_segment_download_leaves_no_files() {
    let server = slow_seg_server().await;
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    let total = common::SEG_LEN as u64;

    let mgr = manager_in(dir.path());
    let id = mgr.add(server.url("/seg"), &downloads, None, None);
    let target = wait_for(&mgr, id, T, |s| s.downloaded > 0 && s.downloaded < total)
        .await
        .expect("never reached partial progress")
        .target;
    mgr.remove(id);

    let part = state::part_path(&target);
    let sidecar = state::sidecar_path(&target);
    let all_gone = || {
        let tmp_left = std::fs::read_dir(&downloads)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            })
            .unwrap_or(false);
        !part.exists() && !sidecar.exists() && !tmp_left
    };
    let deadline = Instant::now() + T;
    while !all_gone() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(all_gone(), "partial state left after remove");

    // Outlast one checkpoint interval: nothing reappears.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(all_gone(), "a late checkpoint resurrected partial state");
}

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

/// Poll `cond` until it holds or `timeout` elapses.
async fn wait_until(timeout: Duration, cond: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while !cond() {
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    true
}

/// Ids currently in `status`, in display order.
fn ids_in(mgr: &Manager, status: Status) -> Vec<u64> {
    mgr.snapshot()
        .into_iter()
        .filter(|s| s.status == status)
        .map(|s| s.id)
        .collect()
}

fn status_of(mgr: &Manager, id: u64) -> Option<Status> {
    mgr.snapshot()
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| s.status)
}

/// Long enough for an unwanted promotion to have shown up as a request.
const SETTLE: Duration = Duration::from_millis(200);

/// M4 AC: with a cap of 3, ten downloads never run more than three at once;
/// the rest wait as Queued and start in FIFO order as slots free.
#[tokio::test]
async fn queue_caps_concurrency_and_promotes_in_fifo_order() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    assert_eq!(mgr.settings().max_concurrent, 3, "default cap");
    mgr.set_max_connections(1);
    let gated = server.stats("/gated");

    let ids: Vec<u64> = (0..10)
        .map(|_| mgr.add(server.url("/gated"), dir.path(), None, None))
        .collect();
    assert!(wait_until(T, || gated.requests() == 3).await);
    tokio::time::sleep(SETTLE).await;
    assert_eq!(gated.requests(), 3, "a fourth download started");
    assert_eq!(ids_in(&mgr, Status::Downloading), ids[..3]);
    assert_eq!(ids_in(&mgr, Status::Queued), ids[3..]);

    server.gate().release_next().expect("a held request");
    assert!(wait_until(T, || gated.requests() == 4).await);
    assert!(wait_status(&mgr, ids[3], Status::Downloading, T).await);
    assert_eq!(ids_in(&mgr, Status::Completed).len(), 1);
    assert_eq!(ids_in(&mgr, Status::Queued), ids[4..]);

    server.gate().release_all();
    for &id in &ids {
        completed(&mgr, id).await;
    }
    assert_eq!(gated.requests(), 10);
}

/// M4 AC: pausing, cancelling or removing a queued download takes it out of
/// the queue without a request; the rest still promote in order, and pausing
/// or removing the running download promotes the queue head.
#[tokio::test]
async fn queued_downloads_leave_the_queue_without_starting() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    mgr.set_max_concurrent(1);
    let gated = server.stats("/gated");

    let ids: Vec<u64> = (0..7)
        .map(|_| mgr.add(server.url("/gated"), dir.path(), None, None))
        .collect();
    let [a, b, c, d, e, f, g] = ids[..] else {
        unreachable!()
    };
    assert!(wait_until(T, || gated.requests() == 1).await);

    mgr.pause(b);
    mgr.cancel(c);
    mgr.remove(d);
    assert_eq!(status_of(&mgr, b), Some(Status::Paused));
    assert_eq!(status_of(&mgr, c), Some(Status::Cancelled));
    assert_eq!(status_of(&mgr, d), None);
    tokio::time::sleep(SETTLE).await;
    assert_eq!(gated.requests(), 1, "a queued download made a request");
    assert_eq!(ids_in(&mgr, Status::Downloading), [a]);
    assert_eq!(ids_in(&mgr, Status::Queued), [e, f, g]);

    // Releasing the runner starts the next *remaining* queued download.
    server.gate().release_next().expect("a held request");
    completed(&mgr, a).await;
    assert!(wait_until(T, || gated.requests() == 2).await);
    assert_eq!(status_of(&mgr, e), Some(Status::Downloading));
    assert_eq!(status_of(&mgr, b), Some(Status::Paused));

    // Pausing the running (trickling) download promotes the head.
    mgr.pause(e);
    assert!(wait_status(&mgr, e, Status::Paused, T).await);
    assert!(wait_until(T, || gated.requests() == 3).await);
    assert_eq!(status_of(&mgr, f), Some(Status::Downloading));

    // So does removing it.
    mgr.remove(f);
    assert!(wait_until(T, || gated.requests() == 4).await);
    assert_eq!(status_of(&mgr, g), Some(Status::Downloading));

    // A paused-while-queued download rejoins the queue and completes.
    mgr.resume(b);
    assert_eq!(status_of(&mgr, b), Some(Status::Queued));
    server.gate().release_all();
    completed(&mgr, g).await;
    completed(&mgr, b).await;
    assert_eq!(gated.requests(), 5);
    assert_eq!(status_of(&mgr, e), Some(Status::Paused));
}

/// M4: lowering the cap never interrupts running downloads; it only holds
/// back promotion until fewer than the new cap are running.
#[tokio::test]
async fn lowering_the_cap_never_preempts() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let gated = server.stats("/gated");

    let ids: Vec<u64> = (0..4)
        .map(|_| mgr.add(server.url("/gated"), dir.path(), None, None))
        .collect();
    assert!(wait_until(T, || gated.requests() == 3).await);

    mgr.set_max_concurrent(1);
    tokio::time::sleep(SETTLE).await;
    assert_eq!(ids_in(&mgr, Status::Downloading), ids[..3], "preempted");
    assert_eq!(mgr.settings().max_concurrent, 1);

    for done in 1..=2 {
        server.gate().release_next().expect("a held request");
        assert!(wait_until(T, || ids_in(&mgr, Status::Completed).len() == done).await);
        tokio::time::sleep(SETTLE).await;
        assert_eq!(gated.requests(), 3, "promoted while at or over the cap");
        assert_eq!(status_of(&mgr, ids[3]), Some(Status::Queued));
    }

    server.gate().release_next().expect("a held request");
    assert!(wait_until(T, || gated.requests() == 4).await);
    server.gate().release_all();
    completed(&mgr, ids[3]).await;
}

/// M4 AC: a global limit caps the aggregate throughput of concurrent
/// downloads. With debt, the last wait ends only once the shared balance is
/// repaid, so `elapsed >= (total - burst) / rate`; slow CI only lengthens it.
#[tokio::test]
async fn global_limit_caps_aggregate_throughput() {
    const SEG_LEN: usize = 512 * 1024;
    const RATE: u64 = 512 * KIB;
    let server = common::spawn_with(FixtureConfig {
        seg_len: SEG_LEN,
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    mgr.set_global_limit(Some(RATE));
    let burst = RateLimiter::new(Some(RATE)).burst();
    let total = 3 * SEG_LEN as u64;
    let ideal = Duration::from_secs_f64((total - burst) as f64 / RATE as f64);

    let started = Instant::now();
    let ids: Vec<u64> = (0..3)
        .map(|_| mgr.add(server.url("/seg"), dir.path(), None, None))
        .collect();

    // All three run at once (cap 3) with no per-download limit, so only the
    // shared global bucket can be throttling them. Progress is reported after
    // each chunk's throttle wait, so "has bytes" isn't a reliable overlap probe.
    assert_eq!(ids_in(&mgr, Status::Downloading), ids, "not all concurrent");

    for &id in &ids {
        let snap = wait_for(&mgr, id, 3 * T, |s| s.status == Status::Completed)
            .await
            .unwrap_or_else(|| panic!("download {id} never completed"));
        assert_eq!(
            std::fs::read(snap.final_path.unwrap()).unwrap(),
            server.seg_payload()
        );
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed >= ideal.mul_f64(0.95),
        "took {elapsed:?}, expected at least ~{ideal:?}"
    );
}

/// A fixture whose `/seg` outlasts several one-second ramp windows on one
/// connection (8 MiB at ~800 KiB/s), so the default ramp gets to add one.
async fn ramping_seg_server() -> common::TestServer {
    common::spawn_with(FixtureConfig {
        seg_len: 8 * MIB as usize,
        seg_chunk_delay: Duration::from_millis(40),
        ..FixtureConfig::default()
    })
    .await
}

/// M4: the max-connections setting bounds each download's parallel requests,
/// while the ramp still opens more than one.
#[tokio::test]
async fn max_connections_setting_bounds_parallel_requests() {
    let server = ramping_seg_server().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    mgr.set_max_connections(2);

    let id = mgr.add(server.url("/seg"), dir.path(), None, None);
    wait_for(&mgr, id, 3 * T, |s| s.status == Status::Completed)
        .await
        .expect("never completed");
    let max = server.stats("/seg").max_in_flight();
    assert!(max <= 2, "{max} connections, setting is 2");
    assert!(max > 1, "never ramped past one connection");
}

/// M4: a snapshot shows the live connection count while a download ramps,
/// and zero once it is paused.
#[tokio::test]
async fn snapshot_reports_live_connections() {
    let server = ramping_seg_server().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/seg"), dir.path(), None, None);
    let ramped = wait_for(&mgr, id, 3 * T, |s| s.connections >= 2)
        .await
        .expect("never reported a second connection");
    assert_eq!(ramped.status, Status::Downloading);
    assert!(
        ramped.connections <= 4,
        "{} connections",
        ramped.connections
    );

    mgr.pause(id);
    assert!(wait_status(&mgr, id, Status::Paused, T).await);
    let paused = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(paused.connections, 0);
}

/// M4: settings and per-download limits persist across a restart, and the
/// setters normalise their input.
#[tokio::test]
async fn settings_and_limits_survive_restart() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let (limited, unlimited) = {
        let mgr = manager_in(dir.path());
        mgr.set_max_concurrent(0);
        assert_eq!(mgr.settings().max_concurrent, 1, "cap is at least 1");
        mgr.set_max_concurrent(5);
        mgr.set_global_limit(Some(MIB));
        mgr.set_max_connections(200);
        assert_eq!(mgr.settings().max_connections, 16, "clamped to 16");
        mgr.set_max_connections(8);

        let options = |limit| AddOptions {
            limit,
            ..AddOptions::default()
        };
        let limited = mgr.add_with(server.url("/file"), dir.path(), options(Some(256 * KIB)));
        let unlimited = mgr.add_with(server.url("/file"), dir.path(), options(Some(0)));
        assert_eq!(completed(&mgr, limited).await.limit, Some(256 * KIB));
        assert_eq!(
            completed(&mgr, unlimited).await.limit,
            None,
            "0 = unlimited"
        );
        (limited, unlimited)
    };

    let mgr = manager_in(dir.path());
    assert_eq!(
        mgr.settings(),
        Settings {
            max_concurrent: 5,
            global_limit: Some(MIB),
            max_connections: 8,
        }
    );
    let snap = mgr.snapshot();
    let limit_of = |id| snap.iter().find(|s| s.id == id).unwrap().limit;
    assert_eq!(limit_of(limited), Some(256 * KIB));
    assert_eq!(limit_of(unlimited), None);
}

/// M4: a limit changed with `set_limit` on an unfinished download (the GUI's
/// row editor) persists across a restart, and so does clearing it.
#[tokio::test]
async fn set_limit_survives_restart() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let limit_of = |mgr: &Manager, id| mgr.snapshot().iter().find(|s| s.id == id).unwrap().limit;

    let id = {
        let mgr = manager_in(dir.path());
        let id = mgr.add(server.url("/gated"), dir.path(), None, None);
        mgr.set_limit(id, Some(128 * KIB));
        id
    };
    let mgr = manager_in(dir.path());
    assert_eq!(limit_of(&mgr, id), Some(128 * KIB));

    mgr.set_limit(id, None);
    drop(mgr);
    assert_eq!(limit_of(&manager_in(dir.path()), id), None);
}

/// M4: a per-download limit changes live: lifting it mid-download lets one
/// that would take a minute finish at once.
#[tokio::test]
async fn set_limit_applies_to_a_running_download() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    // 4 MiB at 64 KiB/s would take ~64 s.
    let options = AddOptions {
        limit: Some(64 * KIB),
        ..AddOptions::default()
    };
    let id = mgr.add_with(server.url("/seg"), dir.path(), options);
    let total = common::SEG_LEN as u64;
    wait_for(&mgr, id, T, |s| s.downloaded > 0 && s.downloaded < total)
        .await
        .expect("never streamed");

    mgr.set_limit(id, None);
    let snap = completed(&mgr, id).await;
    assert_eq!(snap.limit, None);
    assert_eq!(
        std::fs::read(snap.final_path.unwrap()).unwrap(),
        server.seg_payload()
    );
}

/// M4: raising the cap promotes queued downloads at once, without waiting
/// for a running one to finish.
#[tokio::test]
async fn raising_the_cap_promotes_queued_downloads() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    mgr.set_max_concurrent(1);
    mgr.set_max_connections(1);
    let gated = server.stats("/gated");

    let ids: Vec<u64> = (0..3)
        .map(|_| mgr.add(server.url("/gated"), dir.path(), None, None))
        .collect();
    assert!(wait_until(T, || gated.requests() == 1).await);
    tokio::time::sleep(SETTLE).await;
    assert_eq!(ids_in(&mgr, Status::Queued), ids[1..]);

    mgr.set_max_concurrent(3);
    assert!(
        wait_until(T, || gated.requests() == 3).await,
        "raising the cap didn't promote the queue"
    );
    assert_eq!(ids_in(&mgr, Status::Downloading), ids);

    server.gate().release_all();
    for &id in &ids {
        completed(&mgr, id).await;
    }
}

/// M4: a download still queued at shutdown is restored paused, like a running
/// one, so launching the app never starts it unasked; resuming it completes.
#[tokio::test]
async fn queued_download_is_restored_paused() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let gated = server.stats("/gated");

    let queued = {
        let mgr = manager_in(dir.path());
        mgr.set_max_concurrent(1);
        mgr.set_max_connections(1);
        mgr.add(server.url("/gated"), dir.path(), None, None);
        let queued = mgr.add(server.url("/gated"), dir.path(), None, None);
        assert!(wait_until(T, || gated.requests() == 1).await);
        assert_eq!(status_of(&mgr, queued), Some(Status::Queued));
        queued
    };

    let mgr = manager_in(dir.path());
    assert_eq!(status_of(&mgr, queued), Some(Status::Paused));
    tokio::time::sleep(SETTLE).await;
    assert_eq!(gated.requests(), 1, "a restored queued download started");

    mgr.resume(queued);
    assert!(wait_until(T, || gated.requests() == 2).await);
    server.gate().release_all();
    completed(&mgr, queued).await;
}

/// M4: cancelling a download that is queued after a resume (so it owns a
/// `.part` but has no task to clean up) discards its partial state.
#[tokio::test]
async fn cancelling_a_queued_resumed_download_discards_partial() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    mgr.set_max_concurrent(1);
    let gated = server.stats("/gated");

    let slow = mgr.add(server.url("/slow"), dir.path(), None, None);
    let target = wait_for(&mgr, slow, T, |s| s.downloaded > 0)
        .await
        .expect("never streamed")
        .target;
    mgr.pause(slow);
    assert!(wait_status(&mgr, slow, Status::Paused, T).await);
    let (part, sidecar) = (state::part_path(&target), state::sidecar_path(&target));
    assert!(
        part.exists() && sidecar.exists(),
        "paused without partial state"
    );

    let runner = mgr.add(server.url("/gated"), dir.path(), None, None);
    assert!(wait_until(T, || gated.requests() == 1).await);
    mgr.resume(slow);
    assert_eq!(status_of(&mgr, slow), Some(Status::Queued));

    mgr.cancel(slow);
    assert_eq!(status_of(&mgr, slow), Some(Status::Cancelled));
    assert!(!part.exists(), "`.part` must be gone");
    assert!(!sidecar.exists(), "sidecar must be gone");

    server.gate().release_all();
    completed(&mgr, runner).await;
}
