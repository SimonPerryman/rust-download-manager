//! What a download says as it runs: the one-line events behind the Log tab
//! and `rdm -v`. INFO and up is the concise story (start, lane changes,
//! reconnects, pause/resume, rate limits, the checksum, the summary); DEBUG is
//! the per-request detail.

mod common;

use std::time::Duration;

use sha2::{Digest, Sha256};
use tracing::Level;

use common::{capture, FixtureConfig, Lines};
use rdm::engine::state::{self, DownloadState};
use rdm::engine::Ramping;
use rdm::{download, Checksum, Control, DownloadRequest, Error, RetryPolicy};

const KIB: u64 = 1024;

fn fast_retry(max_attempts: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        ..RetryPolicy::none()
    }
}

fn sha256(bytes: &[u8]) -> Checksum {
    let hex: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{hex}").parse().unwrap()
}

fn has(lines: &[String], prefix: &str) -> bool {
    lines.iter().any(|l| l.starts_with(prefix))
}

/// A clean four-lane download reads start to finish in a few lines: how it
/// started, where it was saved, and the summary. No per-request noise.
#[tokio::test]
async fn a_multi_lane_download_tells_its_story() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("seg.bin");
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/seg"), &out)
        .with_min_segment_size(256 * KIB)
        .with_ramp(Ramping::Off);
    download(&req, &Control::new(), |_| {}).await.unwrap();

    let concise = lines.concise();
    assert_eq!(concise.len(), 3, "{concise:#?}");
    assert_eq!(
        concise[0],
        "Started · 4.00 MB · resumable · 4 pieces, 4 lanes"
    );
    assert_eq!(concise[1], format!("Saved to {}", out.display()));
    let done = &concise[2];
    assert!(done.starts_with("Done · 4.00 MB in "), "{done}");
    assert!(
        done.contains(" · avg ") && done.contains(" · peak "),
        "{done}"
    );
    assert!(done.ends_with(" · no retries"), "{done}");
    assert!(!done.contains("re-downloaded"), "{done}");

    let detail = lines.detail();
    assert!(
        has(&detail, "Requested the file · HTTP 200 · 4.00 MB"),
        "{detail:#?}"
    );
    for lane in 1..=4 {
        assert!(has(&detail, &format!("Lane {lane} opened")), "{detail:#?}");
    }
    // Lane 1 rides the first response; the others each take a piece.
    for lane in 2..=4 {
        assert!(
            has(&detail, &format!("Lane {lane} started piece ")),
            "{detail:#?}"
        );
    }
    assert!(
        detail
            .iter()
            .any(|l| l.contains(" requested bytes ") && l.ends_with(" · HTTP 206")),
        "{detail:#?}"
    );
}

/// Ramping says each time it adds a lane, and how far it may go.
#[tokio::test]
async fn ramping_says_when_it_adds_a_lane() {
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(10),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/seg"), dir.path().join("ramp.bin"))
        .with_min_segment_size(256 * KIB)
        .with_ramp(Ramping::Window(Duration::from_millis(100)));
    download(&req, &Control::new(), |_| {}).await.unwrap();

    let concise = lines.concise();
    assert_eq!(
        concise[0],
        "Started · 4.00 MB · resumable · 4 pieces, up to 4 lanes"
    );
    assert!(has(&concise, "Added Lane 2 — now 2 lanes"), "{concise:#?}");
    assert!(
        concise.last().unwrap().starts_with("Done · 4.00 MB"),
        "{concise:#?}"
    );
}

/// Against a host that gets no faster past four connections, ramping says
/// each lane it adds, then closes the last one it added: the highest-numbered,
/// so Lane 1 stays.
// Multi-threaded, as in `ramping.rs`, so the fixture's pacing stays even.
// How far the ramp climbs before backing off varies with the machine (a slow
// runner reached six), so this checks the shape of the story, not its length.
#[tokio::test(flavor = "multi_thread")]
async fn ramping_says_when_it_backs_off() {
    let server = common::spawn_with(FixtureConfig {
        seg_len: 8 * 1024 * 1024,
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/seg-ramp"), dir.path().join("ramp.bin"))
        .with_connections(8)
        .with_min_segment_size(64 * KIB)
        .with_ramp(Ramping::Window(Duration::from_millis(500)))
        // No fallback: on a slow runner the bottom lane must still be the one.
        .with_shed_grace(Duration::from_secs(3600));
    download(&req, &Control::new(), |_| {}).await.unwrap();

    let concise = lines.concise();
    assert!(
        concise[0].starts_with("Started · ") && concise[0].ends_with(" · 8 pieces, up to 8 lanes"),
        "{concise:#?}"
    );
    let ramped: Vec<_> = concise
        .iter()
        .filter(|l| l.starts_with("Added Lane "))
        .collect();
    assert!(ramped.len() >= 2, "{concise:#?}");
    for (i, line) in ramped.iter().enumerate() {
        let n = i + 2;
        assert_eq!(
            **line,
            format!("Added Lane {n} — now {n} lanes"),
            "{concise:#?}"
        );
    }
    let peak = ramped.len() + 1;
    let backed_off: Vec<_> = concise
        .iter()
        .filter(|l| l.starts_with("Closing Lane "))
        .collect();
    assert_eq!(
        backed_off,
        [&format!(
            "Closing Lane {peak} — more lanes didn't speed this up"
        )],
        "{concise:#?}"
    );
    let detail = lines.detail();
    let closed: Vec<_> = detail
        .iter()
        .filter(|l| l.starts_with("Lane ") && l.contains(" closed"))
        .collect();
    assert_eq!(closed.len(), 1, "{detail:#?}");
    assert!(
        closed[0].starts_with(&format!("Lane {peak} closed")),
        "the bottom lane closed, so Lane 1 stays: {detail:#?}"
    );
    let at = |prefix: &str| {
        concise
            .iter()
            .rposition(|l| l.starts_with(prefix))
            .unwrap_or_else(|| panic!("no {prefix:?} in {concise:#?}"))
    };
    assert!(
        at("Added Lane ") < at("Closing Lane ")
            && at("Closing Lane ") < at("Saved to ")
            && at("Saved to ") < at("Done · "),
        "{concise:#?}"
    );
}

/// When the ramp sheds a lane, the highest-numbered one closes, so Lane 1
/// stays. Here that lane (and any opened after it) is held by the fixture
/// with no body; once the grace passes, a lane that can stop closes in its
/// place, and nothing moves on the held lane until the test releases it.
#[tokio::test(flavor = "multi_thread")]
async fn a_stuck_bottom_lane_does_not_hold_up_a_shed() {
    let server = common::spawn_with(FixtureConfig {
        seg_len: 12 * 1024 * 1024,
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("stuck.bin");
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/seg-ramp-stuck"), &out)
        .with_connections(8)
        .with_min_segment_size(64 * KIB)
        .with_ramp(Ramping::Window(Duration::from_millis(500)))
        .with_shed_grace(Duration::from_millis(200))
        .with_read_timeout(Duration::from_secs(60));
    let busy = |lines: &Lines| {
        lines
            .concise()
            .into_iter()
            .find(|l| l.contains(" is still busy, so Lane "))
    };
    let release = async {
        // The gate holds the stuck lane, so this should end one way: the shed
        // goes ahead without it. A ramp that never got that far would leave
        // the download held, so give up (and let it finish) after a while.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        while busy(&lines).is_none() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        server.gate().release_all();
    };
    let control = Control::new();
    let (result, ()) = tokio::join!(download(&req, &control, |_| {}), release);
    result.unwrap();
    assert!(
        busy(&lines).is_some(),
        "no lane stood in: {:#?}",
        lines.concise()
    );

    let concise = lines.concise();
    let peak = concise
        .iter()
        .filter(|l| l.starts_with("Added Lane "))
        .count()
        + 1;
    assert!(
        concise.contains(&format!(
            "Closing Lane {peak} — more lanes didn't speed this up"
        )),
        "{concise:#?}"
    );
    let line = busy(&lines).unwrap();
    let (stuck, closer) = line
        .strip_prefix("Lane ")
        .and_then(|rest| rest.split_once(" is still busy, so Lane "))
        .and_then(|(stuck, rest)| Some((stuck, rest.strip_suffix(" closes instead")?)))
        .unwrap_or_else(|| panic!("{line}"));
    let (stuck, closer): (usize, usize) = (stuck.parse().unwrap(), closer.parse().unwrap());
    assert_eq!(
        stuck, peak,
        "the bottom lane was the stuck one: {concise:#?}"
    );
    assert!(closer < peak, "{concise:#?}");
    assert_eq!(std::fs::read(&out).unwrap(), server.seg_payload());
}

/// A pause and a resume each get a line, the resume saying where it picked
/// up, and the summary covers both runs.
#[tokio::test]
async fn pause_and_resume_say_where_they_stopped() {
    let server = common::spawn_with(FixtureConfig {
        slow_chunk_delay: Duration::from_millis(5),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/slow"), dir.path().join("slow.bin"));
    let control = Control::new();
    let paused = download(&req, &control, |p| {
        if p.downloaded > 0 {
            control.pause();
        }
    })
    .await;
    assert!(matches!(paused, Err(Error::Paused)), "{paused:?}");
    // The same request carries the same tally, as the Manager's resume does.
    download(&req, &Control::new(), |_| {}).await.unwrap();

    let concise = lines.concise();
    assert!(
        concise[0].starts_with("Started · ") && concise[0].ends_with(" · resumable · 1 lane"),
        "{concise:#?}"
    );
    assert_eq!(concise[1], "Paused");
    assert!(concise[2].starts_with("Resumed at "), "{concise:#?}");
    assert!(has(&concise, "Done · "), "{concise:#?}");
    assert_eq!(concise.len(), 5, "{concise:#?}");
    let detail = lines.detail();
    assert!(has(&detail, "Requested bytes "), "{detail:#?}");
}

/// A lane whose connection drops says so, retries on the same lane, and says
/// when it's back; the summary counts the retry.
#[tokio::test]
async fn a_dropped_lane_reconnects() {
    // Slow enough that the other lanes can't finish and take the lost lane's
    // work before its replacement does.
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(10),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/seg-flaky"), dir.path().join("flaky.bin"))
        .with_min_segment_size(256 * KIB)
        .with_ramp(Ramping::Off)
        .with_retry(fast_retry(5));
    download(&req, &Control::new(), |_| {}).await.unwrap();

    let concise = lines.concise();
    let lost = concise
        .iter()
        .find(|l| l.ends_with(" lost connection — retrying (attempt 2)"))
        .unwrap_or_else(|| panic!("no lost lane in {concise:#?}"));
    let lane = lost.split(' ').nth(1).unwrap();
    assert!(lost.starts_with("Lane "), "{lost}");
    assert!(
        concise.contains(&format!("Lane {lane} reconnected")),
        "{concise:#?}"
    );
    assert!(
        concise.last().unwrap().ends_with(" · 1 retry"),
        "{concise:#?}"
    );
    assert!(
        has(&lines.detail(), &format!("Lane {lane} failed: ")),
        "{:#?}",
        lines.detail()
    );
}

/// A rate-limited request says how long rdm waits before trying again.
#[tokio::test]
async fn a_rate_limit_says_how_long_it_waits() {
    let server = common::spawn_with(FixtureConfig {
        seg_retry_after: Some(1),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/limited-once"), dir.path().join("lim.bin"))
        .with_retry(fast_retry(3));
    download(&req, &Control::new(), |_| {}).await.unwrap();

    let concise = lines.concise();
    assert_eq!(
        concise[0], "Server rate-limited — waiting 1.0s",
        "{concise:#?}"
    );
    assert!(concise[1].starts_with("Started · "), "{concise:#?}");
    assert!(
        concise.last().unwrap().ends_with(" · 1 retry"),
        "{concise:#?}"
    );
    assert!(has(
        &lines.detail(),
        "Attempt 1 failed: server returned HTTP 429"
    ));
}

/// A matching checksum is confirmed before the file is saved.
#[tokio::test]
async fn a_good_checksum_is_confirmed() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/file"), dir.path().join("file.bin"))
        .with_checksum(sha256(&common::payload()));
    download(&req, &Control::new(), |_| {}).await.unwrap();

    let concise = lines.concise();
    assert_eq!(
        concise[0], "Started · 97.6 KB · not resumable · 1 lane",
        "{concise:#?}"
    );
    assert_eq!(
        &concise[1..3],
        [
            "Checksum verified ✓".to_owned(),
            format!("Saved to {}", dir.path().join("file.bin").display())
        ]
    );
}

/// A wrong checksum gets one error line naming both digests, and nothing is
/// saved.
#[tokio::test]
async fn a_checksum_mismatch_names_both_digests() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let lines = Lines::default();
    let _guard = capture(&lines);

    let wrong = format!("sha256:{}", "0".repeat(64));
    let req = DownloadRequest::to_path(server.url("/file"), dir.path().join("file.bin"))
        .with_checksum(wrong.parse().unwrap());
    let err = download(&req, &Control::new(), |_| {}).await.unwrap_err();
    assert!(matches!(err, Error::ChecksumMismatch(_)), "{err:?}");

    let actual = sha256(&common::payload()).hex;
    let errors = lines.at(|level| level == Level::ERROR);
    assert_eq!(
        errors,
        [format!(
            "Checksum mismatch (sha256) — expected {}, got {actual}",
            "0".repeat(64)
        )]
    );
    let concise = lines.concise();
    assert!(!has(&concise, "Saved to"), "{concise:#?}");
    assert!(!has(&concise, "Done"), "{concise:#?}");
}

/// A host that caps connections gets one line saying how many lanes are left.
#[tokio::test]
async fn a_connection_cap_says_how_many_lanes_are_left() {
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(5),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/seg-capped"), dir.path().join("capped.bin"))
        .with_retry(fast_retry(10))
        .with_ramp(Ramping::Off);
    download(&req, &Control::new(), |_| {}).await.unwrap();

    let concise = lines.concise();
    assert!(
        has(&concise, "Server limited connections — down to "),
        "{concise:#?}"
    );
    assert!(
        !has(&concise, "Lane "),
        "a capped lane isn't a lost one: {concise:#?}"
    );
}

/// A server that only pretends to take ranges is caught, and the restart on
/// one lane is said once.
#[tokio::test]
async fn a_range_fallback_says_it_starts_over() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/liar-etag"), dir.path().join("liar.bin"))
        .with_ramp(Ramping::Off);
    download(&req, &Control::new(), |_| {}).await.unwrap();

    let concise = lines.concise();
    assert_eq!(
        concise[0],
        "Started · 4.00 MB · resumable · 4 pieces, 4 lanes"
    );
    assert_eq!(
        concise[1],
        "Server sent the whole file instead of a part — starting over on 1 lane"
    );
    assert!(
        concise.last().unwrap().starts_with("Done · 4.00 MB"),
        "{concise:#?}"
    );
    assert_eq!(concise.len(), 4, "{concise:#?}");
}

/// A resume the server answers with the whole (changed) file says so, starts
/// over, and the summary counts the bytes thrown away.
#[tokio::test]
async fn a_changed_file_starts_over_and_counts_the_waste() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("mutable.bin");
    // A partial of v1, as if an earlier run stopped before the server moved on.
    let v1 = common::payload();
    std::fs::write(state::part_path(&out), &v1[..15_000]).unwrap();
    DownloadState::new(
        server.url("/mutable"),
        &out,
        Some(v1.len() as u64),
        Some("\"v1\"".to_owned()),
        Some(common::LAST_MODIFIED.to_owned()),
    )
    .save(&state::sidecar_path(&out))
    .unwrap();
    server.bump_mutable();
    let lines = Lines::default();
    let _guard = capture(&lines);

    let req = DownloadRequest::to_path(server.url("/mutable"), &out);
    download(&req, &Control::new(), |_| {}).await.unwrap();

    let concise = lines.concise();
    assert_eq!(
        concise[0],
        "Couldn't resume (the server sent the whole file again) — starting over"
    );
    assert!(!has(&concise, "Resumed"), "{concise:#?}");
    assert!(!has(&concise, "Started"), "{concise:#?}");
    let done = concise.last().unwrap();
    assert!(done.ends_with(" · 14.6 KB re-downloaded"), "{done}");
    assert!(has(&lines.detail(), "Requested bytes 15000– · HTTP 200"));
}
