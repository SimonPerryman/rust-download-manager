//! A panic in the CLI (driven by the `fault-injection` feature's `/__panic`
//! URL) exits non-zero and tells the user it's a bug worth reporting, with
//! the version and where to report it.
//! Run with `cargo test --features fault-injection --test cli_panic`.
#![cfg(feature = "fault-injection")]

mod common;

use tokio::process::Command;

#[tokio::test]
async fn a_panic_asks_for_a_bug_report() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .arg(server.url("/__panic"))
        .arg("-o")
        .arg(dir.path().join("out.bin"))
        .env_remove("RUST_LOG")
        .output()
        .await
        .expect("spawn rdm");

    // Rust's exit code for a panic that ends `main`.
    assert_eq!(output.status.code(), Some(101), "{}", output.status);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("injected fault"), "stderr: {stderr}");
    let hint = concat!(
        "rdm ",
        env!("CARGO_PKG_VERSION"),
        " hit a bug. Please report it at ",
        env!("CARGO_PKG_REPOSITORY"),
        "/issues with the output above (run again with -v for more detail)."
    );
    assert!(stderr.contains(hint), "no {hint:?} in stderr: {stderr}");
}

/// A panic in a background step (here the checksum check, on the blocking
/// pool) fails the download as an internal error, with the same exit code.
#[tokio::test]
async fn a_panic_in_a_background_step_exits_as_a_bug() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .arg(server.url("/file"))
        .arg("-o")
        .arg(dir.path().join("__panic.bin"))
        .arg("--checksum")
        .arg(format!("sha256:{}", "0".repeat(64)))
        .env_remove("RUST_LOG")
        .output()
        .await
        .expect("spawn rdm");

    assert_eq!(output.status.code(), Some(101), "{}", output.status);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("internal error, please report it: injected fault: hashing panicked"),
        "stderr: {stderr}"
    );
}
