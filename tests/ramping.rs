//! M4 PR B acceptance tests, in-process and on the real clock: a multi-segment
//! download starts on one connection and ramps while the measured speed keeps
//! rising, settling where the host stops getting faster.
//!
//! The `/seg-ramp` fixture serves each connection at R and all of them at 4R
//! (deadline-paced, no burst), so the ramp's decisions are known in advance.
//! Assertions are on connection counts, never on elapsed time.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{FixtureConfig, SEG_RAMP_RATE};
use rdm::engine::state::{self, DownloadState};
use rdm::engine::{Ramping, Segment};
use rdm::{download, Control, DownloadRequest, Error, Progress, RateLimiter};

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

/// A short window so a full ramp fits in a few seconds.
const WINDOW: Duration = Duration::from_millis(500);

/// Small enough that a fixture chunk time is a sliver of a window.
const MIN_SEGMENT: u64 = 64 * KIB;

async fn ramp_server(seg_len: u64) -> common::TestServer {
    common::spawn_with(FixtureConfig {
        seg_len: seg_len as usize,
        ..FixtureConfig::default()
    })
    .await
}

/// A ramping request for `route` with up to `max` connections.
fn ramping(url: String, out: &std::path::Path, max: u8) -> DownloadRequest {
    DownloadRequest::to_path(url, out)
        .with_connections(max)
        .with_min_segment_size(MIN_SEGMENT)
        .with_ramp(Ramping::Window(WINDOW))
}

fn peak(seen: &[Progress]) -> u8 {
    seen.iter().map(|p| p.connections).max().unwrap_or(0)
}

/// The reports from the first one below `peak` that follows the peak.
fn after_peak(seen: &[Progress], peak: u8) -> &[Progress] {
    let at = seen
        .iter()
        .position(|p| p.connections == peak)
        .expect("peak reported");
    let settled = seen[at..]
        .iter()
        .position(|p| p.connections < peak)
        .expect("never dropped below the peak");
    &seen[at + settled..]
}

/// AC (R9): against a host whose aggregate caps at 4 connections' worth, a
/// download allowed 8 tries a fifth, finds it no faster, and settles at 4.
#[tokio::test(flavor = "multi_thread")]
async fn settles_at_the_fixture_aggregate_cap() {
    let server = ramp_server(12 * MIB).await;
    let stats = server.stats("/seg-ramp");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("ramped.bin");
    let total = 12 * MIB;

    let mut seen = Vec::new();
    let summary = download(
        &ramping(server.url("/seg-ramp"), &out, 8),
        &Control::new(),
        |p| seen.push(p),
    )
    .await
    .unwrap();

    assert_eq!(summary.status, 206);
    assert_eq!(seen.first().unwrap().connections, 1, "starts on one");
    assert_eq!(peak(&seen), 5, "the trial fifth connection");
    assert!(
        stats.max_in_flight() <= 5,
        "{} responses in flight",
        stats.max_in_flight()
    );
    let settled: Vec<u8> = after_peak(&seen, 5)
        .iter()
        .take_while(|p| p.downloaded <= total * 3 / 4)
        .map(|p| p.connections)
        .collect();
    assert!(!settled.is_empty(), "retired only past 75%");
    assert!(
        settled.iter().all(|&c| c == 4),
        "after the retire: {settled:?}"
    );
    assert_eq!(std::fs::read(&out).unwrap(), server.seg_payload());
}

/// AC (R9): a speed limit well below one connection's speed means every
/// window is spent waiting on tokens, so the ramp never adds a connection.
#[tokio::test(flavor = "multi_thread")]
async fn binding_speed_limit_never_ramps() {
    let server = ramp_server(12 * MIB).await;
    let stats = server.stats("/seg-ramp");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("limited.bin");

    let limiter = Arc::new(RateLimiter::new(Some(SEG_RAMP_RATE / 8)));
    let control = Control::new().with_limiters([limiter]);
    // Eight windows: unthrottled, the ramp would have added by the second.
    let stopper = control.clone();
    tokio::spawn(async move {
        tokio::time::sleep(WINDOW * 8).await;
        stopper.cancel();
    });
    let mut seen = Vec::new();
    let result = download(&ramping(server.url("/seg-ramp"), &out, 8), &control, |p| {
        seen.push(p)
    })
    .await;

    assert!(matches!(result, Err(Error::Cancelled)), "got {result:?}");
    assert!(seen.iter().any(|p| p.downloaded > 0), "never streamed");
    assert_eq!(stats.max_in_flight(), 1, "a second connection was opened");
    assert_eq!(peak(&seen), 1);
}

/// A host that refuses a third connection (503) settles the ramp at two:
/// after the refusal no more than two connections are ever reported.
#[tokio::test(flavor = "multi_thread")]
async fn capping_host_settles_below_its_cap() {
    // ~1 MiB/s per connection, so 16 MiB outlasts well over 8 windows.
    let server = common::spawn_with(FixtureConfig {
        seg_len: 16 * MIB as usize,
        seg_chunk_delay: Duration::from_millis(30),
        ..FixtureConfig::default()
    })
    .await;
    let stats = server.stats("/seg-capped");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("capped.bin");

    let req = DownloadRequest::to_path(server.url("/seg-capped"), &out)
        .with_connections(4)
        .with_ramp(Ramping::Window(WINDOW));
    let mut seen = Vec::new();
    download(&req, &Control::new(), |p| seen.push(p))
        .await
        .unwrap();

    assert!(stats.refused() >= 1, "the cap was never hit");
    assert!(stats.max_in_flight() <= common::SEG_CAP);
    assert_eq!(peak(&seen), 3, "the refused trial connection");
    let later: Vec<u8> = after_peak(&seen, 3).iter().map(|p| p.connections).collect();
    assert!(
        later.iter().all(|&c| c <= 2),
        "ramped past the cap again: {later:?}"
    );
    assert_eq!(std::fs::read(&out).unwrap(), server.seg_payload());
}

/// Resuming a v2 sidecar ramps from one connection (stealing from the two
/// seeded segments) and still produces a byte-identical file.
#[tokio::test(flavor = "multi_thread")]
async fn v2_resume_ramps_and_completes() {
    let total = 4 * MIB;
    let server = ramp_server(total).await;
    let stats = server.stats("/seg-ramp");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("resumed.bin");
    let payload = server.seg_payload();

    let segments = vec![
        Segment {
            start: 0,
            end: 2 * MIB,
            done: MIB,
        },
        Segment {
            start: 2 * MIB,
            end: total,
            done: 0,
        },
    ];
    let mut part = vec![0u8; payload.len()];
    part[..MIB as usize].copy_from_slice(&payload[..MIB as usize]);
    std::fs::write(state::part_path(&out), part).unwrap();
    DownloadState::segmented(
        server.url("/seg-ramp"),
        &out,
        total,
        Some(common::SEG_ETAG.to_string()),
        Some(common::LAST_MODIFIED.to_string()),
        segments,
        7,
    )
    .save(&state::sidecar_path(&out))
    .unwrap();

    let mut seen = Vec::new();
    let summary = download(
        &ramping(server.url("/seg-ramp"), &out, 8),
        &Control::new(),
        |p| seen.push(p),
    )
    .await
    .unwrap();

    assert_eq!(summary.status, 206);
    assert_eq!(stats.unranged(), 0, "resumed, not restarted");
    assert!(peak(&seen) >= 2, "never ramped: peak {}", peak(&seen));
    assert!(stats.closed_ranges().iter().all(|&(s, _)| s >= MIB));
    assert_eq!(std::fs::read(&out).unwrap(), payload);
}

/// Start a ramping `/seg-ramp` download and stop it (via `stop`) once a
/// second connection is reported.
async fn stop_once_ramped(
    server: &common::TestServer,
    out: &std::path::Path,
    stop: fn(&Control),
) -> rdm::Result<rdm::Summary> {
    let control = Control::new();
    let stopper = control.clone();
    download(&ramping(server.url("/seg-ramp"), out, 8), &control, |p| {
        if p.connections >= 2 {
            stop(&stopper);
        }
    })
    .await
}

/// Pausing mid-ramp keeps a consistent v2 state that resumes (ramping again)
/// to a byte-identical file.
#[tokio::test(flavor = "multi_thread")]
async fn pause_while_ramping_resumes_cleanly() {
    let server = ramp_server(4 * MIB).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("paused.bin");

    let first = stop_once_ramped(&server, &out, Control::pause).await;
    assert!(matches!(first, Err(Error::Paused)), "got {first:?}");
    let st = DownloadState::load(&state::sidecar_path(&out)).unwrap();
    assert_eq!(st.version, 2);
    let done: u64 = st.segments.iter().flatten().map(|s| s.done).sum();
    assert!(done > 0 && done < 4 * MIB, "checkpointed {done}");

    let summary = download(
        &ramping(server.url("/seg-ramp"), &out, 8),
        &Control::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(summary.bytes, 4 * MIB);
    assert_eq!(std::fs::read(&out).unwrap(), server.seg_payload());
}

/// Cancelling mid-ramp discards all partial state.
#[tokio::test(flavor = "multi_thread")]
async fn cancel_while_ramping_discards_partial_state() {
    let server = ramp_server(4 * MIB).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("cancelled.bin");

    let result = stop_once_ramped(&server, &out, Control::cancel).await;
    assert!(matches!(result, Err(Error::Cancelled)), "got {result:?}");
    assert!(!state::part_path(&out).exists(), "`.part` left behind");
    assert!(!state::sidecar_path(&out).exists(), "sidecar left behind");
    assert!(!out.exists());
}
