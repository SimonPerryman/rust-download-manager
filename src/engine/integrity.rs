//! Integrity verification for a completed `.part` file.
//!
//! Size is always checked when the server gave a total. An optional
//! [`Checksum`] (e.g. `sha256:<hex>`) is hashed and compared before the file is
//! renamed into place, so a corrupt download is never promoted to the target;
//! the same hashing core verifies an already-finished file on request.

use std::fmt;
use std::io::Read;
use std::path::Path;
use std::str::FromStr;

use crate::engine::Error;
use sha2::{Digest, Sha256};

/// A supported checksum algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo {
    /// SHA-256.
    Sha256,
}

impl Algo {
    /// The lowercase name used in `<algo>:<hex>`, e.g. `sha256`.
    pub fn name(self) -> &'static str {
        match self {
            Algo::Sha256 => "sha256",
        }
    }
}

/// An expected checksum: an algorithm plus the lowercase hex digest.
///
/// Parse from a `"<algo>:<hex>"` string, e.g. `sha256:abcdef…`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checksum {
    /// Which hash algorithm `hex` is a digest of.
    pub algo: Algo,
    /// Lowercase hex-encoded expected digest.
    pub hex: String,
}

impl FromStr for Checksum {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let (algo, hex) = s
            .split_once(':')
            .ok_or_else(|| format!("expected '<algo>:<hex>', got {s:?}"))?;
        let algo = match algo.to_ascii_lowercase().as_str() {
            "sha256" => Algo::Sha256,
            other => return Err(format!("unsupported checksum algorithm: {other}")),
        };
        let hex = hex.trim().to_ascii_lowercase();
        if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("invalid hex digest: {hex:?}"));
        }
        Ok(Checksum { algo, hex })
    }
}

impl fmt::Display for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.algo.name(), self.hex)
    }
}

impl Checksum {
    /// Hash `path` and return [`Error::ChecksumMismatch`] if the digest doesn't
    /// match, logging the outcome either way. Hashes on the blocking pool, off
    /// the async runtime.
    pub async fn verify(&self, path: &Path) -> crate::engine::Result<()> {
        let (checksum, path) = (self.clone(), path.to_path_buf());
        let actual = tokio::task::spawn_blocking(move || checksum.digest_file(&path))
            .await
            .map_err(std::io::Error::other)??;
        if actual != self.hex {
            tracing::error!(
                "Checksum mismatch ({}) — expected {}, got {actual}",
                self.algo.name(),
                self.hex
            );
            return Err(Error::ChecksumMismatch(format!(
                "{} mismatch: expected {}, got {}",
                self.algo.name(),
                self.hex,
                actual
            )));
        }
        tracing::info!("Checksum verified ✓");
        Ok(())
    }

    /// The lowercase hex digest of `path` under this checksum's algorithm.
    /// Synchronous and blocking: callers run it via `spawn_blocking`.
    pub fn digest_file(&self, path: &Path) -> std::io::Result<String> {
        let mut file = std::fs::File::open(path)?;
        let mut buf = vec![0u8; 64 * 1024];
        match self.algo {
            Algo::Sha256 => {
                let mut hasher = Sha256::new();
                loop {
                    let n = file.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buf[..n]);
                }
                Ok(hex_encode(&hasher.finalize()))
            }
        }
    }
}

/// Verify the finished `.part` size equals `expected` when one is known.
pub fn verify_size(actual: u64, expected: Option<u64>) -> crate::engine::Result<()> {
    if let Some(expected) = expected {
        if actual != expected {
            return Err(Error::Integrity(format!(
                "size mismatch: expected {expected} bytes, got {actual}"
            )));
        }
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_sha256() {
        let c: Checksum = "sha256:ABCdef0123".parse().unwrap();
        assert_eq!(c.algo, Algo::Sha256);
        assert_eq!(c.hex, "abcdef0123"); // lowercased
        assert_eq!(c.to_string(), "sha256:abcdef0123");
    }

    #[test]
    fn rejects_bad_input() {
        assert!("abcdef".parse::<Checksum>().is_err()); // no algo
        assert!("md5:abc".parse::<Checksum>().is_err()); // unsupported algo
        assert!("sha256:xyz".parse::<Checksum>().is_err()); // non-hex
        assert!("sha256:".parse::<Checksum>().is_err()); // empty digest
    }

    #[tokio::test]
    async fn verify_matches_and_mismatches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"hello world").unwrap();
        }
        // Known sha256("hello world").
        let good: Checksum =
            "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
                .parse()
                .unwrap();
        good.verify(&path).await.unwrap();

        let bad: Checksum = "sha256:deadbeef".parse().unwrap();
        let err = bad.verify(&path).await.unwrap_err();
        assert!(matches!(err, Error::ChecksumMismatch(_)), "got {err:?}");
        assert!(err
            .to_string()
            .starts_with("integrity check failed: sha256 mismatch"));
    }

    #[test]
    fn size_check() {
        verify_size(10, None).unwrap();
        verify_size(10, Some(10)).unwrap();
        assert!(matches!(verify_size(9, Some(10)), Err(Error::Integrity(_))));
    }
}
