//! Downloads through an HTTP or SOCKS5 proxy, against the in-process fixture
//! and the in-process proxies in `common::proxy`.

mod common;

use common::proxy::{spawn_http_proxy, spawn_socks5_proxy, Seen};
use rdm::{download, Control, DownloadRequest, Proxy};

/// A download through an HTTP proxy completes, and the proxy relayed it with
/// the login from the proxy URL as `Proxy-Authorization`.
#[tokio::test]
async fn downloads_through_an_http_proxy() {
    let server = common::spawn().await;
    let proxy = spawn_http_proxy().await;
    let dir = tempfile::tempdir().unwrap();
    let url = server.url("/file");
    let via: Proxy = proxy.url("http", Some("user:pw")).parse().unwrap();

    let req = DownloadRequest::to_path(&url, dir.path().join("f.bin")).with_proxy(via);
    download(&req, &Control::new(), |_| {}).await.unwrap();

    assert_eq!(
        std::fs::read(dir.path().join("f.bin")).unwrap(),
        common::payload()
    );
    assert_eq!(
        proxy.seen(),
        [Seen {
            target: format!("GET {url} HTTP/1.1"),
            // base64("user:pw")
            login: Some("Basic dXNlcjpwdw==".into()),
        }]
    );
}

/// A download through a SOCKS5 proxy completes, whether the proxy or this
/// machine resolves the host (`socks5h` / `socks5`), with or without a login.
#[tokio::test]
async fn downloads_through_a_socks5_proxy() {
    let server = common::spawn().await;
    let target = server.url("/file");
    let authority = target
        .trim_start_matches("http://")
        .trim_end_matches("/file")
        .to_owned();
    for (scheme, login) in [("socks5h", Some("user:pw")), ("socks5", None)] {
        let proxy = spawn_socks5_proxy().await;
        let dir = tempfile::tempdir().unwrap();
        let via: Proxy = proxy.url(scheme, login).parse().unwrap();

        let req = DownloadRequest::to_path(&target, dir.path().join("f.bin")).with_proxy(via);
        download(&req, &Control::new(), |_| {}).await.unwrap();

        assert_eq!(
            std::fs::read(dir.path().join("f.bin")).unwrap(),
            common::payload(),
            "{scheme}"
        );
        assert_eq!(
            proxy.seen(),
            [Seen {
                target: authority.clone(),
                login: login.map(str::to_owned),
            }],
            "{scheme}"
        );
    }
}
