//! M5: each download's recent log lines, captured by [`Manager::log_layer`]
//! and read back with [`Manager::download_log`]: kept per download, never
//! carrying URL secrets, and dropped with the download. The bound itself
//! (`LOG_LINES`, oldest dropped first) is unit-tested in the engine module.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use tracing_subscriber::prelude::*;

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

fn messages(lines: &[LogLine]) -> Vec<&str> {
    lines.iter().map(|l| l.message.as_str()).collect()
}

/// Two downloads running side by side each keep only their own lines: the
/// finished one its start and finish, the failed one its start and the error.
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
    assert_eq!(
        messages(&done_log),
        ["download started", "download finished"],
        "{done_log:#?}"
    );
    assert!(done_log.iter().all(|l| l.level == LogLevel::Info));
    let before = done_log[0].at;
    assert!(before > 0 && done_log[1].at >= before, "{done_log:#?}");

    let failed_log = mgr.download_log(failed);
    assert_eq!(failed_log[0].message, "download started", "{failed_log:#?}");
    let error = failed_log.last().unwrap();
    assert_eq!(
        (error.level, error.message.as_str()),
        (LogLevel::Error, "download failed"),
        "{failed_log:#?}"
    );
    assert!(error.fields.contains("HTTP 403"), "{error:?}");
    assert!(!messages(&failed_log).contains(&"download finished"));
}

/// A download whose finished file fails its check logs the failure once, as
/// an error naming the mismatch.
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
    let id = mgr.add_with(server.url("/file"), dir.path(), options);
    wait_status(&mgr, id, Status::Failed).await;

    let lines = mgr.download_log(id);
    let failures: Vec<_> = lines
        .iter()
        .filter(|l| l.message == "download failed")
        .collect();
    assert_eq!(failures.len(), 1, "{lines:#?}");
    assert_eq!(failures[0].level, LogLevel::Error);
    assert!(
        failures[0].fields.contains("integrity check failed"),
        "{lines:#?}"
    );
    assert!(!messages(&lines).contains(&"download finished"));
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
/// `RUST_LOG=error`) beside it doesn't hide the INFO lines from the log.
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
    assert_eq!(
        messages(&mgr.download_log(id)),
        ["download started", "download finished"]
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
    assert!(messages(&lines).contains(&"download failed"), "{lines:#?}");
    let text = format!("{lines:?}");
    for secret in ["SECRET123", "PASSWORD9", "frag", "token"] {
        assert!(!text.contains(secret), "{text}");
    }
}
