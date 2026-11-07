//! M5: checksum state in the snapshot — [`Manager::verify_checksum`] on a
//! finished file, and the add-time checksum feeding the same field.

mod common;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use rdm::engine::state;
use rdm::{
    Checksum, ChecksumState, DownloadSnapshot, Error, JsonStore, Manager, Status, Store, StoreData,
};

const T: Duration = Duration::from_secs(10);

/// A [`JsonStore`] that also records every saved document, so a test can see
/// each state a download was persisted in, in order (no watch-channel race).
struct RecordingStore {
    inner: JsonStore,
    saves: Arc<Mutex<Vec<StoreData>>>,
}

impl Store for RecordingStore {
    fn load(&self) -> rdm::Result<StoreData> {
        self.inner.load()
    }

    fn save(&self, data: &StoreData) -> rdm::Result<()> {
        self.saves.lock().unwrap().push(data.clone());
        self.inner.save(data)
    }
}

/// A manager over a recording store in `dir`, plus its save log.
fn recording_manager(dir: &Path) -> (Manager, Arc<Mutex<Vec<StoreData>>>) {
    let saves = Arc::default();
    let store = RecordingStore {
        inner: JsonStore::new(dir.join("state.json")),
        saves: Arc::clone(&saves),
    };
    let mgr = Manager::load(Box::new(store), tokio::runtime::Handle::current()).unwrap();
    (mgr, saves)
}

/// The distinct checksum states download `id` was saved with, in order.
fn saved_states(saves: &Mutex<Vec<StoreData>>, id: u64) -> Vec<ChecksumState> {
    let mut states: Vec<ChecksumState> = Vec::new();
    for data in saves.lock().unwrap().iter() {
        if let Some(rec) = data.downloads.iter().find(|r| r.id == id) {
            if states.last() != Some(&rec.checksum_state) {
                states.push(rec.checksum_state);
            }
        }
    }
    states
}

/// SHA-256 of a buffer as a `sha256:<hex>` checksum.
fn sha256_of(bytes: &[u8]) -> Checksum {
    let hex: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{hex}").parse().unwrap()
}

fn wrong_checksum() -> Checksum {
    format!("sha256:{}", "0".repeat(64)).parse().unwrap()
}

/// Poll snapshots for download `id` until `pred` holds.
async fn wait_for(
    mgr: &Manager,
    id: u64,
    pred: impl Fn(&DownloadSnapshot) -> bool,
) -> DownloadSnapshot {
    let deadline = Instant::now() + T;
    loop {
        if let Some(s) = mgr.snapshot().into_iter().find(|s| s.id == id) {
            if pred(&s) {
                return s;
            }
        }
        assert!(Instant::now() < deadline, "download {id} timed out");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Download `/file` to completion without a checksum; returns its id.
async fn completed_file(mgr: &Manager, server: &common::TestServer, dir: &Path) -> u64 {
    let id = mgr.add(server.url("/file"), dir, None, None);
    wait_for(mgr, id, |s| s.status == Status::Completed).await;
    id
}

/// AC: verifying with the right SHA-256 gives `verified`, passing through a
/// persisted `pending` first.
#[tokio::test]
async fn right_checksum_verifies_through_pending() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let (mgr, saves) = recording_manager(dir.path());
    let id = completed_file(&mgr, &server, dir.path()).await;
    assert_eq!(saved_states(&saves, id), [ChecksumState::None]);

    let right = sha256_of(&common::payload());
    mgr.verify_checksum(id, right.clone()).unwrap();
    let done = wait_for(&mgr, id, |s| s.checksum_state != ChecksumState::Pending).await;

    assert_eq!(done.checksum_state, ChecksumState::Verified);
    assert_eq!(done.checksum, Some(right.to_string()));
    assert_eq!(
        saved_states(&saves, id),
        [
            ChecksumState::None,
            ChecksumState::Pending,
            ChecksumState::Verified
        ]
    );
}

/// AC: verifying with a wrong SHA-256 gives `mismatch`, via `pending`; the
/// file itself is left alone.
#[tokio::test]
async fn wrong_checksum_mismatches_through_pending() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let (mgr, saves) = recording_manager(dir.path());
    let id = completed_file(&mgr, &server, dir.path()).await;

    mgr.verify_checksum(id, wrong_checksum()).unwrap();
    let done = wait_for(&mgr, id, |s| s.checksum_state != ChecksumState::Pending).await;

    assert_eq!(done.checksum_state, ChecksumState::Mismatch);
    assert_eq!(done.status, Status::Completed);
    assert!(done.final_path.unwrap().exists());
    assert_eq!(
        saved_states(&saves, id),
        [
            ChecksumState::None,
            ChecksumState::Pending,
            ChecksumState::Mismatch
        ]
    );
}

/// Verifying something that can't be verified is an error the GUI can show,
/// and changes nothing.
#[tokio::test]
async fn verify_rejects_unknown_unfinished_and_missing() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let (mgr, _) = recording_manager(dir.path());
    let right = sha256_of(&common::payload());

    let err = mgr.verify_checksum(99, right.clone()).unwrap_err();
    assert!(matches!(err, Error::InvalidRequest(_)), "got {err:?}");

    let running = mgr.add(server.url("/gated"), dir.path(), None, None);
    let err = mgr.verify_checksum(running, right.clone()).unwrap_err();
    assert!(matches!(err, Error::InvalidRequest(_)), "got {err:?}");

    let id = completed_file(&mgr, &server, dir.path()).await;
    let path = wait_for(&mgr, id, |_| true).await.final_path.unwrap();
    std::fs::remove_file(&path).unwrap();
    let err = mgr.verify_checksum(id, right).unwrap_err();
    assert!(matches!(err, Error::InvalidRequest(_)), "got {err:?}");
    let snap = mgr.snapshot().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(snap.checksum_state, ChecksumState::None);
    assert_eq!(snap.checksum, None);

    server.gate().release_all();
}

/// The add-time checksum feeds the same field: a match finishes `verified`.
#[tokio::test]
async fn add_time_checksum_verifies() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let (mgr, _) = recording_manager(dir.path());

    let right = sha256_of(&common::payload());
    let id = mgr.add(server.url("/ranged"), dir.path(), None, Some(right.clone()));
    let done = wait_for(&mgr, id, |s| s.status == Status::Completed).await;
    assert_eq!(done.checksum_state, ChecksumState::Verified);
    assert_eq!(done.checksum, Some(right.to_string()));
}

/// A bad add-time checksum fails as `mismatch` and removes the known-bad
/// partial, so a retry downloads afresh instead of re-checking the same bytes.
#[tokio::test]
async fn add_time_mismatch_discards_the_partial_and_retry_redownloads() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let (mgr, saves) = recording_manager(dir.path());
    let stats = server.stats("/seg");

    let id = mgr.add(server.url("/seg"), dir.path(), None, Some(wrong_checksum()));
    let failed = wait_for(&mgr, id, |s| s.status == Status::Failed).await;
    assert_eq!(failed.checksum_state, ChecksumState::Mismatch);
    assert!(failed.error.unwrap().contains("sha256 mismatch"));
    assert_eq!(failed.downloaded, 0);
    assert!(
        !state::part_path(&failed.target).exists(),
        "bad `.part` kept"
    );
    assert!(!state::sidecar_path(&failed.target).exists());
    assert_eq!(stats.unranged(), 1);

    mgr.resume(id);
    wait_for(&mgr, id, |s| s.status == Status::Failed).await;
    assert_eq!(stats.unranged(), 2, "the retry fetched the file afresh");
    assert_eq!(
        saved_states(&saves, id),
        [
            ChecksumState::None,
            ChecksumState::Mismatch,
            ChecksumState::None,
            ChecksumState::Mismatch
        ],
        "a retry clears the state, then re-checks"
    );
}
