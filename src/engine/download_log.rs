//! Each download's recent log lines, kept in memory so a frontend can show them.
//!
//! [`LogLayer`] is a tracing layer: it tags the Manager's `task` span with its
//! download id and files every event under it into that download's bounded
//! buffer. INFO-and-up lines and DEBUG ones (the "detailed" view: per-request
//! detail) are capped separately, so a burst of detail never pushes out the
//! lines that say what happened. It records only the event's own message and
//! fields, never span fields (the `download` span's URL), so it shows nothing
//! the redacted file log doesn't.
//!
//! INFO-and-up lines are also appended, as JSON lines, to `<id>.jsonl` in a
//! `logs` folder beside the download list, and read back (lazily, the first
//! time a download's log is touched) after a restart. DEBUG lines never reach
//! disk. [`prune`] bounds what stays there.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::{self, Write as _};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::field::{Field, Visit};
use tracing::{span, Event, Level, Subscriber};
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

use crate::engine::lock::lock;

/// How many INFO-and-up lines each download keeps; the oldest go first.
pub const LOG_LINES: usize = 200;

/// How many DEBUG lines each download keeps, apart from [`LOG_LINES`].
pub(crate) const DETAILED_LOG_LINES: usize = 300;

/// Log files untouched for this long (90 days) are deleted at start, so the
/// logs of downloads nobody has looked at in a quarter don't pile up.
pub(crate) const LOG_MAX_AGE: Duration = Duration::from_secs(90 * 24 * 60 * 60);

/// The most bytes all log files may take together (20 MB); at start the least
/// recently written files go first until they fit.
pub(crate) const LOG_TOTAL_CAP: u64 = 20 * 1024 * 1024;

/// A log file that grows past this many lines is rewritten with only the last
/// [`LOG_LINES`], so one noisy download can't grow its file without bound.
pub(crate) const LOG_FILE_MAX_LINES: usize = 1000;

/// The span the Manager runs each download under; its `id` field keys the lines.
pub(crate) const TASK_SPAN: &str = "task";

/// Longest line text kept, in bytes, so one huge error can't defeat the bound.
const MAX_TEXT: usize = 2000;

/// How serious a [`LogLine`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<&Level> for LogLevel {
    fn from(level: &Level) -> Self {
        match *level {
            Level::ERROR => Self::Error,
            Level::WARN => Self::Warn,
            Level::INFO => Self::Info,
            Level::DEBUG => Self::Debug,
            Level::TRACE => Self::Trace,
        }
    }
}

/// One line a download logged, for a frontend's log view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLine {
    /// When it was logged, in milliseconds since the Unix epoch.
    pub at: u64,
    pub level: LogLevel,
    /// The event's message, e.g. `Paused`.
    pub message: String,
    /// The event's other fields as `name=value` pairs, space-separated.
    pub fields: String,
}

/// Every download's buffer. A buffer exists only between [`open`](Self::open)
/// and [`forget`](Self::forget), so a late line from an aborted task can't
/// bring a removed download's buffer back.
#[derive(Default)]
pub(crate) struct DownloadLogs {
    buffers: Mutex<HashMap<u64, Buffer>>,
    /// Where INFO-and-up lines are kept across restarts; `None` keeps them in
    /// memory only.
    dir: Option<PathBuf>,
    /// Set once a disk failure has been reported, so it is reported once.
    warned: AtomicBool,
}

/// One download's lines, numbered in arrival order so the two caps' lines
/// merge back into one log.
#[derive(Default)]
struct Buffer {
    next: u64,
    concise: VecDeque<(u64, LogLine)>,
    detailed: VecDeque<(u64, LogLine)>,
    /// The saved lines haven't been read back yet.
    unloaded: bool,
    /// Lines in the file, to know when to rewrite it shorter.
    file_lines: usize,
    /// The file is known to end in a newline (or not exist), so the next
    /// line can be appended as is; a torn tail from a crash gets one first.
    tail_clean: bool,
}

impl Buffer {
    fn new(dir: Option<&Path>) -> Self {
        Self {
            unloaded: dir.is_some(),
            ..Self::default()
        }
    }

    fn push_mem(&mut self, line: LogLine) {
        let (lines, cap) = match line.level {
            LogLevel::Debug | LogLevel::Trace => (&mut self.detailed, DETAILED_LOG_LINES),
            _ => (&mut self.concise, LOG_LINES),
        };
        if lines.len() == cap {
            lines.pop_front();
        }
        lines.push_back((self.next, line));
        self.next += 1;
    }

    /// Read the saved lines in, before any newer one is added. A missing or
    /// damaged file just means fewer (or no) lines.
    fn load(&mut self, id: u64, dir: Option<&Path>) {
        if !std::mem::take(&mut self.unloaded) {
            return;
        }
        let Some(dir) = dir else { return };
        let text = match std::fs::read_to_string(log_path(dir, id)) {
            Ok(text) => text,
            Err(e) => {
                self.tail_clean = e.kind() == std::io::ErrorKind::NotFound;
                return;
            }
        };
        self.tail_clean = text.is_empty() || text.ends_with('\n');
        let saved: Vec<LogLine> = text
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        self.file_lines = saved.len();
        let skip = saved.len().saturating_sub(LOG_LINES);
        for line in saved.into_iter().skip(skip) {
            self.push_mem(line);
        }
    }

    /// Add `line`, saving it too if it is INFO or up.
    fn push(&mut self, id: u64, dir: Option<&Path>, line: LogLine) -> std::io::Result<()> {
        self.load(id, dir);
        let keep = !matches!(line.level, LogLevel::Debug | LogLevel::Trace);
        let json = serde_json::to_string(&line);
        self.push_mem(line);
        match (keep, dir, json) {
            (true, Some(dir), Ok(json)) => self.append(id, dir, &json),
            _ => Ok(()),
        }
    }

    fn append(&mut self, id: u64, dir: &Path, json: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = log_path(dir, id);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let lead = if self.tail_clean { "" } else { "\n" };
        file.write_all(format!("{lead}{json}\n").as_bytes())?;
        self.tail_clean = true;
        self.file_lines += 1;
        if self.file_lines > LOG_FILE_MAX_LINES {
            self.rewrite_short(&path)?;
        }
        Ok(())
    }

    /// Replace the file with just the lines still in the buffer.
    fn rewrite_short(&mut self, path: &Path) -> std::io::Result<()> {
        let mut text = String::new();
        for (_, line) in &self.concise {
            if let Ok(json) = serde_json::to_string(line) {
                text.push_str(&json);
                text.push('\n');
            }
        }
        let tmp = path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        self.tail_clean = true;
        self.file_lines = self.concise.len();
        Ok(())
    }

    fn lines(&self) -> Vec<LogLine> {
        let mut all: Vec<_> = self.concise.iter().chain(&self.detailed).collect();
        all.sort_unstable_by_key(|(seq, _)| *seq);
        all.into_iter().map(|(_, line)| line.clone()).collect()
    }
}

fn log_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id}.jsonl"))
}

impl DownloadLogs {
    /// Keep INFO-and-up lines in `dir` (`None`: in memory only).
    pub(crate) fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            ..Self::default()
        }
    }

    /// Start keeping lines for `id` (keeps any it already has); its saved
    /// lines load when it is first read or written.
    pub(crate) fn open(&self, id: u64) {
        lock(&self.buffers)
            .entry(id)
            .or_insert_with(|| Buffer::new(self.dir.as_deref()));
    }

    /// Drop `id`'s lines, delete its file, and stop keeping new ones. Done
    /// under the lock so a racing line can't write the file back.
    pub(crate) fn forget(&self, id: u64) {
        let mut buffers = lock(&self.buffers);
        buffers.remove(&id);
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_file(log_path(dir, id));
        }
    }

    /// `id`'s lines, oldest first (none if it isn't open).
    pub(crate) fn lines(&self, id: u64) -> Vec<LogLine> {
        let mut buffers = lock(&self.buffers);
        let Some(buffer) = buffers.get_mut(&id) else {
            return Vec::new();
        };
        buffer.load(id, self.dir.as_deref());
        buffer.lines()
    }

    /// File one line straight into `id`'s log (opening it if the download
    /// hasn't run this session), for work that happens outside a download's task.
    pub(crate) fn record(&self, id: u64, level: LogLevel, message: String) {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let line = LogLine {
            at,
            level,
            message: capped(message),
            fields: String::new(),
        };
        let result = lock(&self.buffers)
            .entry(id)
            .or_insert_with(|| Buffer::new(self.dir.as_deref()))
            .push(id, self.dir.as_deref(), line);
        self.report(id, result);
    }

    fn push(&self, id: u64, line: LogLine) {
        let result = lock(&self.buffers)
            .get_mut(&id)
            .map(|buffer| buffer.push(id, self.dir.as_deref(), line));
        if let Some(result) = result {
            self.report(id, result);
        }
    }

    /// Say once that the disk copy is failing; the in-memory log carries on.
    /// Called with no lock held. The note goes into `id`'s own log too: a
    /// tracing event raised from inside the log layer is dropped by tracing's
    /// re-entrancy guard, so it alone could go unseen.
    fn report(&self, id: u64, result: std::io::Result<()>) {
        let Err(error) = result else { return };
        if self.warned.swap(true, Ordering::Relaxed) {
            return;
        }
        let message =
            format!("Couldn't save download logs to disk ({error}); they won't survive a restart");
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        if let Some(buffer) = lock(&self.buffers).get_mut(&id) {
            buffer.push_mem(LogLine {
                at,
                level: LogLevel::Warn,
                message: capped(message.clone()),
                fields: String::new(),
            });
        }
        tracing::warn!("{message}");
    }
}

/// Delete log files that are orphans (an id below `next_id` that isn't in
/// `live`), older than `max_age`, or past `total_cap` bytes together, least
/// recently written first. Best-effort; other files in `dir` are left alone.
pub(crate) fn prune(
    dir: &Path,
    live: &HashSet<u64>,
    next_id: u64,
    now: SystemTime,
    max_age: Duration,
    total_cap: u64,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut kept = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(id) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".jsonl"))
            .and_then(|n| n.parse::<u64>().ok())
        else {
            continue;
        };
        let Ok(meta) = entry.metadata() else { continue };
        let modified = meta.modified().unwrap_or(now);
        let orphan = id < next_id && !live.contains(&id);
        let old = now.duration_since(modified).is_ok_and(|age| age > max_age);
        if orphan || old {
            let _ = std::fs::remove_file(&path);
        } else {
            kept.push((modified, meta.len(), path));
        }
    }
    kept.sort();
    let mut total: u64 = kept.iter().map(|(_, len, _)| len).sum();
    for (_, len, path) in kept {
        if total <= total_cap {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total -= len;
        }
    }
}

/// What [`LogLayer`] keeps: INFO and up from anywhere, as the file log does,
/// plus the engine's own DEBUG detail. Other crates' DEBUG lines are theirs to
/// explain, and may name URLs.
pub(crate) fn capture_filter() -> Targets {
    Targets::new()
        .with_default(LevelFilter::INFO)
        .with_target("rdm::engine", LevelFilter::DEBUG)
}

/// The download id a `task` span carries, stored in the span's extensions.
struct TaskId(u64);

/// Files events logged inside a download's `task` span into [`DownloadLogs`].
pub(crate) struct LogLayer {
    pub(crate) logs: std::sync::Arc<DownloadLogs>,
}

impl<S> Layer<S> for LogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let meta = attrs.metadata();
        if meta.name() != TASK_SPAN || !meta.target().starts_with("rdm::") {
            return;
        }
        let mut visitor = IdVisitor(None);
        attrs.record(&mut visitor);
        if let (Some(task), Some(span)) = (visitor.0, ctx.span(id)) {
            span.extensions_mut().insert(TaskId(task));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let Some(task) = ctx.event_scope(event).and_then(|mut scope| {
            scope.find_map(|span| span.extensions().get::<TaskId>().map(|t| t.0))
        }) else {
            return;
        };
        let mut visitor = LineVisitor::default();
        event.record(&mut visitor);
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        self.logs.push(
            task,
            LogLine {
                at,
                level: event.metadata().level().into(),
                message: capped(visitor.message),
                fields: capped(visitor.fields),
            },
        );
    }
}

/// `text` cut to at most [`MAX_TEXT`] bytes, on a character boundary.
fn capped(mut text: String) -> String {
    if text.len() > MAX_TEXT {
        let mut end = MAX_TEXT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    text
}

/// Reads a `task` span's `id` field.
struct IdVisitor(Option<u64>);

impl Visit for IdVisitor {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "id" {
            self.0 = Some(value);
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == "id" {
            self.0 = u64::try_from(value).ok();
        }
    }

    fn record_debug(&mut self, _: &Field, _: &dyn fmt::Debug) {}
}

/// Collects an event's message and its other fields.
#[derive(Default)]
struct LineVisitor {
    message: String,
    fields: String,
}

impl LineVisitor {
    fn separator(&mut self) {
        if !self.fields.is_empty() {
            self.fields.push(' ');
        }
    }
}

impl Visit for LineVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            self.separator();
            let _ = write!(self.fields, "{}={value}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            self.separator();
            let _ = write!(self.fields, "{}={value:?}", field.name());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tracing_subscriber::prelude::*;

    use super::*;

    /// Run `f` with a subscriber whose only layer files lines into `logs`.
    fn with_layer(logs: &Arc<DownloadLogs>, f: impl FnOnce()) {
        let subscriber = tracing_subscriber::registry().with(LogLayer { logs: logs.clone() });
        tracing::subscriber::with_default(subscriber, f);
    }

    #[test]
    fn keeps_the_last_lines_per_download() {
        let logs = Arc::new(DownloadLogs::default());
        logs.open(1);
        with_layer(&logs, || {
            let _task = tracing::error_span!("task", id = 1u64).entered();
            for i in 0..LOG_LINES + 50 {
                tracing::info!(i, "line");
            }
        });
        let lines = logs.lines(1);
        assert_eq!(lines.len(), LOG_LINES);
        assert_eq!(lines[0].fields, "i=50");
        assert_eq!(lines[LOG_LINES - 1].fields, format!("i={}", LOG_LINES + 49));
        assert_eq!(lines[0].message, "line");
        assert_eq!(lines[0].level, LogLevel::Info);
    }

    /// Detail has its own cap: a flood of DEBUG lines keeps the INFO ones,
    /// and the two read back interleaved in the order they were logged.
    #[test]
    fn detail_never_pushes_out_the_concise_lines() {
        let logs = Arc::new(DownloadLogs::default());
        logs.open(1);
        with_layer(&logs, || {
            let _task = tracing::error_span!("task", id = 1u64).entered();
            tracing::info!("started");
            for i in 0..DETAILED_LOG_LINES + 10 {
                tracing::debug!(i, "detail");
            }
            tracing::warn!("retrying");
            tracing::debug!("last detail");
        });
        let lines = logs.lines(1);
        assert_eq!(lines.len(), 2 + DETAILED_LOG_LINES);
        assert_eq!(lines[0].message, "started");
        assert_eq!(lines[1].fields, "i=11");
        let n = lines.len();
        assert_eq!(
            (lines[n - 2].message.as_str(), lines[n - 1].message.as_str()),
            ("retrying", "last detail")
        );
        assert_eq!(lines[n - 1].level, LogLevel::Debug);
    }

    #[test]
    fn files_lines_under_the_innermost_task_and_skips_the_rest() {
        let logs = Arc::new(DownloadLogs::default());
        logs.open(1);
        logs.open(2);
        with_layer(&logs, || {
            tracing::warn!("outside any task");
            let _one = tracing::error_span!("task", id = 1u64).entered();
            let _inner = tracing::error_span!("download", url = "x").entered();
            tracing::warn!(error = %"boom", "one");
            {
                let _two = tracing::error_span!("task", id = 2u64).entered();
                tracing::error!(name = "a b", "two");
            }
            // An unopened id keeps nothing.
            let _three = tracing::error_span!("task", id = 3u64).entered();
            tracing::info!("three");
        });
        let one = logs.lines(1);
        assert_eq!(one.len(), 1, "{one:?}");
        assert_eq!(
            (
                one[0].level,
                one[0].message.as_str(),
                one[0].fields.as_str()
            ),
            (LogLevel::Warn, "one", "error=boom")
        );
        let two = logs.lines(2);
        assert_eq!(two.len(), 1, "{two:?}");
        assert_eq!(
            (
                two[0].level,
                two[0].message.as_str(),
                two[0].fields.as_str()
            ),
            (LogLevel::Error, "two", "name=a b")
        );
        assert!(logs.lines(3).is_empty());
    }

    #[test]
    fn forget_drops_lines_and_stops_keeping_new_ones() {
        let logs = Arc::new(DownloadLogs::default());
        logs.open(7);
        with_layer(&logs, || {
            let _task = tracing::error_span!("task", id = 7u64).entered();
            tracing::info!("before");
            logs.forget(7);
            tracing::info!("after");
        });
        assert!(logs.lines(7).is_empty());
    }

    /// The capture filter keeps the engine's own detail but not other
    /// crates' (which may name URLs), nor anyone's TRACE; INFO and up is kept
    /// from anywhere.
    #[test]
    fn detail_is_kept_only_from_the_engine() {
        let logs = Arc::new(DownloadLogs::default());
        logs.open(1);
        let layer = LogLayer { logs: logs.clone() }.with_filter(capture_filter());
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            let _task =
                tracing::error_span!(target: "rdm::engine::manager", "task", id = 1u64).entered();
            tracing::debug!(target: "rdm::engine::segment", "engine detail");
            tracing::trace!(target: "rdm::engine::segment", "engine trace");
            tracing::debug!(target: "reqwest::connect", "http://host/?token=SECRET");
            tracing::debug!(target: "rdm_gui", "gui detail");
            tracing::info!(target: "hyper::client", "foreign info");
        });
        let kept: Vec<_> = logs.lines(1).into_iter().map(|l| l.message).collect();
        assert_eq!(kept, ["engine detail", "foreign info"]);
    }

    #[test]
    fn caps_long_text_on_a_char_boundary() {
        let text = capped("é".repeat(MAX_TEXT));
        assert!(text.len() <= MAX_TEXT + '…'.len_utf8());
        assert!(text.ends_with('…'));
        assert_eq!(capped("short".into()), "short");
    }

    fn on_disk(dir: &Path) -> Arc<DownloadLogs> {
        Arc::new(DownloadLogs::new(Some(dir.to_path_buf())))
    }

    fn messages(logs: &DownloadLogs, id: u64) -> Vec<String> {
        logs.lines(id).into_iter().map(|l| l.message).collect()
    }

    #[test]
    fn saved_lines_survive_a_restart_but_debug_ones_do_not() {
        let tmp = tempfile::tempdir().unwrap();
        let logs = on_disk(tmp.path());
        logs.open(1);
        with_layer(&logs, || {
            let _task = tracing::error_span!("task", id = 1u64).entered();
            tracing::info!("started");
            tracing::debug!("detail");
            tracing::warn!(error = %"boom", "failed");
        });
        logs.record(1, LogLevel::Info, "Relinked to x".into());
        assert_eq!(
            messages(&logs, 1),
            ["started", "detail", "failed", "Relinked to x"]
        );

        let again = on_disk(tmp.path());
        again.open(1);
        let lines = again.lines(1);
        assert_eq!(
            lines.iter().map(|l| l.message.as_str()).collect::<Vec<_>>(),
            ["started", "failed", "Relinked to x"]
        );
        assert_eq!(lines[1].fields, "error=boom");
        assert_eq!(lines[1].level, LogLevel::Warn);
        // New lines follow the restored ones.
        again.record(1, LogLevel::Info, "later".into());
        assert_eq!(messages(&again, 1).last().unwrap(), "later");
    }

    #[test]
    fn only_the_last_lines_load_and_a_torn_line_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let logs = on_disk(tmp.path());
        logs.open(1);
        for i in 0..LOG_LINES + 5 {
            logs.record(1, LogLevel::Info, format!("line {i}"));
        }
        let path = tmp.path().join("1.jsonl");
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("{\"at\":1,\"lev");
        std::fs::write(&path, text).unwrap();

        let again = on_disk(tmp.path());
        again.open(1);
        let kept = messages(&again, 1);
        assert_eq!(kept.len(), LOG_LINES);
        assert_eq!(kept[0], "line 5");
    }

    /// A crash mid-write leaves a fragment with no newline; the next saved
    /// line must start on its own line, not be glued onto it.
    #[test]
    fn a_torn_tail_does_not_swallow_the_next_line() {
        let tmp = tempfile::tempdir().unwrap();
        let logs = on_disk(tmp.path());
        logs.record(1, LogLevel::Info, "before".into());
        let path = tmp.path().join("1.jsonl");
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("{\"at\":1,\"level\":\"info\",\"mess");
        std::fs::write(&path, text).unwrap();

        let restarted = on_disk(tmp.path());
        restarted.open(1);
        restarted.record(1, LogLevel::Info, "after".into());

        let again = on_disk(tmp.path());
        again.open(1);
        assert_eq!(messages(&again, 1), ["before", "after"]);
    }

    #[test]
    fn forget_deletes_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let logs = on_disk(tmp.path());
        logs.record(1, LogLevel::Info, "hello".into());
        assert!(tmp.path().join("1.jsonl").exists());
        logs.forget(1);
        assert!(!tmp.path().join("1.jsonl").exists());
        logs.record(2, LogLevel::Info, "other".into());
        assert!(tmp.path().join("2.jsonl").exists());
    }

    #[test]
    fn a_noisy_file_is_cut_back_to_the_last_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let logs = on_disk(tmp.path());
        for i in 0..LOG_FILE_MAX_LINES + 1 {
            logs.record(1, LogLevel::Info, format!("line {i}"));
        }
        let text = std::fs::read_to_string(tmp.path().join("1.jsonl")).unwrap();
        assert_eq!(text.lines().count(), LOG_LINES);
        assert!(text
            .lines()
            .last()
            .unwrap()
            .contains(&format!("line {LOG_FILE_MAX_LINES}")));
        // And it keeps appending afterwards.
        logs.record(1, LogLevel::Info, "next".into());
        let again = on_disk(tmp.path());
        again.open(1);
        assert_eq!(messages(&again, 1).last().unwrap(), "next");
    }

    #[test]
    fn a_write_failure_keeps_the_memory_log_and_warns_once() {
        let tmp = tempfile::tempdir().unwrap();
        // A file where the folder should be, so nothing can be created in it.
        let blocked = tmp.path().join("logs");
        std::fs::write(&blocked, "in the way").unwrap();
        let logs = on_disk(&blocked);
        logs.open(1);
        with_layer(&logs, || {
            let _task = tracing::error_span!("task", id = 1u64).entered();
            tracing::info!("one");
            tracing::info!("two");
        });
        logs.record(1, LogLevel::Info, "three".into());
        let kept = messages(&logs, 1);
        assert_eq!(kept[0], "one");
        assert!(
            kept[1].starts_with("Couldn't save download logs"),
            "{kept:?}"
        );
        assert_eq!(kept.iter().filter(|m| m.starts_with("Couldn't")).count(), 1);
        assert!(kept.contains(&"two".to_string()) && kept.contains(&"three".to_string()));
    }

    fn touch(dir: &Path, name: &str, bytes: usize, age_days: u64, now: SystemTime) {
        let path = dir.join(name);
        std::fs::write(&path, vec![b'x'; bytes]).unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(now - Duration::from_secs(age_days * 86_400))
            .unwrap();
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut all: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        all.sort();
        all
    }

    #[test]
    fn prune_drops_old_orphaned_and_over_cap_files() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let now = SystemTime::now();
        touch(d, "1.jsonl", 10, 100, now); // too old
        touch(d, "2.jsonl", 10, 1, now); // kept
        touch(d, "3.jsonl", 10, 1, now); // orphan: below next_id, not live
        touch(d, "9.jsonl", 10, 1, now); // not below next_id: kept
        touch(d, "notes.txt", 10, 200, now); // not a log: left alone
        let live: HashSet<u64> = [1, 2].into();
        prune(d, &live, 5, now, Duration::from_secs(90 * 86_400), 1000);
        assert_eq!(names(d), ["2.jsonl", "9.jsonl", "notes.txt"]);
    }

    #[test]
    fn prune_drops_the_oldest_first_to_fit_the_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let now = SystemTime::now();
        touch(d, "1.jsonl", 100, 3, now);
        touch(d, "2.jsonl", 100, 10, now);
        touch(d, "3.jsonl", 100, 1, now);
        let live: HashSet<u64> = [1, 2, 3].into();
        prune(d, &live, 4, now, Duration::from_secs(90 * 86_400), 250);
        assert_eq!(names(d), ["1.jsonl", "3.jsonl"]);
        prune(d, &live, 4, now, Duration::from_secs(90 * 86_400), 100);
        assert_eq!(names(d), ["3.jsonl"]);
    }
}
