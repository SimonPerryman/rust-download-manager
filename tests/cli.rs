//! CLI smoke test: run the real `rdm` binary against the in-process fixture and
//! assert it exits cleanly with the file on disk.

mod common;

use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::process::Command;

use common::FixtureConfig;
use rdm::engine::state::{self, DownloadState};

/// Lowercase hex of a digest (sha2 0.11 digests no longer implement `LowerHex`).
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

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
    // The fixture holds `/slow` after a few chunks, so the first run is
    // guaranteed to be mid-download when it is killed: pacing alone can't
    // promise that, as a stalled client drains the buffered body all at once.
    const HELD_CHUNKS: usize = 4;
    let server = common::spawn_with(FixtureConfig {
        slow_hold_after: Some(HELD_CHUNKS),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("resumed.bin");
    let part = state::part_path(&out);

    let held = (HELD_CHUNKS * common::STREAM_CHUNK) as u64;
    let exe = env!("CARGO_BIN_EXE_rdm");

    // First run: download until the server holds, then hard-kill it —
    // simulating a crash. No reliance on fixed sleeps.
    let mut child = Command::new(exe)
        .arg(server.url("/slow"))
        .arg("-o")
        .arg(&out)
        .kill_on_drop(true)
        .spawn()
        .expect("spawn rdm");

    assert!(
        wait_until(Duration::from_secs(15), || std::fs::metadata(&part)
            .is_ok_and(|m| m.len() == held))
        .await,
        "`.part` never reached the held size to kill at"
    );

    child.kill().await.expect("hard-kill the first run");
    let _ = child.wait().await;
    assert!(
        part.exists(),
        "a crash must leave the `.part` behind to resume"
    );
    assert!(!out.exists(), "the target must not exist after a crash");
    server.gate().release_all();

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
    let got = hex(&h.finalize());
    let mut h2 = Sha256::new();
    h2.update(&expected);
    let want = hex(&h2.finalize());
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

/// `-c` reaches the engine: `-c 1` sends no ranged requests, and `-c 2` ramps
/// to exactly two responses in flight on a download long enough to ramp past it.
#[tokio::test]
async fn cli_connections_flag_reaches_engine() {
    let server = common::spawn_with(FixtureConfig {
        seg_len: 8 * 1024 * 1024,
        seg_chunk_delay: Duration::from_millis(40),
        ..FixtureConfig::default()
    })
    .await;
    let stats = server.stats("/seg");
    let dir = tempfile::tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_rdm");

    // Only the -c 2 run needs a download long enough to ramp; -c 1 just
    // mustn't go ranged, so it gets a quick default-sized one.
    let small = common::spawn().await;
    let small_stats = small.stats("/seg");
    let single = dir.path().join("single.bin");
    let status = Command::new(exe)
        .arg(small.url("/seg"))
        .arg("-o")
        .arg(&single)
        .args(["-c", "1"])
        .status()
        .await
        .expect("spawn rdm");
    assert!(status.success(), "rdm exited with {status}");
    assert!(
        small_stats.ranges().is_empty(),
        "ranged: {:?}",
        small_stats.ranges()
    );
    assert_eq!(small_stats.unranged(), 1);
    assert_eq!(std::fs::read(&single).unwrap(), small.seg_payload());

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
    assert_eq!(stats.max_in_flight(), 2);
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

/// `--limit-rate` rejects values outside its grammar (or zero) before any
/// download starts, and accepts a valid rate.
#[tokio::test]
async fn cli_limit_rate_validates_and_downloads() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("limited.bin");
    let exe = env!("CARGO_BIN_EXE_rdm");

    for bad in ["0", "5X", "-1", ""] {
        let output = Command::new(exe)
            .arg(server.url("/file"))
            .arg("-o")
            .arg(&out)
            .arg(format!("--limit-rate={bad}"))
            .output()
            .await
            .expect("spawn rdm");
        assert!(
            !output.status.success(),
            "--limit-rate {bad:?} was accepted"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--limit-rate"), "unhelpful error: {stderr}");
        assert!(!out.exists());
    }

    let status = Command::new(exe)
        .arg(server.url("/file"))
        .arg("-o")
        .arg(&out)
        .arg("--limit-rate")
        .arg("2M")
        .status()
        .await
        .expect("spawn rdm");
    assert!(status.success(), "rdm exited with {status}");
    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
}

/// `--limit-rate` actually throttles: beyond the one burst a limiter can bank
/// while a connection idles, the payload can't arrive faster than the rate
/// (one-sided; slow CI only lengthens it).
#[tokio::test]
async fn cli_limit_rate_throttles_the_download() {
    const RATE: u64 = 48 * 1024;
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("throttled.bin");
    let size = common::payload().len() as u64;
    let burst = rdm::RateLimiter::new(Some(RATE)).burst();
    let ideal = Duration::from_secs_f64((size - burst) as f64 / RATE as f64);

    let started = Instant::now();
    let status = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .arg(server.url("/file"))
        .arg("-o")
        .arg(&out)
        .arg("--limit-rate")
        .arg("48K")
        .status()
        .await
        .expect("spawn rdm");
    let elapsed = started.elapsed();

    assert!(status.success(), "rdm exited with {status}");
    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
    assert!(
        elapsed >= ideal.mul_f64(0.95),
        "took {elapsed:?}, expected at least ~{ideal:?}"
    );
}

/// Engine warnings (here, each retry of a dropped connection) reach stderr by
/// default, so a CLI user sees why a download is slow.
#[tokio::test]
async fn cli_prints_engine_warnings_by_default() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("flaky.bin");

    let output = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .arg(server.url("/flaky"))
        .arg("-o")
        .arg(&out)
        .env_remove("RUST_LOG")
        .output()
        .await
        .expect("spawn rdm");

    assert!(output.status.success(), "rdm exited with {}", output.status);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(" WARN ") && stderr.contains("retrying"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("download{url="), "untagged: {stderr}");
    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
}

/// A clean run logs nothing by default; `-v` and `RUST_LOG` raise the level.
#[tokio::test]
async fn cli_is_quiet_by_default_and_verbose_on_request() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_rdm");

    let run = |name: &str, verbose: bool, rust_log: Option<&str>| {
        let mut cmd = Command::new(exe);
        cmd.arg(server.url("/file"))
            .arg("-o")
            .arg(dir.path().join(name))
            .env_remove("RUST_LOG");
        if verbose {
            cmd.arg("-v");
        }
        if let Some(level) = rust_log {
            cmd.env("RUST_LOG", level);
        }
        async move { cmd.output().await.expect("spawn rdm") }
    };

    let quiet = run("quiet.bin", false, None).await;
    assert!(quiet.status.success());
    assert_eq!(String::from_utf8_lossy(&quiet.stderr), "");

    for (name, verbose, rust_log) in [("v.bin", true, None), ("env.bin", false, Some("info"))] {
        let loud = run(name, verbose, rust_log).await;
        assert!(loud.status.success());
        let stderr = String::from_utf8_lossy(&loud.stderr);
        assert!(stderr.contains("Started · "), "{name}: {stderr}");
    }
}

/// With `--checksum`, the final line says the checksum was verified.
#[tokio::test]
async fn cli_reports_verified_checksum() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("checked.bin");
    let digest = hex(&Sha256::digest(common::payload()));

    let output = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .arg(server.url("/file"))
        .arg("-o")
        .arg(&out)
        .arg("--checksum")
        .arg(format!("sha256:{digest}"))
        .output()
        .await
        .expect("spawn rdm");

    assert!(output.status.success(), "rdm exited with {}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!(
            "Downloaded {} bytes to {} (sha256 verified)",
            common::payload().len(),
            out.display()
        )),
        "stdout: {stdout}"
    );
}

/// Ctrl-C (SIGINT) pauses: the run says how to resume, exits 130 with the
/// partial kept, and the same command then finishes the file. Unix-only: a
/// console Ctrl-C can't be sent to just one child process on Windows.
#[cfg(unix)]
#[tokio::test]
async fn cli_ctrl_c_pauses_then_rerun_resumes() {
    // ~2.5 s of streaming, so spawning `kill` can't outlast the download.
    let server = common::spawn_with(FixtureConfig {
        slow_chunk_delay: Duration::from_millis(100),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("interrupted.bin");
    let part = state::part_path(&out);
    let total = common::payload().len() as u64;
    let exe = env!("CARGO_BIN_EXE_rdm");

    let child = Command::new(exe)
        .arg(server.url("/slow"))
        .arg("-o")
        .arg(&out)
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn rdm");
    let pid = child.id().expect("child pid").to_string();
    assert!(
        wait_until(Duration::from_secs(15), || std::fs::metadata(&part)
            .is_ok_and(|m| m.len() > 0 && m.len() < total))
        .await,
        "`.part` never reached a partial size to interrupt at"
    );

    let sent = Command::new("kill")
        .args(["-INT", &pid])
        .status()
        .await
        .expect("run kill");
    assert!(sent.success());
    let output = child.wait_with_output().await.expect("wait for rdm");

    assert_eq!(output.status.code(), Some(130));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Paused — run the same command again to resume"),
        "stderr: {stderr}"
    );
    assert!(part.exists() && state::sidecar_path(&out).exists());
    assert!(!out.exists());

    let status = Command::new(exe)
        .arg(server.url("/slow"))
        .arg("-o")
        .arg(&out)
        .status()
        .await
        .expect("spawn rdm (resume)");
    assert!(status.success(), "resume run exited with {status}");
    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
}

/// A failed download reports its error once (the CLI's own message), not again
/// as an engine ERROR log line.
#[tokio::test]
async fn cli_reports_a_failure_once() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .arg(server.url("/missing"))
        .arg("-o")
        .arg(dir.path().join("nope.bin"))
        .env_remove("RUST_LOG")
        .output()
        .await
        .expect("spawn rdm");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("HTTP 404").count(), 1, "stderr: {stderr}");
    assert!(stderr.contains("failed to download"), "stderr: {stderr}");
}

/// The CLI's failure message names the URL without its query secrets.
#[tokio::test]
async fn cli_failure_message_redacts_url_secrets() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .arg(server.url("/missing?token=SECRET123"))
        .arg("-o")
        .arg(dir.path().join("nope.bin"))
        .arg("--verbose")
        .env_remove("RUST_LOG")
        .output()
        .await
        .expect("spawn rdm");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("SECRET123"), "stderr: {stderr}");
    assert!(stderr.contains("/missing?<redacted>"), "stderr: {stderr}");
}

/// `-v` starts with the version and OS line a bug report needs.
#[tokio::test]
async fn verbose_logs_the_version_and_os() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .arg(server.url("/file"))
        .arg("-o")
        .arg(dir.path().join("out.bin"))
        .arg("-v")
        .env_remove("RUST_LOG")
        .output()
        .await
        .expect("spawn rdm");

    assert!(output.status.success(), "rdm exited with {}", output.status);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = format!(
        "rdm {} · {}",
        env!("CARGO_PKG_VERSION"),
        rdm::platform::os_description()
    );
    assert!(stderr.contains(&line), "no {line:?} in stderr: {stderr}");
}

/// `--proxy` sends the download through the proxy.
#[tokio::test]
async fn cli_downloads_through_a_proxy() {
    let server = common::spawn().await;
    let proxy = common::proxy::spawn_socks5_proxy().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("f.bin");

    let status = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .arg(server.url("/file"))
        .arg("-o")
        .arg(&out)
        .arg("--proxy")
        .arg(proxy.url("socks5h", Some("user:pw")))
        .status()
        .await
        .expect("spawn rdm");

    assert!(status.success(), "rdm exited with {status}");
    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
    assert_eq!(proxy.seen().len(), 1, "{:?}", proxy.seen());
}

/// An invalid `--proxy` is a usage error (exit 2, where a failed download
/// exits 1) naming the flag, and the message doesn't repeat the value (it may
/// hold a password).
#[tokio::test]
async fn cli_rejects_an_invalid_proxy_before_downloading() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    for bad in ["not-a-url", "ftp://user:SECRETPW@host:21"] {
        let output = Command::new(env!("CARGO_BIN_EXE_rdm"))
            .arg(server.url("/missing"))
            .arg("-o")
            .arg(dir.path().join("f.bin"))
            .arg("--proxy")
            .arg(bad)
            .output()
            .await
            .expect("spawn rdm");

        assert_eq!(output.status.code(), Some(2), "{bad}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--proxy"), "stderr: {stderr}");
        assert!(!stderr.contains("SECRETPW"), "stderr: {stderr}");
    }
}

/// A failed download through a proxy prints neither the proxy's login nor
/// its password, even with `-v` and `RUST_LOG=debug` (a 404, so it fails at
/// once instead of retrying).
#[tokio::test]
async fn cli_output_never_shows_proxy_credentials() {
    let server = common::spawn().await;
    let proxy = common::proxy::spawn_http_proxy().await;
    let dir = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .arg(server.url("/missing"))
        .arg("-o")
        .arg(dir.path().join("f.bin"))
        .arg("--proxy")
        .arg(proxy.url("http", Some("proxyuser:SECRETPW")))
        .arg("--verbose")
        .env("RUST_LOG", "debug")
        .output()
        .await
        .expect("spawn rdm");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(proxy.seen().len(), 1, "{:?}", proxy.seen());
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!all.contains("SECRETPW"), "output: {all}");
    assert!(!all.contains("proxyuser"), "output: {all}");
    assert!(
        all.contains("Through proxy http://<redacted>:<redacted>@127.0.0.1:"),
        "output: {all}"
    );
}

/// Proxy environment variables are ignored: only `--proxy` routes a download
/// through a proxy.
#[tokio::test]
async fn cli_ignores_proxy_environment_variables() {
    let server = common::spawn().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("f.bin");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rdm"));
    cmd.arg(server.url("/file")).arg("-o").arg(&out);
    for var in ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
        cmd.env(var, &closed);
    }
    for var in ["NO_PROXY", "no_proxy"] {
        cmd.env_remove(var);
    }
    let status = cmd.status().await.expect("spawn rdm");

    assert!(status.success(), "rdm exited with {status}");
    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
}
