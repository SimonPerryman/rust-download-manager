//! On-disk partial-download state: a JSON sidecar beside the `.part` file.
//!
//! The sidecar carries the immutable validators needed to safely resume (url,
//! total size, ETag, Last-Modified). Where the resume offset comes from
//! depends on the version:
//!
//! - **v1** (single connection): the offset is NOT stored — it is always the
//!   live `.part` size on disk, which makes resume self-healing after a hard
//!   kill: whatever bytes were durably flushed are what we continue from.
//! - **v2** (multi-segment): the `.part` is pre-allocated to the full size, so
//!   its length says nothing. Instead the sidecar lists every segment with its
//!   checkpointed `done` count, which is only persisted after the data it
//!   covers has been synced to disk.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::engine::segment::{is_strong_etag, Segment};
use crate::engine::{Error, Result};

/// Sidecar schema version for a single-connection download.
const SCHEMA_VERSION: u32 = 1;

/// Sidecar schema version for a multi-segment download (adds `segments`).
const SEGMENTED_VERSION: u32 = 2;

/// Distinguishes concurrent temp files written by one process.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Resume metadata persisted beside a partial download.
///
/// Written before any bytes stream, so a crash always leaves recoverable state
/// paired with the `.part` file. A v1 sidecar is written once per (re)start; a
/// v2 sidecar is then rewritten by each checkpoint of segment progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadState {
    /// Schema version of this sidecar.
    pub version: u32,
    /// The URL being downloaded.
    pub url: String,
    /// Final target path (not the `.part`).
    pub target: PathBuf,
    /// Expected total size, when the server reported one.
    #[serde(default)]
    pub total: Option<u64>,
    /// Server `ETag` validator, for `If-Range`.
    #[serde(default)]
    pub etag: Option<String>,
    /// Server `Last-Modified` validator, for `If-Range` when there's no ETag.
    #[serde(default)]
    pub last_modified: Option<String>,
    /// v2 only: every byte range and how much of it is durably written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<Segment>>,
    /// v2 only: random id minted when a multi-segment download starts, so a
    /// stale checkpoint can't overwrite a newer download at the same path.
    /// An ownership tag, not a cryptographic value; stored as `"nonce"` on disk.
    #[serde(default, rename = "nonce", skip_serializing_if = "Option::is_none")]
    pub owner_tag: Option<u64>,
}

impl DownloadState {
    /// A fresh state for `url` targeting `target` with the given validators.
    pub fn new(
        url: impl Into<String>,
        target: impl Into<PathBuf>,
        total: Option<u64>,
        etag: Option<String>,
        last_modified: Option<String>,
    ) -> Self {
        Self {
            version: SCHEMA_VERSION,
            url: url.into(),
            target: target.into(),
            total,
            etag,
            last_modified,
            segments: None,
            owner_tag: None,
        }
    }

    /// A v2 state for a multi-segment download. Only a strong ETag is kept, so
    /// `If-Range` falls back to Last-Modified for weak-ETag hosts.
    pub fn segmented(
        url: impl Into<String>,
        target: impl Into<PathBuf>,
        total: u64,
        etag: Option<String>,
        last_modified: Option<String>,
        segments: Vec<Segment>,
        owner_tag: u64,
    ) -> Self {
        Self {
            version: SEGMENTED_VERSION,
            url: url.into(),
            target: target.into(),
            total: Some(total),
            etag: etag.filter(|e| is_strong_etag(e)),
            last_modified,
            segments: Some(segments),
            owner_tag: Some(owner_tag),
        }
    }

    /// The `If-Range` validator to send when resuming: prefer the strong ETag,
    /// fall back to Last-Modified.
    pub fn if_range(&self) -> Option<&str> {
        self.etag.as_deref().or(self.last_modified.as_deref())
    }

    /// Load and parse a sidecar from `path`. A newer schema is
    /// [`Error::UnsupportedState`]; unparseable JSON, or v2 segments that don't
    /// exactly tile `[0, total)`, are rejected as invalid data.
    pub fn load(path: &Path) -> Result<Self> {
        #[derive(Deserialize)]
        struct Versioned {
            version: u32,
        }

        let bytes = std::fs::read(path)?;
        let invalid =
            |e: serde_json::Error| std::io::Error::new(std::io::ErrorKind::InvalidData, e);
        // Checked before the full parse: a newer schema may not parse as this
        // one, and must not be mistaken for corruption (which is discarded).
        let Versioned { version } = serde_json::from_slice(&bytes).map_err(invalid)?;
        if version > SEGMENTED_VERSION {
            return Err(Error::UnsupportedState(version));
        }
        let state: Self = serde_json::from_slice(&bytes).map_err(invalid)?;
        state
            .validate()
            .map_err(|msg| std::io::Error::new(std::io::ErrorKind::InvalidData, msg))?;
        Ok(state)
    }

    fn validate(&self) -> std::result::Result<(), String> {
        let Some(segments) = &self.segments else {
            return Ok(());
        };
        let total = self.total.ok_or("segments without a total size")?;
        let mut sorted = segments.clone();
        sorted.sort_by_key(|s| s.start);
        let mut next = 0;
        for seg in &sorted {
            if seg.start != next || seg.end < seg.start || seg.done > seg.end - seg.start {
                return Err(format!("invalid segment {seg:?}"));
            }
            next = seg.end;
        }
        if next != total {
            return Err(format!("segments cover {next} of {total} bytes"));
        }
        Ok(())
    }

    /// Write the sidecar crash-safely: serialize to a temp file, flush+fsync,
    /// then atomically rename over `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.write_atomically(path, false).map(drop)
    }

    /// Like [`save`](Self::save), but only replaces a sidecar that is still on
    /// disk with this state's `owner_tag`; returns whether it was written.
    pub(crate) fn save_checkpoint(&self, path: &Path) -> Result<bool> {
        self.write_atomically(path, true)
    }

    fn write_atomically(&self, path: &Path, owned_only: bool) -> Result<bool> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        // Unique per writer, so overlapping saves never share a temp file.
        let unique = format!(
            "{}.{}.tmp",
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let tmp = append_ext(path, &unique);
        let written = (|| -> std::io::Result<bool> {
            {
                let mut f = std::fs::File::create(&tmp)?;
                f.write_all(&json)?;
                f.flush()?;
                f.sync_all()?;
            }
            // Re-checked right before the rename: a stale checkpoint must not
            // replace a newer download's sidecar (or resurrect a removed one).
            if owned_only && !self.owns(path) {
                std::fs::remove_file(&tmp)?;
                return Ok(false);
            }
            std::fs::rename(&tmp, path)?;
            Ok(true)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        Ok(written?)
    }

    /// Whether the sidecar on disk at `path` is this download's (same owner tag).
    fn owns(&self, path: &Path) -> bool {
        self.owner_tag.is_some()
            && Self::load(path).is_ok_and(|on_disk| on_disk.owner_tag == self.owner_tag)
    }
}

/// Bytes downloaded so far for `target`: the checkpointed sum of segment
/// progress for a v2 sidecar, else the `.part` length. `None` without a `.part`
/// or when a newer rdm wrote the sidecar (its `.part` length means nothing here).
pub fn downloaded_bytes(target: &Path) -> Option<u64> {
    let part_len = std::fs::metadata(part_path(target)).ok()?.len();
    match DownloadState::load(&sidecar_path(target)) {
        Ok(DownloadState {
            segments: Some(segments),
            ..
        }) => Some(segments.iter().map(|s| s.done).sum()),
        Err(Error::UnsupportedState(_)) => None,
        _ => Some(part_len),
    }
}

/// Best-effort removal of temp files a hard kill left mid-save beside
/// `sidecar` (`<sidecar>.<pid>.<n>.tmp`); nothing else is touched.
pub fn remove_stale_temps(sidecar: &Path) {
    let (Some(dir), Some(name)) = (sidecar.parent(), sidecar.file_name()) else {
        return;
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let prefix = format!("{}.", name.to_string_lossy());
    for entry in entries.filter_map(|e| e.ok()) {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let is_stale_temp = file_name
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(".tmp"))
            .and_then(|rest| rest.split_once('.'))
            .is_some_and(|(pid, n)| is_digits(pid) && is_digits(n));
        if is_stale_temp {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// A fresh random id for a v2 sidecar's `owner_tag`.
pub(crate) fn new_owner_tag() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    hasher.write_u128(now);
    hasher.write_u64(TMP_COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.finish()
}

/// The partial-data path for a target: `<target>.part`.
pub fn part_path(target: &Path) -> PathBuf {
    append_ext(target, "part")
}

/// The sidecar path for a target: `<target>.rdm`.
pub fn sidecar_path(target: &Path) -> PathBuf {
    append_ext(target, "rdm")
}

/// Append `ext` as an extra extension, preserving any existing one
/// (`foo.zip` → `foo.zip.part`).
fn append_ext(path: &Path, ext: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_helpers_append_without_clobbering() {
        let target = Path::new("/tmp/archive.zip");
        assert_eq!(part_path(target), Path::new("/tmp/archive.zip.part"));
        assert_eq!(sidecar_path(target), Path::new("/tmp/archive.zip.rdm"));
    }

    #[test]
    fn serde_roundtrip_and_atomic_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("x.rdm");

        let state = DownloadState::new(
            "https://example.com/x",
            dir.path().join("x"),
            Some(1234),
            Some("\"abc\"".to_string()),
            Some("Wed, 01 Jan 2020 00:00:00 GMT".to_string()),
        );
        state.save(&sidecar).unwrap();

        let loaded = DownloadState::load(&sidecar).unwrap();
        assert_eq!(state, loaded);
        // Temp file does not linger after an atomic save.
        assert_eq!(tmp_files(dir.path()), 0);
    }

    fn tmp_files(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .count()
    }

    fn seg(start: u64, end: u64, done: u64) -> Segment {
        Segment { start, end, done }
    }

    fn v2(segments: Vec<Segment>, owner_tag: u64) -> DownloadState {
        DownloadState::segmented(
            "u",
            "t",
            100,
            Some("\"e\"".into()),
            Some("lm".into()),
            segments,
            owner_tag,
        )
    }

    #[test]
    fn v1_json_is_unchanged_and_loads_without_segments() {
        let st = DownloadState::new("u", "t", Some(5), Some("\"e\"".into()), None);
        let json = serde_json::to_string(&st).unwrap();
        assert_eq!(
            json,
            r#"{"version":1,"url":"u","target":"t","total":5,"etag":"\"e\"","last_modified":null}"#
        );
        let loaded: DownloadState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.segments, None);
        assert_eq!(loaded.owner_tag, None);
    }

    #[test]
    fn owner_tag_is_stored_under_the_nonce_key() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("x.rdm");
        // A sidecar as earlier rdm versions wrote it still loads and resumes.
        std::fs::write(
            &sidecar,
            r#"{"version":2,"url":"u","target":"t","total":100,"etag":null,
                "last_modified":null,"segments":[{"start":0,"end":100,"done":7}],
                "nonce":12345}"#,
        )
        .unwrap();
        let loaded = DownloadState::load(&sidecar).unwrap();
        assert_eq!(loaded.owner_tag, Some(12345));

        loaded.save(&sidecar).unwrap();
        let on_disk: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(on_disk["nonce"], 12345);
        assert!(on_disk.get("owner_tag").is_none());
    }

    #[test]
    fn v2_roundtrip_and_weak_etag_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("x.rdm");
        let st = v2(vec![seg(0, 40, 10), seg(40, 100, 60)], 9);
        st.save(&sidecar).unwrap();
        let loaded = DownloadState::load(&sidecar).unwrap();
        assert_eq!(loaded, st);
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.if_range(), Some("\"e\""));

        let weak = DownloadState::segmented(
            "u",
            "t",
            1,
            Some("W/\"e\"".into()),
            Some("lm".into()),
            vec![seg(0, 1, 0)],
            1,
        );
        assert_eq!(weak.etag, None);
        assert_eq!(weak.if_range(), Some("lm"));
    }

    #[test]
    fn load_rejects_future_versions_and_bad_tilings() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("x.rdm");
        let mut future = DownloadState::new("u", "t", None, None, None);
        future.version = 3;
        future.save(&sidecar).unwrap();
        assert!(matches!(
            DownloadState::load(&sidecar),
            Err(Error::UnsupportedState(3))
        ));
        // Even one whose other fields this version can't parse.
        std::fs::write(&sidecar, r#"{"version":7,"url":42}"#).unwrap();
        assert!(matches!(
            DownloadState::load(&sidecar),
            Err(Error::UnsupportedState(7))
        ));
        // Corruption is invalid data, not a newer version.
        std::fs::write(&sidecar, b"{not json").unwrap();
        assert!(matches!(
            DownloadState::load(&sidecar),
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::InvalidData
        ));

        for bad in [
            vec![seg(0, 40, 0), seg(50, 100, 0)],  // gap
            vec![seg(0, 60, 0), seg(40, 100, 0)],  // overlap
            vec![seg(0, 40, 41), seg(40, 100, 0)], // done > len
            vec![seg(0, 40, 0)],                   // short of total
        ] {
            v2(bad, 1).save(&sidecar).unwrap();
            assert!(DownloadState::load(&sidecar).is_err());
        }
        // Order on disk doesn't matter (steals append segments).
        v2(vec![seg(40, 100, 0), seg(0, 40, 0)], 1)
            .save(&sidecar)
            .unwrap();
        assert!(DownloadState::load(&sidecar).is_ok());
    }

    #[test]
    fn downloaded_bytes_reads_v1_part_and_v2_segments() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("x.bin");
        assert_eq!(downloaded_bytes(&target), None);

        std::fs::write(part_path(&target), [0u8; 100]).unwrap();
        DownloadState::new("u", &target, Some(100), None, None)
            .save(&sidecar_path(&target))
            .unwrap();
        assert_eq!(downloaded_bytes(&target), Some(100));

        v2(vec![seg(0, 40, 10), seg(40, 100, 25)], 1)
            .save(&sidecar_path(&target))
            .unwrap();
        assert_eq!(downloaded_bytes(&target), Some(35));

        // A newer rdm's sidecar: the pre-allocated `.part` isn't progress.
        std::fs::write(sidecar_path(&target), r#"{"version":3}"#).unwrap();
        assert_eq!(downloaded_bytes(&target), None);
    }

    #[test]
    fn checkpoint_save_is_discarded_when_owner_tag_differs_or_sidecar_missing() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("x.rdm");
        let mine = v2(vec![seg(0, 100, 50)], 1);
        assert!(!mine.save_checkpoint(&sidecar).unwrap(), "missing sidecar");
        assert!(!sidecar.exists());

        let theirs = v2(vec![seg(0, 100, 0)], 2);
        theirs.save(&sidecar).unwrap();
        assert!(
            !mine.save_checkpoint(&sidecar).unwrap(),
            "foreign owner tag"
        );
        assert_eq!(DownloadState::load(&sidecar).unwrap(), theirs);

        let newer = v2(vec![seg(0, 100, 80)], 2);
        assert!(newer.save_checkpoint(&sidecar).unwrap());
        assert_eq!(DownloadState::load(&sidecar).unwrap(), newer);
        assert_eq!(tmp_files(dir.path()), 0);
    }

    #[test]
    fn back_to_back_saves_do_not_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = std::sync::Arc::new(dir.path().join("x.rdm"));
        v2(vec![seg(0, 100, 0)], 5).save(&sidecar).unwrap();
        let threads: Vec<_> = (0..8u64)
            .map(|i| {
                let sidecar = sidecar.clone();
                std::thread::spawn(move || {
                    for j in 0..20 {
                        let st = v2(vec![seg(0, 100, i * 10 + j % 10)], 5);
                        // Concurrent renames may lose races (e.g. a sharing
                        // violation on Windows); what matters is the result.
                        let _ = st.save_checkpoint(&sidecar);
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let loaded = DownloadState::load(&sidecar).unwrap();
        assert_eq!(loaded.owner_tag, Some(5));
        assert_eq!(tmp_files(dir.path()), 0);
    }

    #[test]
    fn remove_stale_temps_only_touches_this_sidecars_temps() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("x.bin.rdm");
        let stale = ["x.bin.rdm.123.0.tmp", "x.bin.rdm.9.42.tmp"];
        let kept = [
            "x.bin.rdm",
            "x.bin.rdm.tmp",
            "x.bin.rdm.a.1.tmp",
            "x.bin.rdm.1.2.3.tmp",
            "other.bin.rdm.1.2.tmp",
            "xx.bin.rdm.1.2.tmp",
            "x.bin.part",
        ];
        for name in stale.iter().chain(&kept) {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        remove_stale_temps(&sidecar);
        for name in stale {
            assert!(!dir.path().join(name).exists(), "{name} not swept");
        }
        for name in kept {
            assert!(dir.path().join(name).exists(), "{name} wrongly swept");
        }
    }

    #[test]
    fn missing_optional_fields_default() {
        // A sidecar written by a future/leaner writer that omits optional
        // fields must still load (schema evolution via serde default).
        let json = r#"{"version":1,"url":"u","target":"t"}"#;
        let state: DownloadState = serde_json::from_str(json).unwrap();
        assert_eq!(state.total, None);
        assert_eq!(state.etag, None);
        assert_eq!(state.last_modified, None);
    }

    #[test]
    fn if_range_prefers_etag() {
        let with_etag = DownloadState::new("u", "t", None, Some("etag".into()), Some("lm".into()));
        assert_eq!(with_etag.if_range(), Some("etag"));

        let lm_only = DownloadState::new("u", "t", None, None, Some("lm".into()));
        assert_eq!(lm_only.if_range(), Some("lm"));

        let neither = DownloadState::new("u", "t", None, None, None);
        assert_eq!(neither.if_range(), None);
    }
}
