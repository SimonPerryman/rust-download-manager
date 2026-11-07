//! M5: [`Manager::move_queued`] reorders the waiting set; the scheduler starts
//! the new first, and the order survives a restart of the store.

mod common;

use std::time::{Duration, Instant};

use rdm::{JsonStore, Manager, Status, Store};

const T: Duration = Duration::from_secs(10);

/// Build a manager over a fresh JSON store inside `dir`.
fn manager_in(dir: &std::path::Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

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

fn status_of(mgr: &Manager, id: u64) -> Option<Status> {
    mgr.snapshot()
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| s.status)
}

/// AC: with a cap of 1 and three queued downloads, moving the third to index
/// 0 makes it the next to start when the slot frees, and the order persists.
#[tokio::test]
async fn moved_download_starts_next_and_order_persists() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    mgr.set_max_concurrent(1);
    let gated = server.stats("/gated");

    let [a, b, c, d] = [(); 4].map(|_| mgr.add(server.url("/gated"), dir.path(), None, None));
    assert!(wait_until(T, || gated.requests() == 1).await);
    assert_eq!(status_of(&mgr, a), Some(Status::Downloading));

    mgr.move_queued(d, 0);
    let order: Vec<u64> = mgr.snapshot().iter().map(|s| s.id).collect();
    assert_eq!(order, [a, d, b, c]);

    server.gate().release_next().expect("A is held");
    assert!(
        wait_until(T, || status_of(&mgr, d) == Some(Status::Downloading)).await,
        "the moved download didn't start next"
    );
    assert_eq!(status_of(&mgr, a), Some(Status::Completed));
    assert_eq!(status_of(&mgr, b), Some(Status::Queued));
    assert_eq!(status_of(&mgr, c), Some(Status::Queued));

    let stored = JsonStore::new(dir.path().join("state.json"))
        .load()
        .unwrap();
    let order: Vec<u64> = stored.downloads.iter().map(|r| r.id).collect();
    assert_eq!(order, [a, d, b, c], "the store keeps the new order");
    let reloaded = manager_in(dir.path());
    let order: Vec<u64> = reloaded.snapshot().iter().map(|s| s.id).collect();
    assert_eq!(
        order,
        [a, d, b, c],
        "a restarted manager keeps the new order"
    );

    server.gate().release_all();
}

/// Only a waiting download moves (a running one or an unknown id is a no-op),
/// and an index past the end clamps to the last waiting slot.
#[tokio::test]
async fn only_waiting_downloads_move() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    mgr.set_max_concurrent(1);
    let gated = server.stats("/gated");

    let [a, b, c] = [(); 3].map(|_| mgr.add(server.url("/gated"), dir.path(), None, None));
    assert!(wait_until(T, || gated.requests() == 1).await);
    let order = || -> Vec<u64> { mgr.snapshot().iter().map(|s| s.id).collect() };

    mgr.move_queued(a, 2);
    assert_eq!(order(), [a, b, c], "a running download doesn't move");
    mgr.move_queued(99, 0);
    assert_eq!(order(), [a, b, c]);
    mgr.move_queued(b, 99);
    assert_eq!(order(), [a, c, b], "clamped to the last waiting slot");

    server.gate().release_all();
}
