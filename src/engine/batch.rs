//! What a pasted list of links means: the one set of rules the CLI and the
//! GUI share. Pure functions, no I/O.
//!
//! Links split on newlines and whitespace, and on a comma or pipe that is
//! followed by another `http(s)://` link (so a comma inside one URL stays
//! put). Blank lines and `#` comments are ignored; only `http` and `https`
//! count.
//!
//! A `[a-b]` range in a link's path or query (never its host) stands for every
//! value between: `part[01-12].zip` is twelve links, `file[a-f].txt` six.
//! Numbers keep their zero padding. A range is expanded first and the links
//! deduped after, so overlapping ranges and repeats show once with their
//! count. Only a bracket group shaped like a range (`[x-y]`, both ends
//! letters or digits) is read as one; anything else (`[]`, `[name]`, an
//! IPv6 host's `[::1]`) is part of the link as written. A range that is
//! shaped right but wrong (reversed, mixed kinds, uneven padding, a second
//! range, too many links, or in the host) is refused as a skipped line with
//! its reason. Write `%5B` for a literal bracket that looks like a range.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use serde::Serialize;
use url::Url;

/// Most distinct links one list may hold; a longer list is refused.
pub const MAX_LIST_LINKS: usize = 5000;

/// Most links one `[a-b]` range may make; a bigger range is refused.
pub const MAX_PATTERN_LINKS: usize = 500;

/// A link found in a list, and how many times the list gave it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListLink {
    /// The link exactly as written.
    pub url: String,
    /// How many times it appeared (1 when it didn't repeat).
    pub count: usize,
    /// Which of [`ParsedList::patterns`] first made this link, if a range did.
    pub pattern: Option<usize>,
}

/// A `[a-b]` range that expanded into links, for showing as one row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListPattern {
    /// The text before the range in its filename (`lesson` for
    /// `lesson[01-12].mp4`), else the host: a natural name for the batch.
    pub stem: String,
    /// How many of [`ParsedList::links`] this range made (a link an earlier
    /// range or line already gave belongs to that one).
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
    /// A `[a-b]` range in the host, where it can't apply.
    RangeInHost,
    /// A range whose ends aren't both numbers or both single letters of one case.
    RangeInvalid,
    /// A numeric range padded on one end only or to different widths (`[01-015]`).
    RangeWidthMismatch,
    /// A range that counts down (`[9-1]`).
    RangeReversed,
    /// More than one range in a link.
    MultipleRanges,
    /// A range making more than [`MAX_PATTERN_LINKS`] links; the line's
    /// `count` says how many.
    RangeTooBig,
}

impl SkipReason {
    /// Whether this is a refused `[a-b]` range rather than a plain non-link.
    pub fn is_range_problem(self) -> bool {
        !matches!(self, Self::NotALink | Self::UnsupportedScheme)
    }

    /// A sentence for a frontend that shows the reason as text; `count` is the
    /// skipped line's [`SkippedLine::count`].
    pub fn message(self, count: Option<u64>) -> String {
        match self {
            Self::NotALink => "not a link".into(),
            Self::UnsupportedScheme => "only http and https links work".into(),
            Self::RangeInHost => "a range can't be in the host".into(),
            Self::RangeInvalid => {
                "a range's ends must both be numbers or both be single letters of one case".into()
            }
            Self::RangeWidthMismatch => "a padded range's ends must have the same width".into(),
            Self::RangeReversed => "a range must count up".into(),
            Self::MultipleRanges => "only one range per link".into(),
            Self::RangeTooBig => match count {
                Some(n) => format!("that range makes {n} links, the most is {MAX_PATTERN_LINKS}"),
                None => format!("that range makes more than {MAX_PATTERN_LINKS} links"),
            },
        }
    }
}

/// A piece of a list that was left out, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkippedLine {
    pub line: String,
    pub reason: SkipReason,
    /// For [`SkipReason::RangeTooBig`], how many links the range would make.
    pub count: Option<u64>,
}

/// A list read into its distinct links (in first-seen order) and what was skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ParsedList {
    pub links: Vec<ListLink>,
    /// The ranges that made links, indexed by [`ListLink::pattern`].
    pub patterns: Vec<ListPattern>,
    pub skipped: Vec<SkippedLine>,
}

/// A list the engine refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ListError {
    /// More distinct links than [`MAX_LIST_LINKS`].
    #[error("{count} links is more than the {max} a list can hold")]
    TooManyLinks { count: usize, max: usize },
}

/// Read pasted `text` into links and skipped lines, expanding `[a-b]` ranges.
///
/// Refuses a list of more than [`MAX_LIST_LINKS`] distinct links, saying how
/// many (at least that many, if a range pushed it over before the end).
pub fn parse_list(text: &str) -> Result<ParsedList, ListError> {
    let mut links: Vec<ListLink> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut patterns: Vec<ListPattern> = Vec::new();
    let mut skipped = Vec::new();

    'lines: for raw in text.lines() {
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
            let range = match find_range(piece) {
                None => None,
                Some(Ok(range)) => Some(range),
                Some(Err((reason, count))) => {
                    skipped.push(SkippedLine {
                        line: piece.to_string(),
                        reason,
                        count,
                    });
                    continue;
                }
            };
            let Some(range) = range else {
                match classify(piece) {
                    Ok(()) => {
                        found_any = true;
                        add_link(&mut links, &mut index, piece.to_string(), None);
                    }
                    Err(reason) => leftovers.push((piece, reason)),
                }
                continue;
            };
            // Every link of a range differs only in its value, so one check serves all.
            let first = range.urls().next().unwrap_or_default();
            if let Err(reason) = classify(&first) {
                skipped.push(SkippedLine {
                    line: piece.to_string(),
                    reason,
                    count: None,
                });
                continue;
            }
            found_any = true;
            let id = patterns.len();
            patterns.push(ListPattern {
                stem: range.stem(&first),
                count: 0,
            });
            for url in range.urls() {
                add_link(&mut links, &mut index, url, Some(id));
            }
            if links.len() > MAX_LIST_LINKS {
                break 'lines;
            }
        }
        if found_any {
            skipped.extend(leftovers.into_iter().map(|(p, reason)| SkippedLine {
                line: p.to_string(),
                reason,
                count: None,
            }));
        } else if let Some(&(_, reason)) = leftovers.first() {
            // Prose, not links: one skipped line, not one per word.
            skipped.push(SkippedLine {
                line: line.to_string(),
                reason,
                count: None,
            });
        }
    }

    if links.len() > MAX_LIST_LINKS {
        return Err(ListError::TooManyLinks {
            count: links.len(),
            max: MAX_LIST_LINKS,
        });
    }
    for link in &links {
        if let Some(p) = link.pattern {
            patterns[p].count += 1;
        }
    }
    Ok(ParsedList {
        links,
        patterns,
        skipped,
    })
}

/// Add `url` once, counting a repeat; `pattern` credits the range that made it.
fn add_link(
    links: &mut Vec<ListLink>,
    index: &mut HashMap<String, usize>,
    url: String,
    pattern: Option<usize>,
) {
    match index.get(&url) {
        Some(&i) => links[i].count += 1,
        None => {
            index.insert(url.clone(), links.len());
            links.push(ListLink {
                url,
                count: 1,
                pattern,
            });
        }
    }
}

/// The values a range steps through.
#[derive(Debug, Clone, Copy)]
enum Values {
    /// `start..=end`, zero-padded to `width` (0 = unpadded).
    Numbers {
        start: u128,
        end: u128,
        width: usize,
    },
    /// `from..=to`, ASCII letters.
    Letters { from: u8, to: u8 },
}

/// A link split around its one `[a-b]` range. Holds the range, never its links.
#[derive(Debug, Clone, Copy)]
struct Range<'a> {
    prefix: &'a str,
    suffix: &'a str,
    values: Values,
}

impl Range<'_> {
    /// How many links the range makes, by arithmetic.
    fn count(&self) -> u128 {
        match self.values {
            Values::Numbers { start, end, .. } => end - start + 1,
            Values::Letters { from, to } => u128::from(to - from) + 1,
        }
    }

    /// The links, made one at a time (call only once `count` is within the cap).
    fn urls(&self) -> impl Iterator<Item = String> + '_ {
        let n = self.count() as usize;
        (0..n).map(move |i| {
            let value = match self.values {
                Values::Numbers { start, width, .. } => {
                    format!("{:0width$}", start + i as u128)
                }
                Values::Letters { from, .. } => char::from(from + i as u8).to_string(),
            };
            format!("{}{value}{}", self.prefix, self.suffix)
        })
    }

    /// The name part before the range: text after the last `/`, `?` or
    /// `&` of the prefix, else the host of `first` (one of the links).
    fn stem(&self, first: &str) -> String {
        let tail = self
            .prefix
            .rsplit(['/', '?', '&'])
            .next()
            .unwrap_or_default()
            .trim_end_matches(['-', '_', '.', ' ', '=']);
        if !tail.is_empty() {
            return tail.to_string();
        }
        Url::parse(first)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default()
    }
}

/// A bracket group's ends if it is shaped like a range: `x-y`, both ends
/// non-empty letters and digits.
fn range_ends(content: &str) -> Option<(&str, &str)> {
    let (a, b) = content.split_once('-')?;
    let word = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric());
    (word(a) && word(b)).then_some((a, b))
}

/// Byte spans `(open, close)` of the range-shaped `[..]` groups in `text`.
fn range_groups(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find('[').map(|i| from + i) {
        let Some(close) = text[open..].find(']').map(|i| open + i) else {
            break;
        };
        if range_ends(&text[open + 1..close]).is_some() {
            out.push((open, close));
            from = close + 1;
        } else {
            from = open + 1;
        }
    }
    out
}

type Refusal = (SkipReason, Option<u64>);

/// The range in an `http(s)` link, if it has one; `Err` is why it's refused.
fn find_range(piece: &str) -> Option<Result<Range<'_>, Refusal>> {
    if !starts_link(piece) || !piece.contains('[') {
        return None;
    }
    let after_scheme = piece.find("://")? + 3;
    let host_end = piece[after_scheme..]
        .find(['/', '?', '#'])
        .map_or(piece.len(), |i| after_scheme + i);
    if !range_groups(&piece[after_scheme..host_end]).is_empty() {
        return Some(Err((SkipReason::RangeInHost, None)));
    }
    let body_end = piece.find('#').unwrap_or(piece.len());
    let groups = range_groups(&piece[host_end..body_end]);
    let &(open, close) = groups.first()?;
    if groups.len() > 1 {
        return Some(Err((SkipReason::MultipleRanges, None)));
    }
    let (open, close) = (host_end + open, host_end + close);
    let (a, b) = range_ends(&piece[open + 1..close])?;
    let values = match parse_values(a, b) {
        Ok(v) => v,
        Err(reason) => return Some(Err((reason, None))),
    };
    let range = Range {
        prefix: &piece[..open],
        suffix: &piece[close + 1..],
        values,
    };
    let count = range.count();
    if count > MAX_PATTERN_LINKS as u128 {
        let shown = u64::try_from(count).unwrap_or(u64::MAX);
        return Some(Err((SkipReason::RangeTooBig, Some(shown))));
    }
    Some(Ok(range))
}

/// Read a range's two ends. Digits keep padding (and then need equal widths);
/// otherwise both must be single letters of one case.
fn parse_values(a: &str, b: &str) -> Result<Values, SkipReason> {
    let digits = |s: &str| s.bytes().all(|c| c.is_ascii_digit());
    if digits(a) && digits(b) {
        let padded = |s: &str| s.len() > 1 && s.starts_with('0');
        let width = if padded(a) || padded(b) {
            if a.len() != b.len() {
                return Err(SkipReason::RangeWidthMismatch);
            }
            a.len()
        } else {
            0
        };
        let (Ok(start), Ok(end)) = (a.parse::<u128>(), b.parse::<u128>()) else {
            return Err(SkipReason::RangeInvalid);
        };
        if start > end {
            return Err(SkipReason::RangeReversed);
        }
        return Ok(Values::Numbers { start, end, width });
    }
    match (a.as_bytes(), b.as_bytes()) {
        ([from], [to])
            if from.is_ascii_alphabetic()
                && to.is_ascii_alphabetic()
                && from.is_ascii_lowercase() == to.is_ascii_lowercase() =>
        {
            if from > to {
                Err(SkipReason::RangeReversed)
            } else {
                Ok(Values::Letters {
                    from: *from,
                    to: *to,
                })
            }
        }
        _ => Err(SkipReason::RangeInvalid),
    }
}

/// Run `run` over `items`, at most `jobs` at a time (at least 1), and return
/// the results in item order. `run` gets the item's index too.
///
/// A failing item is the caller's to report in its result; it never stops the
/// others. Items start in order, so a caller can stop starting new ones by
/// having `run` return early.
pub async fn run_limited<I, T, F, Fut>(items: Vec<I>, jobs: usize, run: F) -> Vec<T>
where
    F: Fn(usize, I) -> Fut,
    Fut: Future<Output = T>,
{
    let run = &run;
    let mut done: Vec<(usize, T)> = futures_util::stream::iter(items.into_iter().enumerate())
        .map(|(i, item)| async move { (i, run(i, item).await) })
        .buffer_unordered(jobs.max(1))
        .collect()
        .await;
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, t)| t).collect()
}

/// Keeps the files of one batch run apart: two links that derive the same
/// filename get `name (1).ext` for the second, so concurrent `.part` files
/// never collide. Clones share one set of claims.
///
/// Only this run's claims count, not what is on disk, so a rerun resumes an
/// interrupted file from its `.part` under the same name.
#[derive(Debug, Clone, Default)]
pub struct TargetClaims(Arc<Mutex<HashSet<PathBuf>>>);

impl TargetClaims {
    /// An empty set of claims.
    pub fn new() -> Self {
        Self::default()
    }

    /// The hook for a [`DownloadRequest`](super::DownloadRequest) that claims
    /// its target from this set.
    pub fn claim_target(&self) -> impl Fn(&Path) -> PathBuf + Send + Sync + 'static {
        let claims = self.clone();
        move |proposed| claims.claim(proposed)
    }

    fn claim(&self, proposed: &Path) -> PathBuf {
        let mut taken = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut candidate = proposed.to_path_buf();
        let stem = proposed
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("download");
        let ext = proposed.extension().and_then(|e| e.to_str());
        let mut n = 0u32;
        while !taken.insert(candidate.clone()) {
            n += 1;
            let name = match ext {
                Some(ext) => format!("{stem} ({n}).{ext}"),
                None => format!("{stem} ({n})"),
            };
            candidate = proposed.with_file_name(name);
        }
        candidate
    }
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

    #[test]
    fn claims_keep_same_names_apart() {
        let claims = TargetClaims::new();
        let a = claims.claim(Path::new("d/file.bin"));
        let b = claims.claim(Path::new("d/file.bin"));
        let c = claims.claim(Path::new("d/file.bin"));
        assert_eq!(a, Path::new("d/file.bin"));
        assert_eq!(b, Path::new("d/file (1).bin"));
        assert_eq!(c, Path::new("d/file (2).bin"));
    }

    #[tokio::test]
    async fn run_limited_keeps_order_and_the_limit() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (now, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let out = run_limited((0..8).collect(), 3, |i, n: u32| {
            let (now, peak) = (&now, &peak);
            async move {
                let running = now.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(running, Ordering::SeqCst);
                tokio::task::yield_now().await;
                now.fetch_sub(1, Ordering::SeqCst);
                (i, n * 2)
            }
        })
        .await;
        assert_eq!(out, (0..8).map(|n| (n as usize, n * 2)).collect::<Vec<_>>());
        assert!(peak.load(Ordering::SeqCst) <= 3);
    }

    fn urls(l: &ParsedList) -> Vec<(&str, usize)> {
        l.links.iter().map(|l| (l.url.as_str(), l.count)).collect()
    }

    fn skip(line: &str, reason: SkipReason) -> SkippedLine {
        SkippedLine {
            line: line.into(),
            reason,
            count: None,
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

    fn all(l: &ParsedList) -> Vec<&str> {
        l.links.iter().map(|l| l.url.as_str()).collect()
    }

    fn refused(text: &str) -> SkippedLine {
        let l = parse_list(text).unwrap();
        assert!(l.links.is_empty(), "{:?}", l.links);
        assert_eq!(l.skipped.len(), 1, "{:?}", l.skipped);
        l.skipped.into_iter().next().unwrap()
    }

    #[test]
    fn numeric_range_keeps_zero_padding() {
        let l = parse_list("https://a.example/p[01-12].zip").unwrap();
        let got = all(&l);
        assert_eq!(got.len(), 12);
        assert_eq!(got[0], "https://a.example/p01.zip");
        assert_eq!(got[11], "https://a.example/p12.zip");
        assert_eq!(
            l.patterns,
            [ListPattern {
                stem: "p".into(),
                count: 12
            }]
        );
        assert!(l.links.iter().all(|l| l.pattern == Some(0)));
        let l = parse_list("https://a.example/f[8-11]").unwrap();
        assert_eq!(
            all(&l),
            [
                "https://a.example/f8",
                "https://a.example/f9",
                "https://a.example/f10",
                "https://a.example/f11"
            ]
        );
    }

    #[test]
    fn letter_range_expands() {
        let l = parse_list("https://a.example/x[a-c].txt https://a.example/y[X-Z]").unwrap();
        assert_eq!(
            all(&l),
            [
                "https://a.example/xa.txt",
                "https://a.example/xb.txt",
                "https://a.example/xc.txt",
                "https://a.example/yX",
                "https://a.example/yY",
                "https://a.example/yZ",
            ]
        );
        assert_eq!(l.patterns.len(), 2);
    }

    #[test]
    fn uneven_padding_is_refused() {
        let s = refused("https://a.example/f[01-015]");
        assert_eq!(s.reason, SkipReason::RangeWidthMismatch);
        assert_eq!(s.line, "https://a.example/f[01-015]");
        assert_eq!(
            refused("https://a.example/f[1-015]").reason,
            SkipReason::RangeWidthMismatch
        );
    }

    #[test]
    fn reversed_and_invalid_ranges_are_refused_with_a_reason() {
        for text in ["https://a.example/f[9-1]", "https://a.example/f[z-a]"] {
            assert_eq!(refused(text).reason, SkipReason::RangeReversed, "{text}");
        }
        for text in [
            "https://a.example/f[a-1]",
            "https://a.example/f[a-Z]",
            "https://a.example/f[ab-cd]",
            "https://a.example/f[first-name]",
        ] {
            assert_eq!(refused(text).reason, SkipReason::RangeInvalid, "{text}");
        }
        assert_eq!(
            refused("https://a.example/f[1-2]/g[1-2]").reason,
            SkipReason::MultipleRanges
        );
    }

    #[test]
    fn a_range_over_the_cap_is_refused_with_its_count() {
        let s = refused("https://a.example/f[1-501]");
        assert_eq!(s.reason, SkipReason::RangeTooBig);
        assert_eq!(s.count, Some(501));
        assert_eq!(
            s.reason.message(s.count),
            "that range makes 501 links, the most is 500"
        );
        assert_eq!(
            parse_list("https://a.example/f[1-500]")
                .unwrap()
                .links
                .len(),
            MAX_PATTERN_LINKS
        );
    }

    #[test]
    fn a_huge_range_is_counted_not_generated() {
        let started = std::time::Instant::now();
        let s = refused("https://a.example/f[1-999999999]");
        assert_eq!(
            (s.reason, s.count),
            (SkipReason::RangeTooBig, Some(999_999_999))
        );
        let s = refused(&format!("https://a.example/f[0-{}]", "9".repeat(30)));
        assert_eq!(s.reason, SkipReason::RangeTooBig);
        let s = refused(&format!("https://a.example/f[0-{}]", "9".repeat(60)));
        assert_eq!(s.reason, SkipReason::RangeInvalid);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn ranges_expand_before_deduping() {
        let l = parse_list(
            "https://a.example/lesson[01-15]
https://a.example/lesson[13-17]",
        )
        .unwrap();
        assert_eq!(l.links.len(), 17);
        let counts: Vec<usize> = l.links.iter().map(|l| l.count).collect();
        assert_eq!(counts[12..15], [2, 2, 2]);
        assert_eq!(counts.iter().sum::<usize>(), 20);
        // A repeated link is credited to the range that made it first.
        assert_eq!(
            l.patterns.iter().map(|p| p.count).collect::<Vec<_>>(),
            [15, 2]
        );
    }

    #[test]
    fn a_range_counts_toward_the_list_cap() {
        let text = (0..11)
            .map(|i| {
                format!(
                    "https://a.example/{i}/f[1-500]
"
                )
            })
            .collect::<String>();
        assert_eq!(
            parse_list(&text).unwrap_err(),
            ListError::TooManyLinks {
                count: 5500,
                max: MAX_LIST_LINKS
            }
        );
    }

    #[test]
    fn a_range_in_the_query_expands() {
        let l = parse_list("https://a.example/get?id=[1-3]&x=1#frag[1-9]").unwrap();
        assert_eq!(
            all(&l),
            [
                "https://a.example/get?id=1&x=1#frag[1-9]",
                "https://a.example/get?id=2&x=1#frag[1-9]",
                "https://a.example/get?id=3&x=1#frag[1-9]",
            ]
        );
        assert_eq!(l.patterns[0].stem, "id");
    }

    #[test]
    fn a_range_in_the_host_is_refused() {
        assert_eq!(
            refused("https://cdn[1-3].example/f.bin").reason,
            SkipReason::RangeInHost
        );
        assert_eq!(
            refused("https://user[1-3]@a.example/f.bin").reason,
            SkipReason::RangeInHost
        );
    }

    #[test]
    fn ipv6_hosts_and_other_brackets_stay_literal() {
        let l = parse_list(
            "http://[::1]:8080/f.bin http://[2001:db8::1]/a[1-2].bin              https://a.example/x?filter[name]=1 https://a.example/y[abc] https://a.example/z[]",
        )
        .unwrap();
        assert_eq!(
            all(&l),
            [
                "http://[::1]:8080/f.bin",
                "http://[2001:db8::1]/a1.bin",
                "http://[2001:db8::1]/a2.bin",
                "https://a.example/x?filter[name]=1",
                "https://a.example/y[abc]",
                "https://a.example/z[]",
            ]
        );
        assert!(l.skipped.is_empty());
    }

    #[test]
    fn a_stem_falls_back_to_the_host() {
        let l = parse_list("https://a.example/[1-2]").unwrap();
        assert_eq!(l.patterns[0].stem, "a.example");
        let l = parse_list("https://a.example/dir/lesson-[01-02].mp4").unwrap();
        assert_eq!(l.patterns[0].stem, "lesson");
    }

    #[test]
    fn a_refused_range_leaves_the_rest_of_the_list() {
        let l = parse_list(
            "https://a.example/ok
https://a.example/f[1-9999]
https://a.example/g[1-2]",
        )
        .unwrap();
        assert_eq!(l.links.len(), 3);
        assert_eq!(l.skipped.len(), 1);
    }
}
