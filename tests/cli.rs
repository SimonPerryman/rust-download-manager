//! CLI smoke test: run the real `rdm` binary against the in-process fixture and
//! assert it exits cleanly with the file on disk.

mod common;

use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::process::Command;

use rdm::engine::state;

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
