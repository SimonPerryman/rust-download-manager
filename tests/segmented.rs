//! M3 acceptance tests, in-process: multi-segment parallel downloads against
//! the fixture's `/seg*` routes — correctness vs a single connection, speed-up,
//! resume of only the missing ranges, fallback for servers that don't honour
//! ranges, disconnects, work stealing and retry accounting.

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use common::{FixtureConfig, RouteStats};
use rdm::engine::state::{self, DownloadState};
use rdm::engine::{Ramping, Segment};
use rdm::{download, Control, DownloadRequest, Error, Progress, RateLimiter, RetryPolicy};

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

/// SHA-256 of a buffer, hex-encoded.
fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// A retry policy that never waits.
fn fast_retry(max_attempts: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        base_delay: Duration::ZERO,
        factor: 1.0,
        max_delay: Duration::ZERO,
    }
}

/// Poll until `cond()` holds or `timeout` elapses; returns whether it held.
async fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    cond()
}

/// Wait for every response body on a route to be released, so counters can be
/// reset without a straggler from an earlier leg leaking in.
async fn wait_idle(stats: &RouteStats) {
    assert!(
        wait_until(Duration::from_secs(10), || stats.in_flight() == 0).await,
        "route never went idle ({} in flight)",
        stats.in_flight()
    );
}

/// The `[start, end)` ranges a sidecar still needs.
fn missing(st: &DownloadState) -> Vec<(u64, u64)> {
    let mut gaps: Vec<_> = st
        .segments
        .as_ref()
        .expect("a v2 sidecar")
        .iter()
        .filter(|s| s.start + s.done < s.end)
        .map(|s| (s.start + s.done, s.end))
        .collect();
    gaps.sort();
    gaps
}

fn total_len(ranges: &[(u64, u64)]) -> u64 {
    ranges.iter().map(|(s, e)| e - s).sum()
}

/// Seed a v2 sidecar plus a full-size `.part` holding each segment's `done`
/// prefix, as if an earlier multi-segment run was interrupted.
fn seed_v2(target: &Path, url: &str, payload: &[u8], segments: Vec<Segment>) {
    let mut part = vec![0u8; payload.len()];
    for s in &segments {
        let (a, b) = (s.start as usize, (s.start + s.done) as usize);
        part[a..b].copy_from_slice(&payload[a..b]);
    }
    std::fs::write(state::part_path(target), part).unwrap();
    DownloadState::segmented(
        url,
        target,
        payload.len() as u64,
        Some(common::SEG_ETAG.to_string()),
        Some(common::LAST_MODIFIED.to_string()),
        segments,
        42,
    )
    .save(&state::sidecar_path(target))
    .unwrap();
}

/// Assert a run's progress reports are coherent: the total is always known,
/// `downloaded` never goes backwards or past it, and the last report is 100%.
fn assert_progress_coherent(seen: &[Progress], len: u64) {
    assert!(!seen.is_empty(), "no progress reported");
    assert!(seen.iter().all(|p| p.total == Some(len)), "total changed");
    assert!(
        seen.windows(2).all(|w| w[0].downloaded <= w[1].downloaded),
        "downloaded went backwards"
    );
    assert!(seen.iter().all(|p| p.downloaded <= len), "overshot total");
    assert_eq!(seen.last().unwrap().downloaded, len);
}

/// A sidecar temp file as a hard kill mid-save would leave it.
fn stale_temp(target: &Path) -> std::path::PathBuf {
    let mut name = state::sidecar_path(target).into_os_string();
    name.push(".4242.7.tmp");
    name.into()
}

fn assert_no_partial_state(target: &Path) {
    assert!(!state::part_path(target).exists(), "`.part` left behind");
    assert!(!state::sidecar_path(target).exists(), "sidecar left behind");
}

/// AC (R2): a multi-segment download is byte-identical to the payload and to a
/// single-connection download of the same resource.
#[tokio::test]
async fn multi_segment_matches_single_connection() {
    let server = common::spawn().await;
    let stats = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let payload = common::seg_payload();

    let multi_out = dir.path().join("multi.bin");
    let mut seen = Vec::new();
    let multi = download(
        &DownloadRequest::to_path(server.url("/seg"), &multi_out),
        &Control::new(),
        |p| seen.push(p),
    )
    .await
    .unwrap();
    assert_eq!(multi.status, 206, "should have gone multi-segment");
    assert!(stats.ranges().len() >= 3, "ranged: {:?}", stats.ranges());
    assert_no_partial_state(&multi_out);
    assert_progress_coherent(&seen, payload.len() as u64);

    wait_idle(&stats).await;
    stats.reset();
    let single_out = dir.path().join("single.bin");
    let single = download(
        &DownloadRequest::to_path(server.url("/seg"), &single_out).with_connections(1),
        &Control::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(single.status, 200);
    assert!(stats.ranges().is_empty());

    let multi_bytes = std::fs::read(&multi_out).unwrap();
    let single_bytes = std::fs::read(&single_out).unwrap();
    assert_eq!(sha256_hex(&multi_bytes), sha256_hex(&single_bytes));
    assert_eq!(multi_bytes, payload);
    assert_eq!(multi.bytes, payload.len() as u64);
}

/// AC (R3): multi-segment is measurably faster than one connection. The
/// concurrency assertion is the deterministic one; the timing compares two
/// runs of the same test, so both see the same timer inflation.
#[tokio::test]
async fn multi_segment_is_faster_than_single_connection() {
    let server = common::spawn_with(FixtureConfig {
        seg_len: MIB as usize,
        seg_chunk_delay: Duration::from_millis(50),
        ..FixtureConfig::default()
    })
    .await;
    let stats = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let payload = server.seg_payload();
    let total = payload.len() as u64;

    let single_out = dir.path().join("single.bin");
    let started = Instant::now();
    download(
        &DownloadRequest::to_path(server.url("/seg"), &single_out)
            .with_connections(1)
            .with_min_segment_size(128 * KIB),
        &Control::new(),
        |_| {},
    )
    .await
    .unwrap();
    let single_elapsed = started.elapsed();
    assert_eq!(stats.max_in_flight(), 1);

    wait_idle(&stats).await;
    stats.reset();
    let multi_out = dir.path().join("multi.bin");
    let started = Instant::now();
    let multi = download(
        &DownloadRequest::to_path(server.url("/seg"), &multi_out)
            .with_connections(4)
            .with_min_segment_size(128 * KIB)
            .with_ramp(Ramping::Off),
        &Control::new(),
        |_| {},
    )
    .await
    .unwrap();
    let multi_elapsed = started.elapsed();
    assert_eq!(multi.status, 206);
    assert!(stats.max_in_flight() >= 2, "never ran in parallel");

    // Workers drop a response as soon as its segment is done, so little more
    // than the file itself is ever pulled off the wire.
    wait_idle(&stats).await;
    let waste_allowance = 3 * common::SEG_CHUNK as u64 * stats.requests() as u64;
    assert!(
        stats.body_bytes() <= total + waste_allowance,
        "served {} bytes for a {total}-byte file",
        stats.body_bytes()
    );

    assert!(
        multi_elapsed.as_secs_f64() < 0.6 * single_elapsed.as_secs_f64(),
        "multi {multi_elapsed:?} vs single {single_elapsed:?}"
    );
    assert_eq!(std::fs::read(&single_out).unwrap(), payload);
    assert_eq!(std::fs::read(&multi_out).unwrap(), payload);
}

/// AC (R4, in-process): pause a multi-segment download mid-flight, then resume:
/// the resume requests exactly the ranges the sidecar records as missing, and
/// the file is byte-identical.
#[tokio::test]
async fn pause_then_resume_refetches_only_missing_ranges() {
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(20),
        ..FixtureConfig::default()
    })
    .await;
    let stats = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("paused.bin");
    let payload = server.seg_payload();
    let total = payload.len() as u64;

    // Default 1 MiB minimum: four 1 MiB segments, none big enough to steal from,
    // so the missing ranges map one-to-one onto resume requests.
    let req = DownloadRequest::to_path(server.url("/seg"), &out);
    let control = Control::new();
    let pauser = control.clone();
    let first = download(&req, &control, move |p| {
        if p.downloaded > 0 && p.downloaded < total {
            pauser.pause();
        }
    })
    .await;
    assert!(matches!(first, Err(Error::Paused)), "got {first:?}");

    let st = DownloadState::load(&state::sidecar_path(&out)).unwrap();
    assert_eq!(st.version, 2);
    let gaps = missing(&st);
    assert!(!gaps.is_empty() && total_len(&gaps) < total, "{gaps:?}");

    wait_idle(&stats).await;
    stats.reset();
    control.resume();
    let summary = download(&req, &control, |_| {}).await.unwrap();

    assert_eq!(
        stats.closed_ranges(),
        gaps,
        "resume must fetch exactly the gaps"
    );
    assert_eq!(stats.body_bytes(), total_len(&gaps));
    assert_eq!(stats.unranged(), 0);
    assert_eq!(summary.bytes, total);
    assert_eq!(std::fs::read(&out).unwrap(), payload);
    assert_no_partial_state(&out);
}

/// Cancelling a multi-segment download removes its `.part`, sidecar and any
/// stale sidecar temp file.
#[tokio::test]
async fn cancel_multi_segment_discards_partial_state() {
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(20),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("cancelled.bin");
    let total = common::SEG_LEN as u64;
    let temp = stale_temp(&out);

    let control = Control::new();
    let canceller = control.clone();
    let planted = temp.clone();
    let result = download(
        &DownloadRequest::to_path(server.url("/seg"), &out),
        &control,
        move |p| {
            if p.downloaded > 0 && p.downloaded < total {
                // Planted mid-run, so only the cancel path can sweep it.
                std::fs::write(&planted, b"{}").unwrap();
                canceller.cancel();
            }
        },
    )
    .await;
    assert!(matches!(result, Err(Error::Cancelled)), "got {result:?}");
    assert_no_partial_state(&out);
    assert!(!out.exists());
    assert!(!temp.exists(), "stale sidecar temp left behind");
}

/// Sidecar temp files a hard kill left mid-save are swept when a download
/// starts (even one that then fails) and when it finalizes; files that merely
/// look similar are left alone.
#[tokio::test]
async fn stale_sidecar_temps_are_swept() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("swept.bin");
    let temp = stale_temp(&out);
    let unrelated = dir.path().join("swept.bin.rdm.tmp");
    std::fs::write(&unrelated, b"").unwrap();

    std::fs::write(&temp, b"{}").unwrap();
    let req =
        DownloadRequest::to_path(server.url("/missing"), &out).with_retry(RetryPolicy::none());
    download(&req, &Control::new(), |_| {}).await.unwrap_err();
    assert!(!temp.exists(), "not swept at start");

    let planted = temp.clone();
    download(
        &DownloadRequest::to_path(server.url("/seg"), &out),
        &Control::new(),
        move |p| {
            if p.downloaded > 0 {
                std::fs::write(&planted, b"{}").unwrap();
            }
        },
    )
    .await
    .unwrap();
    assert!(!temp.exists(), "not swept at finalize");
    assert!(unrelated.exists(), "an unrelated file was swept");
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
}

/// AC (R5): a server without `Accept-Ranges` is never sent a `Range`, even
/// with a strong validator and a size worth splitting.
#[tokio::test]
async fn server_without_accept_ranges_stays_single_connection() {
    let server = common::spawn().await;
    let stats = server.stats("/seg-noaccept");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("noaccept.bin");

    let req = DownloadRequest::to_path(server.url("/seg-noaccept"), &out);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.status, 200);
    assert!(stats.ranges().is_empty(), "ranged: {:?}", stats.ranges());
    assert_eq!(stats.unranged(), 1);
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
    assert_no_partial_state(&out);
}

/// A server without `Accept-Ranges` or validators is never sent a `Range`.
#[tokio::test]
async fn norange_server_stays_single_connection() {
    let server = common::spawn().await;
    let stats = server.stats("/norange");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("norange.bin");

    // A tiny minimum so the file is big enough; only the missing header keeps
    // it single.
    let req = DownloadRequest::to_path(server.url("/norange"), &out).with_min_segment_size(4 * KIB);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.status, 200);
    assert!(stats.ranges().is_empty());
    assert_eq!(stats.unranged(), 1);
    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
}

/// AC (R5): a server that advertises ranges (with a validator) but answers a
/// `Range` with 200 is detected; the download falls back to exactly one more
/// plain GET and completes correctly.
#[tokio::test]
async fn range_liar_falls_back_to_single_connection() {
    let server = common::spawn().await;
    let stats = server.stats("/liar-etag");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("liar.bin");

    let req = DownloadRequest::to_path(server.url("/liar-etag"), &out).with_connections(4);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    let ranged = stats.ranges().len();
    assert!((1..=4).contains(&ranged), "ranged requests: {ranged}");
    assert_eq!(stats.unranged(), 2, "the probe GET plus one fallback GET");
    assert_eq!(summary.status, 200);
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
    assert_no_partial_state(&out);
}

/// AC (R5): `/liar` has no validator, so ranges can't be pinned: stay single.
#[tokio::test]
async fn liar_without_validator_stays_single_connection() {
    let server = common::spawn().await;
    let stats = server.stats("/liar");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("liar.bin");

    let req = DownloadRequest::to_path(server.url("/liar"), &out).with_min_segment_size(4 * KIB);
    download(&req, &Control::new(), |_| {}).await.unwrap();

    assert!(stats.ranges().is_empty());
    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
}

/// R6: a connection dropped mid-segment is survived and the file is correct.
#[tokio::test]
async fn mid_segment_disconnect_is_survived() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("flaky.bin");

    let req = DownloadRequest::to_path(server.url("/seg-flaky"), &out).with_retry(fast_retry(5));
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.status, 206);
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
}

/// R6: a segment handed back by the last live worker gets a replacement
/// connection, so the download completes even with no outer retries.
#[tokio::test]
async fn segment_dropped_by_last_worker_is_picked_up() {
    let server = common::spawn().await;
    let stats = server.stats("/seg-flaky-tail");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("tail.bin");
    let total = common::SEG_LEN as u64;

    // Default 1 MiB minimum: four 1 MiB segments, none stealable, so the tail
    // worker really is the last one running when its connection drops.
    let req = DownloadRequest::to_path(server.url("/seg-flaky-tail"), &out)
        .with_retry(RetryPolicy::none())
        .with_ramp(Ramping::Off);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.status, 206);
    let tail_requests = stats
        .closed_ranges()
        .iter()
        .filter(|&&(_, end)| end == total)
        .count();
    assert_eq!(tail_requests, 2, "ranges: {:?}", stats.closed_ranges());
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
    assert_no_partial_state(&out);
}

/// R6 + retry accounting: short 206s are re-requested inside the worker, so
/// the download completes even with no outer retries at all.
#[tokio::test]
async fn short_ranged_responses_are_rerequested_in_worker() {
    let server = common::spawn().await;
    let stats = server.stats("/seg-short");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("short.bin");

    let req =
        DownloadRequest::to_path(server.url("/seg-short"), &out).with_retry(RetryPolicy::none());
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.status, 206);
    // Three 1 MiB segments at <= 256 KiB per response.
    assert!(
        stats.ranges().len() >= 12,
        "ranged: {}",
        stats.ranges().len()
    );
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
}

/// A weak-ETag host still goes multi-segment, pinning ranges with
/// `If-Range: <Last-Modified>` (weak ETags are never sent in `If-Range`).
#[tokio::test]
async fn weak_etag_host_uses_last_modified_for_if_range() {
    let server = common::spawn().await;
    let stats = server.stats("/seg-weak-etag");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("weak.bin");

    let summary = download(
        &DownloadRequest::to_path(server.url("/seg-weak-etag"), &out),
        &Control::new(),
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(summary.status, 206);
    let if_ranges = stats.if_ranges();
    assert!(!if_ranges.is_empty());
    assert!(
        if_ranges.iter().all(|v| v == common::LAST_MODIFIED),
        "{if_ranges:?}"
    );
    assert_eq!(if_ranges.len(), stats.ranges().len());
    assert_eq!(stats.unranged(), 1);
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
}

/// A host capping connections (503 beyond 2) still completes: workers whose
/// request is refused hand their segment back to the others.
#[tokio::test]
async fn connection_capping_host_completes() {
    // Slow enough that the three ranged requests overlap and hit the cap.
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(5),
        ..FixtureConfig::default()
    })
    .await;
    let stats = server.stats("/seg-capped");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("capped.bin");

    let req = DownloadRequest::to_path(server.url("/seg-capped"), &out)
        .with_connections(4)
        .with_retry(fast_retry(10))
        .with_ramp(Ramping::Off);
    download(&req, &Control::new(), |_| {}).await.unwrap();

    assert!(stats.refused() >= 1, "the cap was never hit");
    // A refused segment is re-fetched once, not duplicated: at most one
    // accepted ranged request per planned 1 MiB segment.
    let accepted = stats.ranges().len() - stats.refused();
    assert!(accepted <= 4, "{accepted} accepted ranged requests");
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
}

/// Resume uses every connection even when few segments remain: idle workers
/// steal from the one big remaining segment.
#[tokio::test]
async fn resume_spreads_remaining_segment_across_connections() {
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(20),
        ..FixtureConfig::default()
    })
    .await;
    let stats = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("resumed.bin");
    let payload = server.seg_payload();
    let total = payload.len() as u64;

    seed_v2(
        &out,
        &server.url("/seg"),
        &payload,
        vec![
            Segment {
                start: 0,
                end: MIB,
                done: MIB,
            },
            Segment {
                start: MIB,
                end: total,
                done: 0,
            },
        ],
    );
    let req = DownloadRequest::to_path(server.url("/seg"), &out)
        .with_connections(4)
        .with_min_segment_size(256 * KIB)
        .with_ramp(Ramping::Off);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.status, 206);
    assert!(stats.max_in_flight() >= 2, "resume ran on one connection");
    assert_eq!(stats.unranged(), 0);
    // The completed first MiB is never re-requested. (Ranges may overlap: the
    // victim's request still spans the tail it lost to a steal.)
    assert!(stats.closed_ranges().iter().all(|&(s, _)| s >= MIB));
    assert_eq!(std::fs::read(&out).unwrap(), payload);
}

/// Retry accounting: attempts that make progress don't consume the budget, so
/// a download whose every ranged response drops completes with 2 attempts.
#[tokio::test]
async fn progressing_attempts_do_not_consume_retries() {
    let server = common::spawn_with(FixtureConfig {
        seg_abort_after: 4,
        ..FixtureConfig::default()
    })
    .await;
    let stats = server.stats("/seg-drop");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("drop.bin");
    let total = common::SEG_LEN as u64;

    let req = DownloadRequest::to_path(server.url("/seg-drop"), &out)
        .with_connections(2)
        .with_retry(fast_retry(2))
        .with_ramp(Ramping::Off);
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.status, 206);
    // Two attempts of two connections can't fetch 2 MiB at 128 KiB a response:
    // many more attempts ran, each making progress.
    let ranged = stats.ranges().len();
    assert!(ranged > 2 * 2, "only {ranged} ranged requests");
    assert!(stats.body_bytes() >= total);
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
}

/// Retry accounting: a resumed download that makes no progress gives up after
/// exactly `max_attempts` attempts, keeping its v2 state for later.
#[tokio::test]
async fn stalled_resume_gives_up_after_max_attempts() {
    let server = common::spawn().await;
    let stats = server.stats("/seg-503");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("stalled.bin");
    let payload = common::seg_payload();
    let total = payload.len() as u64;

    let segments = vec![
        Segment {
            start: 0,
            end: MIB,
            done: MIB,
        },
        Segment {
            start: MIB,
            end: total,
            done: 0,
        },
    ];
    seed_v2(&out, &server.url("/seg-503"), &payload, segments.clone());

    // One connection: per attempt, one ranged request plus its two bounded
    // replacement connections.
    let req = DownloadRequest::to_path(server.url("/seg-503"), &out)
        .with_connections(1)
        .with_retry(fast_retry(3));
    let err = download(&req, &Control::new(), |_| {}).await.unwrap_err();

    assert!(matches!(err, Error::Status(503)), "got {err:?}");
    assert_eq!(stats.ranges().len(), 3 * 3);
    let st = DownloadState::load(&state::sidecar_path(&out)).unwrap();
    assert_eq!(st.segments, Some(segments), "no progress, no truncation");
}

/// R7: an idle worker steals the back half of the slow segment, so a ranged
/// request starts somewhere other than a planned boundary.
#[tokio::test]
async fn idle_worker_steals_from_slow_segment() {
    let server = common::spawn().await;
    let stats = server.stats("/seg-skewed");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("skewed.bin");

    // 256 KiB minimum: the 1 MiB segment 0 is stealable.
    let req = DownloadRequest::to_path(server.url("/seg-skewed"), &out)
        .with_connections(4)
        .with_min_segment_size(256 * KIB)
        .with_ramp(Ramping::Off);
    let mut seen = Vec::new();
    download(&req, &Control::new(), |p| seen.push(p))
        .await
        .unwrap();
    // A steal shrinks the victim while its response still covers the tail;
    // the overlap must not double-count.
    assert_progress_coherent(&seen, common::SEG_LEN as u64);

    let planned = [0, MIB, 2 * MIB, 3 * MIB];
    let starts: Vec<u64> = stats.closed_ranges().iter().map(|(s, _)| *s).collect();
    assert!(
        starts.iter().any(|s| !planned.contains(s)),
        "no stolen range in {starts:?}"
    );
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
}

/// Ranged requests go to the post-redirect URL.
#[tokio::test]
async fn ranged_requests_follow_the_redirect() {
    let server = common::spawn().await;
    let redirect = server.stats("/seg-redirect");
    let seg = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("redirected.bin");

    let summary = download(
        &DownloadRequest::to_path(server.url("/seg-redirect"), &out),
        &Control::new(),
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(summary.status, 206);
    assert_eq!(redirect.requests(), 1);
    assert!(redirect.ranges().is_empty());
    assert!(!seg.ranges().is_empty());
    assert_eq!(seg.unranged(), 1);
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
}

/// A `.part` that doesn't match a v2 sidecar's total is discarded and the
/// download restarts cleanly.
#[tokio::test]
async fn inconsistent_v2_part_restarts_fresh() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("bad.bin");
    let payload = common::seg_payload();
    let total = payload.len() as u64;

    seed_v2(
        &out,
        &server.url("/seg"),
        &payload,
        vec![Segment {
            start: 0,
            end: total,
            done: MIB,
        }],
    );
    std::fs::write(state::part_path(&out), b"truncated").unwrap();

    download(
        &DownloadRequest::to_path(server.url("/seg"), &out),
        &Control::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), payload);
}

/// A signed redirect target that expires mid-download (403) doesn't fail it:
/// the segment is handed back, and the retry re-resolves the redirect from
/// the original URL.
#[tokio::test]
async fn expired_redirect_target_is_re_resolved() {
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(5),
        ..FixtureConfig::default()
    })
    .await;
    let expiring = server.stats("/seg-expiring");
    let signed = server.stats("/seg-signed");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("signed.bin");

    let req = DownloadRequest::to_path(server.url("/seg-expiring"), &out)
        .with_connections(4)
        .with_retry(fast_retry(3));
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.status, 206);
    assert!(signed.refused() >= 1, "the first target never expired");
    assert!(
        expiring.requests() >= 2,
        "the redirect was never re-resolved"
    );
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
    assert_no_partial_state(&out);
}

/// A `.part` whose sidecar is unloadable (here: v2 segments that don't tile
/// the file) is discarded straight away, so a kill before the fresh download
/// truncates it can't pair it with a new sidecar; the re-run is correct.
#[tokio::test]
async fn part_with_invalid_sidecar_is_discarded() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("invalid.bin");
    let seed = || {
        std::fs::write(state::part_path(&out), vec![0xAB; common::SEG_LEN]).unwrap();
        let only_first_mib = vec![Segment {
            start: 0,
            end: MIB,
            done: MIB,
        }];
        let st = DownloadState::segmented(
            server.url("/seg"),
            &out,
            common::SEG_LEN as u64,
            Some(common::SEG_ETAG.to_string()),
            None,
            only_first_mib,
            1,
        );
        st.save(&state::sidecar_path(&out)).unwrap();
        assert!(DownloadState::load(&state::sidecar_path(&out)).is_err());
    };

    // A run that fails before writing anything still drops the stale pair.
    seed();
    let req =
        DownloadRequest::to_path(server.url("/missing"), &out).with_retry(RetryPolicy::none());
    download(&req, &Control::new(), |_| {}).await.unwrap_err();
    assert_no_partial_state(&out);

    seed();
    download(
        &DownloadRequest::to_path(server.url("/seg"), &out),
        &Control::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
    assert_no_partial_state(&out);
}

/// Resuming a v2 download whose resource changed (stale strong ETag): the
/// ranged requests get full 200s, so the stale bytes are discarded and the
/// file is re-fetched over one connection.
#[tokio::test]
async fn changed_resource_restarts_v2_resume() {
    let server = common::spawn().await;
    let stats = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("changed.bin");
    let total = common::SEG_LEN as u64;

    // Garbage where the sidecar claims bytes are done: only a restart fixes it.
    std::fs::write(state::part_path(&out), vec![0xCD; common::SEG_LEN]).unwrap();
    DownloadState::segmented(
        server.url("/seg"),
        &out,
        total,
        Some("\"seg-v0\"".to_string()),
        None,
        vec![
            Segment {
                start: 0,
                end: 2 * MIB,
                done: MIB,
            },
            Segment {
                start: 2 * MIB,
                end: total,
                done: MIB,
            },
        ],
        42,
    )
    .save(&state::sidecar_path(&out))
    .unwrap();

    let summary = download(
        &DownloadRequest::to_path(server.url("/seg"), &out),
        &Control::new(),
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(summary.status, 200);
    assert_eq!(stats.unranged(), 1, "one fallback GET");
    assert!(stats.if_ranges().iter().all(|v| v == "\"seg-v0\""));
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
    assert_no_partial_state(&out);
}

/// The claim hook runs once even when a segmented attempt falls back to a
/// single connection (the derived target is remembered, not re-claimed).
#[tokio::test]
async fn claim_target_runs_once_across_range_fallback() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let claims = Arc::new(AtomicUsize::new(0));
    let counter = claims.clone();

    let req = DownloadRequest::new(server.url("/liar-etag"), dir.path()).with_claim_target(
        move |proposed| {
            counter.fetch_add(1, Ordering::SeqCst);
            proposed.to_path_buf()
        },
    );
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(claims.load(Ordering::SeqCst), 1);
    assert_eq!(summary.status, 200, "fell back to one connection");
    assert_eq!(summary.path, dir.path().join("liar-etag"));
    assert_eq!(std::fs::read(&summary.path).unwrap(), common::seg_payload());
}

/// A response with any `Content-Encoding` (even `identity`) stays single:
/// byte ranges of an encoded body don't map onto the decoded file.
#[tokio::test]
async fn content_encoded_response_stays_single_connection() {
    let server = common::spawn().await;
    let stats = server.stats("/seg-identity");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("encoded.bin");

    let summary = download(
        &DownloadRequest::to_path(server.url("/seg-identity"), &out),
        &Control::new(),
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(summary.status, 200);
    assert!(stats.ranges().is_empty(), "ranged: {:?}", stats.ranges());
    assert_eq!(std::fs::read(&out).unwrap(), common::seg_payload());
}

/// A sidecar written by a newer rdm is neither resumed nor discarded: the
/// download fails with a clear, non-transient error and leaves the pair alone.
#[tokio::test]
async fn newer_sidecar_version_is_kept_and_fails_clearly() {
    let server = common::spawn().await;
    let stats = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("future.bin");
    let part = state::part_path(&out);
    let sidecar = state::sidecar_path(&out);
    let sidecar_json = r#"{"version":3,"url":"x","target":"future.bin","chunks":[]}"#;
    std::fs::write(&part, vec![0xEE; common::SEG_LEN]).unwrap();
    std::fs::write(&sidecar, sidecar_json).unwrap();

    let req = DownloadRequest::to_path(server.url("/seg"), &out).with_retry(fast_retry(5));
    let err = download(&req, &Control::new(), |_| {}).await.unwrap_err();

    assert!(matches!(err, Error::UnsupportedState(3)), "got {err:?}");
    assert!(err.to_string().contains("version 3"), "{err}");
    assert_eq!(stats.requests(), 0, "nothing fetched, no retries");
    assert_eq!(std::fs::read(&part).unwrap().len(), common::SEG_LEN);
    assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), sidecar_json);
    assert_eq!(state::downloaded_bytes(&out), None);
}

/// Cancelling a single-connection download also sweeps stale sidecar temps.
#[tokio::test]
async fn cancel_single_connection_sweeps_stale_temps() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("cancelled.bin");
    let temp = stale_temp(&out);

    let control = Control::new();
    let canceller = control.clone();
    let planted = temp.clone();
    let result = download(
        &DownloadRequest::to_path(server.url("/slow"), &out),
        &control,
        move |p| {
            if p.downloaded > 0 {
                // Planted mid-run, so only the cancel path can sweep it.
                std::fs::write(&planted, b"{}").unwrap();
                canceller.cancel();
            }
        },
    )
    .await;
    assert!(matches!(result, Err(Error::Cancelled)), "got {result:?}");
    assert_no_partial_state(&out);
    assert!(!temp.exists(), "stale sidecar temp left behind");
}

/// Stale temps beside a derived (not explicit) target are swept once the
/// target is known. A pause keeps everything else, so only that sweep runs.
#[tokio::test]
async fn stale_temps_for_derived_target_are_swept() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let derived = dir.path().join("slow");
    let temp = stale_temp(&derived);
    std::fs::write(&temp, b"{}").unwrap();

    let control = Control::new();
    let pauser = control.clone();
    let result = download(
        &DownloadRequest::new(server.url("/slow"), dir.path()),
        &control,
        move |p| {
            if p.downloaded > 0 {
                pauser.pause();
            }
        },
    )
    .await;
    assert!(matches!(result, Err(Error::Paused)), "got {result:?}");
    assert!(
        state::part_path(&derived).exists(),
        "not the derived target"
    );
    assert!(
        !temp.exists(),
        "stale temp for the derived target not swept"
    );
}

/// A pause is honoured promptly even while every connection is blocked waiting
/// for its first byte: the coordinator polls the control itself.
#[tokio::test]
async fn pause_is_noticed_while_all_workers_wait_on_the_network() {
    const CHUNK_DELAY: Duration = Duration::from_secs(3);
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: CHUNK_DELAY,
        ..FixtureConfig::default()
    })
    .await;
    let stats = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("blocked.bin");

    let req = DownloadRequest::to_path(server.url("/seg"), &out).with_ramp(Ramping::Off);
    let control = Control::new();
    let run = {
        let control = control.clone();
        tokio::spawn(async move { download(&req, &control, |_| {}).await })
    };

    // Four segments: the probe GET serves the first, three ranged requests the
    // rest. Once all are in, every worker is waiting on a delayed first chunk.
    assert!(
        wait_until(CHUNK_DELAY, || stats.ranges().len() == 3).await,
        "ranged requests never all arrived: {:?}",
        stats.ranges()
    );
    assert_eq!(stats.body_bytes(), 0, "a chunk landed before the pause");
    let paused_at = Instant::now();
    control.pause();
    let result = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("download never returned")
        .unwrap();
    let waited = paused_at.elapsed();

    assert!(matches!(result, Err(Error::Paused)), "got {result:?}");
    assert!(waited < Duration::from_secs(1), "pause took {waited:?}");
    let st = DownloadState::load(&state::sidecar_path(&out)).unwrap();
    assert_eq!(st.version, 2);
}

/// M4: a throttled worker sleeping off rate-limit debt still notices a
/// sibling's fatal failure, so the download fails promptly instead of
/// waiting out the debt first.
#[tokio::test]
async fn sibling_failure_interrupts_a_throttled_worker() {
    // A 1 KiB/s cap: past the 64 KiB burst, even one more small chunk is
    // seconds of debt for the probe's worker.
    const RATE: u64 = KIB;
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("forbidden.bin");

    let req = DownloadRequest::to_path(server.url("/seg-403"), &out)
        .with_connections(4)
        .with_retry(fast_retry(1))
        .with_ramp(Ramping::Off);
    let control = Control::new().with_limiters([Arc::new(RateLimiter::new(Some(RATE)))]);
    let started = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(10), download(&req, &control, |_| {}))
        .await
        .expect("download never returned: the throttled worker waited out its debt");
    let elapsed = started.elapsed();

    assert!(matches!(result, Err(Error::Status(403))), "got {result:?}");
    // The 403s land after `FORBIDDEN_DELAY`; the debt would take >= 4 s.
    assert!(
        elapsed < Duration::from_secs(2),
        "took {elapsed:?}: the throttled worker waited out its debt"
    );
}
