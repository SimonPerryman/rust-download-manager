//! The CLI's `--json` event stream: one JSON object per line on stdout.
//! Presentation only: the events are built from the engine's [`Progress`] and
//! [`Summary`], and no download logic lives here.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use rdm::{Progress, Summary};
use serde::Serialize;

/// The event schema's version, in every line's `schema` field. It changes
/// only when a field is removed or changes meaning; new fields don't bump it.
pub const SCHEMA: u32 = 1;

/// Fewest time between two `progress` events of one download.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// One line of the stream. Strings are already redacted by the caller.
#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event<'a> {
    /// A download's current state.
    Progress {
        url: &'a str,
        downloaded: u64,
        total: Option<u64>,
        bytes_per_sec: u64,
        eta_secs: Option<u64>,
        connections: u8,
    },
    /// A download finished and its file is saved.
    Done {
        url: &'a str,
        path: String,
        bytes: u64,
        status: u16,
        /// The algorithm the file was verified with, when `--checksum` was given.
        checksum_verified: Option<&'a str>,
    },
    /// A download failed.
    Error { url: &'a str, message: &'a str },
    /// Ctrl-C paused a download; running the same command resumes it.
    Paused { url: &'a str },
    /// A line of a batch list that was refused.
    Skipped { line: &'a str, reason: &'a str },
    /// Ctrl-C ended the wait for `--at` before anything started.
    Cancelled,
    /// A batch is over; always the last line of one.
    Summary {
        done: usize,
        failed: usize,
        not_finished: usize,
        skipped: usize,
    },
}

#[derive(Serialize)]
struct Line<'a> {
    schema: u32,
    #[serde(flatten)]
    event: &'a Event<'a>,
}

impl<'a> Event<'a> {
    /// A `progress` event from the engine's snapshot.
    pub fn progress(url: &'a str, p: &Progress) -> Self {
        Event::Progress {
            url,
            downloaded: p.downloaded,
            total: p.total,
            bytes_per_sec: p.bytes_per_sec.max(0.0).round() as u64,
            eta_secs: p.eta.map(|d| d.as_secs()),
            connections: p.connections,
        }
    }

    /// A `done` event from the engine's summary.
    pub fn done(url: &'a str, s: &Summary, checksum_verified: Option<&'a str>) -> Self {
        Event::Done {
            url,
            path: s.path.display().to_string(),
            bytes: s.bytes,
            status: s.status,
            checksum_verified,
        }
    }

    /// The event as its one line, without the newline.
    pub fn to_line(&self) -> String {
        serde_json::to_string(&Line {
            schema: SCHEMA,
            event: self,
        })
        .expect("events always serialize")
    }
}

/// Write an event as one line and flush, so a reader sees it at once. The
/// whole line goes out under the stdout lock: concurrent downloads of a batch
/// never interleave. A closed pipe is the reader's choice, so it is ignored.
pub fn emit(event: &Event) {
    let mut line = event.to_line();
    line.push('\n');
    let mut out = io::stdout().lock();
    let _ = out.write_all(line.as_bytes()).and_then(|()| out.flush());
}

/// Lets a `progress` event through at most every [`PROGRESS_INTERVAL`]; the
/// first always passes. Never applied to `done`/`error`: those aren't dropped.
pub struct ProgressThrottle {
    last: Option<Instant>,
}

impl ProgressThrottle {
    pub fn new() -> Self {
        Self { last: None }
    }

    /// Whether an event at `now` may be emitted (and if so, counts as emitted).
    pub fn allow(&mut self, now: Instant) -> bool {
        let due = self
            .last
            .is_none_or(|t| now.duration_since(t) >= PROGRESS_INTERVAL);
        if due {
            self.last = Some(now);
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_carry_schema_and_event_and_flatten_fields() {
        let line = Event::Error {
            url: "http://h/f",
            message: "boom",
        }
        .to_line();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["schema"], 1);
        assert_eq!(v["event"], "error");
        assert_eq!(v["url"], "http://h/f");
        assert_eq!(v["message"], "boom");
        assert!(!line.contains('\n'));
    }

    #[test]
    fn unit_event_has_only_schema_and_event() {
        assert_eq!(
            Event::Cancelled.to_line(),
            r#"{"schema":1,"event":"cancelled"}"#
        );
    }

    #[test]
    fn unknown_total_and_eta_are_null() {
        let line = Event::Progress {
            url: "u",
            downloaded: 5,
            total: None,
            bytes_per_sec: 0,
            eta_secs: None,
            connections: 1,
        }
        .to_line();
        assert!(line.contains(r#""total":null"#) && line.contains(r#""eta_secs":null"#));
    }

    #[test]
    fn throttle_passes_first_then_one_per_interval() {
        let t0 = Instant::now();
        let mut t = ProgressThrottle::new();
        assert!(t.allow(t0));
        assert!(!t.allow(t0 + Duration::from_millis(100)));
        assert!(!t.allow(t0 + Duration::from_millis(249)));
        assert!(t.allow(t0 + Duration::from_millis(250)));
        assert!(!t.allow(t0 + Duration::from_millis(300)));
    }
}
