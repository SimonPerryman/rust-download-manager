//! A download's size and name are probed when it is added, so a row still in
//! the queue shows them: `HEAD`, then a one-byte range, off the queue and
//! quietly. Ordering is asserted on Manager status and the fixture's gates,
//! never on progress bytes.

mod common;

use std::time::{Duration, Instant};

use rdm::{JsonStore, Manager, Status, Store, PROBE_CONCURRENCY};

const T: Duration = Duration::from_secs(10);

/// Long enough for a probe that was going to answer to have done so.
const SETTLE: Duration = Duration::from_millis(300);

fn manager_in(dir: &std::path::Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

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

fn row(mgr: &Manager, id: u64) -> Option<rdm::DownloadSnapshot> {
    mgr.snapshot().into_iter().find(|s| s.id == id)
}

fn total_of(mgr: &Manager, id: u64) -> Option<u64> {
    row(mgr, id).and_then(|s| s.total)
}

/// Fill the manager's `cap` slots with held `/gated` downloads.
async fn hold_slots(
    mgr: &Manager,
    server: &common::TestServer,
    dir: &std::path::Path,
    cap: usize,
) -> Vec<u64> {
    mgr.set_max_concurrent(cap);
    let ids: Vec<u64> = (0..cap)
        .map(|_| mgr.add(server.url("/gated"), dir, None, None))
        .collect();
    let gated = server.stats("/gated");
    assert!(wait_until(T, || gated.requests() == cap).await);
    for id in &ids {
        assert_eq!(row(mgr, *id).unwrap().status, Status::Downloading);
    }
    ids
}

/// AC: a download queued behind the cap of 3 gets its size and name from the
/// probe while it is still queued, and the probe takes no slot.
#[tokio::test]
async fn a_queued_download_is_sized_without_taking_a_slot() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let held = hold_slots(&mgr, &server, dir.path(), 3).await;

    let queued = mgr.add(server.url("/probe/named"), dir.path(), None, None);
    assert!(
        wait_until(T, || total_of(&mgr, queued) == Some(common::PROBE_SIZE)).await,
        "the probe never sized the queued download"
    );
    let snap = row(&mgr, queued).unwrap();
    assert_eq!(snap.status, Status::Queued, "still waiting its turn");
    assert_eq!(snap.filename, common::PROBE_NAME);
    assert_eq!(server.probe_requests(), (1, 0), "one HEAD was enough");
    let running = mgr
        .snapshot()
        .iter()
        .filter(|s| s.status == Status::Downloading)
        .count();
    assert_eq!(running, 3, "the three held downloads kept their slots");
    assert_eq!(server.stats("/gated").requests(), 3);

    // The probed numbers are saved too.
    let stored = JsonStore::new(dir.path().join("state.json"))
        .load()
        .unwrap();
    let record = stored.downloads.iter().find(|r| r.id == queued).unwrap();
    assert_eq!(record.total, Some(common::PROBE_SIZE));

    // The probe only informs: the real download runs and finishes normally.
    server.gate().release_all();
    let all: Vec<u64> = held.iter().copied().chain([queued]).collect();
    assert!(
        wait_until(T, || all.iter().all(
            |id| row(&mgr, *id).is_some_and(|s| s.status == Status::Completed)
        ))
        .await
    );
}

/// A server that refuses `HEAD` (405) is asked for one byte instead, and its
/// `Content-Range` gives the size.
#[tokio::test]
async fn a_refused_head_falls_back_to_a_one_byte_range() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    hold_slots(&mgr, &server, dir.path(), 3).await;

    let queued = mgr.add(server.url("/probe/no-head"), dir.path(), None, None);
    assert!(wait_until(T, || total_of(&mgr, queued) == Some(common::PROBE_SIZE)).await);
    assert_eq!(server.probe_requests(), (1, 1));
    assert_eq!(row(&mgr, queued).unwrap().status, Status::Queued);
    server.gate().release_all();
}

/// A server that refuses both, or sends no size, leaves the size blank; the
/// add still succeeds and nothing fails.
#[tokio::test]
async fn a_probe_that_learns_nothing_leaves_the_size_blank() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    hold_slots(&mgr, &server, dir.path(), 3).await;

    let refused = mgr.add(server.url("/probe/refused"), dir.path(), None, None);
    let no_size = mgr.add(server.url("/probe/no-size"), dir.path(), None, None);
    let unreachable = mgr.add("http://127.0.0.1:9/nothing", dir.path(), None, None);
    assert!(
        wait_until(T, || server.probe_requests() == (2, 2)).await,
        "both probes should try HEAD and then the range"
    );
    tokio::time::sleep(SETTLE).await;

    for id in [refused, no_size, unreachable] {
        let snap = row(&mgr, id).expect("the add stands");
        assert_eq!(snap.total, None);
        assert_eq!(snap.status, Status::Queued);
        assert_eq!(snap.error, None, "a probe never fails a download");
    }
    server.gate().release_all();
}

/// Probes run under their own limit: with a dozen queued, no more than
/// [`PROBE_CONCURRENCY`] are in flight at once, and none takes a slot.
#[tokio::test]
async fn probes_never_exceed_their_concurrency_limit() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    hold_slots(&mgr, &server, dir.path(), 1).await;

    let ids: Vec<u64> = (0..PROBE_CONCURRENCY * 3)
        .map(|_| mgr.add(server.url("/probe/gated"), dir.path(), None, None))
        .collect();
    assert!(
        wait_until(T, || server.probe_in_flight() == PROBE_CONCURRENCY).await,
        "the probes should fill their limit"
    );
    tokio::time::sleep(SETTLE).await;
    assert_eq!(
        server.probe_in_flight(),
        PROBE_CONCURRENCY,
        "no fifth probe starts while four are held"
    );
    assert_eq!(server.probe_requests().0, PROBE_CONCURRENCY);

    server.probe_gate().release_all();
    assert!(
        wait_until(T, || ids
            .iter()
            .all(|id| total_of(&mgr, *id) == Some(common::PROBE_SIZE)))
        .await,
        "every queued download is sized once the probes are let through"
    );
    assert_eq!(server.probe_requests().0, ids.len());
    assert_eq!(server.probe_max_in_flight(), PROBE_CONCURRENCY);
    server.gate().release_all();
}

/// A probe that answers after its download started and finished changes
/// nothing: the first GET's size and name stand.
#[tokio::test]
async fn a_late_probe_does_not_overwrite_the_real_download() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    hold_slots(&mgr, &server, dir.path(), 1).await;

    let id = mgr.add(server.url("/probe/gated"), dir.path(), None, None);
    assert!(wait_until(T, || server.probe_in_flight() == 1).await);
    server.gate().release_all();
    assert!(
        wait_until(T, || row(&mgr, id)
            .is_some_and(|s| s.status == Status::Completed))
        .await
    );
    let before = row(&mgr, id).unwrap();
    assert_eq!(before.total, Some(common::payload().len() as u64));

    server.probe_gate().release_all();
    assert!(wait_until(T, || server.probe_in_flight() == 0).await);
    tokio::time::sleep(SETTLE).await;
    let after = row(&mgr, id).unwrap();
    assert_eq!(after.total, before.total);
    assert_eq!(after.filename, before.filename);
    assert_ne!(after.filename, common::PROBE_NAME);
}

/// A probe that answers after its download was removed or stopped neither
/// brings it back nor touches it.
#[tokio::test]
async fn a_late_probe_does_not_resurrect_a_removed_or_stopped_download() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    hold_slots(&mgr, &server, dir.path(), 1).await;

    let removed = mgr.add(server.url("/probe/gated"), dir.path(), None, None);
    let stopped = mgr.add(server.url("/probe/gated"), dir.path(), None, None);
    assert!(wait_until(T, || server.probe_in_flight() == 2).await);
    mgr.remove(removed);
    mgr.stop(stopped, true);
    assert_eq!(row(&mgr, stopped).unwrap().status, Status::Cancelled);

    server.probe_gate().release_all();
    assert!(wait_until(T, || server.probe_in_flight() == 0).await);
    tokio::time::sleep(SETTLE).await;
    assert!(
        row(&mgr, removed).is_none(),
        "a removed download stays gone"
    );
    let snap = row(&mgr, stopped).unwrap();
    assert_eq!(snap.status, Status::Cancelled);
    assert_eq!(snap.total, None);
    server.gate().release_all();
}

/// A store written before the probe loads unchanged (it needed no new
/// fields), and loading probes nothing: only an add does.
#[tokio::test]
async fn an_older_store_loads_unchanged() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let doc = serde_json::json!({
        "version": 2,
        "next_id": 2,
        "downloads": [{
            "id": 1,
            "url": server.url("/probe/named"),
            "target": dir.path().join("named"),
            "status": "paused",
        }],
    });
    std::fs::write(dir.path().join("state.json"), doc.to_string()).unwrap();

    let mgr = manager_in(dir.path());
    let snap = row(&mgr, 1).expect("the old record loads");
    assert_eq!(snap.status, Status::Paused);
    assert_eq!(snap.total, None);
    tokio::time::sleep(SETTLE).await;
    assert_eq!(server.probe_requests(), (0, 0));
}
