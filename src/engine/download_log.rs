//! Each download's recent log lines, kept in memory so a frontend can show them.
//!
//! [`LogLayer`] is a tracing layer: it tags the Manager's `task` span with its
//! download id and files every event under it into that download's bounded
//! buffer. It records only the event's own message and fields, never span
//! fields (the `download` span's URL), so it shows nothing the redacted file
//! log doesn't.

use std::collections::{HashMap, VecDeque};
use std::fmt::{self, Write as _};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::{span, Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// How many lines each download keeps; the oldest go first.
pub const LOG_LINES: usize = 200;

/// The span the Manager runs each download under; its `id` field keys the lines.
pub(crate) const TASK_SPAN: &str = "task";

/// Longest line text kept, in bytes, so one huge error can't defeat the bound.
const MAX_TEXT: usize = 2000;

/// How serious a [`LogLine`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogLine {
    /// When it was logged, in milliseconds since the Unix epoch.
    pub at: u64,
    pub level: LogLevel,
    /// The event's message, e.g. `download failed`.
    pub message: String,
    /// The event's other fields as `name=value` pairs, space-separated.
    pub fields: String,
}

/// Every download's buffer. A buffer exists only between [`open`](Self::open)
/// and [`forget`](Self::forget), so a late line from an aborted task can't
/// bring a removed download's buffer back.
#[derive(Default)]
pub(crate) struct DownloadLogs {
    buffers: Mutex<HashMap<u64, VecDeque<LogLine>>>,
}

impl DownloadLogs {
    /// Start keeping lines for `id` (keeps any it already has).
    pub(crate) fn open(&self, id: u64) {
        self.buffers.lock().unwrap().entry(id).or_default();
    }

    /// Drop `id`'s lines and stop keeping new ones.
    pub(crate) fn forget(&self, id: u64) {
        self.buffers.lock().unwrap().remove(&id);
    }

    /// `id`'s lines, oldest first (none if it isn't open).
    pub(crate) fn lines(&self, id: u64) -> Vec<LogLine> {
        self.buffers
            .lock()
            .unwrap()
            .get(&id)
            .map(|b| b.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn push(&self, id: u64, line: LogLine) {
        if let Some(buffer) = self.buffers.lock().unwrap().get_mut(&id) {
            if buffer.len() == LOG_LINES {
                buffer.pop_front();
            }
            buffer.push_back(line);
        }
    }
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

    #[test]
    fn caps_long_text_on_a_char_boundary() {
        let text = capped("é".repeat(MAX_TEXT));
        assert!(text.len() <= MAX_TEXT + '…'.len_utf8());
        assert!(text.ends_with('…'));
        assert_eq!(capped("short".into()), "short");
    }
}
