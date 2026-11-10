//! Making per-segment progress durable: syncing the `.part`, then saving the v2 sidecar.

use std::path::Path;

use tokio::task::JoinHandle;

use super::Shared;
use crate::engine::DownloadState;

/// Start a periodic checkpoint of the current table on the blocking pool.
pub(super) fn spawn_checkpoint(
    shared: &Shared<'_>,
    base: &DownloadState,
    sidecar: &Path,
) -> JoinHandle<crate::engine::Result<()>> {
    let mut st = base.clone();
    st.segments = Some(shared.snapshot());
    let part = shared.part.to_path_buf();
    let sidecar = sidecar.to_path_buf();
    tokio::task::spawn_blocking(move || checkpoint_blocking(&part, &sidecar, &st, || {}))
}

/// Periodic checkpoints are best-effort: a failure is logged and the next
/// tick tries again.
pub(super) fn log_periodic_checkpoint(
    joined: std::result::Result<crate::engine::Result<()>, tokio::task::JoinError>,
) {
    match joined {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!(
            "Couldn't save progress, will try again: {}",
            crate::engine::error_detail(&e)
        ),
        Err(e) => tracing::warn!("Couldn't save progress, will try again: {e}"),
    }
}

/// Make `st` durable: sync the `.part` data first (the snapshot was taken
/// before this, so the sidecar never claims bytes that aren't on disk), then
/// save the sidecar if it still belongs to this download. If the `.part` is
/// gone (download removed) nothing is left behind. `after_save` is a test seam.
pub(crate) fn checkpoint_blocking(
    part: &Path,
    sidecar: &Path,
    st: &DownloadState,
    after_save: impl FnOnce(),
) -> crate::engine::Result<()> {
    if !sync_part(part)? {
        return Ok(());
    }
    save_unless_removed(part, sidecar, st, after_save)
}

/// The checkpoint once every segment is written: the data sync must succeed,
/// but a failed sidecar `save` is only logged, since finalizing needs nothing
/// from it beyond `total`, which the sidecar on disk already holds.
pub(super) fn complete_checkpoint_blocking(
    part: &Path,
    save: impl FnOnce() -> crate::engine::Result<()>,
) -> crate::engine::Result<()> {
    if sync_part(part)? {
        if let Err(e) = save() {
            tracing::warn!(
                "Couldn't save final progress: {}",
                crate::engine::error_detail(&e)
            );
        }
    }
    Ok(())
}

/// Sync the `.part` data to disk; `false` if it is gone (download removed).
fn sync_part(part: &Path) -> crate::engine::Result<bool> {
    // Writable: on Windows, FlushFileBuffers needs write access.
    let file = match std::fs::OpenOptions::new().write(true).open(part) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    file.sync_data()?;
    Ok(true)
}

/// Save `st` if the sidecar is still this download's, removing the save again
/// if the `.part` vanished meanwhile.
pub(super) fn save_unless_removed(
    part: &Path,
    sidecar: &Path,
    st: &DownloadState,
    after_save: impl FnOnce(),
) -> crate::engine::Result<()> {
    let wrote = st.save_checkpoint(sidecar)?;
    after_save();
    if wrote && !part.exists() {
        let _ = std::fs::remove_file(sidecar);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::segment::Segment;
    use crate::engine::Error;
    use std::path::PathBuf;

    /// `.part` and sidecar paths for a target inside `dir`.
    fn paths_in(dir: &Path) -> (PathBuf, PathBuf) {
        let target = dir.join("x.bin");
        (
            crate::engine::state::part_path(&target),
            crate::engine::state::sidecar_path(&target),
        )
    }

    fn seg(start: u64, end: u64, done: u64) -> Segment {
        Segment { start, end, done }
    }

    fn v2_state(dir: &Path, nonce: u64) -> DownloadState {
        DownloadState::segmented(
            "u",
            dir.join("x.bin"),
            10,
            Some("\"e\"".into()),
            None,
            vec![seg(0, 10, 4)],
            nonce,
        )
    }

    #[test]
    fn complete_checkpoint_tolerates_save_failure_but_not_sync_failure() {
        let dir = tempfile::tempdir().unwrap();
        let (part, _) = paths_in(dir.path());
        std::fs::write(&part, [0u8; 10]).unwrap();
        let failing_save = || Err(Error::Io(std::io::Error::other("disk full")));
        complete_checkpoint_blocking(&part, failing_save).expect("save failure is not fatal");

        // A `.part` that can't be synced (here: a directory) still fails.
        let unsyncable = dir.path().join("dir.part");
        std::fs::create_dir(&unsyncable).unwrap();
        assert!(complete_checkpoint_blocking(&unsyncable, || Ok(())).is_err());
    }

    #[test]
    fn checkpoint_skips_when_part_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let (part, sidecar) = paths_in(dir.path());
        checkpoint_blocking(&part, &sidecar, &v2_state(dir.path(), 1), || {}).unwrap();
        assert!(!sidecar.exists());
    }

    #[test]
    fn checkpoint_saves_and_deletes_its_save_if_part_vanishes() {
        let dir = tempfile::tempdir().unwrap();
        let (part, sidecar) = paths_in(dir.path());
        std::fs::write(&part, [0u8; 10]).unwrap();
        let st = v2_state(dir.path(), 7);
        st.save(&sidecar).unwrap();

        let mut next = st.clone();
        next.segments = Some(vec![seg(0, 10, 8)]);
        checkpoint_blocking(&part, &sidecar, &next, || {}).unwrap();
        assert_eq!(DownloadState::load(&sidecar).unwrap(), next);

        // The download is removed while the save is in flight.
        checkpoint_blocking(&part, &sidecar, &next, || {
            std::fs::remove_file(&part).unwrap();
            std::fs::remove_file(&sidecar).ok();
        })
        .unwrap();
        assert!(!sidecar.exists(), "no orphan sidecar after a removal race");

        // Variant: the `.part` vanishes but the save landed.
        std::fs::write(&part, [0u8; 10]).unwrap();
        st.save(&sidecar).unwrap();
        checkpoint_blocking(&part, &sidecar, &next, || {
            std::fs::remove_file(&part).unwrap();
        })
        .unwrap();
        assert!(
            !sidecar.exists(),
            "checkpoint removes its own orphaned save"
        );
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files left: {leftovers:?}");
    }
}
