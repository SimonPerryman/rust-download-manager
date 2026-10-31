//! Live end-to-end smoke test against a self-hosted asset.
//!
//! This is the one test that touches the real network, so it is `#[ignore]`d:
//! it runs only on demand (`cargo test -- --ignored`) or in a dedicated CI job,
//! keeping the default `cargo test` gate fast and offline.
//!
//! The asset is a deterministic file published on this repo's GitHub Releases;
//! success is defined by its SHA-256, so a truncated or corrupt download fails.

use sha2::{Digest, Sha256};

use rdm::{download, Control, DownloadRequest};

/// Self-hosted asset on this repo's releases.
const ASSET_URL: &str =
    "https://github.com/SimonPerryman/rust-download-manager/releases/download/testdata-v1/sample.bin";

/// Known SHA-256 of `sample.bin` (hex). The asset is a deterministic 1 MiB
/// file (byte i = i mod 251) published on the `testdata-v1` release.
const ASSET_SHA256: &str = "631b84027d6b9e52b539c4e8373622d23032dfadc64d60af87339c9037e4f769";

#[tokio::test]
#[ignore = "hits the real network; run with `cargo test -- --ignored`"]
async fn live_smoke_github_release_asset() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("sample.bin");

    let req = DownloadRequest::to_path(ASSET_URL, &out);
    let summary = download(&req, &Control::new(), |_| {})
        .await
        .expect("live download should succeed");

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
