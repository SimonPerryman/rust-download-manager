//! Offline integration tests: server- and URL-chosen filenames that try to
//! escape the download directory land inside it under a sanitised name.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use rdm::{download, Control, DownloadRequest, JsonStore, Manager, Status, Store};

/// Download `path` from the fixture into a fresh dir; return the dir and the
/// saved file's name, after checking it sits directly inside the dir.
async fn download_into_dir(path: &str) -> (tempfile::TempDir, String) {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();

    let req = DownloadRequest::new(server.url(path), dir.path());
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(summary.path.parent().unwrap(), dir.path());
    assert_eq!(std::fs::read(&summary.path).unwrap(), common::payload());
    let name = summary
        .path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    (dir, name)
}

/// The only entries in `dir`, by name.
fn entries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[tokio::test]
async fn drive_relative_content_disposition_stays_in_dir() {
    let (dir, name) = download_into_dir("/evil-named").await;
    assert_eq!(name, "C_evil.txt");
    assert_eq!(entries(dir.path()), ["C_evil.txt"]);
}

#[tokio::test]
async fn percent_encoded_drive_relative_url_stays_in_dir() {
    let (dir, name) = download_into_dir("/any/C%3Aevil.txt").await;
    assert_eq!(name, "C_evil.txt");
    assert_eq!(entries(dir.path()), ["C_evil.txt"]);
}

#[tokio::test]
async fn percent_encoded_backslash_traversal_url_stays_in_dir() {
    let (dir, name) = download_into_dir("/any/..%5C..%5Cescape.bin").await;
    assert_eq!(name, "escape.bin");
    assert_eq!(entries(dir.path()), ["escape.bin"]);
}

/// The Manager derives a provisional target from the URL before the request;
/// that path (and the download dir taken from it) must stay inside `dir` too.
#[tokio::test]
async fn manager_keeps_a_hostile_url_name_inside_dir() {
    let server = common::spawn().await;
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store: Box<dyn Store> = Box::new(JsonStore::new(home.path().join("state.json")));
    let mgr = Manager::load(store, tokio::runtime::Handle::current()).unwrap();

    let id = mgr.add(server.url("/any/C%3Aevil.txt"), dir.path(), None, None);
    let snap = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(snap.target.parent().unwrap(), dir.path());

    let deadline = Instant::now() + Duration::from_secs(10);
    let done = loop {
        let snap = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
        if snap.status == Status::Completed {
            break snap;
        }
        assert!(Instant::now() < deadline, "not completed: {snap:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let final_path = done.final_path.unwrap();
    assert_eq!(final_path, dir.path().join("C_evil.txt"));
    assert_eq!(std::fs::read(&final_path).unwrap(), common::payload());
}
