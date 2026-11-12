//! In-process proxies for the proxy tests: an HTTP forward proxy and a SOCKS5
//! one. Each relays to whatever it's asked for (in practice the fixture
//! server) and records what it saw, so a test can prove the request went
//! through it.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A running proxy on an ephemeral loopback port.
pub struct ProxyServer {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
}

/// One request a proxy relayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    /// HTTP: the request line as the client sent it (absolute-form, or
    /// `CONNECT host:port`). SOCKS5: the `host:port` it was asked to reach.
    pub target: String,
    /// HTTP: the `Proxy-Authorization` header. SOCKS5: the `user:pass` login.
    pub login: Option<String>,
}

impl ProxyServer {
    /// `scheme://` + `login@` (when given) + this proxy's address.
    pub fn url(&self, scheme: &str, login: Option<&str>) -> String {
        let login = login.map(|l| format!("{l}@")).unwrap_or_default();
        format!("{scheme}://{login}{}", self.addr)
    }

    /// Every request relayed so far, in arrival order.
    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

/// An HTTP forward proxy: absolute-form requests are rewritten to
/// origin-form and relayed; `CONNECT` is tunnelled.
pub async fn spawn_http_proxy() -> ProxyServer {
    spawn(|client, seen| async move {
        let _ = relay_http(client, seen).await;
    })
    .await
}

/// A SOCKS5 proxy (RFC 1928) taking no login or a user/password one
/// (RFC 1929), whichever the client offers.
pub async fn spawn_socks5_proxy() -> ProxyServer {
    spawn(|client, seen| async move {
        let _ = relay_socks5(client, seen).await;
    })
    .await
}

type SeenLog = Arc<Mutex<Vec<Seen>>>;

async fn spawn<F, Fut>(handle: F) -> ProxyServer
where
    F: Fn(TcpStream, SeenLog) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = SeenLog::default();
    let log = seen.clone();
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            tokio::spawn(handle(client, log.clone()));
        }
    });
    ProxyServer { addr, seen }
}

async fn relay_http(mut client: TcpStream, seen: SeenLog) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let head_end = loop {
        let mut chunk = [0u8; 4096];
        let n = client.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default().to_owned();
    let login = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("proxy-authorization"))
        .map(|(_, value)| value.trim().to_owned());
    seen.lock().unwrap().push(Seen {
        target: request_line.clone(),
        login,
    });

    let mut parts = request_line.splitn(3, ' ');
    let (method, target, version) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    if method == "CONNECT" {
        let mut upstream = TcpStream::connect(target).await?;
        client
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await?;
        upstream.write_all(&buf[head_end..]).await?;
        tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
        return Ok(());
    }
    // `http://host:port/path` → connect to `host:port`, send `/path`.
    let rest = target.strip_prefix("http://").unwrap_or(target);
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let path = if path.is_empty() { "/" } else { path };
    let mut upstream = TcpStream::connect(authority).await?;
    let header_lines = &head[request_line.len()..];
    upstream
        .write_all(format!("{method} {path} {version}{header_lines}").as_bytes())
        .await?;
    upstream.write_all(&buf[head_end..]).await?;
    tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
    Ok(())
}

async fn relay_socks5(mut client: TcpStream, seen: SeenLog) -> std::io::Result<()> {
    const NO_AUTH: u8 = 0x00;
    const USER_PASS: u8 = 0x02;
    let [_version, n_methods] = read_array(&mut client).await?;
    let mut methods = vec![0u8; n_methods.into()];
    client.read_exact(&mut methods).await?;
    let method = if methods.contains(&USER_PASS) {
        USER_PASS
    } else {
        NO_AUTH
    };
    client.write_all(&[5, method]).await?;

    let login = if method == USER_PASS {
        let [_subversion, user_len] = read_array(&mut client).await?;
        let user = read_string(&mut client, user_len.into()).await?;
        let [pass_len] = read_array(&mut client).await?;
        let pass = read_string(&mut client, pass_len.into()).await?;
        client.write_all(&[1, 0]).await?;
        Some(format!("{user}:{pass}"))
    } else {
        None
    };

    let [_version, _connect, _reserved, address_type] = read_array(&mut client).await?;
    let host = match address_type {
        1 => std::net::Ipv4Addr::from(read_array::<4>(&mut client).await?).to_string(),
        4 => format!(
            "[{}]",
            std::net::Ipv6Addr::from(read_array::<16>(&mut client).await?)
        ),
        _ => {
            let [len] = read_array(&mut client).await?;
            read_string(&mut client, len.into()).await?
        }
    };
    let port = u16::from_be_bytes(read_array(&mut client).await?);
    let target = format!("{host}:{port}");
    seen.lock().unwrap().push(Seen {
        target: target.clone(),
        login,
    });

    let mut upstream = TcpStream::connect(&target).await?;
    client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
    tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
    Ok(())
}

async fn read_array<const N: usize>(stream: &mut TcpStream) -> std::io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}

async fn read_string(stream: &mut TcpStream, len: usize) -> std::io::Result<String> {
    let mut bytes = vec![0u8; len];
    stream.read_exact(&mut bytes).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
