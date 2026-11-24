//! Already-downloaded detection: is the file a link would be saved as already
//! on disk? Both frontends ask this one module; neither matches anything
//! itself.
//!
//! A match is a lookup, never a scan: one `stat` of the target path, plus a
//! hash of that one file for a strict match.

use std::path::{Path, PathBuf};

use super::integrity::Checksum;
use super::{build_client, filename, probe, DownloadRequest};

/// How closely an existing file must resemble the link's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchMode {
    /// Same file name and same size as the server reports. Cheap.
    #[default]
    Loose,
    /// Loose, plus the file's SHA-256 equals a known digest.
    Strict,
}

/// How an existing file was matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Name and size agree.
    Loose,
    /// Name, size and digest agree.
    Strict,
}

impl MatchKind {
    /// `"loose"` or `"strict"`, as the CLI's `--json` event spells it.
    pub fn name(self) -> &'static str {
        match self {
            MatchKind::Loose => "loose",
            MatchKind::Strict => "strict",
        }
    }
}

/// A file already on disk that stands for a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Existing {
    /// Where the file is.
    pub path: PathBuf,
    /// How it was matched.
    pub kind: MatchKind,
    /// A strict match was asked for but there was no digest to compare, so
    /// this is only a loose match.
    pub unverified: bool,
}

/// Decide whether `path` is the file described by `remote_name` (skipped when
/// `None`, e.g. the user chose the name) and `remote_size`. `digest` is the
/// expected digest for a [`MatchMode::Strict`] match. `None` when the file is
/// missing, isn't a regular file, or differs; an unknown remote size never
/// matches. Blocking: hashing a big file takes a while.
pub fn check_file(
    path: &Path,
    remote_name: Option<&str>,
    remote_size: Option<u64>,
    mode: MatchMode,
    digest: Option<&Checksum>,
) -> Option<Existing> {
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    if remote_size != Some(meta.len()) {
        return None;
    }
    if let Some(name) = remote_name {
        if path.file_name().and_then(|n| n.to_str()) != Some(name) {
            return None;
        }
    }
    let existing = |kind, unverified| Existing {
        path: path.to_path_buf(),
        kind,
        unverified,
    };
    if mode == MatchMode::Loose {
        return Some(existing(MatchKind::Loose, false));
    }
    let Some(digest) = digest else {
        return Some(existing(MatchKind::Loose, true));
    };
    match digest.digest_file(path) {
        Ok(actual) if actual == digest.hex => Some(existing(MatchKind::Strict, false)),
        _ => None,
    }
}

/// Look for the file `req` would save: ask the server for the size and name
/// (a probe), then check the target path (`req`'s output path, or the probed
/// name in its folder). A strict match compares against `req`'s checksum, else
/// `recorded`, the digest a download list entry kept. `None` when the server
/// can't say or nothing matches; a download then proceeds as usual.
pub async fn find_existing(
    req: &DownloadRequest,
    mode: MatchMode,
    recorded: Option<&Checksum>,
) -> Option<Existing> {
    let client = build_client(&req.proxy).ok()?;
    let probed = probe::probe(&client, &req.url).await?;
    let (path, name) = match &req.out {
        Some(out) => (out.clone(), None),
        None => (
            filename::join_in(&req.dir, &probed.filename),
            Some(probed.filename),
        ),
    };
    let digest = req.checksum.clone().or_else(|| recorded.cloned());
    tokio::task::spawn_blocking(move || {
        check_file(&path, name.as_deref(), probed.size, mode, digest.as_ref())
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SHA: &str =
        "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

    fn file(content: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn loose_needs_name_and_size() {
        let (_d, path) = file(b"hello world");
        let loose = |name, size| check_file(&path, name, size, MatchMode::Loose, None);
        assert_eq!(
            loose(Some("a.bin"), Some(11)).unwrap().kind,
            MatchKind::Loose
        );
        assert!(loose(None, Some(11)).is_some());
        assert!(loose(Some("b.bin"), Some(11)).is_none());
        assert!(loose(Some("a.bin"), Some(12)).is_none());
        assert!(loose(Some("a.bin"), None).is_none());
    }

    #[test]
    fn missing_file_or_folder_never_matches() {
        let (dir, _) = file(b"x");
        let nope = dir.path().join("nope");
        assert!(check_file(&nope, None, Some(1), MatchMode::Loose, None).is_none());
        assert!(check_file(dir.path(), None, Some(0), MatchMode::Loose, None).is_none());
    }

    #[test]
    fn strict_compares_the_digest() {
        let (_d, path) = file(b"hello world");
        let good: Checksum = HELLO_SHA.parse().unwrap();
        let bad: Checksum = "sha256:deadbeef".parse().unwrap();
        let strict = |d| check_file(&path, None, Some(11), MatchMode::Strict, d);
        let m = strict(Some(&good)).unwrap();
        assert_eq!((m.kind, m.unverified), (MatchKind::Strict, false));
        assert!(strict(Some(&bad)).is_none());
        let m = strict(None).unwrap();
        assert_eq!((m.kind, m.unverified), (MatchKind::Loose, true));
    }
}
