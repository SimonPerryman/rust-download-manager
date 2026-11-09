//! The engine's log lines: each carries the identity of the download it belongs
//! to, and a user pause/cancel is ordinary control flow, not an error.

mod common;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tracing_subscriber::fmt::MakeWriter;

use rdm::{download, Control, DownloadRequest, Error, JsonStore, Manager, Status, Store};

/// Collects formatted log output for assertions.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn lines(&self) -> Vec<String> {
        String::from_utf8_lossy(&self.0.lock().unwrap())
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Captured;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Route this thread's events (at every level) into a [`Captured`] buffer.
/// `#[tokio::test]` runs spawned tasks on the same thread, so they're caught too.
fn capture() -> (Captured, tracing::subscriber::DefaultGuard) {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(captured.clone())
        .finish();
    (captured, tracing::subscriber::set_default(subscriber))
}

/// Every line a Manager download logs names its id, so concurrent downloads'
/// lines can be told apart, and its file once the server has named it.
#[tokio::test]
async fn manager_download_lines_carry_its_id() {
    let (captured, _guard) = capture();
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.path().join("state.json")));
    let mgr = Manager::load(store, tokio::runtime::Handle::current()).unwrap();

    let id = mgr.add(server.url("/named"), dir.path(), None, None);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !mgr
        .snapshot()
        .iter()
        .any(|s| s.id == id && s.status == Status::Completed)
    {
        assert!(Instant::now() < deadline, "never completed");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let lines = captured.lines();
    let started = lines
        .iter()
        .find(|l| l.contains("Started · "))
        .unwrap_or_else(|| panic!("no start line in {lines:#?}"));
    // Logged after the first response, so the claimed name is already on it.
    assert!(
        started.contains(&format!("task{{id={id} file=report-final.bin}}")),
        "{started}"
    );
    let claimed = lines
        .iter()
        .find(|l| l.contains("Saving as "))
        .unwrap_or_else(|| panic!("no claim line in {lines:#?}"));
    assert!(
        claimed.contains(&format!("task{{id={id} file=report-final.bin}}")),
        "{claimed}"
    );
}

/// Pausing or cancelling logs at INFO ("Paused"/"Cancelled"), never ERROR
/// or WARN, still tagged with the download's URL.
#[tokio::test]
async fn pause_and_cancel_are_not_logged_as_errors() {
    let (captured, _guard) = capture();
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let url = server.url("/slow");

    let paused = Control::new();
    paused.pause();
    let req = DownloadRequest::to_path(&url, dir.path().join("paused.bin"));
    let result = download(&req, &paused, |_| {}).await;
    assert!(matches!(result, Err(Error::Paused)), "{result:?}");

    let cancelled = Control::new();
    cancelled.cancel();
    let req = DownloadRequest::to_path(&url, dir.path().join("cancelled.bin"));
    let result = download(&req, &cancelled, |_| {}).await;
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");

    let lines = captured.lines();
    for msg in [": Paused", ": Cancelled"] {
        let line = lines
            .iter()
            .find(|l| l.contains(msg))
            .unwrap_or_else(|| panic!("no {msg:?} in {lines:#?}"));
        assert!(line.contains(" INFO "), "{line}");
        assert!(line.contains(&format!("download{{url={url}}}")), "{line}");
    }
    assert!(
        !lines
            .iter()
            .any(|l| l.contains(" ERROR ") || l.contains(" WARN ")),
        "{lines:#?}"
    );
}

/// A URL whose port nothing listens on, carrying secrets in its userinfo and
/// query, as signed CDN links and `user:pass@` URLs do.
async fn secret_url_to_closed_port() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://user:PASSWORD9@127.0.0.1:{port}/f.bin?token=SECRET123#frag")
}

/// A failing download's error text and every log line it emits (retries
/// included) redact the URL's secrets, so pasted logs and error messages
/// never leak them.
#[tokio::test]
async fn failed_download_never_logs_or_reports_url_secrets() {
    let (captured, _guard) = capture();
    let url = secret_url_to_closed_port().await;
    let dir = tempfile::tempdir().unwrap();
    let req =
        DownloadRequest::to_path(&url, dir.path().join("f.bin")).with_retry(rdm::RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            ..rdm::RetryPolicy::none()
        });

    let err = download(&req, &Control::new(), |_| {}).await.unwrap_err();
    assert!(matches!(err, Error::Http(_)), "{err:?}");
    let reported = format!("{err} {err:?}");
    let logged = captured.lines().join("\n");
    for secret in ["SECRET123", "PASSWORD9", "frag"] {
        assert!(!reported.contains(secret), "{reported}");
        assert!(!logged.contains(secret), "{logged}");
    }
    assert!(logged.contains("Failed after 2 attempts: "), "{logged}");
    assert!(logged.contains("retrying"), "{logged}");
    assert!(logged.contains("/f.bin?<redacted>"), "{logged}");
}

/// The error a Manager records for a failed download (shown in the GUI and
/// saved to `state.json`) carries no URL secrets either.
#[tokio::test]
async fn manager_error_never_carries_url_secrets() {
    let url = secret_url_to_closed_port().await;
    let dir = tempfile::tempdir().unwrap();
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.path().join("state.json")));
    let mgr = Manager::load(store, tokio::runtime::Handle::current()).unwrap();

    let id = mgr.add(url, dir.path(), None, None);
    let deadline = Instant::now() + Duration::from_secs(60);
    let error = loop {
        let snap = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
        if snap.status == Status::Failed {
            break snap.error.expect("a failed download has an error");
        }
        assert!(Instant::now() < deadline, "never failed: {:?}", snap.status);
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert!(!error.contains("SECRET123"), "{error}");
    assert!(!error.contains("PASSWORD9"), "{error}");
}
