//! Batches: [`Manager::add_batch`] gives two or more links a shared id and name
//! (and, on request, a folder), and [`Manager::retry_batch`] retries one
//! batch's failures only.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rdm::{AddOptions, BatchSpec, DownloadSnapshot, JsonStore, Manager, Status, Store};

const T: Duration = Duration::from_secs(10);

fn manager_in(dir: &Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

async fn wait_until(cond: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + T;
    while !cond() {
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    true
}

fn snap(mgr: &Manager, id: u64) -> DownloadSnapshot {
    mgr.snapshot().into_iter().find(|s| s.id == id).unwrap()
}

async fn finished(mgr: &Manager, ids: &[u64]) {
    assert!(
        wait_until(|| ids.iter().all(|&id| snap(mgr, id).status.is_terminal())).await,
        "the downloads did not finish"
    );
}

fn named(label: &str, folder: bool) -> Option<BatchSpec> {
    Some(BatchSpec {
        label: label.into(),
        folder,
    })
}

fn urls(server: &common::TestServer, paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| server.url(p)).collect()
}

/// A named add of two or more links gives each the batch id (the first
/// download's) and the label, and a restarted manager still has them.
#[tokio::test]
async fn a_named_batch_saves_its_id_and_label() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let added = mgr
        .add_batch(
            urls(&server, &["/any/a.bin", "/any/b.bin", "/any/c.bin"]),
            dir.path(),
            named("  Course files ", false),
            AddOptions::default(),
        )
        .unwrap();
    assert_eq!(added.added.len(), 3);
    assert_eq!(added.batch, Some(added.added[0]));
    assert!(added.failed.is_empty() && added.folder.is_none());
    finished(&mgr, &added.added).await;

    for &id in &added.added {
        let s = snap(&mgr, id);
        assert_eq!(s.batch, added.batch);
        assert_eq!(s.batch_label.as_deref(), Some("Course files"));
        assert_eq!(s.status, Status::Completed);
        assert_eq!(
            s.final_path.unwrap().parent(),
            Some(dir.path()),
            "no folder asked for"
        );
    }

    let reloaded = manager_in(dir.path());
    assert!(reloaded
        .snapshot()
        .iter()
        .all(|s| s.batch == added.batch && s.batch_label.as_deref() == Some("Course files")));
}

/// Without a name, or with a single link, the links are ordinary downloads,
/// and a name never makes a folder for them.
#[tokio::test]
async fn no_name_or_one_link_is_not_a_batch() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let unnamed = mgr
        .add_batch(
            urls(&server, &["/any/a.bin", "/any/b.bin"]),
            dir.path(),
            None,
            AddOptions::default(),
        )
        .unwrap();
    let single = mgr
        .add_batch(
            urls(&server, &["/any/c.bin"]),
            dir.path(),
            named("Alone", true),
            AddOptions::default(),
        )
        .unwrap();
    assert!(unnamed.batch.is_none() && single.batch.is_none() && single.folder.is_none());
    let all: Vec<u64> = [unnamed.added, single.added].concat();
    finished(&mgr, &all).await;
    for id in all {
        let s = snap(&mgr, id);
        assert!(s.batch.is_none() && s.batch_label.is_none());
        assert_eq!(s.final_path.unwrap().parent(), Some(dir.path()));
    }
    assert!(!dir.path().join("Alone").exists());
}

/// A bad name is refused before anything is added.
#[tokio::test]
async fn a_bad_name_adds_nothing() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    for label in ["   ".to_string(), "x".repeat(65)] {
        let err = mgr
            .add_batch(
                urls(&server, &["/any/a.bin", "/any/b.bin"]),
                dir.path(),
                named(&label, true),
                AddOptions::default(),
            )
            .unwrap_err();
        assert!(matches!(err, rdm::Error::InvalidRequest(_)), "{err:?}");
    }
    assert!(mgr.snapshot().is_empty());
}

/// With a folder, the items land in `<destination>/<name>`; a second batch of
/// the same name gets `name (1)` rather than sharing the folder, and two links
/// that name the same file in one batch keep both (`name (1).ext`).
#[tokio::test]
async fn a_folder_per_batch_with_clashes() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let first = mgr
        .add_batch(
            urls(&server, &["/any/a.bin", "/any/a.bin?again", "/any/b.bin"]),
            dir.path(),
            named("Course files", true),
            AddOptions::default(),
        )
        .unwrap();
    let second = mgr
        .add_batch(
            urls(&server, &["/any/a.bin", "/any/b.bin"]),
            dir.path(),
            named("Course files", true),
            AddOptions::default(),
        )
        .unwrap();
    assert_eq!(first.folder, Some(dir.path().join("Course files")));
    assert_eq!(second.folder, Some(dir.path().join("Course files (1)")));
    finished(&mgr, &[first.added.clone(), second.added.clone()].concat()).await;

    let files = |ids: &[u64]| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = ids
            .iter()
            .map(|&id| snap(&mgr, id).final_path.expect("completed"))
            .collect();
        v.sort();
        v
    };
    let folder = dir.path().join("Course files");
    assert_eq!(
        files(&first.added),
        [
            folder.join("a (1).bin"),
            folder.join("a.bin"),
            folder.join("b.bin")
        ]
    );
    let folder = dir.path().join("Course files (1)");
    assert_eq!(
        files(&second.added),
        [folder.join("a.bin"), folder.join("b.bin")]
    );
}

/// A batch's folder is where its files went: the batch folder when there is
/// one, else the destination; an unknown batch has none.
#[tokio::test]
async fn a_batch_knows_its_folder() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let paths = ["/any/a.bin", "/any/b.bin"];
    let with = mgr
        .add_batch(
            urls(&server, &paths),
            dir.path(),
            named("Set", true),
            AddOptions::default(),
        )
        .unwrap();
    let without = mgr
        .add_batch(
            urls(&server, &paths),
            dir.path(),
            named("Flat", false),
            AddOptions::default(),
        )
        .unwrap();
    assert_eq!(
        mgr.batch_folder(with.batch.unwrap()),
        Some(dir.path().join("Set"))
    );
    assert_eq!(
        mgr.batch_folder(without.batch.unwrap()),
        Some(dir.path().to_path_buf())
    );
    assert_eq!(mgr.batch_folder(9999), None);
    finished(&mgr, &[with.added, without.added].concat()).await;
}

/// A name with path characters still makes one folder inside the destination.
#[tokio::test]
async fn a_folder_name_stays_inside_the_destination() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let added = mgr
        .add_batch(
            urls(&server, &["/any/a.bin", "/any/b.bin"]),
            dir.path(),
            named("../Evil: name", true),
            AddOptions::default(),
        )
        .unwrap();
    assert_eq!(added.folder.as_ref().unwrap().parent(), Some(dir.path()));
    finished(&mgr, &added.added).await;
    assert!(snap(&mgr, added.added[0])
        .final_path
        .unwrap()
        .starts_with(dir.path()));
}

/// Retry failed acts on one batch only: another batch's failures, and
/// ordinary failed downloads, stay failed. Asserted on Manager status: a
/// blocker holds the only slot, so the retried items wait as Queued.
#[tokio::test]
async fn retry_batch_touches_only_that_batch() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let bad = ["/missing/a.bin", "/missing/b.bin"];
    let a = mgr
        .add_batch(
            urls(&server, &bad),
            dir.path(),
            named("A", false),
            AddOptions::default(),
        )
        .unwrap();
    let b = mgr
        .add_batch(
            urls(&server, &bad),
            dir.path(),
            named("B", false),
            AddOptions::default(),
        )
        .unwrap();
    let loose = mgr.add(server.url("/missing/c.bin"), dir.path(), None, None);
    let all: Vec<u64> = [a.added.clone(), b.added.clone(), vec![loose]].concat();
    finished(&mgr, &all).await;
    assert!(all
        .iter()
        .all(|&id| snap(&mgr, id).status == Status::Failed));

    mgr.set_max_concurrent(1);
    let gated = server.stats("/gated");
    let blocker = mgr.add(server.url("/gated"), dir.path(), None, None);
    assert!(wait_until(|| gated.requests() == 1).await);
    assert_eq!(snap(&mgr, blocker).status, Status::Downloading);

    assert_eq!(mgr.retry_batch(a.batch.unwrap()), 2);
    for &id in &a.added {
        assert_eq!(snap(&mgr, id).status, Status::Queued);
    }
    for &id in b.added.iter().chain([&loose]) {
        assert_eq!(snap(&mgr, id).status, Status::Failed);
    }
    assert_eq!(mgr.retry_batch(a.batch.unwrap()), 0, "nothing failed now");
    assert_eq!(mgr.retry_batch(9999), 0, "no such batch");
    server.gate().release_all();
}
