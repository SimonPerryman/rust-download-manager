//! On-disk partial-download state: a JSON sidecar beside the `.part` file.
//!
//! The sidecar carries only the immutable validators needed to safely resume
//! (url, total size, ETag, Last-Modified). The resume *offset* is deliberately
//! NOT stored — it is always the live `.part` size on disk, which makes resume
//! self-healing after a hard kill: whatever bytes were durably flushed are what
//! we continue from.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::Result;

/// Current sidecar schema version. Bump when the on-disk shape changes.
const SCHEMA_VERSION: u32 = 1;

/// Resume metadata persisted beside a partial download.
///
/// Written once at the start of a fresh download, before any bytes stream, so a
/// crash always leaves recoverable state paired with the `.part` file.
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
        }
    }

    /// The `If-Range` validator to send when resuming: prefer the strong ETag,
    /// fall back to Last-Modified.
    pub fn if_range(&self) -> Option<&str> {
        self.etag.as_deref().or(self.last_modified.as_deref())
    }

    /// Load and parse a sidecar from `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let state = serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(state)
    }

    /// Write the sidecar crash-safely: serialize to a temp file, flush+fsync,
    /// then atomically rename over `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("rdm.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.flush()?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
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
        assert!(!sidecar.with_extension("rdm.tmp").exists());
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
