//! Live end-to-end smoke test over the real network.
//!
//! `#[ignore]`d so the default gate stays offline; run with
//! `cargo test -- --ignored` or the manual-dispatch CI job. The asset is a file
//! pinned to an immutable commit on microsoft/TypeScript, so its SHA-256 never
//! drifts; a truncated or corrupt download fails the checksum.

use sha2::{Digest, Sha256};

use rdm::{download, Control, DownloadRequest};

/// A file pinned to an immutable commit (TypeScript v5.7.3) so the checksum
/// below is stable over time.
const ASSET_URL: &str = "https://raw.githubusercontent.com/microsoft/TypeScript/a5e123d9e0690fcea92878ea8a0a382922009fc9/lib/lib.es5.d.ts";

/// Expected SHA-256 (hex) and exact byte length of the pinned asset.
const ASSET_SHA256: &str = "e41c290ef7dd7dab3493e6cbe5909e0148edf4a8dad0271be08edec368a0f7b9";
const ASSET_LEN: u64 = 217_027;

#[tokio::test]
#[ignore = "hits the real network; run with `cargo test -- --ignored`"]
async fn live_smoke_pinned_public_asset() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("lib.es5.d.ts");

    let req = DownloadRequest::to_path(ASSET_URL, &out);
    let summary = download(&req, &Control::new(), |_| {})
        .await
        .expect("live download should succeed");

    assert_eq!(summary.bytes, ASSET_LEN);
    let bytes = std::fs::read(&summary.path).unwrap();
    assert_eq!(bytes.len() as u64, summary.bytes);

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let got = hex(&hasher.finalize());
    assert_eq!(got, ASSET_SHA256, "downloaded asset checksum mismatch");
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}
