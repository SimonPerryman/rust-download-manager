//! Making URLs safe to log or show in error text.

/// `url` with its userinfo and fragment dropped and any query replaced by
/// `?<redacted>`, for logs and messages: signed URLs and `user:pass@` carry
/// secrets that must not leak into bug reports. Unparsable input yields
/// `<invalid url>`.
pub fn redact_url(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return "<invalid url>".to_string();
    };
    let had_query = parsed.query().is_some();
    // Both fail only for URLs that have no authority, hence no userinfo.
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    let mut out = parsed.to_string();
    if had_query {
        out.push_str("?<redacted>");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::redact_url;

    #[test]
    fn drops_userinfo() {
        assert_eq!(
            redact_url("https://alice:hunter2@example.com/f.zip"),
            "https://example.com/f.zip"
        );
        assert_eq!(
            redact_url("https://alice@example.com/f.zip"),
            "https://example.com/f.zip"
        );
    }

    #[test]
    fn replaces_query_and_drops_fragment() {
        assert_eq!(
            redact_url(
                "https://cdn.example.com:8443/a/b.iso?X-Amz-Signature=abc&token=SECRET#part"
            ),
            "https://cdn.example.com:8443/a/b.iso?<redacted>"
        );
        assert_eq!(
            redact_url("http://example.com/f?"),
            "http://example.com/f?<redacted>"
        );
    }

    #[test]
    fn keeps_plain_urls() {
        assert_eq!(
            redact_url("http://127.0.0.1:8080/dir/file.bin"),
            "http://127.0.0.1:8080/dir/file.bin"
        );
        assert_eq!(
            redact_url("https://example.com/f#frag"),
            "https://example.com/f"
        );
    }

    #[test]
    fn handles_non_http_schemes() {
        assert_eq!(
            redact_url("ftp://bob:pw@files.example.com/pub/x.tar?sig=1"),
            "ftp://files.example.com/pub/x.tar?<redacted>"
        );
        assert_eq!(
            redact_url("mailto:someone@example.com?subject=hi"),
            "mailto:someone@example.com?<redacted>"
        );
    }

    #[test]
    fn unparsable_input_is_replaced() {
        assert_eq!(redact_url(""), "<invalid url>");
        assert_eq!(redact_url("not a url ?token=SECRET"), "<invalid url>");
        assert_eq!(redact_url("http://[::1/x?token=SECRET"), "<invalid url>");
    }
}
