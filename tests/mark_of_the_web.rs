//! A finished download carries Mark-of-the-Web on Windows, as a browser's
//! does. Other OSes compile this file out.

#![cfg(windows)]

mod common;

use rdm::{download, Control, DownloadRequest};

/// The `Zone.Identifier` stream names the Internet zone and the URL, without
/// its login or query string.
#[tokio::test]
async fn a_finished_download_is_marked_as_from_the_internet() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let url = server
        .url("/file?token=SECRET")
        .replacen("://", "://user:pw@", 1);

    let req = DownloadRequest::to_path(url, dir.path().join("file.bin"));
    let summary = download(&req, &Control::new(), |_| {}).await.unwrap();

    let stream = format!("{}:Zone.Identifier", summary.path.display());
    let zone = std::fs::read_to_string(stream).expect("the file is marked");
    assert!(zone.starts_with("[ZoneTransfer]\r\n"), "{zone}");
    assert!(zone.lines().any(|l| l == "ZoneId=3"), "{zone}");
    let host_url = zone.lines().find_map(|l| l.strip_prefix("HostUrl="));
    assert_eq!(host_url, Some(server.url("/file").as_str()), "{zone}");
}
