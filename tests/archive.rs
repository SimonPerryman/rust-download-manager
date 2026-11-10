//! Archive: [`Manager::archive`], [`Manager::unarchive`] and
//! [`Manager::archive_finished`] hide finished downloads from the working
//! views without dropping their records, and `remove` never deletes a
//! downloaded file.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rdm::engine::state;
use rdm::{DownloadRecord, DownloadSnapshot, JsonStore, Manager, Status, Store, StoreData};

const T: Duration = Duration::from_secs(10);

/// Build a manager over the JSON store `dir/state.json`.
fn manager_in(dir: &Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

/// Poll snapshots for download `id` until `pred` holds or `T` elapses.
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

fn row(mgr: &Manager, id: u64) -> DownloadSnapshot {
    mgr.snapshot().into_iter().find(|s| s.id == id).unwrap()
}

fn archived_ids(mgr: &Manager) -> Vec<u64> {
    mgr.snapshot()
        .iter()
        .filter(|s| s.archived)
        .map(|s| s.id)
        .collect()
}

/// AC: `archive` sets the flag on completed, failed and cancelled downloads;
/// a queued, downloading or paused one is left unchanged and not counted.
/// `unarchive` reverses it.
#[tokio::test]
async fn archive_takes_only_finished_downloads() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let completed = mgr.add(server.url("/file"), dir.path(), None, None);
    wait_status(&mgr, completed, Status::Completed).await;
    let failed = mgr.add(server.url("/missing"), dir.path(), None, None);
    wait_status(&mgr, failed, Status::Failed).await;
    let cancelled = mgr.add(server.url("/slow"), dir.path(), None, None);
    wait_for(&mgr, cancelled, |s| s.downloaded > 0).await;
    mgr.stop(cancelled, false);
    wait_status(&mgr, cancelled, Status::Cancelled).await;
    let paused = mgr.add(server.url("/slow"), dir.path(), None, None);
    wait_for(&mgr, paused, |s| s.downloaded > 0).await;
    mgr.pause(paused);
    wait_status(&mgr, paused, Status::Paused).await;
    mgr.set_max_concurrent(1);
    let downloading = mgr.add(server.url("/gated"), dir.path(), None, None);
    wait_status(&mgr, downloading, Status::Downloading).await;
    let queued = mgr.add(server.url("/file"), dir.path(), None, None);
    assert_eq!(row(&mgr, queued).status, Status::Queued);

    let all = [completed, failed, cancelled, paused, downloading, queued];
    assert_eq!(mgr.archive(&all), 3);
    assert_eq!(archived_ids(&mgr), [completed, failed, cancelled]);
    for id in [paused, downloading, queued] {
        assert!(!row(&mgr, id).archived, "download {id} was archived");
    }
    assert_eq!(mgr.archive(&all), 0, "archiving twice changes nothing");

    assert_eq!(mgr.unarchive(&all), 3);
    assert!(archived_ids(&mgr).is_empty());
    assert_eq!(mgr.unarchive(&all), 0);

    server.gate().release_all();
    wait_status(&mgr, queued, Status::Completed).await;
}

/// AC: `archive_finished([completed, cancelled], None)` archives exactly
/// those and returns their ids; `only` limits it to the listed ids.
#[tokio::test]
async fn archive_finished_takes_the_given_statuses() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let a = mgr.add(server.url("/file"), dir.path(), None, None);
    let b = mgr.add(server.url("/file"), dir.path(), None, None);
    wait_status(&mgr, a, Status::Completed).await;
    wait_status(&mgr, b, Status::Completed).await;
    let failed = mgr.add(server.url("/missing"), dir.path(), None, None);
    wait_status(&mgr, failed, Status::Failed).await;
    let stopped = mgr.add(server.url("/slow"), dir.path(), None, None);
    wait_for(&mgr, stopped, |s| s.downloaded > 0).await;
    mgr.stop(stopped, true);
    wait_status(&mgr, stopped, Status::Cancelled).await;

    let statuses = [Status::Completed, Status::Cancelled];
    assert_eq!(mgr.archive_finished(&statuses, Some(&[b, failed])), [b]);
    assert_eq!(mgr.archive_finished(&statuses, None), [a, stopped]);
    assert_eq!(archived_ids(&mgr), [a, b, stopped]);
    assert!(mgr.archive_finished(&statuses, None).is_empty());
    assert!(
        state::part_path(&row(&mgr, stopped).target).exists(),
        "archiving keeps a kept partial"
    );

    let stored = JsonStore::new(dir.path().join("state.json"))
        .load()
        .unwrap();
    let flags: Vec<bool> = stored.downloads.iter().map(|r| r.archived).collect();
    assert_eq!(flags, [true, true, false, true], "the archive is persisted");
}

/// AC: removing an archived completed download drops the record and leaves
/// its file on disk, byte-identical.
#[tokio::test]
async fn remove_keeps_an_archived_download_file() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/file"), dir.path(), None, None);
    let path = wait_status(&mgr, id, Status::Completed)
        .await
        .final_path
        .unwrap();
    assert_eq!(mgr.archive(&[id]), 1);

    mgr.remove(id);
    assert!(mgr.snapshot().is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), common::payload());
    let stored = JsonStore::new(dir.path().join("state.json"))
        .load()
        .unwrap();
    assert!(stored.downloads.is_empty(), "the removal is persisted");
}

/// Owner decision: removing a failed download deletes its leftover partial,
/// which isn't a usable file, archived or not.
#[tokio::test]
async fn remove_deletes_a_failed_download_partial() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("broken.bin");
    std::fs::write(state::part_path(&target), b"half").unwrap();
    std::fs::write(state::sidecar_path(&target), b"{}").unwrap();
    let data = StoreData {
        next_id: 2,
        downloads: vec![failed_record(1, &target)],
        ..StoreData::default()
    };
    JsonStore::new(dir.path().join("state.json"))
        .save(&data)
        .unwrap();

    let mgr = manager_in(dir.path());
    assert!(row(&mgr, 1).archived);
    mgr.remove(1);
    assert!(mgr.snapshot().is_empty());
    assert!(!state::part_path(&target).exists());
    assert!(!state::sidecar_path(&target).exists());
}

/// An archived failed download at `target`, as a store would hold it.
fn failed_record(id: u64, target: &Path) -> DownloadRecord {
    let json = serde_json::json!({
        "id": id,
        "url": "http://example.invalid/broken.bin",
        "target": target,
        "status": "failed",
        "error": "server returned HTTP 500",
        "archived": true,
    });
    serde_json::from_value(json).unwrap()
}

/// AC: archived and tagged records save and reload equal through
/// `Manager::load`, and the store is written as schema version 2.
#[tokio::test]
async fn archive_and_tags_survive_a_restart() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let a = mgr.add(server.url("/file"), dir.path(), None, None);
    let b = mgr.add(server.url("/file"), dir.path(), None, None);
    wait_status(&mgr, a, Status::Completed).await;
    wait_status(&mgr, b, Status::Completed).await;
    mgr.archive(&[a]);
    mgr.add_tags(&[a, b], &["work".into(), "ISO".into()])
        .unwrap();
    mgr.add_tags(&[b], &["course-2".into()]).unwrap();
    let before = mgr.snapshot();
    mgr.shutdown();
    drop(mgr);

    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("state.json")).unwrap()).unwrap();
    assert_eq!(raw["version"], 2);

    let mgr = manager_in(dir.path());
    let after = mgr.snapshot();
    let view = |ss: &[DownloadSnapshot]| -> Vec<(u64, bool, Vec<String>)> {
        ss.iter()
            .map(|s| (s.id, s.archived, s.tags.clone()))
            .collect()
    };
    assert_eq!(view(&after), view(&before));
    assert_eq!(
        view(&after),
        [
            (a, true, vec!["work".to_string(), "ISO".to_string()]),
            (
                b,
                false,
                vec!["work".into(), "ISO".into(), "course-2".into()]
            ),
        ]
    );
}

/// Only finished downloads are archived, so retrying an archived one brings
/// it back to the working views.
#[tokio::test]
async fn resuming_an_archived_download_unarchives_it() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/slow"), dir.path(), None, None);
    wait_for(&mgr, id, |s| s.downloaded > 0).await;
    mgr.stop(id, true);
    wait_status(&mgr, id, Status::Cancelled).await;
    mgr.archive(&[id]);
    assert!(row(&mgr, id).archived);

    mgr.resume(id);
    assert!(!row(&mgr, id).archived);
    wait_status(&mgr, id, Status::Completed).await;
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

/// `remove`'s file rule for each finished status: a completed file and a
/// kept partial stay on disk; a partial that wasn't kept is already gone.
#[tokio::test]
async fn remove_keeps_files_and_kept_partials() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let done = mgr.add(server.url("/file"), dir.path(), None, None);
    let done_path = wait_status(&mgr, done, Status::Completed)
        .await
        .final_path
        .unwrap();
    let deleted_out = dir.path().join("deleted.bin");
    let deleted = stopped(&mgr, &server, deleted_out.clone(), false).await;
    let kept_out = dir.path().join("kept.bin");
    let kept = stopped(&mgr, &server, kept_out.clone(), true).await;
    assert!(state::part_path(&kept_out).exists());

    for id in [done, deleted, kept] {
        mgr.remove(id);
    }
    assert!(mgr.snapshot().is_empty());
    assert!(done_path.exists(), "a completed file stays on disk");
    assert!(!state::part_path(&deleted_out).exists());
    assert!(!state::sidecar_path(&deleted_out).exists());
    assert!(state::part_path(&kept_out).exists(), "a kept partial stays");
    assert!(state::sidecar_path(&kept_out).exists());
}

/// A download stopped without its partial owns nothing, so a newer download
/// may reuse its target. Removing the old row, or reloading the store, must
/// not delete or adopt the newer download's kept partial.
#[tokio::test]
async fn removing_a_deleted_stop_spares_a_newer_partial_at_the_same_target() {
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

    mgr.archive(&[old]);
    mgr.remove(old);
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
