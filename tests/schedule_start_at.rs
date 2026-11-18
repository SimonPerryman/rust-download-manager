//! Start-at schedules: a time holds downloads out of the queue slots, one
//! that passed while rdm was closed is missed rather than fired, and the
//! user's choices (start, reschedule, dismiss) act on whole schedules.
//! Everything runs on a fake clock moved by hand.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use rdm::engine::schedule::{FakeClock, LocalDateTime, StartAt};
use rdm::{AddOptions, BatchSpec, DownloadSnapshot, JsonStore, Manager, Status, Store};

const T: Duration = Duration::from_secs(10);

// 2026-03-09 is a Monday.
fn on(day: u8, h: u8, mi: u8) -> LocalDateTime {
    LocalDateTime::from_parts(2026, 3, 9 + day, h, mi).unwrap()
}

fn start(day: u8, h: u8, mi: u8) -> Option<StartAt> {
    Some(StartAt(on(day, h, mi)))
}

fn manager(dir: &std::path::Path, clock: &Arc<FakeClock>) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load_with_clock(store, tokio::runtime::Handle::current(), clock.clone()).unwrap()
}

fn snap(mgr: &Manager, id: u64) -> DownloadSnapshot {
    mgr.snapshot().into_iter().find(|s| s.id == id).unwrap()
}

async fn wait_status(mgr: &Manager, id: u64, want: Status) {
    let deadline = Instant::now() + T;
    while snap(mgr, id).status != want {
        assert!(Instant::now() < deadline, "stuck: {:?}", snap(mgr, id));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn at_time(start_at: Option<StartAt>) -> AddOptions {
    AddOptions {
        start_at,
        ..AddOptions::default()
    }
}

fn nightly() -> Option<BatchSpec> {
    Some(BatchSpec {
        label: "Nightly".into(),
        folder: false,
        limit: None,
    })
}

/// A manager over the state a previous run left, with the clock at `now`.
fn relaunch(dir: &std::path::Path, now: LocalDateTime) -> (Manager, Arc<FakeClock>) {
    let clock = Arc::new(FakeClock::new(now));
    (manager(dir, &clock), clock)
}

#[tokio::test]
async fn a_start_time_holds_a_download_out_of_the_queue_slots() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_max_concurrent(1);

    let later = mgr
        .add_with(server.url("/slow"), dir.path(), at_time(start(0, 22, 0)))
        .unwrap();
    // The one slot is free for this, though the other was added first.
    let now = mgr.add(server.url("/gated"), dir.path(), None, None);
    assert_eq!(snap(&mgr, now).status, Status::Downloading);
    let held = snap(&mgr, later);
    assert_eq!(held.status, Status::Queued);
    assert_eq!(held.start_at, start(0, 22, 0));
    assert!(!held.start_missed);

    clock.set(on(0, 21, 59));
    mgr.check_schedule();
    assert_eq!(snap(&mgr, later).start_at, start(0, 22, 0), "not yet");

    // At the time it joins the queue behind the running one: the cap holds.
    clock.set(on(0, 22, 0));
    mgr.check_schedule();
    let due = snap(&mgr, later);
    assert_eq!((due.status, due.start_at), (Status::Queued, None));
    assert_eq!(server.stats("/slow").requests(), 0, "no free slot yet");

    server.gate().release_all();
    wait_status(&mgr, now, Status::Completed).await;
    wait_status(&mgr, later, Status::Completed).await;
}

#[tokio::test]
async fn a_batch_with_one_start_time_is_one_schedule() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
    let mgr = manager(dir.path(), &clock);
    let urls = vec![server.url("/slow"), server.url("/crawl")];
    let added = mgr
        .add_batch(urls, dir.path(), nightly(), at_time(start(0, 22, 0)))
        .unwrap();
    let other = mgr
        .add_with(server.url("/file"), dir.path(), at_time(start(0, 22, 0)))
        .unwrap();

    let schedules = mgr.start_schedules();
    assert_eq!(schedules.len(), 2, "a separate add is its own schedule");
    assert_eq!(schedules[0].downloads, added.added);
    assert_eq!(schedules[1].downloads, [other]);
}

#[tokio::test]
async fn a_start_time_that_passed_while_closed_is_missed_and_starts_nothing() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let (batch_ids, single) = {
        let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
        let mgr = manager(dir.path(), &clock);
        let urls = vec![server.url("/slow"), server.url("/crawl")];
        let batch = mgr
            .add_batch(urls, dir.path(), nightly(), at_time(start(0, 22, 0)))
            .unwrap();
        // Added second, but due earlier.
        let single = mgr
            .add_with(server.url("/gated"), dir.path(), at_time(start(0, 20, 0)))
            .unwrap();
        mgr.shutdown();
        (batch.added, single)
    };

    // Reopened at 09:00 the next morning.
    let (mgr, clock) = relaunch(dir.path(), on(1, 9, 0));
    mgr.check_schedule();
    for &id in batch_ids.iter().chain([&single]) {
        let s = snap(&mgr, id);
        assert_eq!(s.status, Status::Queued, "held, not paused or started");
        assert!(s.start_missed);
    }
    assert_eq!(snap(&mgr, single).start_at, start(0, 20, 0), "was due then");
    let missed = mgr.missed_schedules();
    assert_eq!((missed.schedules, missed.downloads), (2, 3));
    assert_eq!(server.stats("/gated").requests(), 0);

    // Time passing does not start it either.
    clock.set(on(1, 10, 0));
    mgr.check_schedule();
    assert_eq!(server.stats("/gated").requests(), 0);
    assert_eq!(mgr.missed_schedules().schedules, 2);

    // Start all: oldest due first, under the cap of one.
    mgr.set_max_concurrent(1);
    let groups: Vec<u64> = mgr.start_schedules().iter().map(|s| s.group).collect();
    mgr.start_schedule_now(&groups);
    assert_eq!(mgr.missed_schedules().schedules, 0);
    assert_eq!(snap(&mgr, single).status, Status::Downloading);
    for &id in &batch_ids {
        let s = snap(&mgr, id);
        assert_eq!((s.status, s.start_at), (Status::Queued, None), "waits");
    }
    server.gate().release_all();
    for &id in batch_ids.iter().chain([&single]) {
        wait_status(&mgr, id, Status::Completed).await;
    }
}

#[tokio::test]
async fn dismissing_a_missed_schedule_keeps_its_downloads_unstarted() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
        let mgr = manager(dir.path(), &clock);
        let id = mgr
            .add_with(server.url("/slow"), dir.path(), at_time(start(0, 22, 0)))
            .unwrap();
        mgr.shutdown();
        id
    };
    let (mgr, _clock) = relaunch(dir.path(), on(1, 9, 0));
    let group = mgr.start_schedules()[0].group;
    mgr.dismiss_schedules(&[group]);

    let s = snap(&mgr, id);
    assert_eq!((s.status, s.start_at), (Status::Paused, None));
    assert!(mgr.start_schedules().is_empty());
    assert_eq!(server.stats("/slow").requests(), 0);
    mgr.resume(id);
    wait_status(&mgr, id, Status::Completed).await;
}

#[tokio::test]
async fn rescheduling_a_missed_schedule_waits_for_the_new_time() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
        let mgr = manager(dir.path(), &clock);
        let id = mgr
            .add_with(server.url("/slow"), dir.path(), at_time(start(0, 22, 0)))
            .unwrap();
        mgr.shutdown();
        id
    };
    let (mgr, clock) = relaunch(dir.path(), on(1, 9, 0));
    let group = mgr.start_schedules()[0].group;
    mgr.reschedule(&[group], StartAt(on(1, 22, 0)));
    let s = snap(&mgr, id);
    assert_eq!((s.start_at, s.start_missed), (start(1, 22, 0), false));
    assert_eq!(mgr.missed_schedules().schedules, 0);

    clock.set(on(1, 22, 0));
    mgr.check_schedule();
    wait_status(&mgr, id, Status::Completed).await;
}

#[tokio::test]
async fn a_future_start_time_survives_a_restart_and_a_selection_can_be_scheduled() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let (held, paused) = {
        let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
        let mgr = manager(dir.path(), &clock);
        let held = mgr
            .add_with(server.url("/slow"), dir.path(), at_time(start(0, 22, 0)))
            .unwrap();
        // Pausing a held download takes it out of its schedule.
        let paused = mgr
            .add_with(server.url("/file"), dir.path(), at_time(start(0, 22, 0)))
            .unwrap();
        mgr.pause(paused);
        assert_eq!(snap(&mgr, paused).status, Status::Paused);
        assert_eq!(mgr.schedule_start(&[paused], StartAt(on(0, 23, 0))), 1);
        mgr.shutdown();
        (held, paused)
    };
    let (mgr, clock) = relaunch(dir.path(), on(0, 15, 0));
    for (id, at) in [(held, start(0, 22, 0)), (paused, start(0, 23, 0))] {
        let s = snap(&mgr, id);
        assert_eq!(
            (s.status, s.start_at, s.start_missed),
            (Status::Queued, at, false)
        );
    }
    assert_eq!(mgr.missed_schedules().schedules, 0);

    clock.set(on(0, 23, 0));
    mgr.check_schedule();
    wait_status(&mgr, held, Status::Completed).await;
    wait_status(&mgr, paused, Status::Completed).await;
}

#[tokio::test]
async fn a_state_file_from_before_start_times_loads_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("state.json"),
        r#"{"version":3,"next_id":3,"downloads":[
            {"id":1,"url":"http://example.invalid/a","target":"a.bin","status":"paused"},
            {"id":2,"url":"http://example.invalid/b","target":"b.bin","status":"queued"}],
            "settings":{}}"#,
    )
    .unwrap();
    let (mgr, _clock) = relaunch(dir.path(), on(0, 12, 0));
    let all = mgr.snapshot();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|s| s.start_at.is_none() && !s.start_missed));
    assert_eq!(all[1].status, Status::Paused, "as any queued one restores");
    assert!(mgr.start_schedules().is_empty());
}
