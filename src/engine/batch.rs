//! What a pasted list of links means: the one set of rules the CLI and the
//! GUI share. Pure functions, no I/O.
//!
//! Links split on newlines and whitespace, and on a comma or pipe that is
//! followed by another `http(s)://` link (so a comma inside one URL stays
//! put). Blank lines and `#` comments are ignored; only `http` and `https`
//! count. A link repeated in the list is kept once with its count.

use std::collections::HashMap;

use serde::Serialize;
use url::Url;

/// Most distinct links one list may hold; a longer list is refused.
pub const MAX_LIST_LINKS: usize = 5000;

/// A link found in a list, and how many times the list gave it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListLink {
    /// The link exactly as written.
    pub url: String,
    /// How many times it appeared (1 when it didn't repeat).
    pub count: usize,
}

/// Why a piece of a list wasn't taken as a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Not a web address at all.
    NotALink,
    /// A link, but not `http` or `https` (for example `ftp://`).
    UnsupportedScheme,
}

/// A piece of a list that was left out, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkippedLine {
    pub line: String,
    pub reason: SkipReason,
}

/// A list read into its distinct links (in first-seen order) and what was skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ParsedList {
    pub links: Vec<ListLink>,
    pub skipped: Vec<SkippedLine>,
}

/// A list the engine refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ListError {
    /// More distinct links than [`MAX_LIST_LINKS`].
    #[error("{count} links is more than the {max} a list can hold")]
    TooManyLinks { count: usize, max: usize },
}

/// Read pasted `text` into links and skipped lines.
///
/// Refuses a list of more than [`MAX_LIST_LINKS`] distinct links, saying how many.
pub fn parse_list(text: &str) -> Result<ParsedList, ListError> {
    let mut links: Vec<ListLink> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut skipped = Vec::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let pieces: Vec<&str> = line
            .split_whitespace()
            .flat_map(split_joined)
            .filter(|p| !p.chars().all(is_separator))
            .collect();
        let mut leftovers = Vec::new();
        let mut found_any = false;
        for piece in pieces {
            match classify(piece) {
                Ok(()) => {
                    found_any = true;
                    match index.get(piece) {
                        Some(&i) => links[i].count += 1,
                        None => {
                            index.insert(piece.to_string(), links.len());
                            links.push(ListLink {
                                url: piece.to_string(),
                                count: 1,
                            });
                        }
                    }
                }
                Err(reason) => leftovers.push((piece, reason)),
            }
        }
        if found_any {
            skipped.extend(leftovers.into_iter().map(|(p, reason)| SkippedLine {
                line: p.to_string(),
                reason,
            }));
        } else if let Some(&(_, reason)) = leftovers.first() {
            // Prose, not links: one skipped line, not one per word.
            skipped.push(SkippedLine {
                line: line.to_string(),
                reason,
            });
        }
    }

    if links.len() > MAX_LIST_LINKS {
        return Err(ListError::TooManyLinks {
            count: links.len(),
            max: MAX_LIST_LINKS,
        });
    }
    Ok(ParsedList { links, skipped })
}

fn is_separator(c: char) -> bool {
    c == ',' || c == '|'
}

fn starts_link(s: &str) -> bool {
    let lower = |n: usize| s.get(..n).map(str::to_ascii_lowercase);
    lower(7).as_deref() == Some("http://") || lower(8).as_deref() == Some("https://")
}

/// Split one whitespace-free word at each comma or pipe that is followed by
/// another link, dropping that separator.
fn split_joined(word: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in word.char_indices() {
        if is_separator(c) && starts_link(&word[i + 1..]) {
            out.push(&word[start..i]);
            start = i + 1;
        }
    }
    out.push(&word[start..]);
    out
}

fn classify(piece: &str) -> Result<(), SkipReason> {
    match Url::parse(piece) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host().is_some() => Ok(()),
        Ok(u) if !matches!(u.scheme(), "http" | "https") && piece.contains("://") => {
            Err(SkipReason::UnsupportedScheme)
        }
        _ => Err(SkipReason::NotALink),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(l: &ParsedList) -> Vec<(&str, usize)> {
        l.links.iter().map(|l| (l.url.as_str(), l.count)).collect()
    }

    fn skip(line: &str, reason: SkipReason) -> SkippedLine {
        SkippedLine {
            line: line.into(),
            reason,
        }
    }

    #[test]
    fn ignores_blanks_and_comments() {
        let l = parse_list("\n# a note\n  \nhttps://a.example/x\n   # indented\n").unwrap();
        assert_eq!(urls(&l), [("https://a.example/x", 1)]);
        assert!(l.skipped.is_empty());
    }

    #[test]
    fn repeats_are_kept_once_with_a_count() {
        let l = parse_list(
            "https://a.example/x\nhttps://a.example/y\nhttps://a.example/x\r\nhttps://a.example/x",
        )
        .unwrap();
        assert_eq!(
            urls(&l),
            [("https://a.example/x", 3), ("https://a.example/y", 1)]
        );
    }

    #[test]
    fn a_comma_inside_a_url_stays_put() {
        let l = parse_list("https://a.example/f,1.zip\nhttps://a.example/x?a=1,2").unwrap();
        assert_eq!(
            urls(&l),
            [
                ("https://a.example/f,1.zip", 1),
                ("https://a.example/x?a=1,2", 1)
            ]
        );
    }

    #[test]
    fn splits_on_whitespace_and_on_comma_or_pipe_before_a_link() {
        let l = parse_list(
            "https://a.example/1 https://a.example/2,https://a.example/3|http://a.example/4\n\
             https://a.example/5 , HTTPS://a.example/6",
        )
        .unwrap();
        let got: Vec<&str> = l.links.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(
            got,
            [
                "https://a.example/1",
                "https://a.example/2",
                "https://a.example/3",
                "http://a.example/4",
                "https://a.example/5",
                "HTTPS://a.example/6",
            ]
        );
        assert!(l.skipped.is_empty());
    }

    #[test]
    fn other_schemes_are_skipped_with_a_reason() {
        let l = parse_list("ftp://a.example/x\nfile:///etc/passwd\nhttps://a.example/ok").unwrap();
        assert_eq!(urls(&l), [("https://a.example/ok", 1)]);
        assert_eq!(
            l.skipped,
            [
                skip("ftp://a.example/x", SkipReason::UnsupportedScheme),
                skip("file:///etc/passwd", SkipReason::UnsupportedScheme),
            ]
        );
    }

    #[test]
    fn bad_lines_are_reported_whole_and_the_rest_still_count() {
        let l = parse_list(
            "release notes from the forum\nhttps://\nhttps://a.example/ok\nexample.com/a",
        )
        .unwrap();
        assert_eq!(urls(&l), [("https://a.example/ok", 1)]);
        assert_eq!(
            l.skipped,
            [
                skip("release notes from the forum", SkipReason::NotALink),
                skip("https://", SkipReason::NotALink),
                skip("example.com/a", SkipReason::NotALink),
            ]
        );
    }

    #[test]
    fn a_stray_word_beside_links_is_skipped_alone() {
        let l = parse_list("see https://a.example/x now").unwrap();
        assert_eq!(urls(&l), [("https://a.example/x", 1)]);
        assert_eq!(
            l.skipped,
            [
                skip("see", SkipReason::NotALink),
                skip("now", SkipReason::NotALink)
            ]
        );
    }

    #[test]
    fn five_thousand_links_pass_and_one_more_is_refused_with_its_count() {
        let make = |n: usize| {
            (0..n)
                .map(|i| format!("https://a.example/{i}\n"))
                .collect::<String>()
        };
        assert_eq!(
            parse_list(&make(MAX_LIST_LINKS)).unwrap().links.len(),
            MAX_LIST_LINKS
        );
        let err = parse_list(&make(MAX_LIST_LINKS + 1)).unwrap_err();
        assert_eq!(
            err,
            ListError::TooManyLinks {
                count: 5001,
                max: 5000
            }
        );
        assert_eq!(
            err.to_string(),
            "5001 links is more than the 5000 a list can hold"
        );
    }

    #[test]
    fn repeats_do_not_count_toward_the_cap() {
        let text = "https://a.example/x\n".repeat(MAX_LIST_LINKS + 10);
        let l = parse_list(&text).unwrap();
        assert_eq!(urls(&l), [("https://a.example/x", MAX_LIST_LINKS + 10)]);
    }
}
