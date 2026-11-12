//! The Proxy setting as the window sees it: the saved proxy's password never
//! goes to the webview, only a mask, and a mask sent back for the same proxy
//! and user means "keep the saved password".

use rdm::Proxy;

/// What the field shows in place of a saved password.
pub(crate) const PASSWORD_MASK: &str = "••••••";

/// `proxy` as the field shows it: its password replaced by [`PASSWORD_MASK`],
/// and without the bare `/` path the URL parser adds to `http(s)://host`.
pub(crate) fn masked(proxy: &Proxy) -> String {
    let url = proxy.as_str();
    let url = match authority(url) {
        Some((_, end)) if &url[end..] == "/" => &url[..end],
        _ => url,
    };
    match password_span(url) {
        Some((start, end)) => format!("{}{PASSWORD_MASK}{}", &url[..start], &url[end..]),
        None => url.to_owned(),
    }
}

/// Why a masked password can't be kept.
const RETYPE: &str =
    "Type the proxy's password again: the saved one is kept only for the same proxy and user.";

/// The proxy to save for what the field holds: `None` for blank. A mask in
/// the password's place keeps the saved password, but only for the saved
/// proxy and user, so a changed host never receives it. The error is the
/// field's inline message, and never repeats the text.
pub(crate) fn resolve(text: &str, saved: Option<&Proxy>) -> Result<Option<Proxy>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if !text.contains('•') {
        return parse(text).map(Some);
    }
    let saved = saved.ok_or(RETYPE)?;
    let (start, end) = password_span(text)
        .filter(|&(start, end)| &text[start..end] == PASSWORD_MASK)
        .ok_or(RETYPE)?;
    let (saved_start, saved_end) = password_span(saved.as_str()).ok_or(RETYPE)?;
    let kept = format!(
        "{}{}{}",
        &text[..start],
        &saved.as_str()[saved_start..saved_end],
        &text[end..]
    );
    let proxy = parse(&kept)?;
    if masked(&proxy) == masked(saved) {
        Ok(Some(proxy))
    } else {
        Err(RETYPE.to_owned())
    }
}

fn parse(text: &str) -> Result<Proxy, String> {
    text.parse()
        .map_err(|e: String| format!("Invalid proxy: {e}"))
}

/// Where the password sits in a normalised URL (`scheme://user:pass@host…`).
fn password_span(url: &str) -> Option<(usize, usize)> {
    let (start, end) = authority(url)?;
    let at = url[start..end].rfind('@')?;
    let colon = url[start..start + at].find(':')?;
    Some((start + colon + 1, start + at))
}

/// Where the authority (`user:pass@host:port`) sits in `url`.
fn authority(url: &str) -> Option<(usize, usize)> {
    let start = url.find("://")? + 3;
    let len = url[start..]
        .find(['/', '?', '#'])
        .unwrap_or(url.len() - start);
    Some((start, start + len))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy(url: &str) -> Proxy {
        url.parse().unwrap()
    }

    #[test]
    fn the_field_never_gets_the_password() {
        assert_eq!(
            masked(&proxy("socks5h://alice:hunter2@127.0.0.1:1080")),
            format!("socks5h://alice:{PASSWORD_MASK}@127.0.0.1:1080")
        );
        assert_eq!(
            masked(&proxy("http://alice@proxy.example:3128")),
            "http://alice@proxy.example:3128"
        );
        assert_eq!(
            masked(&proxy("http://proxy.example:3128")),
            "http://proxy.example:3128"
        );
    }

    #[test]
    fn a_masked_password_keeps_the_saved_one() {
        let saved = proxy("socks5h://alice:hunter2@127.0.0.1:1080");
        assert_eq!(
            resolve(&masked(&saved), Some(&saved)),
            Ok(Some(saved.clone()))
        );
        // A newly typed password replaces it, wherever the proxy is.
        let retyped = resolve("socks5h://alice:s3cret@10.0.0.2:1080", Some(&saved))
            .unwrap()
            .unwrap();
        assert_eq!(retyped.as_str(), "socks5h://alice:s3cret@10.0.0.2:1080");
    }

    #[test]
    fn the_saved_password_never_goes_to_another_proxy_or_user() {
        let saved = proxy("http://alice:hunter2@corp.example:3128");
        for text in [
            // Another host, port, scheme or user.
            format!("http://alice:{PASSWORD_MASK}@attacker.example:3128"),
            format!("http://alice:{PASSWORD_MASK}@corp.example:8080"),
            format!("socks5://alice:{PASSWORD_MASK}@corp.example:3128"),
            format!("http://bob:{PASSWORD_MASK}@corp.example:3128"),
            // The mask anywhere but whole in the password's place.
            format!("http://{PASSWORD_MASK}@corp.example:3128"),
            format!("http://alice:{PASSWORD_MASK}x@corp.example:3128"),
            "http://alice:•••••@corp.example:3128".to_owned(),
            format!("http://alice:pw@{PASSWORD_MASK}:3128"),
        ] {
            assert_eq!(
                resolve(&text, Some(&saved)),
                Err(RETYPE.to_owned()),
                "{text}"
            );
        }
    }

    #[test]
    fn a_mask_with_no_saved_password_asks_for_it() {
        let text = format!("http://alice:{PASSWORD_MASK}@proxy.example:3128");
        assert_eq!(resolve(&text, None), Err(RETYPE.to_owned()));
        let no_password = proxy("http://alice@proxy.example:3128");
        assert_eq!(resolve(&text, Some(&no_password)), Err(RETYPE.to_owned()));
    }

    #[test]
    fn blank_clears_and_invalid_is_refused_without_echoing() {
        assert_eq!(resolve("  ", None), Ok(None));
        let err = resolve("ftp://alice:hunter2@host", None).unwrap_err();
        assert!(err.starts_with("Invalid proxy: "), "{err}");
        assert!(!err.contains("hunter2"), "{err}");
    }
}
