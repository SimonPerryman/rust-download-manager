//! What Copy diagnostics hides beyond the log's own redaction. The report is
//! the text that leaves the machine, so links lose their site and path,
//! known download file names and folders become numbered placeholders, and
//! the home folder becomes `~`. The log files keep the full detail.

use std::path::{Path, PathBuf};

/// What the app knows that a report should hide.
#[derive(Debug, Default)]
pub(super) struct Known {
    /// Download file names (as shown, and as saved on disk).
    pub files: Vec<String>,
    /// Download folders: the default and every download's own. The log
    /// folder too, when it isn't under home.
    pub folders: Vec<PathBuf>,
    /// The sites of the known downloads' links.
    pub hosts: Vec<String>,
    /// The user's home folder, written as `~`.
    pub home: Option<PathBuf>,
}

/// `text` with every link reduced to its scheme, login marker and numbered
/// site (`https://<redacted>@<site-1>/<path>?<redacted>`), the known folders
/// as `<folder-N>`, the home folder as `~`, the known file names as
/// `<file-N>.<ext>`, and the known sites as `<site-N>` wherever else they
/// appear. Numbers are stable within one report. `ignore_case` matches
/// folders in any letter case (Windows paths).
pub(super) fn anonymize(text: &str, known: &Known, ignore_case: bool) -> String {
    let text = strip_colours(text);
    let mut sites: Vec<String> = Vec::new();
    let text = rdm::map_urls_in(&text, |url| hide_link(url, &mut sites));

    let mut text = text;
    for (with, folder) in numbered_folders(&known.folders, known.home.as_deref(), ignore_case) {
        text = replace_path(&text, &folder, &with, ignore_case);
    }
    if let Some(home) = &known.home {
        text = replace_path(&text, home, "~", ignore_case);
    }

    let files = numbered_files(&known.files);
    let text = replace_words(&text, &files, false, is_file_char, continues_file);

    for host in &known.hosts {
        site_number(&mut sites, host);
    }
    let hosts: Vec<(String, String)> = sites
        .iter()
        .enumerate()
        .map(|(i, host)| (host.clone(), format!("<site-{}>", i + 1)))
        .collect();
    replace_words(&text, &hosts, true, is_host_char, continues_host)
}

/// `text` without terminal colour codes (`ESC [ … letter`), which an older
/// log may hold and which would hide a name's word boundary.
fn strip_colours(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            while chars.next().is_some_and(|c| !c.is_ascii_alphabetic()) {}
            continue;
        }
        out.push(c);
    }
    out
}

/// The login markers [`rdm::redact_url`] leaves, longest first.
const LOGINS: [&str; 2] = ["<redacted>:<redacted>@", "<redacted>@"];

/// One link as the report shows it: [`rdm::redact_url`]'s form, then the
/// host (and port) as `<site-N>` and any path as `/<path>`.
fn hide_link(url: &str, sites: &mut Vec<String>) -> String {
    let redacted = rdm::redact_url(url);
    let Some(sep) = redacted.find("://") else {
        return redacted; // `<invalid url>`
    };
    let (scheme, mut rest) = redacted.split_at(sep + 3);
    let login = LOGINS
        .iter()
        .find(|login| rest.starts_with(**login))
        .copied()
        .unwrap_or("");
    rest = &rest[login.len()..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, rest) = rest.split_at(authority_end);
    let (path, query) = rest.split_at(rest.find('?').unwrap_or(rest.len()));

    let mut out = format!("{scheme}{login}");
    let host = host_of(authority);
    if !host.is_empty() {
        out.push_str(&format!("<site-{}>", site_number(sites, host)));
    }
    match path {
        "" | "/" => out.push_str(path),
        _ => out.push_str("/<path>"),
    }
    if !query.is_empty() {
        out.push_str("?<redacted>");
    }
    out
}

/// The host of link `url` (lower-cased, port dropped), if it has one.
pub(super) fn link_host(url: &str) -> Option<String> {
    let rest = &url[url.find("://")? + 3..];
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    Some(host_of(authority).to_ascii_lowercase()).filter(|h| !h.is_empty())
}

/// The host of `authority` (`host`, `host:port`, `[v6]:port`), port dropped.
fn host_of(authority: &str) -> &str {
    if authority.starts_with('[') {
        return authority
            .find(']')
            .map_or(authority, |end| &authority[..=end]);
    }
    authority.split(':').next().unwrap_or("")
}

/// `host`'s number in this report (1-based), adding it if new.
fn site_number(sites: &mut Vec<String>, host: &str) -> usize {
    let host = host.to_ascii_lowercase();
    match sites.iter().position(|s| *s == host) {
        Some(i) => i + 1,
        None => {
            sites.push(host);
            sites.len()
        }
    }
}

/// Each distinct folder with its `<folder-N>` (numbered in `folders`'
/// order), longest first so a folder inside another is replaced whole. The
/// home folder and the folders holding it are left alone, so home still
/// shows as `~` rather than as a numbered folder and the user's name.
fn numbered_folders(
    folders: &[PathBuf],
    home: Option<&Path>,
    ignore_case: bool,
) -> Vec<(String, PathBuf)> {
    let key = |p: &Path| {
        let text = p.display().to_string();
        let text = text.trim_end_matches(['/', '\\']).replace('\\', "/");
        if ignore_case {
            text.to_ascii_lowercase()
        } else {
            text
        }
    };
    let home_key = home.map(key);
    let mut seen: Vec<String> = home_key.iter().cloned().collect();
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for folder in folders {
        let k = key(folder);
        // A root (`/`, `C:`) would swallow every path.
        let holds_home = home_key
            .as_ref()
            .is_some_and(|h| h.strip_prefix(&k).is_some_and(|rest| rest.starts_with('/')));
        if k.len() < 3 || holds_home || seen.contains(&k) {
            continue;
        }
        seen.push(k);
        out.push((format!("<folder-{}>", out.len() + 1), folder.clone()));
    }
    out.sort_by_key(|(_, folder)| std::cmp::Reverse(folder.as_os_str().len()));
    out
}

/// Each distinct file name with its `<file-N>.<ext>` (numbered in `files`'
/// order), keeping an extension only if it looks like one (1–4 letters or
/// digits), so `J.Smith` is just `<file-N>`.
fn numbered_files(files: &[String]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for name in files {
        if name.is_empty() || out.iter().any(|(n, _)| n == name) {
            continue;
        }
        let n = out.len() + 1;
        let with = match name.rsplit_once('.') {
            Some((stem, ext))
                if !stem.is_empty()
                    && (1..=4).contains(&ext.len())
                    && ext.bytes().all(|b| b.is_ascii_alphanumeric()) =>
            {
                format!("<file-{n}>.{ext}")
            }
            _ => format!("<file-{n}>"),
        };
        out.push((name.clone(), with));
    }
    out
}

/// A character that continues a file name, so a match must not touch one.
fn is_file_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-')
}

/// Whether `rest` continues a file name (a `.` may follow: `a.iso.part`).
fn continues_file(rest: &str) -> bool {
    rest.chars().next().is_some_and(is_file_char)
}

/// A character that can come before a host name's start.
fn is_host_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.')
}

/// Whether `rest` continues a host name; a `.` ending a sentence doesn't.
fn continues_host(rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        Some('.') => chars.next().is_some_and(char::is_alphanumeric),
        Some(c) => is_host_char(c),
        None => false,
    }
}

/// `text` with each `(needle, with)` replaced in one pass, left to right,
/// the longest needle first where several start at the same place, and only
/// where the needle isn't part of a
/// longer word: the character before it fails `before`, and the text after
/// it fails `continues`.
fn replace_words(
    text: &str,
    pairs: &[(String, String)],
    ignore_case: bool,
    before: fn(char) -> bool,
    continues: fn(&str) -> bool,
) -> String {
    let mut pairs: Vec<&(String, String)> = pairs.iter().filter(|(n, _)| !n.is_empty()).collect();
    if pairs.is_empty() {
        return text.to_owned();
    }
    pairs.sort_by_key(|(needle, _)| std::cmp::Reverse(needle.len()));
    let matches_at = |at: usize, needle: &str| {
        text.get(at..at + needle.len()).is_some_and(|s| {
            if ignore_case {
                s.eq_ignore_ascii_case(needle)
            } else {
                s == needle
            }
        })
    };
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut prev: Option<char> = None;
    let mut iter = text.char_indices().peekable();
    while let Some((at, c)) = iter.next() {
        let free = at >= last && !prev.is_some_and(before);
        let found = free
            .then(|| {
                pairs.iter().find(|(needle, _)| {
                    matches_at(at, needle) && !continues(&text[at + needle.len()..])
                })
            })
            .flatten();
        if let Some((needle, with)) = found {
            out.push_str(&text[last..at]);
            out.push_str(with);
            last = at + needle.len();
            prev = text[..last].chars().next_back();
            while iter.peek().is_some_and(|(i, _)| *i < last) {
                iter.next();
            }
            continue;
        }
        prev = Some(c);
    }
    out.push_str(&text[last..]);
    out
}

/// `text` with every path under `path` (and `path` itself) starting `with`
/// instead. Also matches `path` with `/` separators, and, when
/// `ignore_case`, in any letter case. A longer name that starts the same
/// (`/home/al` in `/home/alice`) isn't matched.
pub(super) fn replace_path(text: &str, path: &Path, with: &str, ignore_case: bool) -> String {
    let path = path.display().to_string();
    let path = path.trim_end_matches(['/', '\\']);
    if path.len() < 2 {
        return text.to_owned();
    }
    let fold = |s: &str| {
        if ignore_case {
            s.to_ascii_lowercase()
        } else {
            s.to_owned()
        }
    };
    let mut out = text.to_owned();
    for form in [path.to_owned(), path.replace('\\', "/")] {
        let needle = fold(&form);
        let mut result = String::with_capacity(out.len());
        let haystack = fold(&out);
        let mut last = 0;
        for (at, _) in haystack.match_indices(&needle) {
            let end = at + needle.len();
            let next = out[end..].chars().next();
            if at < last || next.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                continue;
            }
            result.push_str(&out[last..at]);
            result.push_str(with);
            last = end;
        }
        result.push_str(&out[last..]);
        out = result;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{anonymize, Known};
    use std::path::PathBuf;

    fn known() -> Known {
        Known {
            files: vec![
                "report.pdf".into(),
                "ubuntu-24.04.iso".into(),
                "x.pdf".into(),
            ],
            folders: vec![
                PathBuf::from("/srv/media/Downloads"),
                PathBuf::from("/srv/media/Downloads/isos"),
            ],
            hosts: vec!["files.example".into()],
            home: Some(PathBuf::from("/home/alice")),
        }
    }

    /// Every kind of private detail is hidden, and none of the originals
    /// survive anywhere in the text.
    #[test]
    fn hides_links_files_and_folders() {
        let text = "\
Started https://cdn.example.com/a/b/report.pdf?X-Amz-Signature=SIG1\n\
Login https://bob:hunter2@files.example/private/ubuntu-24.04.iso\n\
Bot https://api.telegram.example/file/bot123:ABCSECRET/x.pdf\n\
IP http://192.168.1.20:8080/share/x.pdf\n\
Again https://CDN.example.com/other/path\n\
Saved /srv/media/Downloads/isos/ubuntu-24.04.iso.part and /srv/media/Downloads/report.pdf\n\
Lookup failed for files.example. See /home/alice/.rdm/rdm.log\n\
Checksum of report.pdf matched\n";
        let out = anonymize(text, &known(), false);
        for original in [
            "cdn.example.com",
            "CDN.example.com",
            "/a/b",
            "SIG1",
            "bob",
            "hunter2",
            "files.example",
            "private",
            "telegram",
            "bot123",
            "ABCSECRET",
            "192.168",
            "8080",
            "share",
            "/srv",
            "media",
            "Downloads",
            "isos",
            "report",
            "ubuntu",
            "x.pdf",
            "alice",
        ] {
            assert!(!out.contains(original), "{original} survived:\n{out}");
        }
        assert_eq!(
            out,
            "\
Started https://<site-1>/<path>?<redacted>\n\
Login https://<redacted>:<redacted>@<site-2>/<path>\n\
Bot https://<site-3>/<path>\n\
IP http://<site-4>/<path>\n\
Again https://<site-1>/<path>\n\
Saved <folder-2>/<file-2>.iso.part and <folder-1>/<file-1>.pdf\n\
Lookup failed for <site-2>. See ~/.rdm/rdm.log\n\
Checksum of <file-1>.pdf matched\n"
        );
    }

    /// A user name alone keeps its own marker; a bare host and an empty path
    /// keep their shape.
    #[test]
    fn keeps_the_shape_of_a_link() {
        let out = anonymize(
            "a https://user@h.example b http://h.example/ c ftp://[::1]:21/x",
            &Known::default(),
            false,
        );
        assert_eq!(
            out,
            "a https://<redacted>@<site-1>/ b http://<site-1>/ c ftp://<site-2>/<path>"
        );
    }

    /// A file name is replaced only as a whole word, the longest known name
    /// wins where two start at the same place, and an extension is kept
    /// only if it looks like one.
    #[test]
    fn file_names_match_whole_words_longest_first() {
        let known = Known {
            files: vec![
                "a.iso".into(),
                "data.iso".into(),
                "my a.iso".into(),
                "x".into(),
                "x y.iso".into(),
                "J.Smith".into(),
            ],
            ..Known::default()
        };
        let out = anonymize(
            "data.iso, my a.iso, a.iso.part, banana.iso, x y.iso, x, J.Smith",
            &known,
            false,
        );
        assert_eq!(
            out,
            "<file-2>.iso, <file-3>.iso, <file-1>.iso.part, banana.iso, <file-5>.iso, <file-4>, <file-6>"
        );
    }

    /// A download folder holding home doesn't swallow it: home stays `~`.
    #[test]
    fn a_folder_above_home_leaves_home_alone() {
        let known = Known {
            folders: vec![PathBuf::from("/home"), PathBuf::from("/")],
            home: Some(PathBuf::from("/home/alice")),
            ..Known::default()
        };
        let out = anonymize("Logs: /home/alice/.rdm", &known, false);
        assert_eq!(out, "Logs: ~/.rdm");
    }

    /// Colour codes are dropped, so a name right after one is still found.
    #[test]
    fn colour_codes_are_dropped() {
        let known = Known {
            files: vec!["a.iso".into()],
            ..Known::default()
        };
        let out = anonymize(
            "task{\x1b[3mfile\x1b[0m\x1b[2m=\x1b[0ma.iso}",
            &known,
            false,
        );
        assert_eq!(out, "task{file=<file-1>.iso}");
    }

    /// Windows folders match in any case and with either separator; a longer
    /// folder name that starts the same is left alone.
    #[test]
    fn folders_match_windows_paths() {
        let known = Known {
            folders: vec![PathBuf::from(r"D:\Media")],
            home: Some(PathBuf::from(r"C:\Users\Someone")),
            ..Known::default()
        };
        let out = anonymize(
            r"to d:\media\a and D:/Media/b, not D:\Media2; log c:\users\someone\.rdm",
            &known,
            true,
        );
        assert_eq!(
            out,
            r"to <folder-1>\a and <folder-1>/b, not D:\Media2; log ~\.rdm"
        );
    }
}
