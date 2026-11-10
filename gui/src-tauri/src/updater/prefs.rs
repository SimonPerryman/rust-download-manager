//! The updater's preferences, in `updater.json` beside the download list:
//! whether to check automatically (absent until the first-run question is
//! answered) and the version that last ran, to tell an update restart from a
//! plain one. Kept on the Rust side so a test's `RDM_HOME` isolates them.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// The file's name in the rdm home directory.
pub const FILE: &str = "updater.json";

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Saved {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auto_check: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_version: Option<String>,
}

/// What the frontend reads at start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePrefs {
    /// `None` until the first-run question is answered.
    pub auto_check: Option<bool>,
    /// This app's version.
    pub version: String,
    /// The version the previous run was, when it's older than this one.
    pub updated_from: Option<String>,
}

/// The preferences file and what it said when this process started.
pub struct PrefsStore {
    path: PathBuf,
    saved: Mutex<Saved>,
    version: String,
    updated_from: Option<String>,
}

impl PrefsStore {
    /// Read `dir/updater.json` (missing or not JSON: unanswered), note the
    /// version that ran last if it's older than `version`, and record
    /// `version` as the last to run. A file that can't be read is left alone.
    pub fn open(dir: &Path, version: &str) -> Self {
        let path = dir.join(FILE);
        let (mut saved, writable) = match load(&path) {
            Ok(saved) => (saved, true),
            Err(e) => {
                tracing::warn!(path = %path.display(), "couldn't read the updater's preferences: {e}");
                (Saved::default(), false)
            }
        };
        let updated_from = saved
            .last_version
            .take()
            .filter(|last| is_older(last, version));
        saved.last_version = Some(version.to_owned());
        if writable {
            if let Err(e) = save(&path, &saved) {
                tracing::warn!(path = %path.display(), "couldn't save the updater's preferences: {e}");
            }
        }
        Self {
            path,
            saved: Mutex::new(saved),
            version: version.to_owned(),
            updated_from,
        }
    }

    pub fn prefs(&self) -> UpdatePrefs {
        UpdatePrefs {
            auto_check: super::lock(&self.saved).auto_check,
            version: self.version.clone(),
            updated_from: self.updated_from.clone(),
        }
    }

    /// Turn automatic checks on or off (which also answers the first-run question).
    pub fn set_auto_check(&self, on: bool) -> Result<(), String> {
        let mut saved = super::lock(&self.saved);
        saved.auto_check = Some(on);
        save(&self.path, &saved).map_err(|e| format!("Couldn't save the setting: {e}"))
    }
}

/// Whether `last` is an older version than `current` (a downgrade, or an
/// unparsable version, isn't an update).
fn is_older(last: &str, current: &str) -> bool {
    match (
        semver::Version::parse(last),
        semver::Version::parse(current),
    ) {
        (Ok(last), Ok(current)) => last < current,
        _ => false,
    }
}

/// The saved preferences; a missing file or one that isn't valid JSON counts
/// as nothing saved. Any other read error is returned.
fn load(path: &Path) -> std::io::Result<Saved> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            tracing::warn!(path = %path.display(), "ignoring unreadable updater preferences: {e}");
            Saved::default()
        })),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Saved::default()),
        Err(e) => Err(e),
    }
}

/// Write atomically (temp file, fsync, rename), as the download list is, so
/// a crash never leaves half a file.
fn save(path: &Path, saved: &Saved) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(saved).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&json)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(dir: &Path) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(dir.join(FILE)).unwrap()).unwrap()
    }

    #[test]
    fn a_first_start_is_unanswered_and_records_the_version() {
        let dir = tempfile::tempdir().unwrap();
        let store = PrefsStore::open(dir.path(), "0.3.0");
        assert_eq!(
            store.prefs(),
            UpdatePrefs {
                auto_check: None,
                version: "0.3.0".into(),
                updated_from: None,
            }
        );
        assert_eq!(
            read(dir.path()),
            serde_json::json!({ "last_version": "0.3.0" })
        );
    }

    #[test]
    fn the_answer_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        PrefsStore::open(dir.path(), "0.3.0")
            .set_auto_check(true)
            .unwrap();
        let again = PrefsStore::open(dir.path(), "0.3.0");
        assert_eq!(again.prefs().auto_check, Some(true));
        again.set_auto_check(false).unwrap();
        assert_eq!(
            PrefsStore::open(dir.path(), "0.3.0").prefs().auto_check,
            Some(false)
        );
    }

    #[test]
    fn a_corrupt_file_counts_as_unanswered() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), b"{not json").unwrap();
        let store = PrefsStore::open(dir.path(), "0.3.0");
        assert_eq!(store.prefs().auto_check, None);
        assert_eq!(store.prefs().updated_from, None);
        // It's rewritten as a valid file.
        assert_eq!(read(dir.path())["last_version"], "0.3.0");
    }

    #[test]
    fn a_new_version_knows_what_it_updated_from_for_this_run_only() {
        let dir = tempfile::tempdir().unwrap();
        PrefsStore::open(dir.path(), "0.3.0")
            .set_auto_check(true)
            .unwrap();

        let updated = PrefsStore::open(dir.path(), "0.3.1");
        assert_eq!(updated.prefs().updated_from.as_deref(), Some("0.3.0"));
        assert_eq!(updated.prefs().auto_check, Some(true));

        let next = PrefsStore::open(dir.path(), "0.3.1");
        assert_eq!(next.prefs().updated_from, None);
        assert!(!dir.path().join("updater.json.tmp").exists());
    }

    #[test]
    fn a_downgrade_is_not_an_update() {
        let dir = tempfile::tempdir().unwrap();
        PrefsStore::open(dir.path(), "0.3.1");
        let older = PrefsStore::open(dir.path(), "0.3.0");
        assert_eq!(older.prefs().updated_from, None);
        assert_eq!(read(dir.path())["last_version"], "0.3.0");
    }

    /// A file that exists but can't be read isn't overwritten: the saved
    /// answer may be in it.
    #[test]
    fn an_unreadable_file_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        // A directory in the file's place: reading it fails, and not as missing.
        std::fs::create_dir(dir.path().join(FILE)).unwrap();
        let store = PrefsStore::open(dir.path(), "0.3.0");
        assert_eq!(store.prefs().auto_check, None);
        assert!(dir.path().join(FILE).is_dir());
        assert!(!dir.path().join("updater.json.tmp").exists());
    }
}
