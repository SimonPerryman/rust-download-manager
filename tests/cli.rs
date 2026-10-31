//! CLI smoke test: run the real `rdm` binary against the in-process fixture and
//! assert it exits cleanly with the file on disk.

mod common;

use tokio::process::Command;

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
