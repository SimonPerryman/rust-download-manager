//! M4 acceptance tests, in-process and on the real clock: a per-download speed
//! limit holds whether the download uses one connection or several.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::FixtureConfig;
use rdm::engine::Ramping;
use rdm::{download, Control, DownloadRequest, RateLimiter};

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const LIMIT: u64 = 512 * KIB;

/// Download `/seg` (1 MiB) with `connections` under a 512 KiB/s limit and
/// check the time taken (ramping off, so every connection runs from the
/// start). With debt, the last chunk's wait ends only once the
/// balance is back to zero, so `elapsed ≥ (size − burst) / rate` however the
/// bytes were split across connections; slow CI can only lengthen it.
async fn assert_limited(connections: u8) {
    let server = common::spawn_with(FixtureConfig {
        seg_len: MIB as usize,
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("limited.bin");
    let limiter = Arc::new(RateLimiter::new(Some(LIMIT)));
    let control = Control::new().with_limiters([limiter.clone()]);
    let size = server.seg_payload().len() as u64;
    let ideal = Duration::from_secs_f64((size - limiter.burst()) as f64 / LIMIT as f64);

    let started = Instant::now();
    download(
        &DownloadRequest::to_path(server.url("/seg"), &out)
            .with_connections(connections)
            .with_min_segment_size(256 * KIB)
            .with_ramp(Ramping::Off),
        &control,
        |_| {},
    )
    .await
    .unwrap();
    let elapsed = started.elapsed();

    assert_eq!(std::fs::read(&out).unwrap(), server.seg_payload());
    assert!(
        elapsed >= ideal.mul_f64(0.95),
        "{elapsed:?} is faster than the {LIMIT} B/s limit allows ({ideal:?})"
    );
    // A sanity bound only (catches a hang or gross over-throttling).
    assert!(elapsed <= ideal * 3, "{elapsed:?} vs ideal {ideal:?}");
    // One request per connection (the fixture is fast, so none are retried).
    assert_eq!(
        server.stats("/seg").requests(),
        usize::from(connections),
        "the download should use the requested connections"
    );
}

/// AC (R2): the limit holds across four parallel connections sharing one
/// limiter.
#[tokio::test]
async fn rate_limit_holds_across_parallel_connections() {
    assert_limited(4).await;
}

/// The read timeout is an idle limit on the server, not a total deadline: our
/// own throttle sleeping past it between reads doesn't fail the download.
#[tokio::test(flavor = "multi_thread")]
async fn throttle_sleeps_longer_than_read_timeout_still_complete() {
    // Nothing is banked at the start, so 12 KiB at 4 KiB/s sleeps ~3 s.
    let server = common::spawn_with(FixtureConfig {
        seg_len: 12 * KIB as usize,
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("slow.bin");
    let read_timeout = Duration::from_secs(1);
    let control = Control::new().with_limiters([Arc::new(RateLimiter::new(Some(4 * KIB)))]);

    let mut reported = Vec::new();
    download(
        &DownloadRequest::to_path(server.url("/seg"), &out)
            .with_connections(1)
            .with_retry(rdm::RetryPolicy::none())
            .with_read_timeout(read_timeout),
        &control,
        |_| reported.push(Instant::now()),
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&out).unwrap(), server.seg_payload());
    let longest = reported
        .windows(2)
        .map(|w| w[1] - w[0])
        .max()
        .unwrap_or_default();
    assert!(
        longest > read_timeout,
        "no throttle sleep outlasted the read timeout ({longest:?})"
    );
}

/// AC (R2): the limit holds on the single-connection path.
#[tokio::test]
async fn rate_limit_holds_on_a_single_connection() {
    assert_limited(1).await;
}

/// Download `/seg` under the 512 KiB/s limit and return every speed reading.
async fn readings(connections: u8) -> Vec<f64> {
    let server = common::spawn_with(FixtureConfig {
        seg_len: MIB as usize,
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("read.bin");
    let control = Control::new().with_limiters([Arc::new(RateLimiter::new(Some(LIMIT)))]);
    let mut seen = Vec::new();
    download(
        &DownloadRequest::to_path(server.url("/seg"), &out)
            .with_connections(connections)
            .with_min_segment_size(256 * KIB)
            .with_ramp(Ramping::Off),
        &control,
        |p| seen.push(p.bytes_per_sec),
    )
    .await
    .unwrap();
    seen
}

/// A limited download's speed reads close to the limit from its first
/// reading on: no zero once bytes flow, and no overshoot from a burst or from
/// connections' chunks written ahead of their throttle wait. One-sided: a slow
/// runner only lowers the reading.
async fn assert_reads_the_limit(connections: u8) {
    let seen = readings(connections).await;
    let ceiling = LIMIT as f64 * 1.25;
    // The first report comes before any byte, so it has nothing to measure.
    for (i, &bps) in seen.iter().enumerate().skip(1) {
        assert!(
            bps > 0.0 && bps <= ceiling,
            "reading {i} of {} on {connections} connection(s) was {bps:.0} B/s against a {LIMIT} B/s limit",
            seen.len()
        );
    }
}

#[tokio::test]
async fn speed_reads_the_limit_on_a_single_connection() {
    assert_reads_the_limit(1).await;
}

#[tokio::test]
async fn speed_reads_the_limit_across_parallel_connections() {
    assert_reads_the_limit(4).await;
}

/// A resumed download reads the limit too: the credit its limiter banked
/// while paused doesn't pass as a burst. One-sided, like the above.
#[tokio::test]
async fn speed_reads_the_limit_after_a_resume() {
    let server = common::spawn_with(FixtureConfig {
        seg_len: MIB as usize,
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("resumed.bin");
    let control = Control::new().with_limiters([Arc::new(RateLimiter::new(Some(LIMIT)))]);
    let request = DownloadRequest::to_path(server.url("/seg"), &out)
        .with_connections(4)
        .with_min_segment_size(256 * KIB)
        .with_ramp(Ramping::Off);

    let paused = download(&request, &control, |p| {
        if p.downloaded >= MIB / 4 {
            control.pause();
        }
    })
    .await;
    assert!(matches!(paused, Err(rdm::Error::Paused)), "{paused:?}");
    // Long enough to bank a whole burst, were idle credit kept.
    tokio::time::sleep(Duration::from_millis(500)).await;
    control.resume();

    let mut seen = Vec::new();
    download(&request, &control, |p| seen.push(p.bytes_per_sec))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), server.seg_payload());
    let ceiling = LIMIT as f64 * 1.25;
    for (i, &bps) in seen.iter().enumerate().skip(1) {
        assert!(
            bps > 0.0 && bps <= ceiling,
            "reading {i} of {} after the resume was {bps:.0} B/s against a {LIMIT} B/s limit",
            seen.len()
        );
    }
}
