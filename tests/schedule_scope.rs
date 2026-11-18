//! A schedule rule scoped to a tag, a host or a batch touches only the
//! downloads it matches. Runs on a fake clock moved by hand.

mod common;

use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rdm::engine::schedule::{
    Days, FakeClock, LocalDateTime, QueueWindow, Schedule, Scope, SpeedEntry, TimeOfDay, Window,
};
use rdm::{AddOptions, BatchSpec, JsonStore, Manager, Status, Store};

const MB: u64 = 1_000_000;

// 2026-03-09 is a Monday.
fn on(h: u8, mi: u8) -> LocalDateTime {
    LocalDateTime::from_parts(2026, 3, 9, h, mi).unwrap()
}

fn window(start: &str, end: &str) -> Window {
    let t = |s: &str| s.parse::<TimeOfDay>().unwrap();
    Window::new(t(start), t(end), Days::DAILY).unwrap()
}

fn queue_rule(scope: Scope) -> QueueWindow {
    QueueWindow {
        name: None,
        enabled: true,
        window: window("22:00", "06:00"),
        finish_running: false,
        scope,
    }
}

fn speed_rule(scope: Scope, limit: u64) -> SpeedEntry {
    SpeedEntry {
        name: None,
        enabled: true,
        window: window("08:00", "18:00"),
        limit: NonZeroU64::new(limit).unwrap(),
        scope,
    }
}

fn queue_only(rule: QueueWindow) -> Schedule {
    Schedule {
        queue_windows: vec![rule],
        ..Schedule::default()
    }
}

fn tag(t: &str) -> Scope {
    Scope::Tag { tag: t.into() }
}

fn manager(dir: &std::path::Path, clock: &Arc<FakeClock>) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load_with_clock(store, tokio::runtime::Handle::current(), clock.clone()).unwrap()
}

fn status(mgr: &Manager, id: u64) -> Status {
    mgr.snapshot().iter().find(|s| s.id == id).unwrap().status
}

fn tagged(t: &str) -> AddOptions {
    AddOptions {
        tags: vec![t.into()],
        ..AddOptions::default()
    }
}

async fn wait_status(mgr: &Manager, id: u64, want: Status) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while status(mgr, id) != want {
        assert!(
            Instant::now() < deadline,
            "download {id} never became {want:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn a_tag_window_holds_only_tagged_downloads() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(12, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(queue_only(queue_rule(tag("Course-2"))));
    assert!(
        mgr.queue_state().open,
        "the status-bar state is for all-downloads windows only"
    );

    let held = mgr
        .add_with(server.url("/gated"), dir.path(), tagged("course-2"))
        .unwrap();
    let free = mgr.add(server.url("/gated"), dir.path(), None, None);
    assert_eq!(status(&mgr, held), Status::Queued);
    assert_eq!(status(&mgr, free), Status::Downloading);

    clock.set(on(22, 0));
    mgr.check_schedule();
    assert_eq!(status(&mgr, held), Status::Downloading, "its window opened");
    server.gate().release_all();
}

#[tokio::test]
async fn a_closing_tag_window_pauses_only_its_downloads() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(23, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(queue_only(queue_rule(tag("night"))));
    let night = mgr
        .add_with(server.url("/crawl"), dir.path(), tagged("night"))
        .unwrap();
    let other = mgr.add(server.url("/crawl"), dir.path(), None, None);
    assert_eq!(status(&mgr, night), Status::Downloading);

    clock.set(on(6, 0));
    mgr.check_schedule();
    wait_status(&mgr, night, Status::Queued).await;
    assert_eq!(status(&mgr, other), Status::Downloading);
    mgr.cancel(night);
    mgr.cancel(other);
}

#[tokio::test]
async fn a_host_window_holds_downloads_from_that_host() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(12, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(queue_only(queue_rule(Scope::Host {
        host: "127.0.0.1".into(),
    })));
    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    assert_eq!(
        status(&mgr, id),
        Status::Queued,
        "the fixture's host is held"
    );

    mgr.set_schedule(queue_only(queue_rule(Scope::Host {
        host: "other.example".into(),
    })));
    assert_eq!(
        status(&mgr, id),
        Status::Downloading,
        "a window for another host doesn't apply"
    );
    server.gate().release_all();
}

#[tokio::test]
async fn a_batch_window_holds_that_batch_only() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(12, 0)));
    let mgr = manager(dir.path(), &clock);
    // A batch takes its first download's id, and ids start at 1.
    mgr.set_schedule(queue_only(queue_rule(Scope::Batch { id: 1, label: None })));
    let spec = BatchSpec {
        label: "Course".into(),
        folder: false,
        limit: None,
    };
    let batch = mgr
        .add_batch(
            vec![server.url("/gated?a=1"), server.url("/gated?a=2")],
            dir.path(),
            Some(spec),
            AddOptions::default(),
        )
        .unwrap();
    assert_eq!(batch.batch, Some(1));
    let single = mgr.add(server.url("/gated?a=3"), dir.path(), None, None);
    for id in &batch.added {
        assert_eq!(status(&mgr, *id), Status::Queued);
    }
    assert_eq!(status(&mgr, single), Status::Downloading);

    clock.set(on(22, 0));
    mgr.check_schedule();
    for id in &batch.added {
        assert_eq!(status(&mgr, *id), Status::Downloading);
    }
    server.gate().release_all();
}

#[tokio::test]
async fn a_rule_for_a_tag_nothing_carries_matches_nothing_and_is_kept() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(12, 0)));
    let mgr = manager(dir.path(), &clock);
    let schedule = queue_only(queue_rule(tag("gone")));
    mgr.set_schedule(schedule.clone());
    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    assert_eq!(status(&mgr, id), Status::Downloading);
    assert_eq!(mgr.settings().schedule, schedule);
    server.gate().release_all();
}

#[tokio::test]
async fn a_tag_speed_entry_caps_only_tagged_downloads_and_follows_its_window() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(7, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(Schedule {
        speed_entries: vec![speed_rule(tag("slow"), MB)],
        ..Schedule::default()
    });
    let slow = mgr
        .add_with(server.url("/gated?a=1"), dir.path(), tagged("Slow"))
        .unwrap();
    let fast = mgr.add(server.url("/gated?a=2"), dir.path(), None, None);
    assert_eq!(mgr.running_speed_cap(slow), None, "outside the window");

    clock.set(on(8, 0));
    mgr.check_schedule();
    assert_eq!(mgr.running_speed_cap(slow), Some(MB));
    assert_eq!(mgr.running_speed_cap(fast), None);
    assert_eq!(
        mgr.effective_global_limit(),
        None,
        "the global limit is untouched"
    );
    assert_eq!(mgr.queue_state().limit, None);

    // A tag added mid-download brings the cap with it.
    mgr.add_tags(&[fast], &["slow".to_string()]).unwrap();
    mgr.check_schedule();
    assert_eq!(mgr.running_speed_cap(fast), Some(MB));

    clock.set(on(18, 0));
    mgr.check_schedule();
    assert_eq!(mgr.running_speed_cap(slow), None);
    assert_eq!(mgr.running_speed_cap(fast), None);

    clock.set(on(9, 0));
    mgr.check_schedule();
    assert_eq!(mgr.running_speed_cap(slow), Some(MB));
    mgr.set_schedule(Schedule::default());
    assert_eq!(
        mgr.running_speed_cap(slow),
        None,
        "removing the rule lifts it"
    );
    server.gate().release_all();
}

#[tokio::test]
async fn an_all_downloads_entry_still_replaces_the_global_limit_beside_a_scoped_one() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(9, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_global_limit(Some(2 * MB));
    mgr.set_schedule(Schedule {
        speed_entries: vec![speed_rule(tag("x"), MB), speed_rule(Scope::All, 5 * MB)],
        ..Schedule::default()
    });
    assert_eq!(mgr.effective_global_limit(), Some(5 * MB));
    assert_eq!(mgr.queue_state().limit, Some(5 * MB));
}
