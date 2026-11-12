//! The proxy a download's requests go through.

use std::fmt;
use std::str::FromStr;

use super::redact_url;

/// The proxy schemes a download can go through; `socks5h` resolves host
/// names through the proxy instead of locally.
const SCHEMES: [&str; 4] = ["http", "https", "socks5", "socks5h"];

/// The port a SOCKS proxy URL without one gets.
const SOCKS_PORT: u16 = 1080;

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
        // Not naming what was taken for the scheme: with the scheme left off,
        // as in `user:pass@host:1080`, that's the user name.
        if !SCHEMES.contains(&url.scheme()) {
            return Err(format!(
                "a proxy URL starts with http://, https://, socks5:// or socks5h://; {EXAMPLE}"
            ));
        }
        // `socks5` URLs are parsed loosely (opaque host), but the HTTP client
        // re-parses the proxy and, if that fails, quietly connects directly:
        // so refuse any host it couldn't use.
        let usable = match url.host() {
            Some(url::Host::Domain(name)) => {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b))
            }
            Some(_) => true,
            None => false,
        };
        if !usable {
            return Err(format!(
                "the proxy URL needs a host name (in ASCII) or IP address; {EXAMPLE}"
            ));
        }
        let mut url = url;
        if url.scheme().starts_with("socks") && url.port().is_none() {
            // SOCKS's usual port, written out so the saved URL says which.
            let _ = url.set_port(Some(SOCKS_PORT));
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
            ("ftp://host:21", "starts with http://"),
            ("socks4://host:1080", "starts with http://"),
            ("localhost:8080", "starts with http://"),
            ("socks5:host", "needs a host name"),
            ("socks5h://localhost}:9050", "needs a host name"),
            ("socks5h://ho%7Bst:1080", "needs a host name"),
            ("socks5h://прокси.lan:1080", "needs a host name"),
            ("http://:8080", "not a valid proxy URL"),
        ] {
            let err = url.parse::<Proxy>().unwrap_err();
            assert!(err.contains(says), "{url}: {err}");
        }
    }

    #[test]
    fn a_socks_proxy_without_a_port_gets_1080() {
        let proxy: Proxy = "socks5h://127.0.0.1".parse().unwrap();
        assert_eq!(proxy.as_str(), "socks5h://127.0.0.1:1080");
        let proxy: Proxy = "socks5://[::1]:9050".parse().unwrap();
        assert_eq!(proxy.as_str(), "socks5://[::1]:9050");
        let proxy: Proxy = "http://proxy.example".parse().unwrap();
        assert_eq!(proxy.as_str(), "http://proxy.example/");
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

        for url in [
            "ftp://alice:hunter2@host",
            "alice:hunter2@host:1080",
            "http://alice:hunter2@:1080",
        ] {
            let err = url.parse::<Proxy>().unwrap_err();
            assert!(!err.contains("alice"), "{err}");
            assert!(!err.contains("hunter2"), "{err}");
        }
    }
}
