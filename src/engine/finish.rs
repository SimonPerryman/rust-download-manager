//! The end of a download: verifying and renaming the `.part`, or discarding it.

use std::path::{Path, PathBuf};

use super::{filename, integrity, state, DownloadRequest, DownloadState, Result, Summary};

/// Verify the finished `.part` (size, then checksum), rename it to a
/// collision-free final path, and delete the sidecar.
pub(super) async fn finalize(
    req: &DownloadRequest,
    part: &Path,
    bytes: u64,
    status: u16,
) -> Result<Summary> {
    // The sidecar carries the authoritative expected total (survives restarts).
    let sidecar = sidecar_for_part(part);
    let expected_total = DownloadState::load(&sidecar).ok().and_then(|s| s.total);

    integrity::verify_size(bytes, expected_total)?;
    if let Some(checksum) = &req.checksum {
        checksum.verify(part).await?;
    }

    let target = target_for_part(part);
    let final_path = filename::unique_path(&target);
    tokio::fs::rename(part, &final_path).await?;
    let _ = tokio::fs::remove_file(&sidecar).await;
    state::remove_stale_temps(&sidecar);

    Ok(Summary {
        path: final_path,
        bytes,
        status,
    })
}

/// Strip the trailing `.part` to recover the target path.
fn target_for_part(part: &Path) -> PathBuf {
    let s = part.to_string_lossy();
    match s.strip_suffix(".part") {
        Some(base) => PathBuf::from(base),
        None => part.to_path_buf(),
    }
}

/// The sidecar path matching a `.part` path.
fn sidecar_for_part(part: &Path) -> PathBuf {
    state::sidecar_path(&target_for_part(part))
}

/// Remove a cancelled download's `.part`, sidecar and temps, once its target
/// is known.
pub(super) async fn discard_partial(known_target: &Option<PathBuf>) {
    if let Some(target) = known_target {
        let sidecar = state::sidecar_path(target);
        let _ = tokio::fs::remove_file(state::part_path(target)).await;
        let _ = tokio::fs::remove_file(&sidecar).await;
        state::remove_stale_temps(&sidecar);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_and_sidecar_derive_from_part() {
        let part = Path::new("/tmp/archive.zip.part");
        assert_eq!(target_for_part(part), Path::new("/tmp/archive.zip"));
        assert_eq!(sidecar_for_part(part), Path::new("/tmp/archive.zip.rdm"));
    }
}
