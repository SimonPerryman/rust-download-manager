//! M5: the snapshot's `segments` for a multi-connection download — sampled
//! through `subscribe` for the whole download, every view is a coherent tiling
//! of the file, and work-stealing shows up as a range shrinking.

mod common;

use std::time::{Duration, Instant};

use common::FixtureConfig;
use rdm::engine::state;
use rdm::{DownloadSnapshot, JsonStore, Manager, Segment, Status, Store};

const MIB: usize = 1024 * 1024;

/// Check one sample's tiling: it covers `[0, total)` exactly, with no gap or
/// overlap, and its `done` sum is the download's `downloaded`.
fn assert_tiles(segments: &[Segment], total: u64, downloaded: u64) {
    let mut sorted = segments.to_vec();
    sorted.sort_by_key(|s| s.start);
    let mut next = 0;
    for seg in &sorted {
        assert_eq!(seg.start, next, "gap or overlap at {seg:?} in {segments:?}");
        assert!(
            seg.end >= seg.start && seg.done <= seg.end - seg.start,
            "{seg:?}"
        );
        next = seg.end;
    }
    assert_eq!(next, total, "segments stop short: {segments:?}");
    let done: u64 = segments.iter().map(|s| s.done).sum();
    assert_eq!(done, downloaded, "Σdone != downloaded in {segments:?}");
}

/// AC: for a 4-connection download, every snapshot's `segments` cover the file
/// without overlap, `done` never decreases within a segment, the sum of `done`
/// equals `downloaded`, and a steal shows as a range shrinking.
#[tokio::test]
async fn segments_tile_the_file_and_show_steals() {
    // 4 MiB segments, twice the 1 MiB minimum, so once the ramped-up workers
    // finish segments 1-3 the slow segment 0 is still worth stealing from.
    let server = common::spawn_with(FixtureConfig {
        seg_len: 16 * MIB,
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.path().join("state.json")));
    let mgr = Manager::load(store, tokio::runtime::Handle::current()).unwrap();
    assert_eq!(mgr.settings().max_connections, 4);
    let total = (16 * MIB) as u64;

    let mut rx = mgr.subscribe();
    let id = mgr.add(server.url("/seg-skewed"), dir.path(), None, None);

    let mut samples = 0;
    let mut shrank = false;
    let mut prev: Vec<Segment> = Vec::new();
    let finished = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let snap = rx
                .borrow_and_update()
                .iter()
                .find(|s| s.id == id)
                .cloned()
                .expect("download listed");
            if !snap.segments.is_empty() {
                assert_tiles(&snap.segments, total, snap.downloaded);
                assert!(snap.segments.len() >= prev.len(), "a segment vanished");
                for (i, (before, now)) in prev.iter().zip(&snap.segments).enumerate() {
                    assert_eq!(before.start, now.start, "segment {i} moved");
                    assert!(now.done >= before.done, "segment {i}'s done went back");
                    shrank |= now.end < before.end;
                }
                prev = snap.segments.clone();
                samples += 1;
            }
            match snap.status {
                Status::Completed => return snap,
                Status::Downloading | Status::Queued => {}
                other => panic!("download ended {other:?}: {:?}", snap.error),
            }
            rx.changed().await.expect("manager alive");
        }
    })
    .await
    .expect("download never completed");

    assert_eq!(finished.downloaded, total);
    assert!(
        finished.segments.iter().all(|s| s.done == s.end - s.start),
        "the completed snapshot keeps the finished tiling"
    );
    assert!(samples > 5, "only {samples} samples");
    assert!(shrank, "no range shrank: {:?}", finished.segments);
    assert!(finished.segments.len() > 4, "no stolen range appended");
    assert_eq!(
        std::fs::read(finished.final_path.unwrap()).unwrap(),
        server.seg_payload()
    );
}

/// A single-connection download reports one segment over the whole file.
#[tokio::test]
async fn single_connection_reports_one_segment() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.path().join("state.json")));
    let mgr = Manager::load(store, tokio::runtime::Handle::current()).unwrap();

    let mut rx = mgr.subscribe();
    let id = mgr.add(server.url("/ranged"), dir.path(), None, None);
    let finished = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            rx.changed().await.expect("manager alive");
            let snap = rx.borrow().iter().find(|s| s.id == id).cloned().unwrap();
            if snap.status == Status::Completed {
                return snap;
            }
        }
    })
    .await
    .expect("download never completed");

    let total = common::payload().len() as u64;
    assert_eq!(
        finished.segments,
        [Segment {
            start: 0,
            end: total,
            done: total
        }]
    );
}

/// Poll download `id` until `pred` holds, panicking after 10 s.
async fn wait_for(
    mgr: &Manager,
    id: u64,
    pred: impl Fn(&DownloadSnapshot) -> bool,
) -> DownloadSnapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
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

/// A paused single-connection download (v1 sidecar) restores its one segment
/// from the on-disk `.part` after a restart, then resumes byte-identical.
#[tokio::test]
async fn restored_single_connection_download_shows_one_segment() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let manager = || {
        let store: Box<dyn Store> = Box::new(JsonStore::new(dir.path().join("state.json")));
        Manager::load(store, tokio::runtime::Handle::current()).unwrap()
    };
    let total = common::payload().len() as u64;

    let (id, target) = {
        let mgr = manager();
        mgr.set_max_connections(1);
        let id = mgr.add(server.url("/slow"), dir.path(), None, None);
        let partial = wait_for(&mgr, id, |s| s.downloaded > 0 && s.downloaded < total).await;
        mgr.pause(id);
        wait_for(&mgr, id, |s| s.status == Status::Paused).await;
        (id, partial.target)
    };
    let on_disk = state::downloaded_bytes(&target).expect("a `.part` is kept");
    assert!(on_disk > 0 && on_disk < total);

    let mgr = manager();
    let restored = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(restored.status, Status::Paused);
    assert_eq!(restored.downloaded, on_disk);
    assert_eq!(
        restored.segments,
        [Segment {
            start: 0,
            end: total,
            done: on_disk
        }]
    );

    mgr.resume(id);
    let done = wait_for(&mgr, id, |s| s.status == Status::Completed).await;
    assert_eq!(
        std::fs::read(done.final_path.unwrap()).unwrap(),
        common::payload()
    );
}
