//! M5: the snapshot's `active_ms` — how long a download has spent running, for
//! the GUI's "5.8 GB in 2 min 14 s" summary. Waiting and paused time don't
//! count, it survives a restart of the store, and a start from zero resets it.
//! Also pins the `/seg-gated` fixture the GUI's lanes E2E relies on.

mod common;

use std::time::{Duration, Instant};

use rdm::{DownloadSnapshot, JsonStore, Manager, Status, Store};

const T: Duration = Duration::from_secs(10);

fn manager_in(dir: &std::path::Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

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

fn snap(mgr: &Manager, id: u64) -> DownloadSnapshot {
    mgr.snapshot().into_iter().find(|s| s.id == id).unwrap()
}

/// The snapshot's active time, which these downloads always know.
fn ms(s: &DownloadSnapshot) -> u64 {
    s.active_ms.expect("active time unknown")
}

/// A finished download reports the time it ran, bounded by the wall clock, and
/// the figure is persisted.
#[tokio::test]
async fn completed_download_reports_its_running_time() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let added = Instant::now();
    let id = mgr.add(server.url("/slow"), dir.path(), None, None);
    let done = wait_status(&mgr, id, Status::Completed).await;
    let wall = added.elapsed().as_millis() as u64;

    // /slow streams ~25 chunks at 15 ms each.
    assert!(ms(&done) >= 200, "active_ms {}", ms(&done));
    assert!(ms(&done) <= wall, "{} > wall {wall}", ms(&done));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(snap(&mgr, id).active_ms, done.active_ms, "kept counting");

    drop(mgr);
    let reloaded = manager_in(dir.path());
    assert_eq!(snap(&reloaded, id).active_ms, done.active_ms);
}

/// Paused time doesn't count, and resuming a partial keeps adding to it.
#[tokio::test]
async fn paused_time_is_not_counted() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    // The download's log goes into a failure message: it says whether the
    // resume continued the partial or started over.
    let _log = tracing::subscriber::set_default(tracing_subscriber::layer::SubscriberExt::with(
        tracing_subscriber::registry(),
        mgr.log_layer(),
    ));
    let id = mgr.add(server.url("/crawl"), dir.path(), None, None);
    // Bytes, not just time: a download paused at 0 bytes resumes as a start
    // from zero, which resets the clock by design (a slow runner got there).
    wait_for(&mgr, id, T, |s| ms(s) >= 300 && s.downloaded > 0)
        .await
        .expect("never ran 300 ms with bytes on disk");
    mgr.pause(id);
    let paused = wait_status(&mgr, id, Status::Paused).await;
    assert!(paused.downloaded > 0, "paused at 0 bytes");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        snap(&mgr, id).active_ms,
        paused.active_ms,
        "counted paused time"
    );

    mgr.resume(id);
    let resumed = wait_for(&mgr, id, T, |s| {
        s.status == Status::Downloading && s.downloaded > paused.downloaded
    })
    .await
    .expect("never resumed");
    let log: Vec<String> = mgr
        .download_log(id)
        .into_iter()
        .map(|l| l.message)
        .collect();
    assert!(
        ms(&resumed) >= ms(&paused),
        "a resumed partial lost its earlier time: {} ms at {} B paused, {} ms at {} B resumed; log: {log:#?}",
        ms(&paused),
        paused.downloaded,
        ms(&resumed),
        resumed.downloaded,
    );
    let done = wait_status(&mgr, id, Status::Completed).await;
    assert!(ms(&done) > ms(&paused));
}

/// A resume the server answers from byte 0 (it ignores the range) is a start
/// from zero, so the clock starts again.
#[tokio::test]
async fn a_resume_that_starts_over_resets_it() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    // `/gated` has no ranges: resuming it downloads from the start again.
    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    wait_for(&mgr, id, T, |s| ms(s) >= 500 && s.downloaded >= 8 * 1024)
        .await
        .expect("never ran 500 ms");
    mgr.pause(id);
    let paused = wait_status(&mgr, id, Status::Paused).await;

    mgr.resume(id);
    let restarted = wait_for(&mgr, id, T, |s| {
        s.status == Status::Downloading && s.downloaded > 0 && s.downloaded < paused.downloaded
    })
    .await
    .expect("never started over");
    assert!(
        ms(&restarted) < ms(&paused),
        "{} not reset from {}",
        ms(&restarted),
        ms(&paused)
    );
    mgr.stop(id, false);
}

/// Starting again from zero bytes starts the clock again.
#[tokio::test]
async fn a_restart_from_zero_resets_it() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/gated"), dir.path(), None, None);
    wait_for(&mgr, id, T, |s| ms(s) >= 500)
        .await
        .expect("never ran 500 ms");
    mgr.stop(id, false);
    let stopped = wait_status(&mgr, id, Status::Cancelled).await;
    assert!(ms(&stopped) >= 500);

    mgr.resume(id);
    let restarted = wait_status(&mgr, id, Status::Downloading).await;
    assert!(
        ms(&restarted) < ms(&stopped),
        "{} not reset from {}",
        ms(&restarted),
        ms(&stopped)
    );
    mgr.stop(id, false);
}

/// A crash mid-run (here: dropping the manager, which saves nothing) never
/// recorded that run's time, so after a restart the active time is unknown,
/// not too small, until the download starts again from zero.
#[tokio::test]
async fn a_run_cut_short_by_a_crash_is_unknown() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/crawl"), dir.path(), None, None);
    // Two chunks reported means the first is on disk, so the reload resumes
    // the partial rather than starting from zero.
    wait_for(&mgr, id, T, |s| s.downloaded >= 2 * 4096)
        .await
        .expect("never got bytes");
    drop(mgr);

    let reloaded = manager_in(dir.path());
    let restored = snap(&reloaded, id);
    assert_eq!(restored.status, Status::Paused);
    assert_eq!(restored.active_ms, None);

    reloaded.resume(id);
    let done = wait_status(&reloaded, id, Status::Completed).await;
    assert_eq!(done.active_ms, None, "a resumed partial guessed a time");
}

/// `shutdown` (the app quitting) records each running download's time and
/// restores it paused with that time.
#[tokio::test]
async fn shutdown_keeps_the_running_time() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/crawl"), dir.path(), None, None);
    let before = wait_for(&mgr, id, T, |s| ms(s) >= 300)
        .await
        .expect("never ran 300 ms");
    mgr.shutdown();
    drop(mgr);

    let reloaded = manager_in(dir.path());
    let restored = snap(&reloaded, id);
    assert_eq!(restored.status, Status::Paused);
    assert!(ms(&restored) >= ms(&before), "lost the running time");

    reloaded.resume(id);
    let done = wait_status(&reloaded, id, Status::Completed).await;
    assert!(ms(&done) > ms(&restored));
}

/// A download that wasn't running at shutdown keeps its recorded time.
#[tokio::test]
async fn a_paused_download_keeps_its_time_across_a_restart() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let id = mgr.add(server.url("/crawl"), dir.path(), None, None);
    wait_for(&mgr, id, T, |s| ms(s) >= 200)
        .await
        .expect("never ran 200 ms");
    mgr.pause(id);
    let paused = wait_status(&mgr, id, Status::Paused).await;
    drop(mgr);

    let reloaded = manager_in(dir.path());
    assert_eq!(snap(&reloaded, id).active_ms, paused.active_ms);
}

/// `/seg-gated` holds a 4-connection download at its 4-way plan: the ramp
/// opens Lanes 1–4, one per piece, the tiling never gains a stolen piece while
/// the gate is shut, and releasing it lets the download finish.
#[tokio::test]
async fn seg_gated_holds_the_initial_tiling() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    assert_eq!(mgr.settings().max_connections, 4);

    let id = mgr.add(server.url("/seg-gated"), dir.path(), None, None);
    wait_for(&mgr, id, T, |s| s.segments.len() == 4)
        .await
        .expect("never planned 4 segments");
    // Each piece trickles at the same rate, so every lane the ramp adds helps.
    wait_for(&mgr, id, Duration::from_secs(30), |s| {
        let mut lanes = s.lanes.clone();
        lanes.sort();
        lanes == [Some(1), Some(2), Some(3), Some(4)]
    })
    .await
    .expect("never ran Lanes 1-4, one per piece");
    assert_eq!(snap(&mgr, id).peak_lanes, 4);
    // Long enough for any steal to show.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let s = snap(&mgr, id);
        assert_eq!(s.status, Status::Downloading);
        assert_eq!(s.segments.len(), 4, "the tiling changed: {:?}", s.segments);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    server.gate().release_all();
    let done = wait_status(&mgr, id, Status::Completed).await;
    assert!(
        done.lanes.is_empty(),
        "lanes are live only: {:?}",
        done.lanes
    );
    assert_eq!(done.peak_lanes, 4, "the peak outlives the lanes");
}
