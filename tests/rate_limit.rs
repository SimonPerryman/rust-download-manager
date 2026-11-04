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
    // The burst passes 64 KiB; the 12 KiB left at 4 KiB/s sleeps ~3 s.
    let server = common::spawn_with(FixtureConfig {
        seg_len: 76 * KIB as usize,
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
