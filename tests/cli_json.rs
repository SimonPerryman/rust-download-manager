//! `rdm --json`: the real binary against the fixture, asserting on the parsed
//! lines of stdout (the event stream documented in `docs/cli.md`).

mod common;

use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::process::Command;

use common::FixtureConfig;
use rdm::engine::state;

/// Run `rdm` in `dir`; returns (status, stdout, stderr).
async fn run(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> (ExitStatus, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .args(args)
        .envs(envs.iter().copied())
        .current_dir(dir)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .expect("spawn rdm");
    (
        out.status,
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Every stdout line parsed; each must be a JSON object with `schema` 1 and
/// an `event` name.
fn events(stdout: &str) -> Vec<Value> {
    assert!(!stdout.is_empty(), "no output");
    stdout
        .lines()
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap_or_else(|e| panic!("{l:?}: {e}"));
            assert_eq!(v["schema"], 1, "{l}");
            assert!(v["event"].is_string(), "{l}");
            v
        })
        .collect()
}

fn of<'a>(events: &'a [Value], name: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["event"] == name).collect()
}

/// `downloaded` of the progress events never goes down.
fn assert_progress_monotonic(events: &[Value], total: u64) {
    let progress = of(events, "progress");
    assert!(!progress.is_empty(), "no progress events");
    let mut last = 0;
    for p in progress {
        let d = p["downloaded"].as_u64().unwrap();
        assert!(d >= last, "progress went back: {last} -> {d}");
        assert!(d <= total);
        assert!(p["connections"].as_u64().unwrap() >= 1);
        last = d;
    }
}

fn slow_server() -> FixtureConfig {
    FixtureConfig {
        // ~1 s of streaming: several progress events through the throttle.
        slow_chunk_delay: Duration::from_millis(40),
        ..FixtureConfig::default()
    }
}

#[tokio::test]
async fn json_single_download_streams_progress_then_done_last() {
    let server = common::spawn_with(slow_server()).await;
    let dir = tempfile::tempdir().unwrap();
    let url = server.url("/slow");

    let (status, stdout, stderr) = run(dir.path(), &["--json", "-o", "a.bin", &url], &[]).await;

    assert_eq!(status.code(), Some(0), "{stderr}");
    let ev = events(&stdout);
    let total = common::payload().len() as u64;
    assert_progress_monotonic(&ev, total);
    assert!(of(&ev, "progress").len() >= 2, "{stdout}");
    let done = ev.last().unwrap();
    assert_eq!(done["event"], "done", "{stdout}");
    assert_eq!(of(&ev, "done").len(), 1);
    assert_eq!(done["bytes"], total);
    assert_eq!(done["url"], url.as_str());
    assert_eq!(done["status"], 200);
    assert!(done["checksum_verified"].is_null());
    assert!(done["path"].as_str().unwrap().ends_with("a.bin"));
    assert_eq!(
        std::fs::read(dir.path().join("a.bin")).unwrap(),
        common::payload()
    );
    // Human text never reaches stdout.
    assert!(!stdout.contains("Downloaded"));
}

#[tokio::test]
async fn json_done_names_the_verified_checksum() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let digest: String = Sha256::digest(common::payload())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let (status, stdout, _) = run(
        dir.path(),
        &[
            "--json",
            "--checksum",
            &format!("sha256:{digest}"),
            &server.url("/file"),
        ],
        &[],
    )
    .await;

    assert_eq!(status.code(), Some(0));
    let ev = events(&stdout);
    assert_eq!(ev.last().unwrap()["event"], "done");
    assert_eq!(ev.last().unwrap()["checksum_verified"], "sha256");
}

#[tokio::test]
async fn json_checksum_mismatch_is_an_error_event_with_exit_1() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let wrong = format!("sha256:{}", "0".repeat(64));

    let (status, stdout, stderr) = run(
        dir.path(),
        &["--json", "--checksum", &wrong, &server.url("/file")],
        &[],
    )
    .await;

    assert_eq!(status.code(), Some(1));
    let ev = events(&stdout);
    assert_eq!(ev.last().unwrap()["event"], "error", "{stdout}");
    assert!(of(&ev, "done").is_empty());
    assert!(stderr.contains("failed to download"), "{stderr}");
}

#[tokio::test]
async fn json_failure_is_an_error_event_and_exit_1() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let url = server.url("/missing");

    let (status, stdout, stderr) = run(dir.path(), &["--json", &url], &[]).await;

    assert_eq!(status.code(), Some(1));
    let ev = events(&stdout);
    assert_eq!(ev.len(), 1, "{stdout}");
    assert_eq!(ev[0]["event"], "error");
    assert_eq!(ev[0]["url"], url.as_str());
    assert!(ev[0]["message"].as_str().unwrap().contains("404"));
    // The same failure still reads on stderr for a person.
    assert!(stderr.contains("404"), "{stderr}");
}

#[tokio::test]
async fn json_multi_connection_download_reports_progress_and_done() {
    const LEN: usize = 4 * 1024 * 1024;
    let server = common::spawn_with(FixtureConfig {
        seg_len: LEN,
        seg_chunk_delay: Duration::from_millis(2),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();

    let (status, stdout, stderr) =
        run(dir.path(), &["--json", "-c", "4", &server.url("/seg")], &[]).await;

    assert_eq!(status.code(), Some(0), "{stderr}");
    let ev = events(&stdout);
    assert_progress_monotonic(&ev, LEN as u64);
    let done = ev.last().unwrap();
    assert_eq!(done["event"], "done");
    assert_eq!(done["bytes"], LEN as u64);
}

#[tokio::test]
async fn json_batch_has_events_per_link_skips_and_a_summary_last() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("urls.txt"),
        format!(
            "{}\n{}\n{}\nhttps://example.com/x[5-1].bin\n",
            server.url("/file"),
            server.url("/missing"),
            server.url("/file?again=1"),
        ),
    )
    .unwrap();

    let (status, stdout, _) = run(dir.path(), &["--json", "-i", "urls.txt"], &[]).await;

    assert_eq!(status.code(), Some(1), "{stdout}");
    let ev = events(&stdout);
    assert_eq!(of(&ev, "done").len(), 2);
    assert_eq!(of(&ev, "error").len(), 1);
    assert!(of(&ev, "error")[0]["url"]
        .as_str()
        .unwrap()
        .ends_with("/missing"));
    assert_eq!(of(&ev, "skipped").len(), 1);
    let summary = ev.last().unwrap();
    assert_eq!(summary["event"], "summary", "{stdout}");
    assert_eq!(summary["done"], 2);
    assert_eq!(summary["failed"], 1);
    assert_eq!(summary["not_finished"], 0);
    assert_eq!(of(&ev, "summary").len(), 1);
}

#[tokio::test]
async fn json_skipped_event_for_an_already_downloaded_file() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.bin"), common::payload()).unwrap();
    std::fs::write(
        dir.path().join("urls.txt"),
        format!(
            "{}\n{}\n",
            server.url("/probe/any/a.bin"),
            server.url("/probe/any/b.bin")
        ),
    )
    .unwrap();

    let (status, stdout, _) = run(
        dir.path(),
        &["--json", "--skip-existing", "-i", "urls.txt"],
        &[],
    )
    .await;

    assert_eq!(status.code(), Some(0), "{stdout}");
    let ev = events(&stdout);
    let skipped = of(&ev, "skipped");
    assert_eq!(skipped.len(), 1, "{stdout}");
    assert_eq!(skipped[0]["reason"], "already_downloaded");
    assert_eq!(skipped[0]["match"], "loose");
    assert!(skipped[0]["url"]
        .as_str()
        .unwrap()
        .ends_with("/probe/any/a.bin"));
    assert!(skipped[0]["path"].as_str().unwrap().ends_with("a.bin"));
    assert_eq!(of(&ev, "done").len(), 1);
    let summary = ev.last().unwrap();
    assert_eq!(summary["event"], "summary");
    assert_eq!(summary["already_downloaded"], 1);
    assert_eq!(summary["skipped"], 0);
    assert!(!stdout.contains("Already downloaded"));
}

#[tokio::test]
async fn quiet_prints_nothing_on_success_and_json_quiet_drops_progress() {
    let server = common::spawn_with(slow_server()).await;
    let dir = tempfile::tempdir().unwrap();
    let url = server.url("/slow");

    let (status, stdout, stderr) = run(dir.path(), &["-q", "-o", "q.bin", &url], &[]).await;
    assert_eq!(status.code(), Some(0));
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.is_empty(), "{stderr}");

    let (status, stdout, _) = run(dir.path(), &["--json", "-q", "-o", "j.bin", &url], &[]).await;
    assert_eq!(status.code(), Some(0));
    let ev = events(&stdout);
    assert_eq!(ev.len(), 1, "{stdout}");
    assert_eq!(ev[0]["event"], "done");

    let (status, stdout, stderr) = run(dir.path(), &["-q", &server.url("/missing")], &[]).await;
    assert_eq!(status.code(), Some(1));
    assert!(stdout.is_empty());
    assert!(stderr.contains("404"), "{stderr}");
}

#[tokio::test]
async fn json_keeps_stdout_clean_while_waiting_for_at() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let (status, stdout, stderr) = run(
        dir.path(),
        &["--json", "--at", "01:30", &server.url("/file")],
        &[("RDM_TEST_NOW", "2026-03-09T01:29")],
    )
    .await;

    assert_eq!(status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("Waiting until"), "{stderr}");
    let ev = events(&stdout);
    assert_eq!(ev.last().unwrap()["event"], "done");
}

#[tokio::test]
async fn json_resume_continues_from_the_partial() {
    const HELD_CHUNKS: usize = 4;
    let server = common::spawn_with(FixtureConfig {
        slow_hold_after: Some(HELD_CHUNKS),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("r.bin");
    let part = state::part_path(&out);
    let url = server.url("/slow");
    let held = (HELD_CHUNKS * common::STREAM_CHUNK) as u64;

    // First run: killed once the server holds it mid-body, leaving a partial.
    let mut child = Command::new(env!("CARGO_BIN_EXE_rdm"))
        .args(["--json", "-o"])
        .arg(&out)
        .arg(&url)
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while !std::fs::metadata(&part).is_ok_and(|m| m.len() == held) {
        assert!(
            std::time::Instant::now() < deadline,
            "never reached the hold"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    child.kill().await.unwrap();
    let _ = child.wait().await;
    server.gate().release_all();

    let (status, stdout, stderr) = run(
        dir.path(),
        &["--json", "-o", out.to_str().unwrap(), &url],
        &[],
    )
    .await;

    assert_eq!(status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("Resuming"), "{stderr}");
    let ev = events(&stdout);
    // The first progress already counts the bytes kept from before.
    assert!(of(&ev, "progress")[0]["downloaded"].as_u64().unwrap() >= held);
    assert_eq!(ev.last().unwrap()["event"], "done");
    assert_eq!(std::fs::read(&out).unwrap(), common::payload());
}
