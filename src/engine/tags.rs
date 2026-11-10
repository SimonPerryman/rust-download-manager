//! The rules for a download's tags: user labels kept on its record.

use crate::engine::{Error, Result};

/// Most tags one download may carry.
pub const MAX_TAGS: usize = 16;

/// Longest tag, in characters (after trimming).
pub const MAX_TAG_LEN: usize = 32;

/// Trim `tags` and drop repeats ignoring case (the first spelling wins),
/// keeping their order. An empty or over-long tag is
/// [`Error::InvalidRequest`].
pub(crate) fn normalize(tags: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() {
            return Err(Error::InvalidRequest("a tag can't be empty".into()));
        }
        if tag.chars().count() > MAX_TAG_LEN {
            return Err(Error::InvalidRequest(format!(
                "tag \"{tag}\" is longer than {MAX_TAG_LEN} characters"
            )));
        }
        if !contains(&out, tag) {
            out.push(tag.to_string());
        }
    }
    Ok(out)
}

/// `existing` with each of `added` appended unless already there ignoring
/// case; [`Error::InvalidRequest`] if that would exceed [`MAX_TAGS`].
pub(crate) fn merged(existing: &[String], added: &[String]) -> Result<Vec<String>> {
    let mut out = existing.to_vec();
    for tag in added {
        if !contains(&out, tag) {
            out.push(tag.clone());
        }
    }
    if out.len() > MAX_TAGS {
        return Err(Error::InvalidRequest(format!(
            "a download can have at most {MAX_TAGS} tags"
        )));
    }
    Ok(out)
}

/// Whether `tags` holds `tag`, ignoring case.
pub(crate) fn contains(tags: &[String], tag: &str) -> bool {
    let tag = tag.to_lowercase();
    tags.iter().any(|t| t.to_lowercase() == tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn normalize_trims_dedupes_and_keeps_order() {
        let tags = normalize(&strings(&[" work ", "ISO", "Work", "iso", "b"])).unwrap();
        assert_eq!(tags, ["work", "ISO", "b"]);
    }

    #[test]
    fn normalize_counts_characters_not_bytes() {
        assert!(normalize(&strings(&[&"é".repeat(MAX_TAG_LEN)])).is_ok());
        let long = "x".repeat(MAX_TAG_LEN + 1);
        assert!(matches!(
            normalize(&strings(&[&long])),
            Err(Error::InvalidRequest(_))
        ));
        assert!(matches!(
            normalize(&strings(&["  "])),
            Err(Error::InvalidRequest(_))
        ));
    }

    #[test]
    fn merged_caps_the_count() {
        let full: Vec<String> = (0..MAX_TAGS).map(|i| format!("t{i}")).collect();
        assert_eq!(merged(&full, &strings(&["T0"])).unwrap(), full);
        assert!(matches!(
            merged(&full, &strings(&["new"])),
            Err(Error::InvalidRequest(_))
        ));
    }
}
