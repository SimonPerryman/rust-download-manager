//! Already-downloaded detection against the offline fixture server.

mod common;

use rdm::engine::existing::{find_existing, MatchKind, MatchMode};
use rdm::{Checksum, DownloadRequest};
use sha2::{Digest, Sha256};

fn payload_checksum() -> Checksum {
    let hex: String = Sha256::digest(common::payload())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{hex}").parse().unwrap()
}

#[tokio::test]
async fn loose_match_finds_a_same_name_same_size_file() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.bin"), common::payload()).unwrap();

    let req = DownloadRequest::new(server.url("/probe/any/a.bin"), dir.path());
    let found = find_existing(&req, MatchMode::Loose, None).await.unwrap();

    assert_eq!(found.path, dir.path().join("a.bin"));
    assert_eq!(found.kind, MatchKind::Loose);
    assert!(!found.unverified);
}

#[tokio::test]
async fn a_different_size_or_name_does_not_match() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mut short = common::payload();
    short.pop();
    std::fs::write(dir.path().join("a.bin"), short).unwrap();
    std::fs::write(dir.path().join("other.bin"), common::payload()).unwrap();

    let req = DownloadRequest::new(server.url("/probe/any/a.bin"), dir.path());
    assert!(find_existing(&req, MatchMode::Loose, None).await.is_none());
    let req = DownloadRequest::new(server.url("/probe/any/missing.bin"), dir.path());
    assert!(find_existing(&req, MatchMode::Loose, None).await.is_none());
}

#[tokio::test]
async fn an_explicit_output_path_is_checked_whatever_the_url_names_it() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("mine.bin");
    std::fs::write(&out, common::payload()).unwrap();

    let req = DownloadRequest::to_path(server.url("/probe/any/a.bin"), &out);
    assert!(find_existing(&req, MatchMode::Loose, None).await.is_some());
}

#[tokio::test]
async fn strict_needs_the_digest() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.bin"), common::payload()).unwrap();
    let url = server.url("/probe/any/a.bin");

    // The --checksum digest matches.
    let req = DownloadRequest::new(&url, dir.path()).with_checksum(payload_checksum());
    let found = find_existing(&req, MatchMode::Strict, None).await.unwrap();
    assert_eq!((found.kind, found.unverified), (MatchKind::Strict, false));

    // A recorded digest (a list entry's) matches the same way.
    let req = DownloadRequest::new(&url, dir.path());
    let recorded = payload_checksum();
    let found = find_existing(&req, MatchMode::Strict, Some(&recorded))
        .await
        .unwrap();
    assert_eq!(found.kind, MatchKind::Strict);

    // A different digest means a different file.
    let wrong: Checksum = "sha256:deadbeef".parse().unwrap();
    assert!(find_existing(&req, MatchMode::Strict, Some(&wrong))
        .await
        .is_none());

    // Nothing to compare: reported as a loose match, saying so.
    let found = find_existing(&req, MatchMode::Strict, None).await.unwrap();
    assert_eq!((found.kind, found.unverified), (MatchKind::Loose, true));
}
