//! The Proxy setting as the window sees it: Use system proxy, No proxy or a
//! custom proxy. A custom proxy's password never goes to the webview, only a
//! mask, and a mask sent back for the same proxy and user means "keep the
//! saved password".

use rdm::{Proxy, ProxyMode};

/// The Proxy setting as the window shows and sends it, e.g.
/// `{"mode":"custom","url":"socks5h://host:1080"}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub(crate) enum ProxyChoice {
    System,
    None,
    /// The URL as the field holds it: masked when shown, typed when sent.
    Custom {
        url: String,
    },
}

/// `mode` as the window shows it, a custom proxy's password masked.
pub(crate) fn shown(mode: &ProxyMode) -> ProxyChoice {
    match mode {
        ProxyMode::System => ProxyChoice::System,
        ProxyMode::None => ProxyChoice::None,
        ProxyMode::Custom(proxy) => ProxyChoice::Custom { url: masked(proxy) },
    }
}

/// The mode to save for what the window sent. A custom URL is checked, and
/// may keep the `saved` custom proxy's password (see [`resolve_url`]).
pub(crate) fn resolve(choice: ProxyChoice, saved: &ProxyMode) -> Result<ProxyMode, String> {
    let saved = match saved {
        ProxyMode::Custom(proxy) => Some(proxy),
        _ => None,
    };
    Ok(match choice {
        ProxyChoice::System => ProxyMode::System,
        ProxyChoice::None => ProxyMode::None,
        ProxyChoice::Custom { url } => ProxyMode::Custom(resolve_url(&url, saved)?),
    })
}

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

/// Why a blank custom proxy can't be saved.
const BLANK: &str = "Enter the proxy's URL, e.g. socks5h://host:1080.";

/// The custom proxy to save for what the URL field holds. A mask in the
/// password's place keeps the saved password, but only for the saved proxy
/// and user, so a changed host never receives it. The error is the field's
/// inline message, and never repeats the text.
fn resolve_url(text: &str, saved: Option<&Proxy>) -> Result<Proxy, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(BLANK.to_owned());
    }
    if !text.contains('•') {
        return parse(text);
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
        Ok(proxy)
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
            resolve_url(&masked(&saved), Some(&saved)),
            Ok(saved.clone())
        );
        // A newly typed password replaces it, wherever the proxy is.
        let retyped = resolve_url("socks5h://alice:s3cret@10.0.0.2:1080", Some(&saved)).unwrap();
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
                resolve_url(&text, Some(&saved)),
                Err(RETYPE.to_owned()),
                "{text}"
            );
        }
    }

    #[test]
    fn a_mask_with_no_saved_password_asks_for_it() {
        let text = format!("http://alice:{PASSWORD_MASK}@proxy.example:3128");
        assert_eq!(resolve_url(&text, None), Err(RETYPE.to_owned()));
        let no_password = proxy("http://alice@proxy.example:3128");
        assert_eq!(
            resolve_url(&text, Some(&no_password)),
            Err(RETYPE.to_owned())
        );
    }

    #[test]
    fn blank_and_invalid_are_refused_without_echoing() {
        assert_eq!(resolve_url("  ", None), Err(BLANK.to_owned()));
        let err = resolve_url("ftp://alice:hunter2@host", None).unwrap_err();
        assert!(err.starts_with("Invalid proxy: "), "{err}");
        assert!(!err.contains("hunter2"), "{err}");
    }

    #[test]
    fn the_window_sees_and_sends_each_mode() {
        let custom = ProxyMode::Custom(proxy("socks5h://alice:hunter2@127.0.0.1:1080"));
        assert_eq!(
            serde_json::to_value(shown(&ProxyMode::System)).unwrap(),
            serde_json::json!({ "mode": "system" })
        );
        assert_eq!(
            serde_json::to_value(shown(&ProxyMode::None)).unwrap(),
            serde_json::json!({ "mode": "none" })
        );
        assert_eq!(
            serde_json::to_value(shown(&custom)).unwrap(),
            serde_json::json!({
                "mode": "custom",
                "url": format!("socks5h://alice:{PASSWORD_MASK}@127.0.0.1:1080"),
            })
        );

        let sent = |json| serde_json::from_value::<ProxyChoice>(json).unwrap();
        assert_eq!(
            resolve(sent(serde_json::json!({ "mode": "system" })), &custom),
            Ok(ProxyMode::System)
        );
        assert_eq!(
            resolve(sent(serde_json::json!({ "mode": "none" })), &custom),
            Ok(ProxyMode::None)
        );
        // The masked custom proxy sent back unchanged keeps its password.
        assert_eq!(resolve(shown(&custom), &custom), Ok(custom.clone()));
        // With no saved custom proxy, a mask can't stand in for a password.
        assert_eq!(
            resolve(shown(&custom), &ProxyMode::System),
            Err(RETYPE.to_owned())
        );
        assert_eq!(
            resolve(ProxyChoice::Custom { url: " ".into() }, &custom),
            Err(BLANK.to_owned())
        );
    }
}
