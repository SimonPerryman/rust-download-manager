//! The Proxy setting as the window sees it: the saved proxy's password never
//! goes to the webview, only a mask, and a mask sent back means "keep the
//! saved password".

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

/// The proxy to save for what the field holds: `None` for blank, the saved
/// password put back where the mask is. The error is the field's inline
/// message, and never repeats the text.
pub(crate) fn resolve(text: &str, saved: Option<&Proxy>) -> Result<Option<Proxy>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let text = if text.contains(PASSWORD_MASK) {
        let password = saved
            .and_then(|p| password_span(p.as_str()).map(|(start, end)| &p.as_str()[start..end]))
            .ok_or("Type the proxy's password again.")?;
        text.replacen(PASSWORD_MASK, password, 1)
    } else {
        text.to_owned()
    };
    text.parse()
        .map(Some)
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
        // Unchanged, or with only the host edited.
        assert_eq!(
            resolve(&masked(&saved), Some(&saved)),
            Ok(Some(saved.clone()))
        );
        let moved = resolve(
            &format!("socks5h://alice:{PASSWORD_MASK}@10.0.0.2:1080"),
            Some(&saved),
        )
        .unwrap()
        .unwrap();
        assert_eq!(moved.as_str(), "socks5h://alice:hunter2@10.0.0.2:1080");
        // A newly typed password replaces it.
        let retyped = resolve("socks5h://alice:s3cret@127.0.0.1:1080", Some(&saved))
            .unwrap()
            .unwrap();
        assert_eq!(retyped.as_str(), "socks5h://alice:s3cret@127.0.0.1:1080");
    }

    #[test]
    fn a_mask_with_no_saved_password_asks_for_it() {
        let text = format!("http://alice:{PASSWORD_MASK}@proxy.example:3128");
        assert!(resolve(&text, None).unwrap_err().contains("password again"));
        let no_password = proxy("http://proxy.example:3128");
        assert!(resolve(&text, Some(&no_password)).is_err());
    }

    #[test]
    fn blank_clears_and_invalid_is_refused_without_echoing() {
        assert_eq!(resolve("  ", None), Ok(None));
        let err = resolve("ftp://alice:hunter2@host", None).unwrap_err();
        assert!(err.starts_with("Invalid proxy: "), "{err}");
        assert!(!err.contains("hunter2"), "{err}");
    }
}
