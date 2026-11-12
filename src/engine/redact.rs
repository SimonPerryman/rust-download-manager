//! Making URLs safe to log or show in error text.

/// What stands in for each removed secret: a login's name and password, and
/// a query string.
const REDACTED: &str = "<redacted>";

/// `url` with its secrets replaced, for logs and messages: signed URLs and
/// `user:pass@` carry secrets that must not leak into bug reports. A login
/// shows as `<redacted>:<redacted>@` (or `<redacted>@` without a password),
/// so a report still says one was used; a query becomes `?<redacted>`, and a
/// fragment is dropped. The scheme, host and path stay. Unparsable input
/// yields `<invalid url>`.
pub fn redact_url(url: &str) -> String {
    scrub(url, true).unwrap_or_else(|| "<invalid url>".to_string())
}

/// `url` with its login, query and fragment removed outright rather than
/// marked, for where it's recorded as a file's origin (Mark-of-the-Web). The
/// same rule as [`redact_url`], nested links included. `None` if unparsable.
pub(super) fn strip_url_secrets(url: &str) -> Option<String> {
    scrub(url, false)
}

/// [`redact_url`]'s work: with `mark`, each removed secret leaves
/// `<redacted>` behind; without, it's simply gone.
fn scrub(url: &str, mark: bool) -> Option<String> {
    // A link inside this one (an archive's copy of another URL) keeps `:` and
    // `@` in the path, so hide its login too: first in the raw text, since
    // once parsed a `?` or `#` in its password would start this URL's query
    // or fragment and split the login in two.
    let raw = match url.find("://") {
        Some(sep) if url[sep + 3..].contains("://") => {
            match hide_nested_logins(&url[sep + 3..], mark) {
                Ok(rest) => format!("{}{rest}", &url[..sep + 3]),
                // A `?` or `#` before the nested `@` may be this URL's query or
                // fragment rather than a password: keep only what comes before
                // that link's login, and say the rest was redacted. (No deeper:
                // the cut text holds no such link.)
                Err(cut) => {
                    let mut out = scrub(&url[..sep + 3 + cut], mark)?;
                    if mark && !out.ends_with(REDACTED) {
                        out.push('?');
                        out.push_str(REDACTED);
                    }
                    return Some(out);
                }
            }
        }
        _ => url.to_owned(),
    };
    let mut parsed = url::Url::parse(&raw).ok()?;
    let had_query = parsed.query().is_some();
    let login = match (parsed.username().is_empty(), parsed.password().is_some()) {
        _ if !mark => String::new(),
        (_, true) => format!("{REDACTED}:{REDACTED}@"),
        (false, false) => format!("{REDACTED}@"),
        (true, false) => String::new(),
    };
    // Both fail only for URLs that have no authority, hence no userinfo.
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    let mut out = parsed.to_string();
    // Again once parsed, which writes the hidden login back as `<redacted>`
    // (the url crate percent-encodes its brackets).
    let path_start = out.len() - parsed.path().len();
    if out[path_start..].contains("://") {
        // A parsed path has no `?` or `#`, so this never cuts.
        if let Ok(path) = hide_nested_logins(&out[path_start..], mark) {
            out.truncate(path_start);
            out.push_str(&path);
        }
    }
    // Spliced in as text: the url crate would percent-encode the brackets.
    if let Some(sep) = out.find("://").filter(|_| !login.is_empty()) {
        out.insert_str(sep + 3, &login);
    }
    if mark && had_query {
        out.push('?');
        out.push_str(REDACTED);
    }
    Some(out)
}

/// `path` (the part of a URL after its own `://`) with the login of every
/// link inside it replaced as [`redact_url`] does (or dropped, without
/// `mark`): the text after each `://`, up to the next `/`, loses everything
/// before its last `@`. A flat
/// scan rather than a parse, so any characters a login holds are covered and
/// no depth of nesting can recurse. `Err` holds where such a link starts
/// when its login text has a `?` or `#`, which may instead be the outer
/// URL's query or fragment.
fn hide_nested_logins(path: &str, mark: bool) -> Result<String, usize> {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(sep) = rest.find("://") {
        let auth_start = sep + 3;
        out.push_str(&rest[..auth_start]);
        let after = &rest[auth_start..];
        let authority = &after[..after.find('/').unwrap_or(after.len())];
        match authority.rfind('@') {
            Some(at) if authority[..at].contains(['?', '#']) => {
                return Err(path.len() - after.len());
            }
            Some(at) => {
                let login = if !mark {
                    String::new()
                } else if authority[..at].contains(':') {
                    format!("{REDACTED}:{REDACTED}@")
                } else {
                    format!("{REDACTED}@")
                };
                out.push_str(&login);
                rest = &after[at + 1..];
            }
            None => rest = after,
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// `text` with every URL in it (`scheme://…`) passed through [`redact_url`],
/// for text gathered from logs and error messages that may not have been
/// redacted already, such as a bug report's diagnostics. Text without URLs
/// comes back unchanged, and so does an already-redacted URL.
pub fn redact_urls_in(text: &str) -> String {
    map_urls_in(text, redact_url)
}

/// `text` with every URL in it (`scheme://…`, found as [`redact_urls_in`]
/// finds them) replaced by `map(url)`; the rest is kept. Lets a frontend
/// hide more of a link than [`redact_url`] does.
pub fn map_urls_in(text: &str, mut map: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(sep) = rest.find("://") {
        let start = scheme_start(&rest[..sep]);
        let len = url_len(&rest[start..]);
        out.push_str(&rest[..start]);
        let url = &rest[start..start + len];
        if start == sep {
            out.push_str(url); // "://" with no scheme
        } else {
            out.push_str(&map(url));
        }
        rest = &rest[start + len..];
    }
    out.push_str(rest);
    out
}

/// Where the URL scheme ending `before` starts: a letter followed by letters,
/// digits, `+`, `-` or `.` (`before.len()` when there's none).
fn scheme_start(before: &str) -> usize {
    let bytes = before.as_bytes();
    let mut start = bytes.len();
    while start > 0
        && matches!(bytes[start - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'+' | b'-' | b'.')
    {
        start -= 1;
    }
    while start < bytes.len() && !bytes[start].is_ascii_alphabetic() {
        start += 1;
    }
    start
}

/// How many bytes of `text` (starting at a scheme) belong to the URL: up to
/// whitespace, a quote or an angle bracket (except in a `<redacted>` that
/// [`redact_url`] wrote), or a closing bracket it didn't open, less trailing
/// punctuation that ends a sentence. A login (up to the authority's last `@`)
/// is taken whole, so a password holding `'`, `)` or `]` isn't cut short and
/// left as plain text; the authority ends at a `/`, whitespace, `"`, a
/// backtick or an angle bracket, so a URL in JSON or HTML doesn't run on
/// into a later `name@host`.
fn url_len(text: &str) -> usize {
    let mut depth = [0usize; 3];
    let mut end = text.len();
    let mut skip_to = 0;
    for (i, c) in text.char_indices() {
        if i < skip_to {
            continue;
        }
        // At each `://` (the URL's own, or a link nested in its path).
        if c == ':' && text[i..].starts_with("://") {
            let after = &text[i + 3..];
            let authority = &after[..after
                .find(|c: char| c.is_whitespace() || matches!(c, '/' | '"' | '`' | '<' | '>'))
                .unwrap_or(after.len())];
            skip_to = authority.rfind('@').map_or(i + 3, |at| i + 3 + at + 1);
            continue;
        }
        if c == '<' && text[i..].starts_with(REDACTED) {
            skip_to = i + REDACTED.len();
            continue;
        }
        let closes = |open: usize, depth: &mut [usize; 3]| {
            let unmatched = depth[open] == 0;
            depth[open] = depth[open].saturating_sub(1);
            unmatched
        };
        let stop = match c {
            c if c.is_whitespace() => true,
            '"' | '\'' | '`' | '<' | '>' => true,
            '(' | '[' | '{' => {
                depth[usize::from(c == '[') + 2 * usize::from(c == '{')] += 1;
                false
            }
            ')' => closes(0, &mut depth),
            ']' => closes(1, &mut depth),
            '}' => closes(2, &mut depth),
            _ => false,
        };
        if stop {
            end = i;
            break;
        }
    }
    text[..end]
        .trim_end_matches(['.', ',', ';', ':', '!'])
        .len()
}

#[cfg(test)]
mod tests {
    use super::{redact_url, redact_urls_in, strip_url_secrets};

    /// Every URL in a text is redacted, wherever it sits; the rest is kept.
    #[test]
    fn redacts_every_url_in_a_text() {
        let text = "Started https://user:pw@host/p?token=SECRET#frag now\n\
                    bare http://h/x?sig=abc, then \"https://q.example/a?k=v\" and \
                    (see http://h/y?sig=abc).\n\
                    reqwest: error for url (http://h/z?x=1): refused";
        let out = redact_urls_in(text);
        for secret in ["SECRET", "sig=abc", "pw", "#frag", "k=v", "x=1"] {
            assert!(!out.contains(secret), "survived: {out}");
        }
        assert_eq!(
            out,
            "Started https://<redacted>:<redacted>@host/p?<redacted> now\n\
             bare http://h/x?<redacted>, then \"https://q.example/a?<redacted>\" and \
             (see http://h/y?<redacted>).\n\
             reqwest: error for url (http://h/z?<redacted>): refused"
        );
    }

    #[test]
    fn text_without_urls_is_unchanged() {
        let text = "Lane 2 lost connection — retrying (attempt 2)\nC:\\dl\\a.iso: done";
        assert_eq!(redact_urls_in(text), text);
        assert_eq!(redact_urls_in("odd ://thing"), "odd ://thing");
    }

    /// An already-redacted URL stays as it is, so redacting twice is harmless.
    #[test]
    fn redacting_is_idempotent() {
        for (text, once, url) in [
            (
                "GET https://h/p?token=SECRET done",
                "GET https://h/p?<redacted> done",
                "https://h/p?<redacted>",
            ),
            (
                "GET https://u:pw@h/p?token=SECRET done",
                "GET https://<redacted>:<redacted>@h/p?<redacted> done",
                "https://<redacted>:<redacted>@h/p?<redacted>",
            ),
            (
                "GET https://u@h/p done",
                "GET https://<redacted>@h/p done",
                "https://<redacted>@h/p",
            ),
        ] {
            assert_eq!(redact_urls_in(text), once);
            assert_eq!(redact_urls_in(once), once);
            assert_eq!(redact_url(url), url);
        }
    }

    /// A URL carried in another's path loses its login too.
    #[test]
    fn redacts_a_url_inside_a_path() {
        for (url, want) in [
            (
                "https://web.archive.org/web/2020/https://u:pw@h/f?t=S",
                "https://web.archive.org/web/2020/https://<redacted>:<redacted>@h/f?<redacted>",
            ),
            (
                "https://a/x,https://u:pw@b/",
                "https://a/x,https://<redacted>:<redacted>@b/",
            ),
        ] {
            let once = redact_url(url);
            assert_eq!(once, want);
            assert_eq!(redact_url(&once), once);
            assert_eq!(
                redact_urls_in(&format!("see {url} now")),
                format!("see {want} now")
            );
        }
        let joined = redact_urls_in("https://a/<redacted>https://u:pw@h/");
        assert!(!joined.contains("pw"), "{joined}");
    }

    /// A password holding a quote or bracket is hidden whole, nested or not.
    #[test]
    fn odd_characters_in_a_password_are_hidden_too() {
        for text in [
            "https://a/https://u:)secretpw@h/",
            "https://a/https://u:p'secretw@h/",
            "see https://u:)secretpw@h/f now",
            "see (https://u:p'secretw@h/f) now",
        ] {
            let out = redact_urls_in(text);
            assert!(!out.contains("secret"), "{text} -> {out}");
            assert!(out.contains("<redacted>:<redacted>@h/"), "{text} -> {out}");
        }
    }

    /// A `?` or `#` before a nested link's `@` might be a password or the
    /// outer URL's query: either way, nothing after it shows.
    #[test]
    fn an_ambiguous_nested_login_redacts_the_rest() {
        for (url, want) in [
            (
                "https://a/https://u:p?secretw@h/",
                "https://a/https://?<redacted>",
            ),
            (
                "https://a/https://u:p#secretw@h/",
                "https://a/https://?<redacted>",
            ),
            (
                "https://web.archive.org/web/2020/https://site.com?email=bob@x.com&sig=SECRET",
                "https://web.archive.org/web/2020/https://?<redacted>",
            ),
            (
                "https://a/https://h#x@y&tok=SECRET",
                "https://a/https://?<redacted>",
            ),
            (
                "https://u:pw@a/p?next=https://x?e=bob@y&sig=SECRET",
                "https://<redacted>:<redacted>@a/p?<redacted>",
            ),
        ] {
            assert_eq!(redact_url(url), want, "{url}");
            assert_eq!(
                redact_urls_in(&format!("see {url} now")),
                format!("see {want} now")
            );
            assert_eq!(redact_url(want), want);
        }
    }

    /// However deep links nest, redacting one is a flat pass, not recursion.
    #[test]
    fn deep_nesting_does_not_recurse() {
        let url = format!("https://h/{}u:pw@x/", "x://".repeat(50_000));
        let out = redact_url(&url);
        assert!(out.starts_with("https://h/x://x://"), "{}", &out[..40]);
        assert!(out.ends_with("x://<redacted>:<redacted>@x/"));
        assert_eq!(redact_urls_in(&out), out);
    }

    /// A URL in JSON or HTML ends at its quote, not at a later `name@host`.
    #[test]
    fn a_url_in_markup_keeps_its_own_host() {
        assert_eq!(
            redact_urls_in(r#"{"url":"https://h/f?t=S","email":"bob@x.com"}"#),
            r#"{"url":"https://h/f?<redacted>","email":"bob@x.com"}"#
        );
        assert_eq!(
            redact_urls_in("<a href='https://h'>bob@x.com</a>"),
            "<a href='https://h/'>bob@x.com</a>"
        );
        assert_eq!(
            redact_urls_in("go https://h\u{a0}bob@x.com"),
            "go https://h/\u{a0}bob@x.com"
        );
    }

    #[test]
    fn a_redacted_looking_url_still_loses_its_secrets() {
        assert_eq!(
            redact_urls_in("x https://u:pw@h/p?<redacted> y"),
            "x https://<redacted>:<redacted>@h/p?<redacted> y"
        );
        assert_eq!(
            redact_urls_in("x https://h/p?token=S?<redacted> y"),
            "x https://h/p?<redacted> y"
        );
    }

    /// Text that isn't ASCII next to a URL neither panics nor survives redaction.
    #[test]
    fn non_ascii_text_around_a_url_is_kept() {
        assert_eq!(
            redact_urls_in("é—https://h/p?t=S—next"),
            "é—https://h/p?<redacted>"
        );
        assert_eq!(
            redact_urls_in("ü https://h/ä?t=S ö"),
            "ü https://h/%C3%A4?<redacted> ö"
        );
    }

    #[test]
    fn brackets_inside_a_url_stay_with_it() {
        assert_eq!(
            redact_urls_in("[link](https://w.example/A_(b)?s=1)"),
            "[link](https://w.example/A_(b)?<redacted>)"
        );
    }

    /// A login is replaced, not dropped, so a report still shows one was used.
    #[test]
    fn marks_a_login_without_its_secrets() {
        assert_eq!(
            redact_url("https://user:pw@example.com/files/report.pdf?token=abc#f"),
            "https://<redacted>:<redacted>@example.com/files/report.pdf?<redacted>"
        );
        assert_eq!(
            redact_url("https://alice:hunter2@example.com/f.zip"),
            "https://<redacted>:<redacted>@example.com/f.zip"
        );
        // A user name alone.
        assert_eq!(
            redact_url("https://alice@example.com/f.zip"),
            "https://<redacted>@example.com/f.zip"
        );
        // A password with no user name still shows there was a login.
        assert_eq!(
            redact_url("https://:hunter2@example.com/f.zip"),
            "https://<redacted>:<redacted>@example.com/f.zip"
        );
        assert_eq!(
            redact_url("http://bob:pw@127.0.0.1:8080/x"),
            "http://<redacted>:<redacted>@127.0.0.1:8080/x"
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
            "ftp://<redacted>:<redacted>@files.example.com/pub/x.tar?<redacted>"
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

    /// Stripping removes the same secrets redaction hides, leaving no marker.
    #[test]
    fn stripping_removes_logins_queries_and_fragments() {
        for (url, want) in [
            ("https://u:pw@h/f.zip?t=S#x", "https://h/f.zip"),
            ("https://u@h/f.zip", "https://h/f.zip"),
            ("http://h:8080/a/b", "http://h:8080/a/b"),
            (
                "https://web.archive.org/web/2020/https://u:pw@h/f?t=S",
                "https://web.archive.org/web/2020/https://h/f",
            ),
            ("https://a/https://u:p?secretw@h/", "https://a/https://"),
        ] {
            assert_eq!(strip_url_secrets(url).as_deref(), Some(want), "{url}");
        }
        assert_eq!(strip_url_secrets("not a url"), None);
    }
}
