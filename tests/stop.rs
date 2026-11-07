//! M5: [`Manager::stop`] with and without keeping the partial file, driven
//! against the in-process fixture. A kept partial resumes byte-identical,
//! fetching only the ranges that were still missing.

mod common;

use std::time::{Duration, Instant};

use common::FixtureConfig;
use rdm::engine::state::{self, DownloadState};
use rdm::{DownloadSnapshot, JsonStore, Manager, Segment, Status, Store};

const T: Duration = Duration::from_secs(10);

/// Build a manager over a fresh JSON store inside `dir`.
fn manager_in(dir: &std::path::Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
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

async fn wait_status(mgr: &Manager, id: u64, want: Status) -> DownloadSnapshot {
    wait_for(mgr, id, T, |s| s.status == want)
        .await
        .unwrap_or_else(|| panic!("download {id} never reached {want:?}"))
}

/// A `/seg` slow enough that a multi-segment download outlasts the stop.
async fn slow_seg_server() -> common::TestServer {
    common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(20),
        ..FixtureConfig::default()
    })
    .await
}

/// Start a multi-segment `/seg` download and wait until it has bytes on disk;
/// returns its id and claimed target.
async fn started(
    mgr: &Manager,
    server: &common::TestServer,
    dir: &std::path::Path,
) -> (u64, std::path::PathBuf) {
    let total = common::SEG_LEN as u64;
    let id = mgr.add(server.url("/seg"), dir, None, None);
    let partial = wait_for(mgr, id, T, |s| s.downloaded > 0 && s.downloaded < total)
        .await
        .expect("never reached partial progress");
    (id, partial.target)
}

/// AC: stopping with `keep_partial` leaves `.part` + sidecar and sets
/// `partial_kept`; resuming completes byte-identical, fetching only the ranges
/// the kept sidecar says are missing.
#[tokio::test]
async fn keep_partial_resumes_only_the_missing_ranges() {
    let server = slow_seg_server().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let (id, target) = started(&mgr, &server, dir.path()).await;

    mgr.stop(id, true);
    let stopped = wait_status(&mgr, id, Status::Cancelled).await;
    let (part, sidecar) = (state::part_path(&target), state::sidecar_path(&target));
    assert!(part.exists(), "`.part` must be kept");
    assert!(sidecar.exists(), "sidecar must be kept");
    assert!(stopped.partial_kept);
    assert!(stopped.downloaded > 0, "progress is kept");

    let kept: Vec<Segment> = DownloadState::load(&sidecar)
        .unwrap()
        .segments
        .expect("ran multi-segment");
    let kept_done: u64 = kept.iter().map(|s| s.done).sum();
    assert!(kept_done > 0 && kept_done < common::SEG_LEN as u64);
    assert_eq!(stopped.segments, kept, "the snapshot shows the kept tiling");

    let stats = server.stats("/seg");
    stats.reset();
    mgr.resume(id);
    let done = wait_status(&mgr, id, Status::Completed).await;
    assert!(!done.partial_kept);
    assert_eq!(
        std::fs::read(done.final_path.unwrap()).unwrap(),
        server.seg_payload()
    );

    assert_eq!(stats.unranged(), 0, "a resume never refetches from the top");
    let ranges = stats.closed_ranges();
    for &(start, end) in &ranges {
        assert!(
            kept.iter()
                .any(|s| s.start + s.done <= start && end <= s.end),
            "range {start}..{end} refetches kept bytes"
        );
    }
    // Each unfinished kept segment is picked up exactly where it stopped.
    for seg in kept.iter().filter(|s| s.done < s.end - s.start) {
        let first = ranges
            .iter()
            .filter(|&&(start, end)| seg.start <= start && end <= seg.end)
            .map(|&(start, _)| start)
            .min();
        assert_eq!(first, Some(seg.start + seg.done), "segment {seg:?}");
    }
}

/// AC: stopping without keeping deletes the `.part` and sidecar.
#[tokio::test]
async fn delete_partial_removes_part_and_sidecar() {
    let server = slow_seg_server().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let (id, target) = started(&mgr, &server, dir.path()).await;

    mgr.stop(id, false);
    let stopped = wait_status(&mgr, id, Status::Cancelled).await;
    assert!(!stopped.partial_kept);
    assert!(stopped.segments.is_empty());
    assert!(!state::part_path(&target).exists(), "`.part` must be gone");
    assert!(
        !state::sidecar_path(&target).exists(),
        "sidecar must be gone"
    );
}

/// A delete-stop sent right after a keep-stop, before the task has acted on
/// either, wins: the task sees the cancel first, and nothing is kept. (The
/// rarer order, a pause already seen when the cancel lands, is a defensive arm
/// in `finish` that no fixture can time deterministically.)
#[tokio::test]
async fn delete_right_after_keep_removes_the_partial() {
    let server = slow_seg_server().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let (id, target) = started(&mgr, &server, dir.path()).await;

    mgr.stop(id, true);
    mgr.stop(id, false);
    let stopped = wait_status(&mgr, id, Status::Cancelled).await;
    assert!(!stopped.partial_kept);
    assert_eq!(stopped.downloaded, 0);
    assert!(!state::part_path(&target).exists(), "`.part` must be gone");
    assert!(
        !state::sidecar_path(&target).exists(),
        "sidecar must be gone"
    );
}

/// A kept partial can be deleted later by stopping again without keeping.
#[tokio::test]
async fn kept_partial_can_be_deleted_later() {
    let server = slow_seg_server().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let (id, target) = started(&mgr, &server, dir.path()).await;

    mgr.stop(id, true);
    assert!(wait_status(&mgr, id, Status::Cancelled).await.partial_kept);
    assert!(state::part_path(&target).exists());

    mgr.stop(id, false);
    let deleted = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(deleted.status, Status::Cancelled);
    assert!(!deleted.partial_kept);
    assert!(!state::part_path(&target).exists(), "`.part` must be gone");
    assert!(
        !state::sidecar_path(&target).exists(),
        "sidecar must be gone"
    );
}

/// A kept partial survives a restart of the store and still resumes.
#[tokio::test]
async fn kept_partial_survives_restart() {
    let server = slow_seg_server().await;
    let dir = tempfile::tempdir().unwrap();
    let (id, kept) = {
        let mgr = manager_in(dir.path());
        let (id, _) = started(&mgr, &server, dir.path()).await;
        mgr.stop(id, true);
        (id, wait_status(&mgr, id, Status::Cancelled).await)
    };

    let mgr = manager_in(dir.path());
    let restored = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(restored.status, Status::Cancelled);
    assert!(restored.partial_kept);
    assert_eq!(restored.downloaded, kept.downloaded);
    assert_eq!(
        restored.segments, kept.segments,
        "segments reload from the sidecar"
    );

    mgr.resume(id);
    let done = wait_status(&mgr, id, Status::Completed).await;
    assert_eq!(
        std::fs::read(done.final_path.unwrap()).unwrap(),
        server.seg_payload()
    );
}

/// A keep-stop of a paused download (no task running) keeps its partial,
/// which then resumes byte-identical.
#[tokio::test]
async fn keep_partial_of_a_paused_download() {
    let server = slow_seg_server().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let (id, target) = started(&mgr, &server, dir.path()).await;
    mgr.pause(id);
    wait_status(&mgr, id, Status::Paused).await;

    mgr.stop(id, true);
    let stopped = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(stopped.status, Status::Cancelled);
    assert!(stopped.partial_kept);
    assert!(state::part_path(&target).exists(), "`.part` must be kept");
    assert!(
        state::sidecar_path(&target).exists(),
        "sidecar must be kept"
    );

    mgr.resume(id);
    let done = wait_status(&mgr, id, Status::Completed).await;
    assert_eq!(
        std::fs::read(done.final_path.unwrap()).unwrap(),
        server.seg_payload()
    );
}

/// A keep-stop of a queued download that never started has nothing to keep.
#[tokio::test]
async fn keep_partial_of_a_never_started_download_keeps_nothing() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    mgr.set_max_concurrent(1);
    let runner = mgr.add(server.url("/gated"), dir.path(), None, None);
    wait_status(&mgr, runner, Status::Downloading).await;
    let out = dir.path().join("waiting.bin");
    let id = mgr.add(server.url("/file"), dir.path(), Some(out.clone()), None);
    assert_eq!(
        mgr.snapshot()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap()
            .status,
        Status::Queued
    );

    mgr.stop(id, true);
    let stopped = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(stopped.status, Status::Cancelled);
    assert!(!stopped.partial_kept);
    assert!(!state::part_path(&out).exists());

    server.gate().release_all();
    wait_status(&mgr, runner, Status::Completed).await;
}

/// Restarting a deleted-stop row whose target a newer download has since
/// claimed moves it to a free path, so the two never share a `.part`.
#[tokio::test]
async fn restarting_a_deleted_stop_never_shares_a_newer_downloads_target() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let old = mgr.add(server.url("/slow"), dir.path(), None, None);
    let target = wait_for(&mgr, old, T, |s| s.downloaded > 0)
        .await
        .expect("old never streamed")
        .target;
    mgr.stop(old, false);
    wait_status(&mgr, old, Status::Cancelled).await;

    let new = mgr.add(server.url("/slow"), dir.path(), None, None);
    let partial = wait_for(&mgr, new, T, |s| s.downloaded > 0)
        .await
        .expect("new never streamed");
    assert_eq!(partial.target, target, "the freed target is reused");
    mgr.stop(new, true);
    assert!(wait_status(&mgr, new, Status::Cancelled).await.partial_kept);
    let kept_sidecar = std::fs::read(state::sidecar_path(&target)).unwrap();

    mgr.resume(old);
    let restarted = mgr.snapshot().into_iter().find(|s| s.id == old).unwrap();
    assert_ne!(restarted.target, target, "restarted onto a claimed target");
    let old_done = wait_status(&mgr, old, Status::Completed).await;
    assert_eq!(
        std::fs::read(old_done.final_path.unwrap()).unwrap(),
        common::payload()
    );
    assert!(state::part_path(&target).exists(), "the kept `.part` went");
    assert_eq!(
        std::fs::read(state::sidecar_path(&target)).unwrap(),
        kept_sidecar,
        "the kept sidecar changed"
    );

    mgr.resume(new);
    let new_done = wait_status(&mgr, new, Status::Completed).await;
    assert_eq!(new_done.final_path.as_deref(), Some(target.as_path()));
    assert_eq!(std::fs::read(&target).unwrap(), common::payload());
}
