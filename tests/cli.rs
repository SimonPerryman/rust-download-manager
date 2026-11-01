//! CLI smoke test: run the real `rdm` binary against the in-process fixture and
//! assert it exits cleanly with the file on disk.

mod common;

use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::process::Command;

use common::FixtureConfig;
use rdm::engine::state::{self, DownloadState};

/// `rdm <url> -o <path>` downloads the file, exits 0, and writes the bytes.
#[tokio::test]
async fn cli_downloads_file() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("downloaded.bin");

    // Cargo sets this to the path of the built `rdm` binary for integration tests.
    let exe = env!("CARGO_BIN_EXE_rdm");
    let status = Command::new(exe)
        .arg(server.url("/file"))
        .arg("-o")
        .arg(&out)
        .status()
        .await
        .expect("spawn rdm");

    assert!(status.success(), "rdm exited with {status}");
    let on_disk = std::fs::read(&out).expect("output file exists");
    assert_eq!(on_disk, common::payload());
}

/// A 404 makes the CLI exit non-zero.
#[tokio::test]
async fn cli_fails_on_missing() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("nope.bin");

    let exe = env!("CARGO_BIN_EXE_rdm");
    let status = Command::new(exe)
        .arg(server.url("/missing"))
        .arg("-o")
        .arg(&out)
        .status()
        .await
        .expect("spawn rdm");

    assert!(!status.success(), "rdm should fail on HTTP 404");
    assert!(!out.exists());
}

/// HEADLINE: kill the real binary mid-download, re-run the same command, and the
/// finished file is byte-identical to a full download (verified by checksum).
/// This is the signature resume-after-restart proof.
#[tokio::test]
async fn kill_mid_download_then_restart_is_byte_identical() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("resumed.bin");
    let part = state::part_path(&out);

    let total = common::payload().len() as u64;
    let exe = env!("CARGO_BIN_EXE_rdm");

    // First run: download the slow stream, then hard-kill it once the `.part`
    // is partway there — simulating a crash. No reliance on fixed sleeps.
    let mut child = Command::new(exe)
        .arg(server.url("/slow"))
        .arg("-o")
        .arg(&out)
        .kill_on_drop(true)
        .spawn()
        .expect("spawn rdm");

    let reached_partial = {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut ok = false;
        while Instant::now() < deadline {
            if let Ok(meta) = std::fs::metadata(&part) {
                if meta.len() > 0 && meta.len() < total {
                    ok = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        ok
    };
    assert!(
        reached_partial,
        "`.part` never reached a partial size to kill at"
    );

    child.kill().await.expect("hard-kill the first run");
    let _ = child.wait().await;
    assert!(
        part.exists(),
        "a crash must leave the `.part` behind to resume"
    );
    assert!(!out.exists(), "the target must not exist after a crash");

    // Second run: identical command resumes and completes.
    let status = Command::new(exe)
        .arg(server.url("/slow"))
        .arg("-o")
        .arg(&out)
        .status()
        .await
        .expect("spawn rdm (resume)");
    assert!(status.success(), "resume run exited with {status}");

    let on_disk = std::fs::read(&out).expect("output file exists");
    let expected = common::payload();
    assert_eq!(on_disk, expected, "resumed file must be byte-identical");

    let mut h = Sha256::new();
    h.update(&on_disk);
    let got = format!("{:x}", h.finalize());
    let mut h2 = Sha256::new();
    h2.update(&expected);
    let want = format!("{:x}", h2.finalize());
    assert_eq!(
        got, want,
        "resumed file checksum must match a full download"
    );
}

/// `-c/--connections` only accepts 1..=16.
#[tokio::test]
async fn cli_rejects_out_of_range_connections() {
    let exe = env!("CARGO_BIN_EXE_rdm");
    for bad in ["0", "17"] {
        let output = Command::new(exe)
            .args(["http://127.0.0.1:1/x", "-c", bad])
            .output()
            .await
            .expect("spawn rdm");
        // 2 is clap's usage-error exit code (a download failure exits 1).
        assert_eq!(output.status.code(), Some(2), "-c {bad} should be rejected");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--connections"), "stderr: {stderr}");
    }
}

/// `-c` reaches the engine: `-c 1` sends no ranged requests, `-c 2` never has
/// more than two responses in flight.
#[tokio::test]
async fn cli_connections_flag_reaches_engine() {
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(2),
        ..FixtureConfig::default()
    })
    .await;
    let stats = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_rdm");

    let single = dir.path().join("single.bin");
    let status = Command::new(exe)
        .arg(server.url("/seg"))
        .arg("-o")
        .arg(&single)
        .args(["-c", "1"])
        .status()
        .await
        .expect("spawn rdm");
    assert!(status.success(), "rdm exited with {status}");
    assert!(stats.ranges().is_empty(), "ranged: {:?}", stats.ranges());
    assert_eq!(stats.unranged(), 1);
    assert_eq!(std::fs::read(&single).unwrap(), server.seg_payload());

    assert!(
        wait_until(Duration::from_secs(10), || stats.in_flight() == 0).await,
        "first run's responses never closed"
    );
    stats.reset();
    let double = dir.path().join("double.bin");
    let status = Command::new(exe)
        .arg(server.url("/seg"))
        .arg("-o")
        .arg(&double)
        .args(["-c", "2"])
        .status()
        .await
        .expect("spawn rdm");
    assert!(status.success(), "rdm exited with {status}");
    assert!(!stats.ranges().is_empty(), "never went multi-segment");
    assert!(stats.max_in_flight() <= 2, "max {}", stats.max_in_flight());
    assert_eq!(std::fs::read(&double).unwrap(), server.seg_payload());
}

/// AC (M3): hard-kill the binary mid multi-segment download, re-run it, and the
/// re-run fetches exactly the ranges the v2 sidecar records as missing; the
/// finished file is byte-identical.
#[tokio::test]
async fn kill_multi_segment_download_then_restart_fetches_only_missing_ranges() {
    // 4 connections x 1 MiB at 100 ms per 32 KiB chunk: ~3.2 s, several
    // one-second checkpoints before completion.
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(100),
        ..FixtureConfig::default()
    })
    .await;
    let stats = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("multi.bin");
    let sidecar = state::sidecar_path(&out);
    let payload = server.seg_payload();
    let total = payload.len() as u64;
    let exe = env!("CARGO_BIN_EXE_rdm");

    let mut child = Command::new(exe)
        .arg(server.url("/seg"))
        .arg("-o")
        .arg(&out)
        .args(["-c", "4"])
        .kill_on_drop(true)
        .spawn()
        .expect("spawn rdm");

    let checkpointed = {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut ok = false;
        while Instant::now() < deadline {
            if let Ok(st) = DownloadState::load(&sidecar) {
                let done: u64 = st.segments.iter().flatten().map(|s| s.done).sum();
                if st.version == 2 && done > 0 && done < total {
                    ok = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        ok
    };
    assert!(checkpointed, "never saw a partial v2 checkpoint to kill at");

    child.kill().await.expect("hard-kill the first run");
    let _ = child.wait().await;
    assert!(!out.exists(), "the target must not exist after a crash");

    // The process is dead, so this sidecar is final.
    let st = DownloadState::load(&sidecar).expect("sidecar survives the crash");
    assert_eq!(st.version, 2);
    let mut gaps: Vec<(u64, u64)> = st
        .segments
        .iter()
        .flatten()
        .filter(|s| s.start + s.done < s.end)
        .map(|s| (s.start + s.done, s.end))
        .collect();
    gaps.sort();
    let gap_bytes: u64 = gaps.iter().map(|(s, e)| e - s).sum();

    assert!(
        wait_until(Duration::from_secs(10), || stats.in_flight() == 0).await,
        "killed run's responses never closed"
    );
    stats.reset();

    let status = Command::new(exe)
        .arg(server.url("/seg"))
        .arg("-o")
        .arg(&out)
        .args(["-c", "4"])
        .status()
        .await
        .expect("spawn rdm (resume)");
    assert!(status.success(), "resume run exited with {status}");

    assert_eq!(std::fs::read(&out).unwrap(), payload, "byte-identical");
    assert_eq!(stats.closed_ranges(), gaps, "only the missing ranges");
    assert_eq!(stats.body_bytes(), gap_bytes);
    assert_eq!(stats.unranged(), 0);
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
