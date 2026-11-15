//! Finding a missing download's file somewhere nearby ("Find file").
//!
//! A search scans each folder to look in once, building an index of the files
//! there by size, then matches every missing download against that index, so
//! a thousand missing files cost one pass, not a thousand. It only proposes
//! candidates ([`FoundFile`]); relinking one is a separate, explicit call
//! ([`Manager::relink_found`]). Results live in memory until the download is
//! relinked, removed or its file reappears; nothing is saved.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

use super::missing::{compare, file_size, name_key, Candidate, Likeness};
use super::{Inner, Manager};
use crate::engine::download_log::LogLevel;
use crate::engine::lock::lock;
use crate::engine::store::Status;
use crate::engine::Error;

/// How many folder levels below a search folder are looked into (the folder
/// itself is level 0). Deeper folders are skipped and mark the search
/// [`truncated`](FindResult::truncated).
pub const MAX_SEARCH_DEPTH: usize = 3;

/// The most directory entries (files and folders) visited in one search
/// folder. Reaching it stops that folder's walk and marks the search
/// [`truncated`](FindResult::truncated).
pub const MAX_SEARCH_ENTRIES: usize = 50_000;

/// How sure a [`FoundFile`] is to be the missing download's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MatchReason {
    /// The same file name and exactly the same size.
    SameNameAndSize,
    /// A different name, but the only file of that size and extension.
    RenamedUniqueSize,
    /// A different name, one of several files of that size, picked out by the
    /// download's stored checksum.
    RenamedChecksumMatch,
}

/// A file that may be a missing download's file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FoundFile {
    pub path: PathBuf,
    pub size: u64,
    /// Last modified, in seconds since the Unix epoch, if the file system says.
    pub modified: Option<u64>,
    pub reason: MatchReason,
}

/// What one search found for one missing download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindResult {
    /// The missing download.
    pub id: u64,
    /// Candidates, best folder first; empty when nothing matched.
    pub matches: Vec<FoundFile>,
    /// The folders the search looked in, in order.
    pub searched_folders: Vec<PathBuf>,
    /// Some folder was not fully searched (depth or entry limit reached).
    pub truncated: bool,
}

/// Progress of a search, from [`Manager::subscribe_find`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FindEvent {
    /// A search folder is about to be scanned.
    Progress {
        search: u64,
        /// Search folders finished so far.
        folders_done: usize,
        folders_total: usize,
        /// The folder being scanned now.
        current: Option<PathBuf>,
    },
    /// A download's result: sent for every download searched for, matched or not.
    Result { search: u64, result: FindResult },
    /// The search is over (the last event of a search).
    Finished {
        search: u64,
        cancelled: bool,
        searched_folders: Vec<PathBuf>,
        truncated: bool,
    },
}

/// Walk limits; [`Limits::DEFAULT`] outside tests.
#[derive(Debug, Clone, Copy)]
struct Limits {
    max_depth: usize,
    max_entries: usize,
}

impl Limits {
    const DEFAULT: Limits = Limits {
        max_depth: MAX_SEARCH_DEPTH,
        max_entries: MAX_SEARCH_ENTRIES,
    };
}

struct Indexed {
    path: PathBuf,
    name: OsString,
    extension: String,
    size: u64,
    modified: Option<u64>,
}

/// Every file seen in the search folders, by size, in discovery order.
#[derive(Default)]
struct Index {
    by_size: HashMap<u64, Vec<Indexed>>,
}

struct Scan {
    index: Index,
    /// Folders actually scanned, in order.
    searched: Vec<PathBuf>,
    truncated: bool,
    cancelled: bool,
}

fn extension_key(name: &OsStr) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Hidden and system folders, and (on Windows) reparse points such as
/// junctions, are not searched.
#[cfg(windows)]
fn skip_folder(_: &OsStr, meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    const REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & (HIDDEN | SYSTEM | REPARSE_POINT) != 0
}

#[cfg(not(windows))]
fn skip_folder(name: &OsStr, _: &std::fs::Metadata) -> bool {
    name.to_string_lossy().starts_with('.')
}

/// Scan `root` breadth first into `index`, calling `on_dir` for each folder
/// listed. Returns whether the walk was cut short by a limit. Symlinks are
/// never followed or indexed. `cancel` is checked between directory entries.
fn walk_root(
    root: &Path,
    limits: Limits,
    cancel: &AtomicBool,
    on_dir: &mut dyn FnMut(&Path),
    index: &mut Index,
) -> bool {
    let mut truncated = false;
    let mut visited = 0usize;
    let mut queue = VecDeque::from([(root.to_path_buf(), 0usize)]);
    while let Some((dir, depth)) = queue.pop_front() {
        on_dir(&dir);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if cancel.load(Ordering::SeqCst) {
                return truncated;
            }
            if visited >= limits.max_entries {
                return true;
            }
            visited += 1;
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let name = entry.file_name();
            if kind.is_dir() {
                if skip_folder(&name, &meta) {
                    continue;
                }
                if depth < limits.max_depth {
                    queue.push_back((entry.path(), depth + 1));
                } else {
                    truncated = true;
                }
            } else if kind.is_file() {
                index.by_size.entry(meta.len()).or_default().push(Indexed {
                    path: entry.path(),
                    extension: extension_key(&name),
                    name,
                    size: meta.len(),
                    modified: meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs()),
                });
            }
        }
    }
    truncated
}

/// Scan every existing folder of `roots` once. `on_root(done, folder)` is
/// called before each; `on_dir` for every folder listed (so tests can prove
/// none is read twice).
fn scan(
    roots: &[PathBuf],
    limits: Limits,
    cancel: &AtomicBool,
    on_root: &mut dyn FnMut(usize, &Path),
    on_dir: &mut dyn FnMut(&Path),
) -> Scan {
    let mut out = Scan {
        index: Index::default(),
        searched: Vec::new(),
        truncated: false,
        cancelled: false,
    };
    for (done, root) in roots.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            out.cancelled = true;
            break;
        }
        on_root(done, root);
        out.truncated |= walk_root(root, limits, cancel, on_dir, &mut out.index);
        out.searched.push(root.clone());
    }
    if cancel.load(Ordering::SeqCst) {
        out.cancelled = true;
    }
    out
}

fn components_key(path: &Path) -> Vec<OsString> {
    path.components().map(|c| name_key(c.as_os_str())).collect()
}

/// Search folders in the order given, without repeats and without any folder
/// inside another. A nested folder is dropped even though the walk of its
/// parent stops at [`MAX_SEARCH_DEPTH`]: simpler, and a file that deep is
/// rarely where a download was put.
fn dedupe_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let keys: Vec<Vec<OsString>> = roots.iter().map(|r| components_key(r)).collect();
    let mut kept: Vec<usize> = Vec::new();
    for (i, key) in keys.iter().enumerate() {
        let inside_another = keys
            .iter()
            .any(|other| other.len() < key.len() && key.starts_with(other));
        let repeat = kept.iter().any(|&k| &keys[k] == key);
        if !inside_another && !repeat {
            kept.push(i);
        }
    }
    kept.into_iter().map(|i| roots[i].clone()).collect()
}

/// Everything one search needs, gathered under the state lock.
struct Job {
    targets: Vec<Candidate>,
    /// Where other downloads' files are now, so one is never offered for a
    /// missing download (name_key of the full path).
    taken: HashSet<OsString>,
    roots: Vec<PathBuf>,
}

fn found(entry: &Indexed, reason: MatchReason) -> FoundFile {
    FoundFile {
        path: entry.path.clone(),
        size: entry.size,
        modified: entry.modified,
        reason,
    }
}

/// Match one missing download against the index. `digests` caches file
/// hashes across downloads. Returns `None` if cancelled while hashing.
fn match_target(
    target: &Candidate,
    index: &Index,
    taken: &HashSet<OsString>,
    digests: &mut HashMap<PathBuf, Option<String>>,
    cancel: &AtomicBool,
) -> Option<Vec<FoundFile>> {
    let pool: Vec<&Indexed> = index
        .by_size
        .get(&target.size)
        .into_iter()
        .flatten()
        .filter(|f| !taken.contains(&name_key(f.path.as_os_str())))
        .collect();
    let exact: Vec<FoundFile> = pool
        .iter()
        .filter(|f| {
            compare(&target.name, target.size, &f.name, f.size) == Likeness::SameNameAndSize
        })
        .map(|f| found(f, MatchReason::SameNameAndSize))
        .collect();
    // An empty file's size says nothing, so only a same-name match counts.
    if !exact.is_empty() || target.size == 0 {
        return Some(exact);
    }
    let extension = extension_key(&target.name);
    let renamed: Vec<&Indexed> = pool
        .into_iter()
        .filter(|f| f.extension == extension)
        .collect();
    match (renamed.as_slice(), &target.checksum) {
        ([only], _) => Some(vec![found(only, MatchReason::RenamedUniqueSize)]),
        ([_, _, ..], Some(checksum)) => {
            let mut matched = Vec::new();
            for f in renamed {
                if cancel.load(Ordering::SeqCst) {
                    return None;
                }
                let digest = digests
                    .entry(f.path.clone())
                    .or_insert_with(|| checksum.digest_file(&f.path).ok());
                if digest.as_deref() == Some(checksum.hex.as_str()) {
                    matched.push(found(f, MatchReason::RenamedChecksumMatch));
                }
            }
            Some(matched)
        }
        _ => Some(Vec::new()),
    }
}

/// `"A, B"` for the log line.
fn folder_list(folders: &[PathBuf]) -> String {
    folders
        .iter()
        .map(|f| f.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// One log line for one search of one download.
fn log_message(result: &FindResult, cancelled: bool) -> String {
    let folders = folder_list(&result.searched_folders);
    let folder_of = |f: &FoundFile| {
        f.path
            .parent()
            .map_or_else(String::new, |p| p.display().to_string())
    };
    let outcome = if cancelled {
        "cancelled".to_owned()
    } else {
        match result.matches.as_slice() {
            [] => "no match".to_owned(),
            [one] if one.reason == MatchReason::SameNameAndSize => {
                format!("found a match in {}", folder_of(one))
            }
            [one] => format!("found a possible match in {}", folder_of(one)),
            many => format!("found {} possible matches", many.len()),
        }
    };
    format!("Searched {folders}: {outcome}")
}

impl Inner {
    fn emit_find(&self, event: FindEvent) {
        let _ = self.find_tx.send(event);
    }

    /// Run one search to its end (on a blocking thread).
    fn run_find(&self, search: u64, job: Job, cancel: &AtomicBool) {
        let Job {
            targets,
            taken,
            roots,
        } = job;
        let roots: Vec<PathBuf> = roots.into_iter().filter(|r| r.is_dir()).collect();
        let total = roots.len();
        let scanned = scan(
            &roots,
            Limits::DEFAULT,
            cancel,
            &mut |done, folder| {
                self.emit_find(FindEvent::Progress {
                    search,
                    folders_done: done,
                    folders_total: total,
                    current: Some(folder.to_path_buf()),
                });
            },
            &mut |_| {},
        );
        let mut cancelled = scanned.cancelled;
        let mut digests = HashMap::new();
        for target in &targets {
            if self.closed.load(Ordering::SeqCst) {
                return;
            }
            let matches = if cancelled {
                Some(Vec::new())
            } else {
                match_target(target, &scanned.index, &taken, &mut digests, cancel)
            };
            let matches = matches.unwrap_or_else(|| {
                cancelled = true;
                Vec::new()
            });
            let result = FindResult {
                id: target.id,
                matches,
                searched_folders: scanned.searched.clone(),
                truncated: scanned.truncated,
            };
            if self.keep_result(&result, cancelled) {
                self.emit_find(FindEvent::Result { search, result });
            }
        }
        if !cancelled {
            self.emit_find(FindEvent::Progress {
                search,
                folders_done: total,
                folders_total: total,
                current: None,
            });
        }
        lock(&self.finds).remove(&search);
        self.emit_find(FindEvent::Finished {
            search,
            cancelled,
            searched_folders: scanned.searched,
            truncated: scanned.truncated,
        });
    }

    /// Remember and log `result` unless its download is gone or no longer
    /// missing (relinked while the search ran). A cancelled search keeps
    /// nothing. Returns whether to announce it.
    fn keep_result(&self, result: &FindResult, cancelled: bool) -> bool {
        let st = lock(&self.state);
        let Some(e) = st.entries.get(&result.id) else {
            return false;
        };
        if !e.file_missing || e.record.status != Status::Completed {
            return false;
        }
        self.logs
            .record(result.id, LogLevel::Info, log_message(result, cancelled));
        if cancelled {
            return false;
        }
        lock(&self.find_results).insert(result.id, result.clone());
        true
    }
}

impl Manager {
    /// Subscribe to Find file events: progress, one result per download, and a
    /// final [`FindEvent::Finished`]. Slow receivers can lag; the stored
    /// [`find_results`](Self::find_results) are the source of truth.
    pub fn subscribe_find(&self) -> tokio::sync::broadcast::Receiver<FindEvent> {
        self.inner.find_tx.subscribe()
    }

    /// Search for missing download `id`'s file: its old folder, the folders of
    /// the other downloads, then `default_folder`. Returns the search id its
    /// events carry. Errors if `id` isn't a missing download.
    pub fn find_file(&self, id: u64, default_folder: Option<&Path>) -> crate::engine::Result<u64> {
        let targets = self.missing_candidates(Some(&[id]), None);
        if targets.is_empty() {
            return Err(Error::InvalidRequest(format!(
                "download {id} isn't a missing file"
            )));
        }
        Ok(self.start_find(targets, default_folder))
    }

    /// Search for every missing download in one pass over the folders.
    /// Returns the search id its events carry.
    pub fn find_files(&self, default_folder: Option<&Path>) -> u64 {
        let targets = self.missing_candidates(None, None);
        self.start_find(targets, default_folder)
    }

    /// Stop search `search`: no further results are sent, and it ends with a
    /// [`FindEvent::Finished`] marked cancelled. Unknown or finished ids are ignored.
    pub fn cancel_find(&self, search: u64) {
        if let Some(flag) = lock(&self.inner.finds).get(&search) {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// The latest result for download `id`, until it is relinked or its file
    /// changes.
    pub fn find_result(&self, id: u64) -> Option<FindResult> {
        lock(&self.inner.find_results).get(&id).cloned()
    }

    /// Every download's latest result.
    pub fn find_results(&self) -> Vec<FindResult> {
        let mut all: Vec<FindResult> = lock(&self.inner.find_results).values().cloned().collect();
        all.sort_by_key(|r| r.id);
        all
    }

    /// Relink each `(id, file)` pair, but only a `file` the last search offered
    /// for that download and that still exists at its size. Nothing else can be
    /// relinked this way. Returns how many were relinked.
    pub fn relink_found(&self, choices: &[(u64, PathBuf)]) -> usize {
        choices
            .iter()
            .filter(|(id, file)| {
                self.find_result(*id)
                    .and_then(|r| r.matches.into_iter().find(|m| &m.path == file))
                    .is_some_and(|m| file_size(file) == Some(m.size))
            })
            .filter(|(id, file)| self.relink(*id, file).is_ok())
            .count()
    }

    fn start_find(&self, targets: Vec<Candidate>, default_folder: Option<&Path>) -> u64 {
        let job = {
            let st = lock(&self.inner.state);
            let mut roots: Vec<PathBuf> = targets.iter().map(|t| t.folder.clone()).collect();
            let mut taken = HashSet::new();
            for e in st.order.iter().filter_map(|id| st.entries.get(id)) {
                let path = e.record.final_path.as_deref().unwrap_or(&e.record.target);
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    roots.push(parent.to_path_buf());
                }
                if e.record.status == Status::Completed && !e.file_missing {
                    taken.insert(name_key(path.as_os_str()));
                }
            }
            roots.extend(default_folder.map(Path::to_path_buf));
            Job {
                targets,
                taken,
                roots: dedupe_roots(roots),
            }
        };
        let search = self.inner.next_find.fetch_add(1, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        lock(&self.inner.finds).insert(search, cancel.clone());
        let inner = self.inner.clone();
        self.inner.handle.spawn_blocking(move || {
            inner.run_find(search, job, &cancel);
        });
        search
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::manager::tests::{record, FixedStore};
    use crate::engine::store::{DownloadRecord, StoreData};
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rdm-find-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let file = dir.join(name);
        std::fs::write(&file, content).unwrap();
        file
    }

    /// A completed download whose file (`size` bytes) should be at `path`.
    fn lost(id: u64, path: &Path, size: u64, checksum: Option<&str>) -> DownloadRecord {
        DownloadRecord {
            status: Status::Completed,
            final_path: Some(path.to_path_buf()),
            total: Some(size),
            downloaded: size,
            checksum: checksum.map(str::to_owned),
            ..record(id, None)
        }
    }

    async fn manager(downloads: Vec<DownloadRecord>) -> Manager {
        let data = StoreData {
            next_id: 100,
            downloads,
            ..StoreData::default()
        };
        let mgr = Manager::load(
            Box::new(FixedStore(data)),
            tokio::runtime::Handle::current(),
        )
        .unwrap();
        mgr.check_missing().await.unwrap();
        mgr
    }

    /// Run a search to its end; its events, last one `Finished`.
    async fn run(mgr: &Manager, start: impl FnOnce(&Manager) -> u64) -> (u64, Vec<FindEvent>) {
        let mut rx = mgr.subscribe_find();
        let search = start(mgr);
        let mut events = Vec::new();
        loop {
            let event = tokio::time::timeout(Duration::from_secs(20), rx.recv())
                .await
                .expect("search never finished")
                .unwrap();
            let done = matches!(event, FindEvent::Finished { .. });
            events.push(event);
            if done {
                return (search, events);
            }
        }
    }

    fn results(events: &[FindEvent]) -> Vec<FindResult> {
        events
            .iter()
            .filter_map(|e| match e {
                FindEvent::Result { result, .. } => Some(result.clone()),
                _ => None,
            })
            .collect()
    }

    fn sha(content: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(content);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        format!("sha256:{hex}")
    }

    fn index_of(roots: &[&Path], limits: Limits) -> (Scan, Vec<PathBuf>) {
        let roots: Vec<PathBuf> = roots.iter().map(|p| p.to_path_buf()).collect();
        let mut dirs = Vec::new();
        let scanned = scan(
            &roots,
            limits,
            &AtomicBool::new(false),
            &mut |_, _| {},
            &mut |d| dirs.push(d.to_path_buf()),
        );
        (scanned, dirs)
    }

    fn indexed_names(scan: &Scan) -> Vec<String> {
        let mut names: Vec<String> = scan
            .index
            .by_size
            .values()
            .flatten()
            .map(|f| f.name.to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn same_name_and_size_in_the_old_folder_is_an_exact_match() {
        let dir = temp_dir("exact");
        let old = dir.join("old");
        let moved = dir.join("moved");
        let found = put(&moved, "a.bin", b"abc");
        put(&moved, "b.bin", b"abcd"); // wrong size
        let mgr = manager(vec![
            lost(1, &old.join("a.bin"), 3, None),
            // Another download's folder, so `moved` is searched.
            lost(2, &moved.join("kept.bin"), 1, None),
        ])
        .await;
        let (_, events) = run(&mgr, |m| m.find_file(1, None).unwrap()).await;
        let r = results(&events);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].id, 1);
        assert_eq!(r[0].matches.len(), 1);
        assert_eq!(r[0].matches[0].path, found);
        assert_eq!(r[0].matches[0].reason, MatchReason::SameNameAndSize);
        assert_eq!(r[0].matches[0].size, 3);
        assert!(r[0].matches[0].modified.is_some());
        assert_eq!(mgr.find_result(1), Some(r[0].clone()));
        // Candidates only: nothing was relinked.
        assert!(mgr.snapshot()[0].file_missing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_renamed_file_with_a_unique_size_and_the_same_extension_is_possible() {
        let dir = temp_dir("rename");
        let moved = dir.join("moved");
        let renamed = put(&moved, "new name.bin", b"abcde");
        put(&moved, "other.txt", b"vwxyz"); // same size, other extension: ignored
        put(&moved, "different.bin", b"ab");
        let mgr = manager(vec![
            lost(1, &dir.join("old").join("a.bin"), 5, None),
            lost(2, &moved.join("kept.bin"), 1, None),
        ])
        .await;
        let (_, events) = run(&mgr, |m| m.find_file(1, None).unwrap()).await;
        let m = &results(&events)[0].matches;
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].path, renamed);
        assert_eq!(m[0].reason, MatchReason::RenamedUniqueSize);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_shared_size_needs_the_stored_checksum_to_choose() {
        let dir = temp_dir("tie");
        let moved = dir.join("moved");
        let right = put(&moved, "one.bin", b"aaaa");
        put(&moved, "two.bin", b"bbbb");
        let kept = lost(9, &moved.join("kept.bin"), 1, None);
        let old = dir.join("old").join("a.bin");

        let without = manager(vec![lost(1, &old, 4, None), kept.clone()]).await;
        let (_, events) = run(&without, |m| m.find_file(1, None).unwrap()).await;
        assert!(
            results(&events)[0].matches.is_empty(),
            "ambiguous: no guess"
        );

        let checksum = sha(b"aaaa");
        let with = manager(vec![lost(1, &old, 4, Some(&checksum)), kept.clone()]).await;
        let (_, events) = run(&with, |m| m.find_file(1, None).unwrap()).await;
        let m = &results(&events)[0].matches;
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].path, right);
        assert_eq!(m[0].reason, MatchReason::RenamedChecksumMatch);

        // No file with the stored digest: still nothing.
        let wrong = sha(b"cccc");
        let none = manager(vec![lost(1, &old, 4, Some(&wrong)), kept]).await;
        let (_, events) = run(&none, |m| m.find_file(1, None).unwrap()).await;
        assert!(results(&events)[0].matches.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn only_the_roots_are_searched_and_the_default_folder_comes_last() {
        let dir = temp_dir("roots");
        let (old, other, default, outside) = (
            dir.join("old"),
            dir.join("other"),
            dir.join("default"),
            dir.join("outside"),
        );
        std::fs::create_dir_all(&old).unwrap();
        put(&outside, "a.bin", b"abc");
        let mgr = manager(vec![
            lost(1, &old.join("a.bin"), 3, None),
            lost(2, &other.join("kept.bin"), 1, None),
        ])
        .await;
        std::fs::create_dir_all(&other).unwrap();
        std::fs::create_dir_all(&default).unwrap();
        let (_, events) = run(&mgr, |m| m.find_file(1, Some(&default)).unwrap()).await;
        let r = &results(&events)[0];
        assert!(r.matches.is_empty(), "a file outside the roots isn't found");
        assert_eq!(r.searched_folders, [old, other, default]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_and_repeated_roots_are_dropped() {
        let roots = dedupe_roots(vec![
            PathBuf::from("/a/b/c"),
            PathBuf::from("/x"),
            PathBuf::from("/a/b"),
            PathBuf::from("/x"),
            PathBuf::from("/a/b/c/d"),
        ]);
        assert_eq!(roots, [PathBuf::from("/x"), PathBuf::from("/a/b")]);
    }

    #[test]
    fn the_walk_honours_the_depth_cap_and_says_it_was_cut() {
        let dir = temp_dir("depth");
        put(&dir, "d0.bin", b"x");
        put(&dir.join("1"), "d1.bin", b"x");
        put(&dir.join("1/2"), "d2.bin", b"x");
        put(&dir.join("1/2/3"), "d3.bin", b"x");
        put(&dir.join("1/2/3/4"), "d4.bin", b"x");
        let (s, _) = index_of(&[&dir], Limits::DEFAULT);
        assert_eq!(indexed_names(&s), ["d0.bin", "d1.bin", "d2.bin", "d3.bin"]);
        assert!(s.truncated);

        let shallow = temp_dir("depth-ok");
        put(&shallow.join("1"), "ok.bin", b"x");
        let (s, _) = index_of(&[&shallow], Limits::DEFAULT);
        assert!(!s.truncated, "nothing was left out");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&shallow);
    }

    #[test]
    fn the_walk_honours_the_entry_cap_per_root() {
        let dir = temp_dir("cap");
        for i in 0..10 {
            put(&dir, &format!("f{i}.bin"), b"x");
        }
        let other = temp_dir("cap-other");
        put(&other, "o.bin", b"x");
        let limits = Limits {
            max_depth: 3,
            max_entries: 4,
        };
        let (s, _) = index_of(&[&dir, &other], limits);
        assert_eq!(
            indexed_names(&s).len(),
            5,
            "4 from the first, 1 from the second"
        );
        assert!(s.truncated);
        assert_eq!(s.searched.len(), 2, "the cap is per folder, not overall");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn hidden_folders_are_skipped() {
        let dir = temp_dir("hidden");
        put(&dir, "seen.bin", b"x");
        put(&dir.join(".secret"), "unseen.bin", b"x");
        #[cfg(windows)]
        {
            let hidden = dir.join("hid");
            put(&hidden, "unseen2.bin", b"x");
            let status = std::process::Command::new("attrib")
                .arg("+h")
                .arg(&hidden)
                .status();
            if status.is_ok_and(|s| s.success()) {
                let (s, _) = index_of(&[&dir], Limits::DEFAULT);
                assert!(!indexed_names(&s).contains(&"unseen2.bin".to_owned()));
            }
        }
        #[cfg(not(windows))]
        {
            let (s, _) = index_of(&[&dir], Limits::DEFAULT);
            assert_eq!(indexed_names(&s), ["seen.bin"]);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn symlinks_are_not_followed() {
        let dir = temp_dir("symlink");
        let target = temp_dir("symlink-target");
        put(&target, "behind.bin", b"x");
        put(&dir, "plain.bin", b"x");
        let link = dir.join("link");
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&target, &link);
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&target, &link);
        if made.is_err() {
            eprintln!("skipped: symlinks can't be created here");
        } else {
            let (s, _) = index_of(&[&dir], Limits::DEFAULT);
            assert_eq!(indexed_names(&s), ["plain.bin"]);
        }
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn cancelling_stops_the_walk_between_entries() {
        let dir = temp_dir("cancel");
        for i in 0..20 {
            put(&dir.join(format!("d{i}")), "f.bin", b"x");
        }
        let cancel = AtomicBool::new(false);
        let mut listed = 0;
        let s = scan(
            std::slice::from_ref(&dir),
            Limits::DEFAULT,
            &cancel,
            &mut |_, _| {},
            &mut |_| {
                listed += 1;
                if listed == 2 {
                    cancel.store(true, Ordering::SeqCst);
                }
            },
        );
        assert!(s.cancelled);
        assert!(listed < 21, "the walk stopped early, listed {listed}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_folder_is_listed_once_however_many_records_there_are() {
        let dir = temp_dir("once");
        let (a, b) = (dir.join("a"), dir.join("b"));
        put(&a.join("deep"), "x.bin", b"x");
        put(&b, "y.bin", b"y");
        let (_, dirs) = index_of(&[&a, &b], Limits::DEFAULT);
        let unique: HashSet<_> = dirs.iter().collect();
        assert_eq!(dirs.len(), unique.len());
        assert_eq!(dirs.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn many_missing_files_resolve_in_one_search() {
        let dir = temp_dir("many");
        let moved = dir.join("moved");
        let old = dir.join("old");
        std::fs::create_dir_all(&old).unwrap();
        let mut downloads = Vec::new();
        for i in 1..=30u64 {
            let name = format!("f{i}.bin");
            if i % 3 != 0 {
                put(&moved, &name, &vec![1; i as usize]);
            }
            downloads.push(lost(i, &old.join(&name), i, None));
        }
        downloads.push(lost(99, &moved.join("kept.bin"), 1000, None));
        let mgr = manager(downloads).await;
        let (_, events) = run(&mgr, |m| m.find_files(None)).await;

        let r = results(&events);
        assert_eq!(r.len(), 31, "one result per missing record");
        let found = r.iter().filter(|r| !r.matches.is_empty()).count();
        assert_eq!(found, 20);
        // One scan: each search folder reported once, and the end says so.
        let started: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                FindEvent::Progress {
                    current: Some(c), ..
                } => Some(c.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(started.len(), 2);
        assert_eq!(started.iter().collect::<HashSet<_>>().len(), 2);
        assert!(matches!(
            events.last(),
            Some(FindEvent::Finished {
                cancelled: false,
                ..
            })
        ));
        assert_eq!(mgr.find_results().len(), 31);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_cancelled_search_ends_cancelled_and_keeps_nothing() {
        let dir = temp_dir("cancel-mgr");
        let moved = dir.join("moved");
        put(&moved, "a.bin", b"abc");
        let mgr = manager(vec![
            lost(1, &dir.join("old").join("a.bin"), 3, None),
            lost(2, &moved.join("kept.bin"), 1, None),
        ])
        .await;
        let mut rx = mgr.subscribe_find();
        // Cancel before the blocking thread can start scanning: hold the flag
        // up by cancelling inside the progress callback's first event.
        let search = mgr.find_files(None);
        mgr.cancel_find(search);
        let mut last = None;
        while let Ok(Ok(e)) = tokio::time::timeout(Duration::from_secs(20), rx.recv()).await {
            let done = matches!(e, FindEvent::Finished { .. });
            last = Some(e);
            if done {
                break;
            }
        }
        // The search may have finished before the cancel landed; if it didn't,
        // it reports as cancelled and stored nothing.
        if let Some(FindEvent::Finished {
            cancelled: true, ..
        }) = last
        {
            assert!(mgr.find_results().is_empty());
        }
        assert!(matches!(last, Some(FindEvent::Finished { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn other_downloads_files_are_never_offered() {
        let dir = temp_dir("taken");
        let moved = dir.join("moved");
        let theirs = put(&moved, "theirs.bin", b"abc");
        let mgr = manager(vec![
            lost(1, &dir.join("old").join("mine.bin"), 3, None),
            lost(2, &theirs, 3, None),
        ])
        .await;
        let (_, events) = run(&mgr, |m| m.find_file(1, None).unwrap()).await;
        assert!(results(&events)[0].matches.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn searches_are_logged_for_every_outcome() {
        let dir = temp_dir("log");
        let moved = dir.join("moved");
        put(&moved, "a.bin", b"abc");
        let mgr = manager(vec![
            lost(1, &dir.join("old").join("a.bin"), 3, None),
            lost(2, &dir.join("old").join("zzz.bin"), 7, None),
            lost(3, &moved.join("kept.bin"), 1, None),
            lost(4, &dir.join("old").join("renamed.bin"), 5, None),
        ])
        .await;
        put(&moved, "other.bin", b"12345");
        run(&mgr, |m| m.find_files(None)).await;
        let text = |id| {
            mgr.download_log(id)
                .into_iter()
                .map(|l| l.message)
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert!(text(1).contains(&format!("found a match in {}", moved.display())));
        assert!(text(1).contains("Searched "));
        assert!(text(2).contains("no match"), "{}", text(2));
        assert!(text(4).contains("found a possible match"), "{}", text(4));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_log_line_names_folders_and_outcome() {
        let result = FindResult {
            id: 1,
            matches: Vec::new(),
            searched_folders: vec![PathBuf::from("D:\\Downloads"), PathBuf::from("E:\\Archive")],
            truncated: false,
        };
        assert_eq!(
            log_message(&result, false),
            "Searched D:\\Downloads, E:\\Archive: no match"
        );
        assert_eq!(
            log_message(&result, true),
            "Searched D:\\Downloads, E:\\Archive: cancelled"
        );
    }

    #[tokio::test]
    async fn relink_found_takes_only_offered_files_and_clears_the_result() {
        let dir = temp_dir("relink-found");
        let moved = dir.join("moved");
        let found = put(&moved, "a.bin", b"abc");
        let stray = put(&dir.join("elsewhere"), "a.bin", b"abc");
        let mgr = manager(vec![
            lost(1, &dir.join("old").join("a.bin"), 3, None),
            lost(2, &moved.join("kept.bin"), 1, None),
        ])
        .await;
        run(&mgr, |m| m.find_file(1, None).unwrap()).await;

        assert_eq!(mgr.relink_found(&[(1, stray)]), 0, "not offered");
        assert!(mgr.snapshot()[0].file_missing);
        assert_eq!(mgr.relink_found(&[(1, found.clone())]), 1);
        assert!(!mgr.snapshot()[0].file_missing);
        assert_eq!(mgr.snapshot()[0].final_path, Some(found));
        assert!(mgr.find_result(1).is_none(), "cleared on relink");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn find_file_rejects_downloads_that_are_not_missing() {
        let dir = temp_dir("not-missing");
        let present = put(&dir, "a.bin", b"abc");
        let mgr = manager(vec![lost(1, &present, 3, None)]).await;
        assert!(mgr.find_file(1, None).is_err());
        assert!(mgr.find_file(42, None).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn locate_and_find_share_the_size_check() {
        let dir = temp_dir("locate");
        let wrong = put(&dir.join("new"), "whatever.bin", b"abcd");
        let right = put(&dir.join("new"), "renamed.bin", b"abc");
        let mgr = manager(vec![lost(1, &dir.join("old").join("a.bin"), 3, None)]).await;
        let err = mgr.relink_with_offer(1, &wrong).unwrap_err();
        assert_eq!(err.to_string(), "That file is a different size.");
        assert!(mgr.snapshot()[0].file_missing, "refused: nothing changed");
        // Any name is fine as long as the size is.
        assert!(mgr.relink_with_offer(1, &right).is_ok());
        assert!(!mgr.snapshot()[0].file_missing);

        assert_eq!(
            compare(OsStr::new("a"), 3, OsStr::new("a"), 3),
            Likeness::SameNameAndSize
        );
        assert_eq!(
            compare(OsStr::new("a"), 3, OsStr::new("b"), 3),
            Likeness::SameSizeOnly
        );
        assert_eq!(
            compare(OsStr::new("a"), 3, OsStr::new("a"), 4),
            Likeness::DifferentSize
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
