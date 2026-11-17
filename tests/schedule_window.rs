//! The queue window: a closed window holds downloads back, a running one
//! pauses at close (not an error) and resumes at open from its kept bytes.
//! Everything runs on a fake clock moved by hand.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use rdm::engine::schedule::{
    Days, FakeClock, LocalDateTime, QueueWindow, Schedule, SpeedEntry, TimeOfDay, Weekday, Window,
};
use rdm::{DownloadSnapshot, JsonStore, Manager, Status, Store};

const T: Duration = Duration::from_secs(10);

fn at(y: i32, mo: u8, d: u8, h: u8, mi: u8) -> LocalDateTime {
    LocalDateTime::from_parts(y, mo, d, h, mi).unwrap()
}

// 2026-03-09 is a Monday.
const MON: (i32, u8, u8) = (2026, 3, 9);

fn on(day_offset: u8, h: u8, mi: u8) -> LocalDateTime {
    at(MON.0, MON.1, MON.2 + day_offset, h, mi)
}

fn tod(s: &str) -> TimeOfDay {
    s.parse().unwrap()
}

fn queue_window(start: &str, end: &str, days: Days, finish_running: bool) -> Schedule {
    Schedule {
        queue_windows: vec![QueueWindow {
            name: None,
            enabled: true,
            window: Window::new(tod(start), tod(end), days).unwrap(),
            finish_running,
        }],
        ..Schedule::default()
    }
}

fn overnight() -> Schedule {
    queue_window("22:00", "06:00", Days::DAILY, false)
}

fn manager(dir: &std::path::Path, clock: &Arc<FakeClock>) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load_with_clock(store, tokio::runtime::Handle::current(), clock.clone()).unwrap()
}

async fn wait_for(
    mgr: &Manager,
    id: u64,
    pred: impl Fn(&DownloadSnapshot) -> bool,
) -> DownloadSnapshot {
    let deadline = Instant::now() + T;
    loop {
        let snap = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
        if pred(&snap) {
            return snap;
        }
        assert!(Instant::now() < deadline, "download {id} stuck: {snap:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_status(mgr: &Manager, id: u64, want: Status) -> DownloadSnapshot {
    wait_for(mgr, id, |s| s.status == want).await
}

fn status(mgr: &Manager, id: u64) -> Status {
    mgr.snapshot().iter().find(|s| s.id == id).unwrap().status
}

#[tokio::test]
async fn a_closed_window_holds_downloads_until_it_opens() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(overnight());
    let state = mgr.queue_state();
    assert!(!state.open);
    assert_eq!(state.until, Some(on(0, 22, 0)));

    let gated = server.stats("/gated");
    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    assert_eq!(status(&mgr, id), Status::Queued);

    clock.set(on(0, 21, 59));
    mgr.check_schedule();
    assert_eq!(status(&mgr, id), Status::Queued);
    assert_eq!(
        gated.requests(),
        0,
        "nothing starts before the window opens"
    );

    clock.set(on(0, 22, 0));
    mgr.check_schedule();
    assert_eq!(status(&mgr, id), Status::Downloading);
    assert!(mgr.queue_state().open);
    assert_eq!(mgr.queue_state().until, Some(on(1, 6, 0)));
    server.gate().release_all();
    wait_status(&mgr, id, Status::Completed).await;
}

#[tokio::test]
async fn a_download_pauses_at_close_and_resumes_at_open_with_its_bytes() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 23, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(overnight());

    let crawl = server.stats("/crawl");
    let id = mgr.add(server.url("/crawl"), dir.path(), None, None);
    let before = wait_for(&mgr, id, |s| s.downloaded > 0).await;
    assert_eq!(before.status, Status::Downloading);

    // The window closes across midnight, at 06:00.
    clock.set(on(1, 6, 0));
    mgr.check_schedule();
    let paused = wait_status(&mgr, id, Status::Queued).await;
    assert!(paused.error.is_none(), "closing the window is not an error");
    assert!(paused.downloaded >= before.downloaded, "bytes are kept");
    assert!(paused.total.is_some_and(|t| paused.downloaded < t));
    assert_eq!(crawl.requests(), 1);

    // Still closed later in the day: it stays put.
    clock.set(on(1, 12, 0));
    mgr.check_schedule();
    assert_eq!(status(&mgr, id), Status::Queued);
    assert_eq!(crawl.requests(), 1);

    clock.set(on(1, 22, 0));
    mgr.check_schedule();
    let done = wait_status(&mgr, id, Status::Completed).await;
    assert_eq!(done.downloaded, done.total.unwrap());
    let resumed_at = crawl.ranges();
    assert_eq!(resumed_at.len(), 1, "one ranged request: {resumed_at:?}");
    let from: u64 = resumed_at[0]
        .strip_prefix("bytes=")
        .and_then(|r| r.strip_suffix('-'))
        .unwrap()
        .parse()
        .unwrap();
    assert!(from > 0, "resumed after the kept bytes, not from the start");
    let bytes = std::fs::read(done.final_path.unwrap()).unwrap();
    let want: Vec<u8> = (0..common::CRAWL_LEN).map(|i| (i % 251) as u8).collect();
    assert_eq!(bytes, want, "the pieces join into the whole file");
}

#[tokio::test]
async fn an_overnight_window_on_selected_days_follows_the_days() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
    let mgr = manager(dir.path(), &clock);
    let mon_wed = Days::new([Weekday::Mon, Weekday::Wed]).unwrap();
    mgr.set_schedule(queue_window("22:00", "06:00", mon_wed, false));
    let open_at = |when| {
        clock.set(when);
        mgr.queue_state().open
    };
    assert!(open_at(on(0, 23, 0)), "Monday night");
    assert!(
        open_at(on(1, 3, 0)),
        "Tuesday's small hours belong to Monday"
    );
    assert!(!open_at(on(1, 23, 0)), "Tuesday night is not chosen");
    assert!(!open_at(on(2, 3, 0)));
    assert!(open_at(on(2, 23, 0)), "Wednesday night");
    assert_eq!(
        {
            clock.set(on(1, 12, 0));
            mgr.queue_state().until
        },
        Some(on(2, 22, 0)),
        "next opening skips Tuesday"
    );
}

#[tokio::test]
async fn a_download_that_cannot_resume_is_let_finish() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 23, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(overnight());

    // `/gated` sends no `Accept-Ranges`: pausing it would lose its bytes.
    let gated = server.stats("/gated");
    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    wait_for(&mgr, id, |s| s.downloaded > 0).await;

    clock.set(on(1, 6, 0));
    mgr.check_schedule();
    assert_eq!(status(&mgr, id), Status::Downloading);

    server.gate().release_all();
    wait_status(&mgr, id, Status::Completed).await;
    assert_eq!(gated.requests(), 1, "never restarted");
}

#[tokio::test]
async fn finish_running_stops_only_new_starts() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 23, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_max_concurrent(1);
    mgr.set_schedule(queue_window("22:00", "06:00", Days::DAILY, true));

    let first = mgr.add(server.url("/crawl"), dir.path(), None, None);
    let second = mgr.add(server.url("/slow"), dir.path(), None, None);
    wait_for(&mgr, first, |s| s.downloaded > 0).await;

    clock.set(on(1, 6, 0));
    mgr.check_schedule();
    assert_eq!(status(&mgr, first), Status::Downloading, "let to finish");
    wait_status(&mgr, first, Status::Completed).await;
    assert_eq!(status(&mgr, second), Status::Queued, "nothing new starts");
    assert_eq!(server.stats("/slow").requests(), 0);

    clock.set(on(1, 22, 0));
    mgr.check_schedule();
    wait_status(&mgr, second, Status::Completed).await;
}

#[tokio::test]
async fn setting_a_closed_window_pauses_what_is_running_and_removing_it_resumes() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
    let mgr = manager(dir.path(), &clock);
    let id = mgr.add(server.url("/crawl"), dir.path(), None, None);
    wait_for(&mgr, id, |s| s.downloaded > 0).await;

    mgr.set_schedule(overnight());
    wait_status(&mgr, id, Status::Queued).await;

    let mut off = overnight();
    off.queue_windows[0].enabled = false;
    mgr.set_schedule(off);
    wait_status(&mgr, id, Status::Completed).await;
}

#[tokio::test]
async fn a_user_pause_during_a_window_pause_stays_paused() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 23, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(overnight());
    let id = mgr.add(server.url("/crawl"), dir.path(), None, None);
    wait_for(&mgr, id, |s| s.downloaded > 0).await;

    mgr.pause(id);
    wait_status(&mgr, id, Status::Paused).await;
    clock.set(on(1, 6, 0));
    mgr.check_schedule();
    clock.set(on(1, 22, 0));
    mgr.check_schedule();
    assert_eq!(
        status(&mgr, id),
        Status::Paused,
        "the window never resumes a user pause"
    );
}

#[tokio::test]
async fn the_window_applies_at_launch() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 12, 0)));
    manager(dir.path(), &clock).set_schedule(overnight());

    let mgr = manager(dir.path(), &clock);
    assert_eq!(mgr.settings().schedule, overnight(), "rules are saved");
    assert!(!mgr.queue_state().open);
    let gated = server.stats("/gated");
    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    assert_eq!(status(&mgr, id), Status::Queued);
    assert_eq!(gated.requests(), 0);

    // Launched inside the window, nothing waits.
    clock.set(on(0, 23, 0));
    let mgr = manager(dir.path(), &clock);
    assert!(mgr.queue_state().open);
    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    assert_eq!(status(&mgr, id), Status::Downloading);
    server.gate().release_all();
}

#[tokio::test]
async fn a_boundary_in_the_repeated_hour_does_not_flap() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    // The clock falls back on Sunday 1 November 2026, 02:00 to 01:00.
    let sat = at(2026, 10, 31, 23, 0);
    let clock = Arc::new(FakeClock::new(sat));
    let mgr = manager(dir.path(), &clock);
    mgr.set_schedule(queue_window("22:00", "01:30", Days::DAILY, false));
    let crawl = server.stats("/crawl");
    let id = mgr.add(server.url("/crawl"), dir.path(), None, None);
    wait_for(&mgr, id, |s| s.downloaded > 0).await;

    clock.set(at(2026, 11, 1, 1, 30));
    mgr.check_schedule();
    wait_status(&mgr, id, Status::Queued).await;

    // 02:00 becomes 01:00 again: by the wall clock 01:00 is inside the
    // window, but its 01:30 close has already been crossed.
    clock.set(at(2026, 11, 1, 2, 0));
    clock.fall_back(60);
    mgr.check_schedule();
    assert!(!mgr.queue_state().open, "the second 01:00 is still closed");
    assert_eq!(status(&mgr, id), Status::Queued);
    clock.advance(45);
    mgr.check_schedule();
    assert_eq!(status(&mgr, id), Status::Queued);
    assert_eq!(crawl.requests(), 1);

    clock.advance(15);
    mgr.check_schedule();
    assert!(!mgr.queue_state().open, "02:00 after the repeat");
    clock.set(at(2026, 11, 1, 22, 0));
    mgr.check_schedule();
    wait_status(&mgr, id, Status::Completed).await;
}

const MB: u64 = 1024 * 1024;

fn weekday_cap(limit: u64) -> Schedule {
    Schedule {
        speed_entries: vec![SpeedEntry {
            name: None,
            enabled: true,
            window: Window::new(tod("08:00"), tod("18:00"), Days::WEEKDAYS).unwrap(),
            limit: std::num::NonZeroU64::new(limit).unwrap(),
        }],
        ..Schedule::default()
    }
}

#[tokio::test]
async fn a_speed_entry_replaces_the_global_limit_inside_its_window_mid_download() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 7, 0)));
    let mgr = manager(dir.path(), &clock);
    mgr.set_global_limit(Some(2 * MB));
    mgr.set_schedule(weekday_cap(5 * MB));
    assert_eq!(
        mgr.effective_global_limit(),
        Some(2 * MB),
        "outside: global"
    );
    assert_eq!(mgr.queue_state().limit_until, Some(on(0, 8, 0)));
    assert_eq!(mgr.queue_state().limit_then, Some(5 * MB));

    let gated = server.stats("/gated");
    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    assert_eq!(status(&mgr, id), Status::Downloading);
    let deadline = Instant::now() + T;
    while gated.requests() == 0 {
        assert!(Instant::now() < deadline, "the download never connected");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    clock.set(on(0, 8, 0));
    mgr.check_schedule();
    assert_eq!(
        status(&mgr, id),
        Status::Downloading,
        "still the same download"
    );
    assert_eq!(
        mgr.effective_global_limit(),
        Some(5 * MB),
        "inside: the entry"
    );
    assert_eq!(mgr.queue_state().limit, Some(5 * MB));
    assert_eq!(
        mgr.settings().global_limit,
        Some(2 * MB),
        "the setting is untouched"
    );

    clock.set(on(0, 18, 0));
    mgr.check_schedule();
    assert_eq!(mgr.effective_global_limit(), Some(2 * MB), "outside again");
    assert_eq!(gated.requests(), 1, "never restarted");

    // Tuesday; an edit to the global limit inside the
    // window must not override the entry.
    clock.set(on(1, 9, 0));
    mgr.check_schedule();
    assert_eq!(mgr.effective_global_limit(), Some(5 * MB));
    mgr.set_global_limit(Some(MB));
    assert_eq!(mgr.effective_global_limit(), Some(5 * MB));
    clock.set(on(1, 18, 0));
    mgr.check_schedule();
    assert_eq!(mgr.effective_global_limit(), Some(MB));
    server.gate().release_all();
    wait_status(&mgr, id, Status::Completed).await;
}

#[tokio::test]
async fn a_speed_entry_applies_at_launch_and_a_download_limit_still_binds() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(0, 9, 0)));
    manager(dir.path(), &clock).set_schedule(weekday_cap(5 * MB));

    let mgr = manager(dir.path(), &clock);
    assert_eq!(mgr.effective_global_limit(), Some(5 * MB));
    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    mgr.set_limit(id, Some(MB));
    clock.set(on(0, 19, 0));
    mgr.check_schedule();
    assert_eq!(mgr.effective_global_limit(), None, "unlimited globally");
    let snap = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(
        snap.limit,
        Some(MB),
        "the schedule never touches a download's own limit"
    );
    server.gate().release_all();
    wait_status(&mgr, id, Status::Completed).await;
}
