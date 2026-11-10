//! A restart into an update (`Manager::shutdown_for_restart`) pauses like a
//! plain quit but remembers which downloads it paused, so they can be resumed
//! together.

mod common;

use std::time::{Duration, Instant};

use rdm::engine::state;
use rdm::{DownloadSnapshot, JsonStore, Manager, Status, Store};

const T: Duration = Duration::from_secs(10);

fn manager_in(dir: &std::path::Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

fn status(mgr: &Manager, id: u64) -> Status {
    snap(mgr, id).status
}

fn snap(mgr: &Manager, id: u64) -> DownloadSnapshot {
    mgr.snapshot().into_iter().find(|s| s.id == id).unwrap()
}

async fn wait_status(mgr: &Manager, id: u64, want: Status) {
    let deadline = Instant::now() + T;
    while status(mgr, id) != want {
        assert!(
            Instant::now() < deadline,
            "download {id} never reached {want:?} (is {:?})",
            status(mgr, id)
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn saved_marks(dir: &std::path::Path) -> Vec<bool> {
    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("state.json")).unwrap()).unwrap();
    raw["downloads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["paused_for_update"].as_bool().unwrap_or(false))
        .collect()
}

/// One download the user paused, one running and one queued (one slot), then
/// a restart into an update and a reload. Returns them in that order.
async fn after_an_update_restart(
    server: &common::TestServer,
    dir: &std::path::Path,
) -> (Manager, [u64; 3]) {
    let mgr = manager_in(dir);
    mgr.set_max_concurrent(1);

    let paused = mgr.add(server.url("/gated"), dir, None, None);
    wait_status(&mgr, paused, Status::Downloading).await;
    mgr.pause(paused);
    wait_status(&mgr, paused, Status::Paused).await;
    // Resumable, so its partial file is kept.
    let running = mgr.add(server.url("/seg-gated"), dir, None, None);
    let queued = mgr.add(server.url("/gated"), dir, None, None);
    wait_status(&mgr, running, Status::Downloading).await;
    assert_eq!(status(&mgr, queued), Status::Queued);

    // The target is settled once bytes arrive.
    let deadline = Instant::now() + T;
    while snap(&mgr, running).downloaded == 0 {
        assert!(
            Instant::now() < deadline,
            "the running download got no bytes"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let part = state::part_path(&snap(&mgr, running).target);
    let rx = mgr.subscribe();
    mgr.shutdown_for_restart();
    // A frontend watching the snapshots sees the pause before the process goes.
    let published = rx
        .borrow()
        .iter()
        .find(|s| s.id == running)
        .cloned()
        .unwrap();
    assert_eq!(published.status, Status::Paused);
    assert!(published.paused_for_update);
    // The Exit handler's plain shutdown runs after it and keeps the marks.
    mgr.shutdown();
    drop(mgr);
    assert_eq!(saved_marks(dir), [false, true, true]);
    assert!(part.exists(), "the running download's partial file is gone");

    (manager_in(dir), [paused, running, queued])
}

/// AC: after an update restart nothing resumes on its own, and exactly the
/// downloads that were running or queued are marked as paused for it.
#[tokio::test]
async fn an_update_restart_pauses_everything_and_remembers_which() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let (mgr, [paused, running, queued]) = after_an_update_restart(&server, dir.path()).await;

    for id in [paused, running, queued] {
        assert_eq!(status(&mgr, id), Status::Paused);
    }
    assert_eq!(mgr.paused_for_update(), [running, queued]);
    assert!(!snap(&mgr, paused).paused_for_update);
    assert!(snap(&mgr, running).paused_for_update);
    assert!(snap(&mgr, queued).paused_for_update);
    // Loading keeps the marks for the next start too.
    assert_eq!(saved_marks(dir.path()), [false, true, true]);
    server.gate().release_all();
}

/// AC: Resume all resumes exactly the marked downloads, in list order under
/// the queue limit, and clears the marks; the user's own pause is kept.
#[tokio::test]
async fn resume_all_resumes_exactly_the_marked_downloads() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let (mgr, [paused, running, queued]) = after_an_update_restart(&server, dir.path()).await;

    mgr.resume_paused_for_update();
    wait_status(&mgr, running, Status::Downloading).await;
    assert_eq!(status(&mgr, queued), Status::Queued);
    assert_eq!(status(&mgr, paused), Status::Paused);
    assert!(mgr.paused_for_update().is_empty());
    assert_eq!(saved_marks(dir.path()), [false, false, false]);

    server.gate().release_all();
    wait_status(&mgr, running, Status::Completed).await;
    wait_status(&mgr, queued, Status::Completed).await;
    assert_eq!(status(&mgr, paused), Status::Paused);
}

/// Resuming one by hand clears its mark; dismissing clears the rest without
/// resuming them.
#[tokio::test]
async fn a_manual_resume_or_dismiss_clears_the_marks() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let (mgr, [_, running, queued]) = after_an_update_restart(&server, dir.path()).await;

    mgr.resume(running);
    assert!(!snap(&mgr, running).paused_for_update);
    assert_eq!(mgr.paused_for_update(), [queued]);

    mgr.forget_paused_for_update();
    assert!(mgr.paused_for_update().is_empty());
    assert_eq!(status(&mgr, queued), Status::Paused);
    assert_eq!(saved_marks(dir.path()), [false, false, false]);
    server.gate().release_all();
}

/// A plain quit still restores running downloads paused, with no marks.
#[tokio::test]
async fn a_plain_quit_marks_nothing() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    wait_status(&mgr, id, Status::Downloading).await;
    mgr.shutdown();
    drop(mgr);
    assert_eq!(saved_marks(dir.path()), [false]);

    let reloaded = manager_in(dir.path());
    assert_eq!(status(&reloaded, id), Status::Paused);
    assert!(reloaded.paused_for_update().is_empty());
    server.gate().release_all();
}
