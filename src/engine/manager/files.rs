//! Which download owns which files on disk: free target names, partials, and
//! the finished file.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{Manager, ManagerState};
use crate::engine::lock::lock;
use crate::engine::store::{ChecksumState, DownloadRecord, Status};
use crate::engine::{state, Error};

impl Manager {
    /// The finished file of completed download `id`, for a frontend to open or
    /// reveal: only paths the engine itself downloaded, never arbitrary ones.
    /// Errors if `id` is unknown, hasn't completed, or its file is gone.
    pub fn completed_file(&self, id: u64) -> crate::engine::Result<PathBuf> {
        let st = lock(&self.inner.state);
        let e = st
            .entries
            .get(&id)
            .ok_or_else(|| Error::InvalidRequest(format!("no download with id {id}")))?;
        finished_file(&e.record)
    }
}

/// Choose a target path that no download owning its partial (see
/// [`owns_partial`]) and no file on disk is already using, so concurrent
/// `.part` files never collide. Mirrors the `name (1).ext` scheme the engine
/// uses at finalize. Provisional targets own nothing on disk, so they never
/// block a name.
pub(super) fn unique_target(st: &ManagerState, base: PathBuf) -> PathBuf {
    let active: HashSet<&Path> = st
        .entries
        .values()
        .filter(|e| owns_partial(&e.record))
        .map(|e| e.record.target.as_path())
        .collect();

    let taken = |p: &Path| active.contains(p) || p.exists() || state::part_path(p).exists();
    if !taken(&base) {
        return base;
    }

    let parent = base.parent();
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download")
        .to_string();
    let ext = base
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_string);
    for n in 1u32.. {
        let name = match &ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = match parent {
            Some(p) if !p.as_os_str().is_empty() => p.join(&name),
            _ => PathBuf::from(&name),
        };
        if !taken(&candidate) {
            return candidate;
        }
    }
    unreachable!("the range is effectively unbounded")
}

/// The files rule `remove` follows: a completed download's file
/// stays, as does a partial the user chose to keep; any other partial the
/// record owns goes, unless another download in `st` owns the same target.
pub(super) fn discard_files(st: &ManagerState, record: &DownloadRecord) {
    let shared = owned_by_other(st, record.id, &record.target);
    if owns_partial(record) && !record.partial_kept && !shared {
        remove_partial(record);
    }
}

/// Whether a record has a claim on its target's `.part` + sidecar: it is
/// running, waiting, paused, failed with its partial left for a retry, or
/// stopped with its partial kept. Completed, deleted-stopped and
/// checksum-mismatched (partial discarded) records own nothing, so their
/// target may be reused by a newer download.
pub(super) fn owns_partial(record: &DownloadRecord) -> bool {
    if record.target_provisional {
        return false;
    }
    match record.status {
        Status::Queued | Status::Downloading | Status::Paused => true,
        Status::Failed => record.checksum_state != ChecksumState::Mismatch,
        Status::Cancelled => record.partial_kept,
        Status::Completed => false,
    }
}

/// Whether some download other than `id` owns `target`'s partial.
fn owned_by_other(st: &ManagerState, id: u64, target: &Path) -> bool {
    st.entries
        .values()
        .any(|e| e.record.id != id && e.record.target == target && owns_partial(&e.record))
}

/// [`owned_by_other`] for download `id`'s own target (false if it's unknown).
pub(super) fn owned_elsewhere(st: &ManagerState, id: u64) -> bool {
    st.entries
        .get(&id)
        .is_some_and(|e| owned_by_other(st, id, &e.record.target))
}

/// Whether a record owns a `.part` on disk (a provisional target owns none).
pub(super) fn has_partial(record: &DownloadRecord) -> bool {
    !record.target_provisional && state::part_path(&record.target).exists()
}

/// Best-effort removal of a record's `.part`, `.rdm` sidecar and any stale
/// sidecar temp files. A provisional target has none (and may name another
/// download's), so it is left alone.
pub(super) fn remove_partial(record: &DownloadRecord) {
    if record.target_provisional {
        return;
    }
    let _ = std::fs::remove_file(state::part_path(&record.target));
    let sidecar = state::sidecar_path(&record.target);
    let _ = std::fs::remove_file(&sidecar);
    state::remove_stale_temps(&sidecar);
}

/// A completed record's file, if it's still on disk.
pub(super) fn finished_file(record: &DownloadRecord) -> crate::engine::Result<PathBuf> {
    let id = record.id;
    if record.status != Status::Completed {
        return Err(Error::InvalidRequest(format!(
            "download {id} hasn't completed"
        )));
    }
    record
        .final_path
        .clone()
        .filter(|p| p.is_file())
        .ok_or_else(|| Error::InvalidRequest(format!("download {id}'s file is no longer on disk")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::manager::tests::{record, FixedStore};
    use crate::engine::store::StoreData;

    /// Only a completed download whose file is still on disk hands out a path,
    /// so a frontend can't be steered into opening anything else.
    #[tokio::test]
    async fn completed_file_only_for_a_finished_file_on_disk() {
        let dir = std::env::temp_dir().join(format!("rdm-completed-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("done.bin");
        std::fs::write(&file, b"done").unwrap();
        let completed = |id, path: &Path| DownloadRecord {
            status: Status::Completed,
            final_path: Some(path.to_path_buf()),
            ..record(id, None)
        };
        let data = StoreData {
            next_id: 4,
            downloads: vec![
                completed(1, &file),
                completed(2, &dir.join("gone.bin")),
                record(3, None),
            ],
            ..StoreData::default()
        };
        let mgr = Manager::load(
            Box::new(FixedStore(data)),
            tokio::runtime::Handle::current(),
        )
        .unwrap();

        assert_eq!(mgr.completed_file(1).unwrap(), file);
        let err = |id| mgr.completed_file(id).unwrap_err().to_string();
        assert!(err(2).contains("no longer on disk"), "{}", err(2));
        assert!(err(3).contains("hasn't completed"), "{}", err(3));
        assert!(err(99).contains("no download with id 99"), "{}", err(99));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
