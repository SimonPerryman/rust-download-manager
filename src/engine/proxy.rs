//! The proxy a download's requests go through.

use std::fmt;
use std::str::FromStr;

use super::redact_url;

/// The proxy schemes a download can go through; `socks5h` resolves host
/// names through the proxy instead of locally.
const SCHEMES: [&str; 4] = ["http", "https", "socks5", "socks5h"];

/// An HTTP, HTTPS or SOCKS5 proxy for a download's requests, e.g.
/// `socks5h://user:pass@127.0.0.1:1080`. It shows (`Display`, `Debug`) only
/// redacted, so its credentials never reach logs or error text.
#[derive(Clone, PartialEq, Eq)]
pub struct Proxy(url::Url);

impl Proxy {
    /// The full URL, credentials included: for handing to an HTTP client or
    /// saving, never for showing.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl FromStr for Proxy {
    type Err = String;

    /// Parse and check a proxy URL. The error never repeats the input, which
    /// may hold a password.
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        const EXAMPLE: &str = "expected e.g. http://host:8080 or socks5://host:1080";
        let url = url::Url::parse(s.trim())
            .map_err(|e| format!("not a valid proxy URL ({e}); {EXAMPLE}"))?;
        if !SCHEMES.contains(&url.scheme()) {
            return Err(format!(
                "unsupported proxy scheme '{}' (use http, https, socks5 or socks5h); {EXAMPLE}",
                url.scheme()
            ));
        }
        if url.host_str().is_none_or(str::is_empty) {
            return Err(format!("the proxy URL has no host; {EXAMPLE}"));
        }
        Ok(Self(url))
    }
}

impl fmt::Display for Proxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&redact_url(self.0.as_str()))
    }
}

impl fmt::Debug for Proxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Proxy({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_https_and_socks5_with_or_without_a_login() {
        for url in [
            "http://127.0.0.1:8080",
            "https://proxy.example.com",
            "socks5://127.0.0.1:1080",
            "socks5h://user:pass@proxy.example.com:1080",
            "  http://user:pass@[::1]:3128  ",
        ] {
            assert!(url.parse::<Proxy>().is_ok(), "{url}");
        }
    }

    #[test]
    fn rejects_what_isnt_a_usable_proxy_url() {
        for (url, says) in [
            ("not-a-url", "not a valid proxy URL"),
            ("", "not a valid proxy URL"),
            ("ftp://host:21", "unsupported proxy scheme 'ftp'"),
            ("socks4://host:1080", "unsupported proxy scheme 'socks4'"),
            ("localhost:8080", "unsupported proxy scheme 'localhost'"),
            ("socks5:host", "has no host"),
            ("http://:8080", "not a valid proxy URL"),
        ] {
            let err = url.parse::<Proxy>().unwrap_err();
            assert!(err.contains(says), "{url}: {err}");
        }
    }

    #[test]
    fn never_shows_its_credentials() {
        let proxy: Proxy = "socks5h://alice:hunter2@proxy.example.com:1080"
            .parse()
            .unwrap();
        let shown = format!("{proxy} {proxy:?}");
        assert!(!shown.contains("alice"), "{shown}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(
            shown.contains("socks5h://<redacted>:<redacted>@proxy.example.com:1080"),
            "{shown}"
        );
        assert_eq!(
            proxy.as_str(),
            "socks5h://alice:hunter2@proxy.example.com:1080"
        );

        let err = "ftp://alice:hunter2@host".parse::<Proxy>().unwrap_err();
        assert!(!err.contains("hunter2"), "{err}");
    }
}
