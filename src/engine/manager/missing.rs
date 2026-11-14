//! Noticing that a finished download's file has been deleted or moved.
//!
//! "Missing" is derived, never persisted: the download stays `Completed` and
//! its snapshot carries [`DownloadSnapshot::file_missing`](super::DownloadSnapshot::file_missing). Files are checked
//! one parent folder at a time (one directory listing per folder, not one
//! lookup per file), each folder on its own blocking task, so a slow or
//! unreachable drive delays only its own records.

use std::collections::{BTreeMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::Serialize;
use tokio::task::JoinHandle;

use super::{Inner, Manager};
use crate::engine::lock::lock;
use crate::engine::store::Status;
use crate::engine::{join_error, Error};

/// A download's missing flag changed: emitted by
/// [`Manager::subscribe_missing`] when a check sets or clears it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MissingChange {
    /// The download whose flag changed.
    pub id: u64,
    /// The flag's new value.
    pub missing: bool,
}

/// Other missing downloads found in a relinked file's new folder: offered to
/// the user, never applied on their own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SameFolderOffer {
    /// The folder the relinked file used to be in.
    pub old_folder: PathBuf,
    /// The folder it was relinked into, where the matches are.
    pub new_folder: PathBuf,
    /// The downloads whose file is in `new_folder`.
    pub matches: Vec<FolderMatch>,
}

/// A missing download and the same-named, same-sized file found for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderMatch {
    /// The missing download.
    pub id: u64,
    /// The file found for it.
    pub file: PathBuf,
}

/// The outcome of [`Manager::relink_from_folder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRelink {
    /// Downloads relinked to a file in the folder.
    pub relinked: Vec<u64>,
    /// Missing downloads with no same-named, same-sized file there.
    pub not_found: Vec<u64>,
}

/// A missing download as seen by the folder matcher.
struct Candidate {
    id: u64,
    name: OsString,
    size: u64,
    folder: PathBuf,
}

fn same_folder(a: &Path, b: &Path) -> bool {
    name_key(a.as_os_str()) == name_key(b.as_os_str())
}

fn file_size(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
    meta.is_file().then_some(meta.len())
}

/// List `folder` once and pair each candidate with the file there that has
/// its name and exactly its size.
fn match_in_folder(candidates: &[Candidate], folder: &Path) -> Vec<FolderMatch> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let names: HashSet<OsString> = entries
        .filter_map(Result::ok)
        .map(|e| name_key(&e.file_name()))
        .collect();
    candidates
        .iter()
        .filter(|c| names.contains(&name_key(&c.name)))
        .filter_map(|c| {
            let file = folder.join(&c.name);
            (file_size(&file) == Some(c.size)).then_some(FolderMatch { id: c.id, file })
        })
        .collect()
}

/// What listing one folder says about one expected file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presence {
    Present,
    Missing,
    /// The folder couldn't be read for a reason other than being gone (an
    /// unreachable drive, no permission): the flag is left as it was.
    Unknown,
}

/// Group `(id, file)` pairs by the folder holding each file, so each folder
/// is listed once. Files with no parent are grouped under the empty path.
fn group_by_parent(files: Vec<(u64, PathBuf)>) -> BTreeMap<PathBuf, Vec<(u64, PathBuf)>> {
    let mut groups: BTreeMap<PathBuf, Vec<(u64, PathBuf)>> = BTreeMap::new();
    for (id, path) in files {
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        groups.entry(parent).or_default().push((id, path));
    }
    groups
}

/// File names compare ignoring case on Windows, as its file system does.
fn name_key(name: &OsStr) -> OsString {
    if cfg!(windows) {
        name.to_string_lossy().to_lowercase().into()
    } else {
        name.to_owned()
    }
}

/// List `folder` once and report each of `files` as present or missing.
fn check_folder(folder: &Path, files: &[(u64, PathBuf)]) -> Vec<(u64, PathBuf, Presence)> {
    let listing = std::fs::read_dir(if folder.as_os_str().is_empty() {
        Path::new(".")
    } else {
        folder
    });
    let names: Result<HashSet<OsString>, std::io::Error> = listing.and_then(|entries| {
        entries
            .map(|entry| entry.map(|e| name_key(&e.file_name())))
            .collect()
    });
    let verdict = |path: &Path| match &names {
        Ok(names) => match path.file_name() {
            Some(n) if names.contains(&name_key(n)) => Presence::Present,
            _ => Presence::Missing,
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Presence::Missing,
        Err(_) => Presence::Unknown,
    };
    files
        .iter()
        .map(|(id, path)| (*id, path.clone(), verdict(path)))
        .collect()
}

impl Manager {
    /// Subscribe to missing-flag changes: one [`MissingChange`] each time a
    /// check sets or clears a download's flag, never for a no-op check.
    pub fn subscribe_missing(&self) -> tokio::sync::broadcast::Receiver<MissingChange> {
        self.inner.missing_tx.subscribe()
    }

    /// Check every completed download's file in the background (one listing
    /// per folder, each folder on its own task) and update the missing flags.
    /// The handle resolves when every folder is done; dropping it is fine.
    /// A folder still being listed by an earlier check is skipped, so a hung
    /// drive can't pile up tasks.
    pub fn check_missing(&self) -> JoinHandle<()> {
        let files = self.completed_files(None);
        let inner = self.inner.clone();
        self.inner.handle.spawn(async move {
            let mut tasks = Vec::new();
            for (folder, group) in group_by_parent(files) {
                if !lock(&inner.missing_busy).insert(folder.clone()) {
                    continue;
                }
                let inner = inner.clone();
                tasks.push(inner.handle.clone().spawn_blocking(move || {
                    inner.check_group(&folder, &group);
                    lock(&inner.missing_busy).remove(&folder);
                }));
            }
            for t in tasks {
                let _ = t.await;
            }
        })
    }

    /// Check one completed download's file now, for right before opening or
    /// revealing it; returns whether it is missing. Errors if `id` is unknown
    /// or not completed. Waits only on this file's folder.
    pub async fn check_missing_one(&self, id: u64) -> crate::engine::Result<bool> {
        {
            let st = lock(&self.inner.state);
            let e = st
                .entries
                .get(&id)
                .ok_or_else(|| Error::InvalidRequest(format!("no download with id {id}")))?;
            if e.record.status != Status::Completed {
                return Err(Error::InvalidRequest(format!(
                    "download {id} hasn't completed"
                )));
            }
        }
        let files = self.completed_files(Some(id));
        let inner = self.inner.clone();
        let joined = self
            .inner
            .handle
            .spawn_blocking(move || {
                for (folder, group) in group_by_parent(files) {
                    inner.check_group(&folder, &group);
                }
            })
            .await;
        joined.map_err(join_error)?;
        let st = lock(&self.inner.state);
        Ok(st.entries.get(&id).is_some_and(|e| e.file_missing))
    }

    /// Point completed download `id` at `file`, a file the user located after
    /// the original went missing: the record's path (and shown name) follow
    /// it, the missing flag clears and subscribers get a [`MissingChange`].
    /// Errors, leaving the record as it was, if `id` is unknown or not
    /// completed, or `file` isn't an existing file.
    pub fn relink(&self, id: u64, file: &Path) -> crate::engine::Result<()> {
        if !file.is_file() {
            return Err(Error::InvalidRequest(format!(
                "{} isn't a file on disk",
                file.display()
            )));
        }
        let st = &mut *lock(&self.inner.state);
        let e = st
            .entries
            .get_mut(&id)
            .ok_or_else(|| Error::InvalidRequest(format!("no download with id {id}")))?;
        if e.record.status != Status::Completed {
            return Err(Error::InvalidRequest(format!(
                "download {id} hasn't completed"
            )));
        }
        e.record.final_path = Some(file.to_path_buf());
        e.record.target = file.to_path_buf();
        let was_missing = std::mem::replace(&mut e.file_missing, false);
        if was_missing {
            let _ = self
                .inner
                .missing_tx
                .send(MissingChange { id, missing: false });
        }
        self.inner.persist(st);
        self.inner.publish(st);
        Ok(())
    }

    /// Like [`Manager::relink`], and then look for the other missing downloads
    /// that sat in the same old folder as `id` and whose same-named,
    /// same-sized file is in the folder `file` is in. Returns them as an offer
    /// (nothing is relinked for them: the user must accept), or `None` when
    /// there are none.
    pub fn relink_with_offer(
        &self,
        id: u64,
        file: &Path,
    ) -> crate::engine::Result<Option<SameFolderOffer>> {
        let old_folder = self
            .snapshot()
            .into_iter()
            .find(|d| d.id == id)
            .and_then(|d| d.final_path)
            .and_then(|p| p.parent().map(Path::to_path_buf));
        self.relink(id, file)?;
        let (Some(old_folder), Some(new_folder)) = (old_folder, file.parent()) else {
            return Ok(None);
        };
        if same_folder(&old_folder, new_folder) {
            return Ok(None);
        }
        let candidates = self.missing_candidates(None, Some(&old_folder));
        let matches = match_in_folder(&candidates, new_folder);
        Ok((!matches.is_empty()).then(|| SameFolderOffer {
            old_folder,
            new_folder: new_folder.to_path_buf(),
            matches,
        }))
    }

    /// Relink every accepted match of an offer. Each is re-verified first
    /// (still missing, file present with the expected size), so a stale offer
    /// can't relink anything wrongly. Returns how many were relinked.
    pub fn apply_matches(&self, matches: &[FolderMatch]) -> usize {
        let wanted: Vec<u64> = matches.iter().map(|m| m.id).collect();
        let candidates = self.missing_candidates(Some(&wanted), None);
        matches
            .iter()
            .filter(|m| {
                candidates
                    .iter()
                    .any(|c| c.id == m.id && file_size(&m.file) == Some(c.size))
            })
            .filter(|m| self.relink(m.id, &m.file).is_ok())
            .count()
    }

    /// Relink each of `ids` that is a missing download whose same-named,
    /// same-sized file is in `folder` (listed once). Reports which were
    /// relinked and which weren't; non-missing ids are ignored.
    pub fn relink_from_folder(&self, ids: &[u64], folder: &Path) -> FolderRelink {
        let candidates = self.missing_candidates(Some(ids), None);
        let matches = match_in_folder(&candidates, folder);
        let relinked: Vec<u64> = matches
            .iter()
            .filter(|m| self.relink(m.id, &m.file).is_ok())
            .map(|m| m.id)
            .collect();
        let not_found = candidates
            .iter()
            .map(|c| c.id)
            .filter(|id| !relinked.contains(id))
            .collect();
        FolderRelink {
            relinked,
            not_found,
        }
    }

    /// Missing completed downloads, optionally only `ids` and/or those whose
    /// file sat in `old_folder`.
    fn missing_candidates(&self, ids: Option<&[u64]>, old_folder: Option<&Path>) -> Vec<Candidate> {
        let st = lock(&self.inner.state);
        st.order
            .iter()
            .filter(|id| ids.is_none_or(|ids| ids.contains(id)))
            .filter_map(|id| st.entries.get(id))
            .filter(|e| e.file_missing && e.record.status == Status::Completed)
            .filter_map(|e| {
                let path = e.record.final_path.as_deref()?;
                Some(Candidate {
                    id: e.record.id,
                    name: path.file_name()?.to_owned(),
                    size: e.record.total.unwrap_or(e.record.downloaded),
                    folder: path.parent()?.to_path_buf(),
                })
            })
            .filter(|c| old_folder.is_none_or(|f| same_folder(&c.folder, f)))
            .collect()
    }

    /// `(id, final path)` of completed downloads that have one: just `only`
    /// if given, else all.
    fn completed_files(&self, only: Option<u64>) -> Vec<(u64, PathBuf)> {
        let st = lock(&self.inner.state);
        st.order
            .iter()
            .filter(|id| only.is_none_or(|o| o == **id))
            .filter_map(|id| st.entries.get(id))
            .filter(|e| e.record.status == Status::Completed)
            .filter_map(|e| Some((e.record.id, e.record.final_path.clone()?)))
            .collect()
    }
}

impl Inner {
    /// List one folder (no lock held) and apply the verdicts.
    fn check_group(&self, folder: &Path, files: &[(u64, PathBuf)]) {
        let results = check_folder(folder, files);
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let st = &mut *lock(&self.state);
        let mut changed = false;
        for (id, path, presence) in results {
            let missing = match presence {
                Presence::Present => false,
                Presence::Missing => true,
                Presence::Unknown => continue,
            };
            // The record may have moved on while the folder was listed.
            let Some(e) = st.entries.get_mut(&id) else {
                continue;
            };
            if e.record.status != Status::Completed
                || e.record.final_path.as_deref() != Some(path.as_path())
                || e.file_missing == missing
            {
                continue;
            }
            e.file_missing = missing;
            changed = true;
            let _ = self.missing_tx.send(MissingChange { id, missing });
        }
        if changed {
            self.publish(st);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::manager::tests::{record, FixedStore};
    use crate::engine::store::{DownloadRecord, StoreData};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rdm-missing-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rec(id: u64, status: Status, path: &Path) -> DownloadRecord {
        DownloadRecord {
            status,
            final_path: Some(path.to_path_buf()),
            ..record(id, None)
        }
    }

    fn manager(downloads: Vec<DownloadRecord>) -> Manager {
        let data = StoreData {
            next_id: 100,
            downloads,
            ..StoreData::default()
        };
        Manager::load(
            Box::new(FixedStore(data)),
            tokio::runtime::Handle::current(),
        )
        .unwrap()
    }

    fn flag(mgr: &Manager, id: u64) -> bool {
        mgr.snapshot()
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .file_missing
    }

    #[test]
    fn grouping_lists_each_folder_once() {
        let a = PathBuf::from("a");
        let b = PathBuf::from("b");
        let groups = group_by_parent(vec![
            (1, a.join("x.bin")),
            (2, b.join("y.bin")),
            (3, a.join("z.bin")),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[&a].iter().map(|g| g.0).collect::<Vec<_>>(), [1, 3]);
        assert_eq!(groups[&b].len(), 1);
    }

    #[tokio::test]
    async fn present_deleted_restored() {
        let dir = temp_dir("cycle");
        let file = dir.join("f.bin");
        std::fs::write(&file, b"x").unwrap();
        let mgr = manager(vec![rec(1, Status::Completed, &file)]);
        let mut events = mgr.subscribe_missing();

        mgr.check_missing().await.unwrap();
        assert!(!flag(&mgr, 1));
        assert!(events.try_recv().is_err(), "no change, no event");

        std::fs::remove_file(&file).unwrap();
        mgr.check_missing().await.unwrap();
        assert!(flag(&mgr, 1));
        assert_eq!(
            events.try_recv().unwrap(),
            MissingChange {
                id: 1,
                missing: true
            }
        );
        // Still finished: only the overlay changed.
        assert_eq!(mgr.snapshot()[0].status, Status::Completed);

        mgr.check_missing().await.unwrap();
        assert!(events.try_recv().is_err(), "unchanged, no repeat event");

        std::fs::write(&file, b"x").unwrap();
        mgr.check_missing().await.unwrap();
        assert!(!flag(&mgr, 1));
        assert_eq!(
            events.try_recv().unwrap(),
            MissingChange {
                id: 1,
                missing: false
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn only_finished_downloads_are_checked() {
        let dir = temp_dir("finished");
        let gone = dir.join("gone.bin");
        let mgr = manager(vec![
            rec(1, Status::Paused, &gone),
            rec(2, Status::Failed, &gone),
            rec(3, Status::Completed, &gone),
        ]);
        mgr.check_missing().await.unwrap();
        assert!(!flag(&mgr, 1));
        assert!(!flag(&mgr, 2));
        assert!(flag(&mgr, 3));
        assert!(mgr.check_missing_one(1).await.is_err());
        assert!(mgr.check_missing_one(99).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn missing_parent_folder_is_missing() {
        let dir = temp_dir("parent");
        let file = dir.join("sub").join("f.bin");
        let mgr = manager(vec![rec(1, Status::Completed, &file)]);
        mgr.check_missing().await.unwrap();
        assert!(flag(&mgr, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn check_one_reports_and_flags() {
        let dir = temp_dir("one");
        let file = dir.join("f.bin");
        let other = dir.join("other.bin");
        std::fs::write(&file, b"x").unwrap();
        let mgr = manager(vec![
            rec(1, Status::Completed, &file),
            rec(2, Status::Completed, &other),
        ]);
        assert!(!mgr.check_missing_one(1).await.unwrap());
        std::fs::remove_file(&file).unwrap();
        assert!(mgr.check_missing_one(1).await.unwrap());
        assert!(flag(&mgr, 1));
        assert!(!flag(&mgr, 2), "other downloads are left unchecked");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn relink_repoints_clears_flag_and_persists() {
        let dir = temp_dir("relink");
        let old = dir.join("old.bin");
        let new = dir.join("moved").join("renamed.bin");
        std::fs::create_dir_all(new.parent().unwrap()).unwrap();
        std::fs::write(&new, b"x").unwrap();
        let mgr = manager(vec![rec(1, Status::Completed, &old)]);
        mgr.check_missing().await.unwrap();
        assert!(flag(&mgr, 1));
        let mut events = mgr.subscribe_missing();

        mgr.relink(1, &new).unwrap();
        let snap = mgr.snapshot().remove(0);
        assert!(!snap.file_missing);
        assert_eq!(snap.final_path.as_deref(), Some(new.as_path()));
        assert_eq!(snap.filename, "renamed.bin");
        assert_eq!(snap.status, Status::Completed);
        assert_eq!(
            events.try_recv().unwrap(),
            MissingChange {
                id: 1,
                missing: false
            }
        );
        // A later check agrees with the new link.
        mgr.check_missing().await.unwrap();
        assert!(!flag(&mgr, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn relink_rejects_bad_requests_and_changes_nothing() {
        let dir = temp_dir("relink-bad");
        let old = dir.join("old.bin");
        let real = dir.join("real.bin");
        std::fs::write(&real, b"x").unwrap();
        let mgr = manager(vec![
            rec(1, Status::Completed, &old),
            rec(2, Status::Paused, &old),
        ]);
        mgr.check_missing().await.unwrap();
        assert!(mgr.relink(1, &dir.join("nope.bin")).is_err());
        assert!(mgr.relink(1, &dir).is_err(), "a folder isn't a file");
        assert!(mgr.relink(2, &real).is_err());
        assert!(mgr.relink(99, &real).is_err());
        let snap = mgr.snapshot().remove(0);
        assert!(snap.file_missing);
        assert_eq!(snap.final_path.as_deref(), Some(old.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn sized(id: u64, path: &Path, size: u64) -> DownloadRecord {
        DownloadRecord {
            total: Some(size),
            downloaded: size,
            ..rec(id, Status::Completed, path)
        }
    }

    /// Lost files 1, 2, 3 from folder `a` and 4 from folder `b` (3 bytes
    /// each), and an empty `new` folder for the test to fill.
    async fn same_folder_setup(tag: &str) -> (PathBuf, Manager) {
        let dir = temp_dir(tag);
        let old = dir.join("a");
        let other = dir.join("b");
        std::fs::create_dir_all(dir.join("new")).unwrap();
        let mgr = manager(vec![
            sized(1, &old.join("one.bin"), 3),
            sized(2, &old.join("two.bin"), 3),
            sized(3, &old.join("three.bin"), 3),
            sized(4, &other.join("four.bin"), 3),
        ]);
        mgr.check_missing().await.unwrap();
        (dir, mgr)
    }

    #[tokio::test]
    async fn offer_matches_by_name_and_size_in_the_new_folder_only() {
        let (dir, mgr) = same_folder_setup("offer").await;
        let new = dir.join("new");
        std::fs::write(new.join("one.bin"), b"abc").unwrap();
        std::fs::write(new.join("two.bin"), b"abc").unwrap();
        std::fs::write(new.join("three.bin"), b"abcd").unwrap(); // wrong size
        std::fs::write(new.join("four.bin"), b"abc").unwrap(); // other old folder

        let offer = mgr
            .relink_with_offer(1, &new.join("one.bin"))
            .unwrap()
            .unwrap();
        assert_eq!(offer.old_folder, dir.join("a"));
        assert_eq!(offer.new_folder, new);
        assert_eq!(
            offer.matches,
            [FolderMatch {
                id: 2,
                file: new.join("two.bin")
            }]
        );
        // Offering relinks nothing.
        assert!(flag(&mgr, 2));
        assert!(!flag(&mgr, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn no_offer_without_matches_and_only_missing_records_count() {
        let (dir, mgr) = same_folder_setup("no-offer").await;
        let new = dir.join("new");
        std::fs::write(new.join("one.bin"), b"abc").unwrap();
        assert!(mgr
            .relink_with_offer(1, &new.join("one.bin"))
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_record_whose_file_is_present_is_not_offered() {
        let (dir, mgr) = same_folder_setup("present").await;
        let (old, new) = (dir.join("a"), dir.join("new"));
        // Download 2's file reappears in its old folder, so it is no longer missing.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("two.bin"), b"abc").unwrap();
        mgr.check_missing().await.unwrap();
        assert!(!flag(&mgr, 2));
        std::fs::write(new.join("one.bin"), b"abc").unwrap();
        std::fs::write(new.join("two.bin"), b"abc").unwrap();
        std::fs::write(new.join("three.bin"), b"abc").unwrap();
        let offer = mgr
            .relink_with_offer(1, &new.join("one.bin"))
            .unwrap()
            .unwrap();
        assert_eq!(offer.matches.iter().map(|m| m.id).collect::<Vec<_>>(), [3]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn applying_an_offer_relinks_and_clears_flags() {
        let (dir, mgr) = same_folder_setup("apply").await;
        let new = dir.join("new");
        for n in ["one.bin", "two.bin", "three.bin"] {
            std::fs::write(new.join(n), b"abc").unwrap();
        }
        let offer = mgr
            .relink_with_offer(1, &new.join("one.bin"))
            .unwrap()
            .unwrap();
        assert_eq!(offer.matches.len(), 2);
        // One changes between the offer and the click: it is skipped.
        std::fs::write(new.join("three.bin"), b"abcdef").unwrap();
        assert_eq!(mgr.apply_matches(&offer.matches), 1);
        assert!(!flag(&mgr, 2));
        assert!(flag(&mgr, 3));
        let snap = mgr.snapshot();
        let two = snap.iter().find(|s| s.id == 2).unwrap();
        assert_eq!(
            two.final_path.as_deref(),
            Some(new.join("two.bin").as_path())
        );
        assert!(flag(&mgr, 4));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn relink_from_folder_reports_matched_and_unmatched() {
        let (dir, mgr) = same_folder_setup("folder").await;
        let new = dir.join("new");
        std::fs::write(new.join("one.bin"), b"abc").unwrap();
        std::fs::write(new.join("three.bin"), b"abcd").unwrap(); // wrong size
        std::fs::write(new.join("four.bin"), b"abc").unwrap();
        let result = mgr.relink_from_folder(&[1, 3, 4, 99], &new);
        assert_eq!(result.relinked, [1, 4]);
        assert_eq!(result.not_found, [3]);
        assert!(!flag(&mgr, 1) && !flag(&mgr, 4));
        assert!(flag(&mgr, 2), "unselected rows are left alone");
        assert!(flag(&mgr, 3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn removing_a_missing_record_drops_only_the_record() {
        let dir = temp_dir("remove-missing");
        let old = dir.join("old.bin");
        let other = dir.join("other.bin");
        std::fs::write(&other, b"x").unwrap();
        let mgr = manager(vec![
            rec(1, Status::Completed, &old),
            rec(2, Status::Completed, &other),
        ]);
        mgr.check_missing().await.unwrap();
        mgr.remove(1);
        let ids: Vec<u64> = mgr.snapshot().iter().map(|s| s.id).collect();
        assert_eq!(ids, [2]);
        assert!(other.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
