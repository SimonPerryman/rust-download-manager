//! M5: [`Manager::clear`] removes finished downloads in bulk by status, with
//! the same file rule as `remove`: completed files and kept partials stay,
//! other partials go.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rdm::engine::state;
use rdm::{DownloadSnapshot, JsonStore, Manager, Status, Store};

const T: Duration = Duration::from_secs(10);

/// Build a manager over a fresh JSON store inside `dir`.
fn manager_in(dir: &Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

/// Poll snapshots for download `id` until `pred` holds or `timeout` elapses.
async fn wait_for(
    mgr: &Manager,
    id: u64,
    pred: impl Fn(&DownloadSnapshot) -> bool,
) -> DownloadSnapshot {
    let deadline = Instant::now() + T;
    loop {
        if let Some(s) = mgr.snapshot().into_iter().find(|s| s.id == id) {
            if pred(&s) {
                return s;
            }
        }
        assert!(Instant::now() < deadline, "download {id} timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_status(mgr: &Manager, id: u64, want: Status) -> DownloadSnapshot {
    wait_for(mgr, id, |s| s.status == want).await
}

/// A `/slow` download to `out`, stopped mid-stream; returns its id.
async fn stopped(mgr: &Manager, server: &common::TestServer, out: PathBuf, keep: bool) -> u64 {
    let id = mgr.add(
        server.url("/slow"),
        out.parent().unwrap().to_path_buf(),
        Some(out),
        None,
    );
    wait_for(mgr, id, |s| s.downloaded > 0).await;
    mgr.stop(id, keep);
    wait_status(mgr, id, Status::Cancelled).await;
    id
}

/// AC: `clear([completed, cancelled])` removes exactly those entries;
/// completed files stay on disk, kept partials stay, non-kept partials go.
#[tokio::test]
async fn clear_removes_exactly_the_given_statuses() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let done = mgr.add(server.url("/file"), dir.path(), None, None);
    let done_path = wait_status(&mgr, done, Status::Completed)
        .await
        .final_path
        .unwrap();
    let deleted_out = dir.path().join("deleted.bin");
    stopped(&mgr, &server, deleted_out.clone(), false).await;
    let kept_out = dir.path().join("kept.bin");
    stopped(&mgr, &server, kept_out.clone(), true).await;
    let failed = mgr.add(server.url("/missing"), dir.path(), None, None);
    wait_status(&mgr, failed, Status::Failed).await;
    assert!(state::part_path(&kept_out).exists());

    let removed = mgr.clear(&[Status::Completed, Status::Cancelled], None);
    assert_eq!(removed, 3);
    let left: Vec<u64> = mgr.snapshot().iter().map(|s| s.id).collect();
    assert_eq!(left, [failed], "only the failed download remains");

    assert!(done_path.exists(), "a completed file stays on disk");
    assert!(!state::part_path(&deleted_out).exists());
    assert!(!state::sidecar_path(&deleted_out).exists());
    assert!(state::part_path(&kept_out).exists(), "a kept partial stays");
    assert!(state::sidecar_path(&kept_out).exists());

    let stored = JsonStore::new(dir.path().join("state.json"))
        .load()
        .unwrap();
    assert_eq!(stored.downloads.len(), 1, "the clear is persisted");
}

/// `only` limits a clear to the listed ids (the rows a user saw), and a
/// non-terminal status never clears a running download.
#[tokio::test]
async fn clear_is_scoped_by_ids_and_skips_unfinished_downloads() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let a = mgr.add(server.url("/file"), dir.path(), None, None);
    let b = mgr.add(server.url("/file"), dir.path(), None, None);
    wait_status(&mgr, a, Status::Completed).await;
    wait_status(&mgr, b, Status::Completed).await;
    let running = mgr.add(server.url("/gated"), dir.path(), None, None);
    wait_status(&mgr, running, Status::Downloading).await;

    assert_eq!(mgr.clear(&[Status::Completed], Some(&[a, running])), 1);
    let left: Vec<u64> = mgr.snapshot().iter().map(|s| s.id).collect();
    assert_eq!(left, [b, running]);

    assert_eq!(
        mgr.clear(&[Status::Downloading, Status::Queued, Status::Paused], None),
        0
    );
    assert_eq!(
        mgr.snapshot()
            .iter()
            .find(|s| s.id == running)
            .unwrap()
            .status,
        Status::Downloading
    );

    server.gate().release_all();
    wait_status(&mgr, running, Status::Completed).await;
}

/// A download stopped without its partial owns nothing, so a newer download
/// may reuse its target. Clearing the old row, or reloading the store, must
/// not delete or adopt the newer download's kept partial.
#[tokio::test]
async fn clearing_a_deleted_stop_spares_a_newer_partial_at_the_same_target() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let old = mgr.add(server.url("/slow"), dir.path(), None, None);
    let target = wait_for(&mgr, old, |s| s.downloaded > 0).await.target;
    mgr.stop(old, false);
    wait_status(&mgr, old, Status::Cancelled).await;

    let new = mgr.add(server.url("/slow"), dir.path(), None, None);
    let partial = wait_for(&mgr, new, |s| s.downloaded > 0).await;
    assert_eq!(partial.target, target, "the freed target is reused");
    mgr.stop(new, true);
    let kept = wait_status(&mgr, new, Status::Cancelled).await;
    assert!(kept.partial_kept);
    drop(mgr);

    let mgr = manager_in(dir.path());
    let restored = mgr.snapshot();
    let old_row = restored.iter().find(|s| s.id == old).unwrap();
    assert_eq!(old_row.downloaded, 0, "the old row adopted the new partial");
    assert!(old_row.segments.is_empty());
    let new_row = restored.iter().find(|s| s.id == new).unwrap();
    assert_eq!(new_row.downloaded, kept.downloaded);

    assert_eq!(mgr.clear(&[Status::Cancelled], Some(&[old])), 1);
    assert!(state::part_path(&target).exists(), "the kept `.part` went");
    assert!(
        state::sidecar_path(&target).exists(),
        "the kept sidecar went"
    );

    mgr.resume(new);
    let done = wait_status(&mgr, new, Status::Completed).await;
    assert_eq!(
        std::fs::read(done.final_path.unwrap()).unwrap(),
        common::payload()
    );
}

/// Removing a deleted-stop row leaves a running download at the same target
/// alone.
#[tokio::test]
async fn removing_a_deleted_stop_spares_a_running_download_at_the_same_target() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let old = mgr.add(server.url("/gated"), dir.path(), None, None);
    let target = wait_for(&mgr, old, |s| s.downloaded > 0).await.target;
    mgr.stop(old, false);
    wait_status(&mgr, old, Status::Cancelled).await;

    let running = mgr.add(server.url("/gated"), dir.path(), None, None);
    let partial = wait_for(&mgr, running, |s| s.downloaded > 0).await;
    assert_eq!(partial.target, target, "the freed target is reused");

    mgr.remove(old);
    assert!(state::part_path(&target).exists(), "the live `.part` went");
    assert!(
        state::sidecar_path(&target).exists(),
        "the live sidecar went"
    );

    server.gate().release_all();
    wait_status(&mgr, running, Status::Completed).await;
}
