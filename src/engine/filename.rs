//! Deriving the output filename for a download.
//!
//! Priority: an explicit output path wins; otherwise the server's
//! `Content-Disposition` filename; otherwise the last path segment of the
//! (post-redirect) URL; otherwise a `download` fallback.
//!
//! Server- and URL-derived names are untrusted: they are cleaned to the same
//! portable, Windows-safe bare filename on every OS, so a file copied between
//! systems keeps its name and can never land outside the download directory.

use std::path::{Component, Path, PathBuf};

/// Name used when nothing usable can be derived.
const FALLBACK_NAME: &str = "download";

/// Longest derived name, in UTF-8 bytes. Filesystems cap a name at 255 (bytes
/// on Linux, UTF-16 units on NTFS); the headroom fits a ` (n)` dedup suffix
/// plus the engine's sidecar temp name around it (`.<name>.rdm.<pid>.<n>.tmp`)
/// without overflowing.
pub const MAX_NAME_BYTES: usize = 200;

/// Longest tail (dot included) kept as an extension when truncating.
const MAX_EXT_BYTES: usize = 16;

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
    FALLBACK_NAME.to_string()
}

/// `dir.join(name)`, but only when `name` is exactly one plain path component
/// that stays directly inside `dir`; otherwise `dir/download`. Defence in depth
/// behind sanitising: on Windows, `join` drops `dir` for `C:x` or `\\srv\x`.
pub fn join_in(dir: &Path, name: &str) -> PathBuf {
    let mut components = Path::new(name).components();
    let single = matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    );
    let candidate = dir.join(name);
    if single && candidate.parent() == Some(dir) {
        candidate
    } else {
        dir.join(FALLBACK_NAME)
    }
}

/// A path that doesn't collide with an existing file: `desired` if free, else
/// `name (1).ext`, `name (2).ext`, … preserving the extension. Used at finalize
/// so a completed download never clobbers an existing file.
pub fn unique_path(desired: &Path) -> PathBuf {
    if !desired.exists() {
        return desired.to_path_buf();
    }
    let parent = desired.parent();
    let stem = desired
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(FALLBACK_NAME);
    let ext = desired.extension().and_then(|e| e.to_str());

    for n in 1u32.. {
        let name = match ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = match parent {
            Some(p) if !p.as_os_str().is_empty() => p.join(&name),
            _ => PathBuf::from(&name),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("the range is effectively unbounded")
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

/// Reduce a candidate to a safe, portable bare filename, or `""` if nothing
/// usable remains. Keeps only the last path segment; replaces every character
/// outside the allowlist with `_`; strips leading/trailing dots and whitespace
/// (no hidden dotfiles, nothing Windows would silently trim); defuses Windows
/// device names; caps the length at [`MAX_NAME_BYTES`].
fn sanitize(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim()
        .trim_matches('"');
    let cleaned: String = base
        .chars()
        .map(|c| if allowed(c) { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace());
    if trimmed.is_empty() {
        return String::new();
    }
    let name = if is_reserved_device(trimmed) {
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    };
    truncate(name)
}

/// Whether `c` may appear in a derived filename. ASCII is an allowlist of
/// printable characters minus those Windows forbids (`:` also opens an NTFS
/// alternate data stream). Beyond ASCII, everything is kept except control
/// characters and invisible format/bidi controls, which can disguise an
/// extension (`evil\u{202E}txt.exe` renders as `evilexe.txt`).
fn allowed(c: char) -> bool {
    if c.is_ascii() {
        return c == ' '
            || (c.is_ascii_graphic()
                && !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'));
    }
    !c.is_control()
        && !matches!(
            c,
            '\u{061C}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{2069}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
        )
}

/// Whether Windows treats `name` as a device (`CON`, `nul.txt`,
/// `COM1.tar.gz`, …): the part before the first dot, ignoring trailing
/// spaces and case, is a reserved device name.
fn is_reserved_device(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let port = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"));
    matches!(
        port,
        Some("0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
    )
}

/// Cap `name` at [`MAX_NAME_BYTES`] on a char boundary, cutting the stem and
/// keeping a short extension so the file type survives.
fn truncate(name: String) -> String {
    if name.len() <= MAX_NAME_BYTES {
        return name;
    }
    let ext = match name.rfind('.') {
        Some(dot) if dot > 0 && name.len() - dot <= MAX_EXT_BYTES => &name[dot..],
        _ => "",
    };
    let cut = name.floor_char_boundary(MAX_NAME_BYTES - ext.len());
    let stem = name[..cut].trim_end_matches(|c: char| c == '.' || c.is_whitespace());
    format!("{stem}{ext}")
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
    fn unique_path_increments_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        let desired = dir.path().join("file.ext");

        // Nothing there yet → the desired path.
        assert_eq!(unique_path(&desired), desired);

        std::fs::write(&desired, b"a").unwrap();
        let one = unique_path(&desired);
        assert_eq!(one.file_name().unwrap(), "file (1).ext");

        std::fs::write(&one, b"b").unwrap();
        let two = unique_path(&desired);
        assert_eq!(two.file_name().unwrap(), "file (2).ext");
    }

    #[test]
    fn unique_path_handles_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let desired = dir.path().join("README");
        std::fs::write(&desired, b"a").unwrap();
        let one = unique_path(&desired);
        assert_eq!(one.file_name().unwrap(), "README (1)");
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

    /// A served filename, as `resolve` derives it from `Content-Disposition`.
    fn served(filename: &str) -> String {
        let cd = format!("attachment; filename=\"{filename}\"");
        resolve(None, "https://example.com/", Some(&cd))
    }

    #[test]
    fn drive_relative_name_loses_its_colon() {
        assert_eq!(served("C:evil.exe"), "C_evil.exe");
        let name = resolve(
            None,
            "https://example.com/x",
            Some("attachment; filename*=UTF-8''C%3Aevil.exe"),
        );
        assert_eq!(name, "C_evil.exe");
    }

    #[test]
    fn drive_absolute_and_unc_paths_keep_only_the_last_segment() {
        assert_eq!(served("C:\\x\\evil.exe"), "evil.exe");
        assert_eq!(served("\\\\server\\share\\x"), "x");
        assert_eq!(served("/etc/passwd"), "passwd");
    }

    #[test]
    fn dot_specials_and_empty_names_fall_back() {
        for name in ["..", ".", "...", "", "   ", ". . .", "a/..", "a\\.."] {
            assert_eq!(served(name), "download", "{name:?}");
        }
    }

    #[test]
    fn percent_encoded_url_path_is_sanitised_after_decoding() {
        let name = resolve(None, "https://example.com/dl/C%3Aevil.exe", None);
        assert_eq!(name, "C_evil.exe");
        let name = resolve(None, "https://example.com/a/..%5C..%5Cevil.exe", None);
        assert_eq!(name, "evil.exe");
        let name = resolve(None, "https://example.com/a/%2E%2E", None);
        assert_eq!(name, "download");
    }

    #[test]
    fn reserved_device_names_are_prefixed() {
        for (input, want) in [
            ("con", "_con"),
            ("CON", "_CON"),
            ("NUL.txt", "_NUL.txt"),
            ("nul.txt", "_nul.txt"),
            ("COM1.tar.gz", "_COM1.tar.gz"),
            ("lpt9", "_lpt9"),
            ("Aux .log", "_Aux .log"),
            ("prn", "_prn"),
            ("conin$", "_conin$"),
            ("COM¹.bin", "_COM¹.bin"),
        ] {
            assert_eq!(served(input), want, "{input:?}");
        }
    }

    #[test]
    fn names_merely_resembling_devices_are_kept() {
        for name in [
            "console.txt",
            "COM10",
            "LPT",
            "CONFIG.SYS",
            "nulls.bin",
            "x.con",
        ] {
            assert_eq!(served(name), name);
        }
    }

    #[test]
    fn alternate_data_stream_colon_is_replaced() {
        assert_eq!(served("a.txt:stream"), "a.txt_stream");
        assert_eq!(served("a.txt::$DATA"), "a.txt__$DATA");
    }

    #[test]
    fn windows_forbidden_chars_are_replaced() {
        assert_eq!(served("a<b>c|d?e*f.txt"), "a_b_c_d_e_f.txt");
        assert_eq!(sanitize("say \"hi\".txt"), "say _hi_.txt");
    }

    #[test]
    fn control_chars_are_replaced() {
        assert_eq!(sanitize("a\0b\tc\u{7F}d\u{85}e.txt"), "a_b_c_d_e.txt");
    }

    #[test]
    fn header_injection_line_breaks_are_neutralised() {
        let name = sanitize("report.txt\r\nSet-Cookie: id=1");
        assert_eq!(name, "report.txt__Set-Cookie_ id=1");
        assert!(!name.contains(['\r', '\n']));
    }

    #[test]
    fn bidi_controls_cannot_disguise_the_extension() {
        assert_eq!(sanitize("evil\u{202E}txt.exe"), "evil_txt.exe");
        for c in [
            '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{2066}',
            '\u{2067}', '\u{2068}', '\u{2069}',
        ] {
            assert_eq!(
                sanitize(&format!("a{c}b.txt")),
                "a_b.txt",
                "U+{:04X}",
                c as u32
            );
        }
    }

    #[test]
    fn trailing_dots_and_spaces_are_trimmed() {
        assert_eq!(served("name. . "), "name");
        assert_eq!(served("archive.zip..."), "archive.zip");
    }

    #[test]
    fn leading_dots_never_produce_a_hidden_file() {
        assert_eq!(served(".bashrc"), "bashrc");
        assert_eq!(served("..hidden.txt"), "hidden.txt");
        assert_eq!(served(" .profile"), "profile");
    }

    #[test]
    fn ordinary_unicode_names_are_untouched() {
        let name = "naïve 日本語 (v2) [final] #1 🎉.tar.gz";
        assert_eq!(served(name), name);
    }

    #[test]
    fn over_long_name_is_capped_keeping_the_extension() {
        let long = format!("{}.txt", "a".repeat(296));
        assert_eq!(long.len(), 300);
        let name = served(&long);
        assert_eq!(name.len(), MAX_NAME_BYTES);
        assert!(name.ends_with(".txt"));
        assert!(name.starts_with("aaaa"));
    }

    #[test]
    fn over_long_multibyte_name_is_cut_on_a_char_boundary() {
        let long = format!("{}.txt", "é".repeat(150));
        let name = served(&long);
        assert!(name.len() <= MAX_NAME_BYTES);
        assert!(name.ends_with(".txt"));
        assert!(name.trim_end_matches(".txt").chars().all(|c| c == 'é'));
    }

    #[test]
    fn over_long_name_with_no_short_extension_is_cut_whole() {
        assert_eq!(served(&"b".repeat(300)), "b".repeat(MAX_NAME_BYTES));
        let odd = format!("x.{}", "y".repeat(300));
        assert_eq!(served(&odd).len(), MAX_NAME_BYTES);
    }

    #[test]
    fn truncation_never_leaves_a_trailing_dot_or_space() {
        let long = format!("{} {}", "a".repeat(MAX_NAME_BYTES - 1), "b".repeat(50));
        assert_eq!(served(&long), "a".repeat(MAX_NAME_BYTES - 1));
    }

    #[test]
    fn join_in_keeps_a_plain_name_inside_dir() {
        let dir = Path::new("downloads");
        assert_eq!(join_in(dir, "file.bin"), dir.join("file.bin"));
    }

    #[test]
    fn join_in_rejects_anything_but_one_plain_component() {
        let dir = Path::new("downloads");
        for name in ["", ".", "..", "a/b", "/etc/passwd"] {
            assert_eq!(join_in(dir, name), dir.join("download"), "{name:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn join_in_rejects_windows_prefixes() {
        let dir = Path::new(r"C:\Users\me\Downloads");
        for name in [r"C:evil.exe", r"C:\x\evil.exe", r"\\server\share\x", r"a\b"] {
            assert_eq!(join_in(dir, name), dir.join("download"), "{name:?}");
        }
    }
}
