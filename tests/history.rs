//! History: what schedules and tags did is recorded per download and per
//! rule, kept in `history.jsonl`, and removed with the download. Schedules
//! run on a fake clock moved by hand.

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rdm::engine::schedule::{
    Days, FakeClock, LocalDateTime, QueueWindow, Schedule, StartAt, TimeOfDay, Window,
};
use rdm::{AddOptions, HistoryEvent, HistoryKind, JsonStore, Manager, Status, Store};

const T: Duration = Duration::from_secs(10);

// 2026-03-09 is a Monday.
fn on(day: u8, h: u8, mi: u8) -> LocalDateTime {
    LocalDateTime::from_parts(2026, 3, 9 + day, h, mi).unwrap()
}

fn manager(dir: &Path, clock: &Arc<FakeClock>) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load_with_clock(store, tokio::runtime::Handle::current(), clock.clone()).unwrap()
}

fn night() -> Schedule {
    Schedule {
        queue_windows: vec![QueueWindow {
            id: 0,
            name: Some("Night".into()),
            enabled: true,
            window: Window::new(
                "22:00".parse::<TimeOfDay>().unwrap(),
                "06:00".parse().unwrap(),
                Days::DAILY,
            )
            .unwrap(),
            finish_running: false,
            scope: Default::default(),
        }],
        ..Schedule::default()
    }
}

fn kinds(events: &[HistoryEvent]) -> Vec<HistoryKind> {
    events.iter().map(|e| e.kind).collect()
}

async fn wait_status(mgr: &Manager, id: u64, want: Status) {
    let deadline = Instant::now() + T;
    loop {
        let s = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
        if s.status == want {
            return;
        }
        assert!(Instant::now() < deadline, "stuck: {s:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn a_window_records_what_it_held_started_and_finished() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(night());
    let rule = mgr.settings().schedule.queue_windows[0].id;
    assert_ne!(rule, 0, "a saved rule has an id");

    let id = mgr.add(server.url("/file"), dir.path(), None, None);
    mgr.check_schedule();
    clock.set(on(0, 22, 0));
    mgr.check_schedule();
    wait_status(&mgr, id, Status::Completed).await;
    clock.set(on(1, 6, 0));
    mgr.check_schedule();

    let own = mgr.download_history(id);
    assert_eq!(kinds(&own), [HistoryKind::Held, HistoryKind::Started]);
    assert_eq!(own[0].details, "\"Night\" is closed until 22:00");
    assert_eq!(own[1].details, "\"Night\" opened");
    assert!(own.iter().all(|e| e.rule_id == Some(rule)));

    let window = mgr.rule_history(rule);
    assert_eq!(kinds(&window), [HistoryKind::Opened, HistoryKind::Closed]);
    assert_eq!(window[0].details, "1 file waiting");
    assert!(
        window[1].details.starts_with("1 file, "),
        "{}",
        window[1].details
    );
}

#[tokio::test]
async fn a_download_a_closing_window_pauses_is_recorded() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 23, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(night());
    let rule = mgr.settings().schedule.queue_windows[0].id;
    let id = mgr.add(server.url("/crawl"), dir.path(), None, None);
    let deadline = Instant::now() + T;
    while mgr.snapshot()[0].downloaded == 0 {
        assert!(Instant::now() < deadline, "never started");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    clock.set(on(1, 6, 0));
    mgr.check_schedule();
    wait_status(&mgr, id, Status::Queued).await;

    let own = mgr.download_history(id);
    assert_eq!(kinds(&own), [HistoryKind::Paused]);
    assert_eq!(own[0].details, "\"Night\" is closed until 22:00");
    // Nothing finished in the run, and the first look opened no event.
    let window = mgr.rule_history(rule);
    assert_eq!(kinds(&window), [HistoryKind::Closed]);
    assert_eq!(window[0].details, "No files finished");
}

#[tokio::test]
async fn a_bulk_tag_edit_logs_one_event_per_download_and_removal_takes_them_away() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
    let mgr = manager(dir.path(), &clock);
    let a = mgr.add("http://127.0.0.1:9/a", dir.path(), None, None);
    let b = mgr.add("http://127.0.0.1:9/b", dir.path(), None, None);
    mgr.stop(a, false);
    mgr.stop(b, false);
    let tags = |t: &[&str]| t.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    mgr.add_tags(&[a, b], &tags(&["course-2", "new"])).unwrap();
    mgr.add_tags(&[a, b], &tags(&["new"])).unwrap();
    mgr.remove_tags(&[a], &tags(&["new"]));
    mgr.add_tags(&[b], &tags(&["extra"])).unwrap();

    let for_a = mgr.download_history(a);
    assert_eq!(
        kinds(&for_a),
        [HistoryKind::TagAdded, HistoryKind::TagRemoved]
    );
    assert_eq!(for_a[0].details, "Added tags course-2, new");
    assert_eq!(for_a[1].details, "Removed tag new");
    let for_b = mgr.download_history(b);
    assert_eq!(for_b.len(), 2, "no event for a tag it already had");
    assert_eq!(for_b[1].details, "Added tag extra");

    // They survive a restart, and go with the download.
    mgr.shutdown();
    drop(mgr);
    let mgr = manager(dir.path(), &clock);
    assert_eq!(mgr.download_history(a).len(), 2);
    mgr.remove(a);
    assert!(mgr.download_history(a).is_empty());
    assert_eq!(mgr.history().len(), 2, "b's events stay");
    let saved = std::fs::read_to_string(dir.path().join("history.jsonl")).unwrap();
    assert_eq!(saved.lines().count(), 2);
}

#[tokio::test]
async fn a_scheduled_start_is_recorded_when_it_fires_or_is_missed() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let at = |day, h, mi| AddOptions {
        start_at: Some(StartAt(on(day, h, mi))),
        ..AddOptions::default()
    };
    let (fires, misses) = {
        let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
        let mgr = manager(dir.path(), &clock);
        let fires = mgr
            .add_with(server.url("/file"), dir.path(), at(0, 13, 0))
            .unwrap();
        let misses = mgr
            .add_with(server.url("/gated"), dir.path(), at(0, 20, 0))
            .unwrap();
        clock.set(on(0, 13, 0));
        mgr.check_schedule();
        wait_status(&mgr, fires, Status::Completed).await;
        let own = mgr.download_history(fires);
        assert_eq!(kinds(&own), [HistoryKind::Started]);
        assert_eq!(own[0].details, "Scheduled start reached, set for 13:00");
        mgr.shutdown();
        (fires, misses)
    };

    // Reopened the next morning: the 20:00 start was missed.
    let clock = Arc::new(FakeClock::new(on(1, 9, 0)));
    let mgr = manager(dir.path(), &clock);
    let own = mgr.download_history(misses);
    assert_eq!(kinds(&own), [HistoryKind::Missed]);
    assert!(
        own[0].details.contains("2026-03-09 20:00"),
        "{}",
        own[0].details
    );
    assert_eq!(mgr.download_history(fires).len(), 1);
}
