//! Troubleshooting commands: frontend errors into the log, the logs folder,
//! the Copy diagnostics report, and saving text (the Log tab's Save…).
//! Everything stays on the user's machine; the report is only ever put on the
//! clipboard for them to paste.

use std::path::{Path, PathBuf};

use tauri::{Manager as _, State};
use tauri_plugin_opener::OpenerExt;

use crate::{commands_guard, logging, startup_lines, test_seam, AppState, OPEN_LOG_ENV};

mod anonymize;

use anonymize::Known;

/// Log lines in the diagnostics report.
const TAIL_LINES: usize = 300;

/// The most of each frontend error field that reaches the log, so one huge
/// message can't swamp it.
const FIELD_LIMIT: usize = 8 * 1024;

/// When set, `save_text_file` writes to this path instead of asking with a
/// save dialog. Lets the webview E2E suite check what Save… writes. Test
/// builds only (see [`test_seam`]).
const SAVE_PATH_ENV: &str = "RDM_SAVE_PATH";

/// Record an error the webview caught (a render error, an uncaught error or
/// an unhandled rejection) in the app log at ERROR, target `rdm::frontend`.
#[tauri::command]
pub(crate) fn log_frontend_error(
    message: String,
    stack: Option<String>,
    component_stack: Option<String>,
    source: String,
) {
    log_frontend(
        &message,
        stack.as_deref(),
        component_stack.as_deref(),
        &source,
    );
}

/// [`log_frontend_error`]'s body: one ERROR event whose text holds the
/// message, then the stack and component stack on the following lines.
fn log_frontend(message: &str, stack: Option<&str>, component_stack: Option<&str>, source: &str) {
    let mut text = format!("{}: {}", clip(source), clip(message));
    for (label, value) in [("Stack", stack), ("Component stack", component_stack)] {
        // React's component stack starts with a line break; keep its indent.
        let value = value.map(|v| v.trim_start_matches(['\r', '\n']).trim_end());
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            text.push_str(&format!("\n{label}:\n{}", clip(value)));
        }
    }
    tracing::error!(target: "rdm::frontend", "{}", rdm::redact_urls_in(&text));
}

/// `text` cut to [`FIELD_LIMIT`] bytes (on a character boundary).
fn clip(text: &str) -> &str {
    if text.len() <= FIELD_LIMIT {
        return text;
    }
    let mut end = FIELD_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The log directory, for the Open logs folder tooltip (`None` without a
/// file log).
#[tauri::command]
pub(crate) fn logs_folder() -> Option<String> {
    logging::log_dir().map(|dir| dir.display().to_string())
}

/// Open the log directory in the OS file manager.
#[tauri::command]
pub(crate) fn open_logs_folder(app: tauri::AppHandle) -> Result<(), String> {
    let dir = logging::log_dir().ok_or("rdm has no log folder")?;
    if let Some(log) = test_seam(OPEN_LOG_ENV) {
        return crate::append_open_log(&log, "open", dir).map_err(|e| e.to_string());
    }
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// GitHub's new-issue page with the bug report form.
const NEW_BUG_REPORT_URL: &str = concat!(
    env!("CARGO_PKG_REPOSITORY"),
    "/issues/new?template=bug_report.yml"
);

/// Open the bug report form in the browser. Takes no URL, so the webview can
/// only open this one page.
#[tauri::command]
pub(crate) fn open_bug_report(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(log) = test_seam(OPEN_LOG_ENV) {
        return crate::append_open_log(&log, "open", Path::new(NEW_BUG_REPORT_URL))
            .map_err(|e| e.to_string());
    }
    app.opener()
        .open_url(NEW_BUG_REPORT_URL, None::<&str>)
        .map_err(|e| e.to_string())
}

/// The text Copy diagnostics puts on the clipboard: the version, OS, WebView,
/// memory and settings lines, the log folder, `extra` (the fallback screen's
/// error) and the last log lines, with links, download file names and
/// folders hidden (see [`anonymize`]).
/// Async, with the files read on the blocking pool, so a large log never
/// holds up the window.
#[tauri::command]
pub(crate) async fn diagnostics(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    extra: Option<String>,
) -> Result<String, String> {
    commands_guard::contained("diagnostics", async move {
        let header = startup_lines(&state.manager, logging::log_level());
        let known = known(&app, &state);
        tokio::task::spawn_blocking(move || {
            report(&header, logging::log_dir(), extra.as_deref(), known)
        })
        .await
        .map_err(|e| e.to_string())
    })
    .await
}

/// What the report hides: every download's file names, folders and site,
/// the default download folder and the home folder.
fn known(app: &tauri::AppHandle, state: &AppState) -> Known {
    let mut known = Known {
        home: app.path().home_dir().ok(),
        ..Known::default()
    };
    known.folders.extend(crate::default_dir(app).ok());
    for d in state.manager.snapshot() {
        known.files.push(d.filename);
        for path in std::iter::once(d.target).chain(d.final_path) {
            known.folders.extend(path.parent().map(Path::to_path_buf));
            known
                .files
                .extend(path.file_name().map(|n| n.to_string_lossy().into_owned()));
        }
        known.hosts.extend(anonymize::link_host(&d.url));
    }
    known
}

/// [`diagnostics`]' body, apart from the app. The log folder is shown under
/// `~` when it's in the home folder, else as a numbered folder.
fn report(header: &[String], dir: Option<&Path>, extra: Option<&str>, mut known: Known) -> String {
    let mut text = String::from("rdm diagnostics\n");
    for line in header {
        text.push_str(line);
        text.push('\n');
    }
    match dir {
        Some(dir) => {
            text.push_str(&format!("Logs: {}\n", dir.display()));
            if let Some(extra) = extra.map(str::trim).filter(|e| !e.is_empty()) {
                text.push_str(&format!("\n{extra}\n"));
            }
            let tail = log_tail(dir, TAIL_LINES);
            text.push_str(&format!("\nLast {} log lines:\n", tail.len()));
            for line in tail {
                text.push_str(&line);
                text.push('\n');
            }
        }
        None => {
            text.push_str("Logs: none (file logging is off)\n");
            if let Some(extra) = extra.map(str::trim).filter(|e| !e.is_empty()) {
                text.push_str(&format!("\n{extra}\n"));
            }
        }
    }
    if let Some(dir) = dir.filter(|dir| !known.home.as_ref().is_some_and(|h| dir.starts_with(h))) {
        known.folders.push(dir.to_path_buf());
    }
    anonymize::anonymize(&text, &known, cfg!(windows))
}

/// The last `n` lines of the daily log files in `dir`, newest file last,
/// reaching back into older files while the newer ones are short.
fn log_tail(dir: &Path, n: usize) -> Vec<String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_log_file(path))
        .collect();
    // `rdm.YYYY-MM-DD.log`: by name is by date.
    files.sort();
    let mut tail: Vec<String> = Vec::new();
    for file in files.iter().rev() {
        let Ok(mut lines) = last_lines(file, n - tail.len().min(n)) else {
            continue;
        };
        lines.append(&mut tail);
        tail = lines;
        if tail.len() >= n {
            break;
        }
    }
    let skip = tail.len().saturating_sub(n);
    tail.split_off(skip)
}

/// The last `n` lines of the file at `path`, read backwards from its end in
/// blocks, so a large log costs only its tail.
fn last_lines(path: &Path, n: usize) -> std::io::Result<Vec<String>> {
    use std::io::{Read, Seek, SeekFrom};
    const BLOCK: u64 = 64 * 1024;
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut file = std::fs::File::open(path)?;
    let mut pos = file.metadata()?.len();
    let mut buf: Vec<u8> = Vec::new();
    // n full lines need n line breaks before them (the file ends with one).
    while pos > 0 && buf.iter().filter(|&&b| b == b'\n').count() <= n {
        let step = BLOCK.min(pos);
        pos -= step;
        let mut block = vec![0; step as usize];
        file.seek(SeekFrom::Start(pos))?;
        file.read_exact(&mut block)?;
        block.append(&mut buf);
        buf = block;
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().collect();
    if pos > 0 && !lines.is_empty() {
        // The first line may have started before the block read.
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(n);
    Ok(lines[skip..].iter().map(|l| (*l).to_owned()).collect())
}

/// Whether `path` names one of the daily log files.
fn is_log_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.starts_with(&format!("{}.", logging::PREFIX))
        && name.ends_with(&format!(".{}", logging::SUFFIX))
}

/// Save `text` to a file the user picks in a save dialog, suggesting
/// `default_name`. Returns the path written, or `None` if they cancelled.
#[tauri::command]
pub(crate) async fn save_text_file(
    app: tauri::AppHandle,
    default_name: String,
    text: String,
) -> Result<Option<String>, String> {
    commands_guard::contained("save_text_file", async move {
        let path = match test_seam(SAVE_PATH_ENV) {
            Some(path) => Some(path),
            None => ask_save_path(&app, &suggested_name(&default_name)).await?,
        };
        let Some(path) = path else {
            return Ok(None);
        };
        std::fs::write(&path, text)
            .map_err(|e| format!("couldn't save {}: {e}", path.display()))?;
        Ok(Some(path.display().to_string()))
    })
    .await
}

/// Ask where to save, with a native save dialog.
async fn ask_save_path(app: &tauri::AppHandle, name: &str) -> Result<Option<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_file_name(name)
        .add_filter("Log", &["log", "txt"])
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    match rx.await.ok().flatten() {
        Some(path) => path.into_path().map(Some).map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

/// `name` as a bare file name for the save dialog: no folders, and not empty.
fn suggested_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    if base.is_empty() || base == "." || base == ".." {
        "rdm.log".to_owned()
    } else {
        base.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::{last_lines, log_frontend, log_tail, report, suggested_name, Known};

    fn write(path: &Path, lines: &[&str]) {
        let mut file = std::fs::File::create(path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    /// The report hides every link from the log, and has the version line,
    /// the folder and the log's last line.
    #[test]
    fn diagnostics_redact_and_end_with_the_log() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("rdm.2026-11-09.log"),
            &[
                "old line",
                "GET https://cdn.example/a.iso?X-Amz-Signature=SECRET1",
            ],
        );
        write(
            &dir.path().join("rdm.2026-11-10.log"),
            &[
                "Started https://alice:hunter2@files.example/b.zip",
                "the last line",
            ],
        );
        write(
            &dir.path().join("state.json"),
            &["https://x/?token=NOTALOG"],
        );
        let header = ["rdm 0.2.0 · Windows 11 x86_64".to_owned()];
        let text = report(
            &header,
            Some(dir.path()),
            Some("Render error: TypeError at https://h/?k=SECRET2"),
            Known::default(),
        );
        for secret in [
            "SECRET1",
            "SECRET2",
            "hunter2",
            "alice",
            "NOTALOG",
            "files.example",
            "b.zip",
        ] {
            assert!(!text.contains(secret), "leaked:\n{text}");
        }
        assert!(text.starts_with("rdm diagnostics\nrdm 0.2.0 · "), "{text}");
        assert!(text.contains("?<redacted>"), "{text}");
        assert!(
            text.contains("https://<redacted>:<redacted>@<site-3>/<path>"),
            "{text}"
        );
        // The log folder isn't under home, so it's a numbered folder.
        assert!(
            text.contains(
                "Logs: <folder-1>
"
            ),
            "{text}"
        );
        assert!(text.contains("Render error: TypeError"), "{text}");
        assert!(text.contains("\nLast 4 log lines:\nold line\n"), "{text}");
        assert!(text.ends_with("the last line\n"), "{text}");
    }

    #[test]
    fn the_tail_reaches_back_only_as_far_as_it_needs() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("rdm.2026-11-08.log"), &["a"]);
        write(&dir.path().join("rdm.2026-11-09.log"), &["b", "c"]);
        write(&dir.path().join("rdm.2026-11-10.log"), &["d", "e"]);
        assert_eq!(log_tail(dir.path(), 3), ["c", "d", "e"]);
        assert_eq!(log_tail(dir.path(), 2), ["d", "e"]);
        assert_eq!(log_tail(dir.path(), 10), ["a", "b", "c", "d", "e"]);
        assert!(log_tail(&dir.path().join("missing"), 3).is_empty());
    }

    /// Paths in log lines lose the home folder too, not just the Logs line,
    /// and a known download's folder and file name are hidden.
    #[test]
    fn the_report_carries_no_home_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let logs = home.join(".rdm");
        std::fs::create_dir(&logs).unwrap();
        let downloads = home.join("Downloads");
        let saved = format!(
            "Saved to {} and {}",
            home.join("x.iso").display(),
            downloads.join("y.iso").display()
        );
        write(&logs.join("rdm.2026-11-10.log"), &[&saved]);
        let known = Known {
            files: vec!["y.iso".into()],
            folders: vec![downloads],
            home: Some(home.to_path_buf()),
            ..Known::default()
        };
        let text = report(&[], Some(&logs), None, known);
        assert!(!text.contains(&home.display().to_string()), "{text}");
        assert!(!text.contains("y.iso"), "{text}");
        let sep = std::path::MAIN_SEPARATOR;
        assert!(
            text.contains(&format!(
                "Logs: ~{sep}.rdm
"
            )),
            "{text}"
        );
        assert!(
            text.contains(&format!(
                "Saved to ~{sep}x.iso and <folder-1>{sep}<file-1>.iso"
            )),
            "{text}"
        );
    }

    /// Only the tail is read, even from a file far larger than one block.
    #[test]
    fn last_lines_reads_a_big_file_from_its_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rdm.2026-11-10.log");
        let lines: Vec<String> = (0..50_000).map(|i| format!("line {i} é")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        write(&path, &refs);
        assert_eq!(
            last_lines(&path, 2).unwrap(),
            ["line 49998 é", "line 49999 é"]
        );
        assert_eq!(last_lines(&path, 0).unwrap(), Vec::<String>::new());
        let all = last_lines(&path, 60_000).unwrap();
        assert_eq!(all.len(), 50_000);
        assert_eq!(all[0], "line 0 é");
    }

    #[test]
    fn save_names_are_bare_file_names() {
        assert_eq!(suggested_name("ubuntu.iso.log"), "ubuntu.iso.log");
        assert_eq!(suggested_name("../../evil.log"), "evil.log");
        assert_eq!(suggested_name("a\\b.log"), "b.log");
        assert_eq!(suggested_name("  "), "rdm.log");
        assert_eq!(suggested_name(".."), "rdm.log");
    }

    /// Collects what a fmt layer writes.
    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// One ERROR event, target `rdm::frontend`, with the message and stacks,
    /// its URLs redacted.
    #[test]
    fn a_frontend_error_is_one_error_line_from_the_frontend() {
        let buffer = Buffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            log_frontend(
                "TypeError: x is undefined at https://h/p?token=SECRET",
                Some("at render (App.tsx:3)"),
                Some("    at LogTab\n    at App"),
                "render",
            );
        });
        let out = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        let first = out.lines().next().unwrap_or_default();
        assert!(
            first.contains("ERROR rdm::frontend: render: TypeError: x is undefined"),
            "{out}"
        );
        assert_eq!(out.matches("ERROR").count(), 1, "{out}");
        assert!(out.contains("Stack:\nat render (App.tsx:3)"), "{out}");
        assert!(
            out.contains("Component stack:\n    at LogTab\n    at App"),
            "{out}"
        );
        assert!(!out.contains("SECRET"), "{out}");
    }
}
