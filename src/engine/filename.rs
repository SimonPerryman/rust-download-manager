//! Deriving the output filename for a download.
//!
//! Priority: an explicit output path wins; otherwise the server's
//! `Content-Disposition` filename; otherwise the last path segment of the
//! (post-redirect) URL; otherwise a `download` fallback.

use std::path::Path;

/// Resolve the bare filename (no directory) to save a download under.
pub fn resolve(
    explicit: Option<&Path>,
    final_url: &str,
    content_disposition: Option<&str>,
) -> String {
    if let Some(name) = explicit
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    {
        return name.to_string();
    }
    if let Some(name) = content_disposition.and_then(from_content_disposition) {
        let name = sanitize(&name);
        if !name.is_empty() {
            return name;
        }
    }
    if let Some(name) = from_url(final_url) {
        let name = sanitize(&name);
        if !name.is_empty() {
            return name;
        }
    }
    "download".to_string()
}

/// Pull a filename out of a `Content-Disposition` header value. Prefers the
/// RFC 5987 `filename*=` form (which carries an encoding) over plain
/// `filename=`.
fn from_content_disposition(cd: &str) -> Option<String> {
    let mut plain: Option<String> = None;
    for param in cd.split(';') {
        let param = param.trim();
        if let Some(rest) = param.strip_prefix("filename*=") {
            // filename*=UTF-8''foo%20bar.txt
            let value = rest.rsplit("''").next().unwrap_or(rest);
            return Some(percent_decode(value.trim_matches('"')));
        }
        if let Some(rest) = param.strip_prefix("filename=") {
            plain = Some(rest.trim_matches('"').to_string());
        }
    }
    plain
}

/// Last non-empty path segment of a URL, percent-decoded.
fn from_url(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw).ok()?;
    let last = url.path_segments()?.rfind(|s| !s.is_empty())?;
    Some(percent_decode(last))
}

/// Reduce a candidate to a safe bare filename: drop any directory components
/// and reject the `.`/`..` specials.
fn sanitize(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim()
        .trim_matches('"');
    if base == "." || base == ".." {
        return String::new();
    }
    base.to_string()
}

/// Minimal `%XX` percent-decoding (UTF-8). Leaves invalid escapes untouched.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn explicit_path_wins() {
        let name = resolve(
            Some(Path::new("/tmp/out/report.bin")),
            "https://example.com/other.zip",
            Some("attachment; filename=\"served.txt\""),
        );
        assert_eq!(name, "report.bin");
    }

    #[test]
    fn content_disposition_plain_filename() {
        let name = resolve(
            None,
            "https://example.com/x",
            Some("attachment; filename=\"a b.txt\""),
        );
        assert_eq!(name, "a b.txt");
    }

    #[test]
    fn content_disposition_extended_filename_is_decoded_and_preferred() {
        let cd = "attachment; filename=\"fallback.txt\"; filename*=UTF-8''na%C3%AFve%20file.txt";
        let name = resolve(None, "https://example.com/x", Some(cd));
        assert_eq!(name, "naïve file.txt");
    }

    #[test]
    fn falls_back_to_url_last_segment_decoded() {
        let name = resolve(
            None,
            "https://example.com/path/to/my%20file.iso?token=1",
            None,
        );
        assert_eq!(name, "my file.iso");
    }

    #[test]
    fn trailing_slash_url_falls_back_to_download() {
        let name = resolve(None, "https://example.com/", None);
        assert_eq!(name, "download");
    }

    #[test]
    fn path_traversal_in_header_is_stripped() {
        let name = resolve(
            None,
            "https://example.com/x",
            Some("attachment; filename=\"../../etc/passwd\""),
        );
        assert_eq!(name, "passwd");
    }
}
