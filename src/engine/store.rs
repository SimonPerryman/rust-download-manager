//! Persistence for the download list + history: a thin, swappable store.
//!
//! M2 ships a JSON-file store ([`JsonStore`]); the [`Store`] trait keeps the
//! boundary minimal so a future SQLite backend is a contained swap (roadmap
//! backlog). This store persists only what must survive a restart — the list of
//! downloads and their status. The engine's per-download `.part`/`.rdm`
//! sidecars remain the source of truth for resume offsets and validators, so
//! this store never duplicates them.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::Result;

/// Current store schema version. Bump when the on-disk shape changes.
const SCHEMA_VERSION: u32 = 1;

/// Lifecycle status of a managed download.
///
/// Distinct from [`DownloadState`](crate::engine::state::DownloadState), which
/// is the per-download resume sidecar; this is the coarse state a frontend
/// renders. `Retrying` is deliberately absent: the engine retries transient
/// failures *inside* a single `download()` call, so a retry is not observable
/// as a separate managed state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Added but not yet streaming (transient; M2 starts immediately).
    Queued,
    /// Actively downloading.
    Downloading,
    /// Paused by the user; `.part` + sidecar kept, resumable.
    Paused,
    /// Finished and verified; the file is at `final_path`.
    Completed,
    /// Stopped with an error (e.g. HTTP status, integrity); retryable.
    Failed,
    /// Cancelled by the user; partial data discarded.
    Cancelled,
}

impl Status {
    /// Whether a download in this status has a live task running (or about to).
    pub fn is_running(self) -> bool {
        matches!(self, Status::Queued | Status::Downloading)
    }

    /// Whether this status can be (re)started via `resume`.
    pub fn is_resumable(self) -> bool {
        matches!(self, Status::Paused | Status::Failed | Status::Cancelled)
    }
}

/// One download as persisted to the store.
///
/// Survives restarts; live throughput (speed/ETA) is layered on top at runtime
/// in a [`DownloadSnapshot`](crate::engine::manager::DownloadSnapshot) and is
/// intentionally not persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRecord {
    /// Manager-assigned identifier, unique within a store.
    pub id: u64,
    /// The source URL.
    pub url: String,
    /// Planned target path; the engine writes `<target>.part` then renames.
    pub target: PathBuf,
    /// `target` is only a URL-derived placeholder: the real name (e.g. from
    /// `Content-Disposition`) is claimed on the first response. Nothing is on
    /// disk at a provisional target.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub target_provisional: bool,
    /// Final path once completed (may differ from `target` via a collision
    /// rename).
    #[serde(default)]
    pub final_path: Option<PathBuf>,
    /// Requested integrity check as a `"<algo>:<hex>"` string, if any.
    #[serde(default)]
    pub checksum: Option<String>,
    /// Coarse lifecycle status.
    pub status: Status,
    /// Bytes downloaded as of the last persist. The on-disk state (the `.part`
    /// size, or a v2 sidecar's segment progress) is authoritative on resume;
    /// this is a best-effort figure for display.
    #[serde(default)]
    pub downloaded: u64,
    /// Expected total size when the server reported one.
    #[serde(default)]
    pub total: Option<u64>,
    /// Human-readable failure reason when `status == Failed`.
    #[serde(default)]
    pub error: Option<String>,
    /// Unix-epoch seconds when the download was added.
    #[serde(default)]
    pub added_at: Option<u64>,
    /// Unix-epoch seconds when it reached a terminal status.
    #[serde(default)]
    pub finished_at: Option<u64>,
}

/// The full on-disk document: schema version, the id counter, and the records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreData {
    /// Schema version of this document.
    #[serde(default = "default_version")]
    pub version: u32,
    /// The next id the manager will hand out. Persisted (not derived from the
    /// records' max) so ids are never reused after all entries are removed.
    #[serde(default = "default_next_id")]
    pub next_id: u64,
    /// Every download, in display order.
    #[serde(default)]
    pub downloads: Vec<DownloadRecord>,
}

fn default_version() -> u32 {
    SCHEMA_VERSION
}

fn default_next_id() -> u64 {
    1
}

impl Default for StoreData {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            next_id: 1,
            downloads: Vec::new(),
        }
    }
}

/// A persistence backend for the download list + history.
///
/// The whole document is loaded and saved at once — simple and atomic, which is
/// ample for JSON v1 scale. A future SQLite backend can implement the same
/// trait for incremental updates.
pub trait Store: Send + Sync {
    /// Load all persisted state. Returns an empty document when nothing has
    /// been saved yet.
    fn load(&self) -> Result<StoreData>;

    /// Persist the full document, atomically where possible.
    fn save(&self, data: &StoreData) -> Result<()>;
}

/// A [`Store`] backed by a single JSON file.
pub struct JsonStore {
    path: PathBuf,
}

impl JsonStore {
    /// A store that reads and writes `path` (e.g. `~/.rdm/state.json`).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The file this store is backed by.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Store for JsonStore {
    fn load(&self) -> Result<StoreData> {
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                let data = serde_json::from_slice(&bytes)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(data)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StoreData::default()),
            Err(e) => Err(e.into()),
        }
    }

    fn save(&self, data: &StoreData) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_vec_pretty(data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        // Atomic replace: write a temp file, flush+fsync, then rename over the
        // target so a crash never leaves a half-written state file.
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.flush()?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record(id: u64, status: Status) -> DownloadRecord {
        DownloadRecord {
            id,
            url: format!("https://example.com/{id}"),
            target: PathBuf::from(format!("/tmp/file-{id}.bin")),
            target_provisional: status == Status::Paused,
            final_path: None,
            checksum: None,
            status,
            downloaded: 0,
            total: Some(100),
            error: None,
            added_at: Some(1_700_000_000),
            finished_at: None,
        }
    }

    #[test]
    fn roundtrips_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonStore::new(dir.path().join("state.json"));

        let data = StoreData {
            version: SCHEMA_VERSION,
            next_id: 3,
            downloads: vec![
                sample_record(1, Status::Completed),
                sample_record(2, Status::Paused),
            ],
        };
        store.save(&data).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded, data);
        // Temp file does not linger after an atomic save.
        assert!(!dir.path().join("state.json.tmp").exists());
    }

    #[test]
    fn missing_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonStore::new(dir.path().join("does-not-exist.json"));
        let loaded = store.load().unwrap();
        assert_eq!(loaded, StoreData::default());
        assert_eq!(loaded.next_id, 1);
        assert!(loaded.downloads.is_empty());
    }

    #[test]
    fn save_creates_missing_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("state.json");
        let store = JsonStore::new(&nested);
        store.save(&StoreData::default()).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn optional_fields_default_on_load() {
        // A leaner/future writer that omits optional fields must still load.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(
            &path,
            r#"{"downloads":[{"id":7,"url":"u","target":"t","status":"downloading"}]}"#,
        )
        .unwrap();

        let store = JsonStore::new(&path);
        let loaded = store.load().unwrap();
        assert_eq!(loaded.version, SCHEMA_VERSION);
        assert_eq!(loaded.next_id, 1);
        let rec = &loaded.downloads[0];
        assert_eq!(rec.id, 7);
        assert_eq!(rec.status, Status::Downloading);
        assert_eq!(rec.downloaded, 0);
        assert_eq!(rec.total, None);
        assert_eq!(rec.checksum, None);
        assert!(!rec.target_provisional);
    }

    #[test]
    fn status_helpers() {
        assert!(Status::Downloading.is_running());
        assert!(Status::Queued.is_running());
        assert!(!Status::Paused.is_running());
        assert!(Status::Paused.is_resumable());
        assert!(Status::Failed.is_resumable());
        assert!(Status::Cancelled.is_resumable());
        assert!(!Status::Completed.is_resumable());
    }
}
