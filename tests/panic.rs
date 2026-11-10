//! A panic inside a download's task fails just that download: it ends Failed
//! with an internal error, says why in its log, frees its queue slot, and the
//! Manager keeps working. Driven by the `fault-injection` feature's
//! `/__panic` URL (`cargo test --features fault-injection --test panic`);
//! without the feature that URL is an ordinary 404.

mod common;

#[cfg(feature = "fault-injection")]
use std::path::Path;
#[cfg(feature = "fault-injection")]
use std::time::{Duration, Instant};

#[cfg(feature = "fault-injection")]
use rdm::{JsonStore, Manager, Status, Store};

#[cfg(feature = "fault-injection")]
const T: Duration = Duration::from_secs(20);

#[cfg(feature = "fault-injection")]
fn manager_in(dir: &Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

#[cfg(feature = "fault-injection")]
async fn wait_status(mgr: &Manager, id: u64, want: Status) {
    let deadline = Instant::now() + T;
    while !mgr
        .snapshot()
        .iter()
        .any(|s| s.id == id && s.status == want)
    {
        assert!(
            Instant::now() < deadline,
            "download {id} never reached {want:?}: {:?}",
            mgr.snapshot().iter().find(|s| s.id == id)
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The panicking download fails with an internal error and an ERROR log line
/// carrying the panic message; the one queued behind it (cap 1) then runs to
/// completion, and every Manager command still works afterwards.
#[cfg(feature = "fault-injection")]
#[tokio::test]
async fn a_panicking_download_fails_alone_and_frees_its_slot() {
    use rdm::LogLevel;
    use tracing_subscriber::prelude::*;

    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    // `#[tokio::test]` runs spawned tasks on this thread, so their lines are caught.
    let _guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(mgr.log_layer()));
    mgr.set_max_concurrent(1);

    let panicking = mgr.add(server.url("/__panic"), dir.path(), None, None);
    let queued = mgr.add(server.url("/file"), dir.path(), None, None);
    wait_status(&mgr, panicking, Status::Failed).await;
    wait_status(&mgr, queued, Status::Completed).await;

    let failed = mgr
        .snapshot()
        .into_iter()
        .find(|s| s.id == panicking)
        .unwrap();
    let error = failed.error.unwrap_or_default();
    assert!(error.contains("internal error"), "{error}");
    assert!(error.contains("injected fault"), "{error}");
    let lines = mgr.download_log(panicking);
    assert!(
        lines
            .iter()
            .any(|l| l.level == LogLevel::Error && l.message.contains("injected fault")),
        "{lines:#?}"
    );

    // Nothing is wedged: list, add, pause, resume and remove still answer.
    assert_eq!(mgr.snapshot().len(), 2);
    let another = mgr.add(server.url("/gated"), dir.path(), None, None);
    wait_status(&mgr, another, Status::Downloading).await;
    mgr.pause(another);
    wait_status(&mgr, another, Status::Paused).await;
    mgr.remove(panicking);
    mgr.remove(another);
    let left: Vec<u64> = mgr.snapshot().iter().map(|s| s.id).collect();
    assert_eq!(left, [queued]);
}

/// Without the feature, `/__panic` is just a path the server doesn't have.
#[cfg(not(feature = "fault-injection"))]
#[tokio::test]
async fn without_the_feature_the_panic_url_is_a_plain_404() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let req = rdm::DownloadRequest::new(server.url("/__panic"), dir.path());
    let result = rdm::download(&req, &rdm::Control::new(), |_| {}).await;
    assert!(matches!(result, Err(rdm::Error::Status(404))), "{result:?}");
}

/// A panic while checking a finished file's checksum ends the check instead
/// of leaving it pending for good.
#[cfg(feature = "fault-injection")]
#[tokio::test]
async fn a_panicking_checksum_check_still_ends() {
    use rdm::{AddOptions, ChecksumState};

    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let options = AddOptions {
        out: Some(dir.path().join("__panic.bin")),
        ..AddOptions::default()
    };
    let id = mgr
        .add_with(server.url("/file"), dir.path(), options)
        .unwrap();
    wait_status(&mgr, id, Status::Completed).await;

    let checksum = format!("sha256:{}", "0".repeat(64)).parse().unwrap();
    mgr.verify_checksum(id, checksum).unwrap();
    let state = || {
        mgr.snapshot()
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .checksum_state
    };
    let deadline = Instant::now() + T;
    while state() == ChecksumState::Pending {
        assert!(Instant::now() < deadline, "the check never ended");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(state(), ChecksumState::None);
}
