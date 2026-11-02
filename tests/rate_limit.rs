//! M4 acceptance tests, in-process and on the real clock: a per-download speed
//! limit holds whether the download uses one connection or several.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::FixtureConfig;
use rdm::{download, Control, DownloadRequest, RateLimiter};

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const LIMIT: u64 = 512 * KIB;

/// Download `/seg` (1 MiB) with `connections` under a 512 KiB/s limit and
/// check the time taken. With debt, the last chunk's wait ends only once the
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
            .with_min_segment_size(256 * KIB),
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

/// AC (R2): the limit holds on the single-connection path.
#[tokio::test]
async fn rate_limit_holds_on_a_single_connection() {
    assert_limited(1).await;
}
