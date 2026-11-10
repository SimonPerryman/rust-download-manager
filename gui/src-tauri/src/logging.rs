//! The GUI's log file and panic hook. Lives apart from the Tauri glue so the
//! tests can drive it in a child process with no app.
//!
//! Normal lines go through a non-blocking writer whose worker thread writes
//! each one to the file as it arrives, so a crash loses only what's still
//! queued. The writer never drops lines under a burst (`lossy(false)`), and
//! after a warning or error it syncs the file to disk, at most every
//! [`SYNC_INTERVAL`]. The startup lines and a panic's record are appended
//! synchronously, so they reach the disk even if the process dies right after.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tracing_appender::non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard};
use tracing_appender::rolling::{Builder, RollingFileAppender, Rotation};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::util::SubscriberInitExt;

/// The log files' names: `rdm.YYYY-MM-DD.log` (UTC dates).
pub(crate) const PREFIX: &str = "rdm";
pub(crate) const SUFFIX: &str = "log";

/// Daily files kept.
const KEPT_FILES: usize = 7;

/// The least time between two syncs after warnings or errors, so a burst of
/// failing downloads doesn't pay for a disk flush per line.
pub(crate) const SYNC_INTERVAL: Duration = Duration::from_secs(5);

/// The thread that writes the file.
const WORKER_THREAD: &str = "rdm-log-writer";

/// How long a panic waits for the lines queued before it to be written, so
/// its record lands after them.
const PANIC_DRAIN: Duration = Duration::from_secs(1);

/// How long the exit flush waits for the queue to drain.
const EXIT_DRAIN: Duration = Duration::from_secs(10);

/// This build's version, and its commit when built from a git checkout.
pub(crate) fn version() -> String {
    let version = env!("CARGO_PKG_VERSION");
    match option_env!("RDM_GIT_SHA") {
        Some(sha) => format!("{version} ({sha})"),
        None => version.to_owned(),
    }
}

/// The non-blocking writer's guard: dropping it flushes and stops the worker.
static GUARD: OnceLock<Mutex<Option<WorkerGuard>>> = OnceLock::new();

/// The log directory, once logging is installed (the panic hook needs it).
static DIR: OnceLock<PathBuf> = OnceLock::new();

/// The file log's level filter, as installed (for the diagnostics report).
static LEVEL: OnceLock<String> = OnceLock::new();

/// The log directory, once logging is installed.
pub(crate) fn log_dir() -> Option<&'static Path> {
    DIR.get().map(PathBuf::as_path)
}

/// The file log's level filter, e.g. `info`, once logging is installed.
pub(crate) fn log_level() -> &'static str {
    LEVEL.get().map(String::as_str).unwrap_or("unknown")
}

/// File log lines handed to the worker, and lines it has written: equal once
/// the queue has drained.
static SENT: AtomicU64 = AtomicU64::new(0);
static WRITTEN: AtomicU64 = AtomicU64::new(0);

/// Install the tracing subscriber: `logs` (the Log tab's per-download layer,
/// which keeps its own level), plus `filter`ed output to stderr (visible under
/// `tauri dev`) and to the daily file in `dir`. Best-effort: logging must never
/// stop the app from starting.
pub(crate) fn install_logging<L>(dir: &Path, filter: tracing_subscriber::EnvFilter, logs: L)
where
    L: tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
{
    let _ = LEVEL.set(filter.to_string());
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(PREFIX)
        .filename_suffix(SUFFIX)
        .max_log_files(KEPT_FILES)
        .build(dir);
    let (file_writer, file_error) = match appender {
        Ok(appender) => {
            // Only with a file log: without one, nothing names a folder.
            let _ = DIR.set(dir.to_path_buf());
            let file = FileWriter {
                appender,
                dir: dir.to_path_buf(),
                throttle: SyncThrottle::default(),
            };
            let (writer, guard) = NonBlockingBuilder::default()
                .lossy(false)
                .thread_name(WORKER_THREAD)
                .finish(file);
            *GUARD
                .get_or_init(Mutex::default)
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(guard);
            (Some(Counted(writer)), None)
        }
        Err(e) => (None, Some(e)),
    };

    let _ = logging_subscriber(filter, logs, io::stderr, file_writer).try_init();
    if let Some(e) = file_error {
        tracing::warn!(dir = %dir.display(), error = %e, "file logging disabled");
    }
}

/// The subscriber [`install_logging`] installs: `logs`, plus `stderr` and the
/// optional `file` output. `filter` applies to the outputs only, so the Log
/// tab's layer keeps its own level whatever `RUST_LOG` says.
pub(crate) fn logging_subscriber<L, E, F>(
    filter: tracing_subscriber::EnvFilter,
    logs: L,
    stderr: E,
    file: Option<F>,
) -> impl tracing::Subscriber + Send + Sync + 'static
where
    L: tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
    E: for<'w> MakeWriter<'w> + Send + Sync + 'static,
    F: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    use tracing_subscriber::prelude::*;

    let output = tracing_subscriber::fmt::layer()
        .with_writer(stderr)
        .and_then(file.map(|writer| {
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer)
        }))
        .with_filter(filter);
    tracing_subscriber::registry().with(logs).with(output)
}

/// Write out every queued line, stop the file writer and sync the file. For
/// the end of the process (`RunEvent::Exit`): lines logged afterwards reach
/// stderr only.
pub(crate) fn flush_logs() {
    wait_for_queue(EXIT_DRAIN);
    let guard = GUARD
        .get()
        .and_then(|slot| slot.lock().unwrap_or_else(PoisonError::into_inner).take());
    drop(guard);
    if let Some(dir) = DIR.get() {
        let _ = sync_file(&today_log_path(dir));
    }
}

/// Wait up to `limit` for the worker to write every line queued so far.
fn wait_for_queue(limit: Duration) {
    let target = SENT.load(Ordering::SeqCst);
    let deadline = Instant::now() + limit;
    while WRITTEN.load(Ordering::SeqCst) < target && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Append `text` (one or more lines) to today's log file at once and sync it
/// to disk, after the lines already queued. For what must survive a crash
/// that follows right after: the startup lines and a panic's record.
pub(crate) fn write_synced(text: &str) -> io::Result<()> {
    let dir = DIR
        .get()
        .ok_or_else(|| io::Error::other("logging isn't installed"))?;
    wait_for_queue(PANIC_DRAIN);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(today_log_path(dir))?;
    let mut text = text.to_owned();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

/// Log each panic (on any thread, contained or not) before the default hook
/// runs: its record goes straight to the file and disk, then a one-line ERROR
/// goes through tracing, so a panic in a download task also shows in that
/// download's Log tab. It doesn't stop the file writer: a contained panic
/// leaves the app running, still logging.
pub(crate) fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info.payload_as_str().unwrap_or("panic with no message");
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        let thread = std::thread::current();
        let thread = thread.name().unwrap_or("<unnamed>");
        let backtrace = std::backtrace::Backtrace::force_capture();
        let record = panic_record(message, &location, thread, &backtrace.to_string());
        let _ = write_synced(&record);
        // The writer's own thread can't queue a line for itself: with the
        // queue full, it would wait on itself forever.
        if thread != WORKER_THREAD {
            tracing::error!(thread, location, "rdm panicked: {message}");
        }
        default_hook(info);
    }));
}

/// A panic's record: a first line in the file log's own shape, then the
/// backtrace.
fn panic_record(message: &str, location: &str, thread: &str, backtrace: &str) -> String {
    format!(
        "{} ERROR rdm::panic: rdm {} panicked on thread '{thread}' at {location}: {message}\n\
         Backtrace:\n{backtrace}",
        timestamp(SystemTime::now()),
        version(),
    )
}

/// Log `lines` at INFO, written synchronously to the file (so a crash right
/// after launch still leaves them) and to stderr.
pub(crate) fn log_startup(lines: &[String]) {
    let now = timestamp(SystemTime::now());
    let text: String = lines
        .iter()
        .map(|line| format!("{now}  INFO rdm::startup: {line}\n"))
        .collect();
    if write_synced(&text).is_err() {
        // No file log: say it where a developer may see it.
        for line in lines {
            tracing::info!("{line}");
        }
        return;
    }
    eprint!("{text}");
}

/// Today's (UTC) log file in `dir`, as the daily appender names it.
pub(crate) fn today_log_path(dir: &Path) -> PathBuf {
    let (y, m, d) = civil_date(days_since_epoch(SystemTime::now()));
    dir.join(format!("{PREFIX}.{y:04}-{m:02}-{d:02}.{SUFFIX}"))
}

/// Flush `path`'s data to disk. Opened for append so Windows grants the write
/// access its flush needs; nothing is written.
fn sync_file(path: &Path) -> io::Result<()> {
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)?
        .sync_all()
}

/// The file layer's writer: the non-blocking one, counting the lines handed
/// over so a panic or exit can wait for them to be written.
struct Counted(NonBlocking);

impl<'a> MakeWriter<'a> for Counted {
    type Writer = Counted;

    fn make_writer(&'a self) -> Self::Writer {
        Counted(self.0.clone())
    }
}

impl Write for Counted {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // The fmt layer writes each line with one call, which queues it whole.
        // Counted first, so a drain that starts after this line was queued
        // always waits for it.
        SENT.fetch_add(1, Ordering::SeqCst);
        self.0.write(buf).inspect_err(|_| {
            SENT.fetch_sub(1, Ordering::SeqCst);
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// What the worker thread writes through: the daily file, synced after a
/// warning or error at most every [`SYNC_INTERVAL`].
struct FileWriter {
    appender: RollingFileAppender,
    dir: PathBuf,
    throttle: SyncThrottle,
}

impl Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let result = self.appender.write_all(buf);
        WRITTEN.fetch_add(1, Ordering::SeqCst);
        result?;
        if self.throttle.after_line(is_urgent(buf), Instant::now()) {
            let _ = sync_file(&today_log_path(&self.dir));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.appender.flush()
    }
}

/// Whether a formatted line (`<timestamp> <LEVEL> …`) is a warning or error.
fn is_urgent(line: &[u8]) -> bool {
    let line = String::from_utf8_lossy(&line[..line.len().min(64)]);
    let level = line.split_whitespace().nth(1);
    matches!(level, Some("WARN" | "ERROR"))
}

/// Decides when the worker syncs: after a warning or error, unless it synced
/// less than [`SYNC_INTERVAL`] ago, in which case the next line once the
/// interval has passed does (the exit flush covers a quiet tail).
#[derive(Debug, Default)]
struct SyncThrottle {
    last_sync: Option<Instant>,
    pending: bool,
}

impl SyncThrottle {
    /// A line was just written; `urgent` if it was a warning or error.
    /// Returns whether to sync now.
    fn after_line(&mut self, urgent: bool, now: Instant) -> bool {
        self.pending |= urgent;
        let due = self
            .last_sync
            .is_none_or(|last| now.duration_since(last) >= SYNC_INTERVAL);
        if self.pending && due {
            self.pending = false;
            self.last_sync = Some(now);
            return true;
        }
        false
    }
}

/// `at` as the file log's RFC 3339 UTC timestamp, e.g.
/// `2026-11-10T12:34:56.789012Z`.
fn timestamp(at: SystemTime) -> String {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let (y, m, d) = civil_date(secs / 86_400);
    let day = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:06}Z",
        day / 3600,
        day / 60 % 60,
        day % 60,
        since.subsec_micros()
    )
}

fn days_since_epoch(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() / 86_400
}

/// The proleptic Gregorian (year, month, day) of `days` since 1970-01-01
/// (Howard Hinnant's `civil_from_days`).
fn civil_date(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + u64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    /// Set (to `panic` or `burst`) when this test binary is re-run as a child
    /// by the tests below; unset, [`child`] does nothing.
    const CHILD_ENV: &str = "RDM_LOG_TEST";
    const CHILD_DIR_ENV: &str = "RDM_LOG_TEST_DIR";
    const BURST_LINES: usize = 200_000;

    /// The child process: install logging and the panic hook as the app does,
    /// then panic, or log a burst and exit through [`flush_logs`].
    #[test]
    fn child() {
        let Some(mode) = std::env::var_os(CHILD_ENV) else {
            return;
        };
        let dir = PathBuf::from(std::env::var_os(CHILD_DIR_ENV).expect("a log dir"));
        install_logging(
            &dir,
            tracing_subscriber::EnvFilter::new("info"),
            tracing_subscriber::layer::Identity::new(),
        );
        install_panic_hook();
        if mode == "panic" {
            log_startup(&[format!("rdm {} · child test", version())]);
            tracing::info!("the line before the panic");
            panic!("child panic for the log test");
        }
        for i in 0..BURST_LINES {
            tracing::info!(i, "burst line");
        }
        // A slow runner may need longer than the exit flush's own wait;
        // what's under test is that no line is dropped, not the disk's speed.
        wait_for_queue(Duration::from_secs(120));
        flush_logs();
    }

    /// Run [`child`] in a fresh process in `mode`, logging to `dir`.
    fn run_child(mode: &str, dir: &Path) -> std::process::ExitStatus {
        Command::new(std::env::current_exe().unwrap())
            .args(["logging::tests::child", "--exact", "--nocapture"])
            .env(CHILD_ENV, mode)
            .env(CHILD_DIR_ENV, dir)
            .env_remove("RUST_LOG")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("spawn the child test")
    }

    /// Every log file's text in `dir`, oldest first (a run can span UTC midnight).
    fn logged(dir: &Path) -> String {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == SUFFIX))
            .collect();
        files.sort();
        files
            .iter()
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect()
    }

    /// A panic's record reaches the file, after the startup line, with its
    /// message and where it happened.
    #[test]
    fn a_panic_reaches_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let status = run_child("panic", dir.path());
        assert!(!status.success(), "the child should have failed: {status}");

        let text = logged(dir.path());
        let startup = text
            .find(&format!("rdm {} · child test", version()))
            .unwrap_or_else(|| panic!("no startup line in:\n{text}"));
        let record = text
            .find("panicked on thread")
            .unwrap_or_else(|| panic!("no panic record in:\n{text}"));
        assert!(startup < record, "{text}");
        let record = &text[record..];
        assert!(record.contains("child panic for the log test"), "{record}");
        let at = record.find("logging.rs:").expect("the panic's file") + "logging.rs:".len();
        assert!(
            record[at..].starts_with(|c: char| c.is_ascii_digit()),
            "{record}"
        );
        assert!(text.contains("the line before the panic"), "{text}");
    }

    /// A burst far bigger than the writer's queue is written in full: the
    /// writer blocks rather than dropping lines, and the exit flush waits.
    #[test]
    fn a_burst_is_not_lossy() {
        let dir = tempfile::tempdir().unwrap();
        let status = run_child("burst", dir.path());
        assert!(status.success(), "the child failed: {status}");
        let lines = logged(dir.path()).matches("burst line").count();
        assert_eq!(lines, BURST_LINES);
    }

    #[test]
    fn civil_dates_and_timestamps() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(19_723), (2024, 1, 1));
        assert_eq!(civil_date(19_782), (2024, 2, 29));
        let at = UNIX_EPOCH + Duration::from_micros(1_762_778_096_000_123);
        assert_eq!(timestamp(at), "2025-11-10T12:34:56.000123Z");
    }

    #[test]
    fn only_warnings_and_errors_are_urgent() {
        assert!(is_urgent(b"2026-11-10T12:00:00.000000Z  WARN rdm: x\n"));
        assert!(is_urgent(b"2026-11-10T12:00:00.000000Z ERROR rdm: x\n"));
        assert!(!is_urgent(
            b"2026-11-10T12:00:00.000000Z  INFO rdm: ERROR in text\n"
        ));
        assert!(!is_urgent(b""));
    }

    /// The first warning syncs at once; more within the interval wait, and
    /// the next line after it syncs them; quiet lines never do.
    #[test]
    fn syncs_are_rate_limited() {
        let start = Instant::now();
        let at = |secs: u64| start + Duration::from_secs(secs);
        let mut throttle = SyncThrottle::default();
        assert!(!throttle.after_line(false, at(0)));
        assert!(throttle.after_line(true, at(1)));
        assert!(!throttle.after_line(true, at(2)));
        assert!(!throttle.after_line(false, at(5)));
        assert!(
            throttle.after_line(false, at(6)),
            "the held-back sync is due"
        );
        assert!(!throttle.after_line(false, at(20)), "nothing pending");
        assert!(throttle.after_line(true, at(21)));
    }
}
