//! M5: each download's recent log lines, captured by [`Manager::log_layer`]
//! and read back with [`Manager::download_log`]: kept per download, never
//! carrying URL secrets, and dropped with the download. INFO and up is the
//! concise story; DEBUG lines are the Detailed view's per-request detail. The
//! bounds themselves are unit-tested in the engine module, and each event's
//! wording in `log_events.rs`.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use tracing_subscriber::prelude::*;

use common::FixtureConfig;
use rdm::{AddOptions, Checksum, JsonStore, LogLevel, LogLine, Manager, Status, Store};

const T: Duration = Duration::from_secs(20);

fn manager_in(dir: &Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

/// Route this thread's events through `mgr`'s log layer. `#[tokio::test]`
/// runs spawned tasks on the same thread, so the downloads' lines are caught.
fn capture(mgr: &Manager) -> tracing::subscriber::DefaultGuard {
    tracing::subscriber::set_default(tracing_subscriber::registry().with(mgr.log_layer()))
}

async fn wait_status(mgr: &Manager, id: u64, want: Status) {
    let deadline = Instant::now() + T;
    while !mgr
        .snapshot()
        .iter()
        .any(|s| s.id == id && s.status == want)
    {
        assert!(
            Instant::now() < deadline,
            "download {id} never reached {want:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The lines the Log tab shows by default: INFO and up.
fn concise(lines: &[LogLine]) -> Vec<&LogLine> {
    lines
        .iter()
        .filter(|l| !matches!(l.level, LogLevel::Debug | LogLevel::Trace))
        .collect()
}

fn starts(line: &LogLine, prefix: &str) -> bool {
    line.message.starts_with(prefix)
}

/// Two downloads running side by side each keep only their own lines: the
/// finished one its start, where it was saved and its summary, the failed one
/// its start and the error.
#[tokio::test]
async fn each_download_keeps_its_own_lines() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let _guard = capture(&mgr);

    let done = mgr.add(server.url("/named"), dir.path(), None, None);
    let failed = mgr.add(server.url("/seg-403"), dir.path(), None, None);
    wait_status(&mgr, done, Status::Completed).await;
    wait_status(&mgr, failed, Status::Failed).await;

    let done_log = mgr.download_log(done);
    let story = concise(&done_log);
    assert_eq!(story.len(), 3, "{done_log:#?}");
    assert!(starts(story[0], "Started · "), "{done_log:#?}");
    assert!(starts(story[1], "Saved to "), "{done_log:#?}");
    assert!(starts(story[2], "Done · "), "{done_log:#?}");
    assert!(story.iter().all(|l| l.level == LogLevel::Info));
    assert!(story.iter().all(|l| l.fields.is_empty()), "{done_log:#?}");
    let before = story[0].at;
    assert!(before > 0 && story[2].at >= before, "{done_log:#?}");

    let failed_log = mgr.download_log(failed);
    let story = concise(&failed_log);
    assert!(starts(story[0], "Started · "), "{failed_log:#?}");
    let error = story.last().unwrap();
    assert_eq!(error.level, LogLevel::Error, "{failed_log:#?}");
    assert!(starts(error, "Failed"), "{error:?}");
    assert!(error.message.contains("HTTP 403"), "{error:?}");
    assert!(!failed_log.iter().any(|l| starts(l, "Done")));
}

/// A download whose finished file fails its check logs the failure once, as
/// an error naming both digests, then that the file was thrown away.
#[tokio::test]
async fn a_failed_check_logs_one_error() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let _guard = capture(&mgr);

    let wrong: Checksum = format!("sha256:{}", "0".repeat(64)).parse().unwrap();
    let options = AddOptions {
        checksum: Some(wrong),
        ..AddOptions::default()
    };
    let id = mgr
        .add_with(server.url("/file"), dir.path(), options)
        .unwrap();
    wait_status(&mgr, id, Status::Failed).await;

    let lines = mgr.download_log(id);
    let errors: Vec<_> = lines
        .iter()
        .filter(|l| l.level == LogLevel::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{lines:#?}");
    let expected = format!(
        "Checksum mismatch (sha256) — expected {}, got ",
        "0".repeat(64)
    );
    assert!(starts(errors[0], &expected), "{lines:#?}");
    assert_eq!(
        concise(&lines).last().unwrap().message,
        "Discarded the downloaded file"
    );
    assert!(!lines.iter().any(|l| starts(l, "Done")));
}

/// An unknown id has no lines, and neither has a download restored from the
/// store: lines live in memory only, for downloads run since the manager loaded.
#[tokio::test]
async fn unknown_and_restored_downloads_have_no_lines() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let mgr = manager_in(dir.path());
        let _guard = capture(&mgr);
        let id = mgr.add(server.url("/file"), dir.path(), None, None);
        wait_status(&mgr, id, Status::Completed).await;
        assert!(!mgr.download_log(id).is_empty());
        id
    };
    let mgr = manager_in(dir.path());
    let _guard = capture(&mgr);
    assert!(mgr.snapshot().iter().any(|s| s.id == id));
    assert!(mgr.download_log(id).is_empty());
    assert!(mgr.download_log(id + 1000).is_empty());
}

/// The layer keeps its own level: an output filtered to errors (as with
/// `RUST_LOG=error`) beside it hides neither the INFO lines nor the DEBUG
/// detail from the log.
#[tokio::test]
async fn the_layer_ignores_other_layers_filters() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let output = tracing_subscriber::fmt::layer()
        .with_writer(std::io::sink)
        .with_filter(tracing_subscriber::EnvFilter::new("error"));
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(mgr.log_layer())
            .with(output),
    );

    let id = mgr.add(server.url("/file"), dir.path(), None, None);
    wait_status(&mgr, id, Status::Completed).await;
    let lines = mgr.download_log(id);
    let story = concise(&lines);
    assert!(starts(story[0], "Started · "), "{lines:#?}");
    assert!(starts(story.last().unwrap(), "Done · "), "{lines:#?}");
    assert!(
        lines.iter().any(|l| l.level == LogLevel::Debug),
        "{lines:#?}"
    );
}

/// Removing a download, or clearing it, drops its lines.
#[tokio::test]
async fn lines_go_with_the_download() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let _guard = capture(&mgr);

    let removed = mgr.add(server.url("/file"), dir.path().join("a"), None, None);
    let cleared = mgr.add(server.url("/file"), dir.path().join("b"), None, None);
    wait_status(&mgr, removed, Status::Completed).await;
    wait_status(&mgr, cleared, Status::Completed).await;
    assert!(!mgr.download_log(removed).is_empty());
    assert!(!mgr.download_log(cleared).is_empty());

    mgr.remove(removed);
    assert!(mgr.download_log(removed).is_empty());
    assert!(!mgr.download_log(cleared).is_empty());
    assert_eq!(mgr.clear(&[Status::Completed], None), 1);
    assert!(mgr.download_log(cleared).is_empty());
}

/// A failing download's lines carry none of its URL's secrets (userinfo,
/// query, fragment), as the file log doesn't. (That span fields such as the
/// URL never reach a line at all is unit-tested in the engine module.)
#[tokio::test]
async fn lines_never_carry_url_secrets() {
    let server = common::spawn().await;
    let url = server.url("/missing?token=SECRET123#frag").replacen(
        "http://",
        "http://user:PASSWORD9@",
        1,
    );

    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let _guard = capture(&mgr);
    let id = mgr.add(url, dir.path(), None, None);
    wait_status(&mgr, id, Status::Failed).await;

    let lines = mgr.download_log(id);
    assert!(lines.iter().any(|l| starts(l, "Failed")), "{lines:#?}");
    let text = format!("{lines:?}");
    for secret in ["SECRET123", "PASSWORD9", "frag", "token"] {
        assert!(!text.contains(secret), "{text}");
    }
}

/// A paused and resumed download says so, and its summary spans both runs.
/// The Detailed view's per-request lines are DEBUG, so the concise story
/// stays free of byte ranges and status codes.
#[tokio::test]
async fn pause_resume_and_detail_through_the_manager() {
    let server = common::spawn_with(FixtureConfig {
        seg_chunk_delay: Duration::from_millis(5),
        ..FixtureConfig::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let _guard = capture(&mgr);

    let id = mgr.add(server.url("/seg"), dir.path(), None, None);
    let deadline = Instant::now() + T;
    while !mgr
        .snapshot()
        .iter()
        .any(|s| s.id == id && s.downloaded > 0)
    {
        assert!(Instant::now() < deadline, "no progress");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    mgr.pause(id);
    wait_status(&mgr, id, Status::Paused).await;
    mgr.resume(id);
    wait_status(&mgr, id, Status::Completed).await;

    let lines = mgr.download_log(id);
    let story = concise(&lines);
    assert_eq!(
        story[0].message, "Started · 4.00 MB · resumable · up to 4 lanes",
        "{lines:#?}"
    );
    let paused = story.iter().position(|l| l.message == "Paused");
    let resumed = story.iter().position(|l| starts(l, "Resumed at "));
    assert!(
        matches!((paused, resumed), (Some(p), Some(r)) if r == p + 1),
        "{lines:#?}"
    );
    assert!(
        starts(story.last().unwrap(), "Done · 4.00 MB in "),
        "{lines:#?}"
    );
    for line in &story {
        assert!(
            !line.message.contains("HTTP 2") && !line.message.contains("requested bytes"),
            "per-request detail in the concise view: {line:?}"
        );
    }

    assert!(
        lines.iter().any(|l| l.level == LogLevel::Debug
            && l.message.contains(" requested bytes ")
            && l.message.ends_with("HTTP 206")),
        "{lines:#?}"
    );
}
