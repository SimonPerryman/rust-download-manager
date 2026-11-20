//! What schedules, tags and post-download actions did, kept so a frontend can
//! show it: a download's events beside its log lines, and a rule's own history.
//!
//! One store, `history.jsonl` beside the download list, one JSON event per
//! line. Like the per-download logs it is bounded: events older than
//! [`HISTORY_MAX_AGE`] go at start, and the oldest go first when the file
//! passes [`HISTORY_CAP`]. A removed download's events go with it.

use std::collections::HashSet;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::engine::lock::lock;

/// Events older than this (90 days) are dropped at start.
pub(crate) const HISTORY_MAX_AGE_MS: u64 = 90 * 24 * 60 * 60 * 1000;

/// The most bytes the history file may take (20 MB); the oldest events go first.
pub(crate) const HISTORY_CAP: u64 = 20 * 1024 * 1024;

/// Longest event text kept, in bytes, so one huge error can't defeat the cap.
const MAX_DETAILS: usize = 2000;

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    /// A queue window opened.
    Opened,
    /// A queue window closed; the details carry what finished inside it.
    Closed,
    /// A schedule held a download back.
    Held,
    /// A schedule let a download start.
    Started,
    /// A schedule paused a running download.
    Paused,
    /// A scheduled start passed while rdm was closed.
    Missed,
    /// The command run after a download.
    Command,
    /// A power action.
    Power,
    /// A tag was added to a download.
    TagAdded,
    /// A tag was removed from a download.
    TagRemoved,
}

/// One thing that happened, for a frontend's history views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEvent {
    /// When, in milliseconds since the Unix epoch.
    pub at: u64,
    pub kind: HistoryKind,
    /// The download it concerns, if one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_id: Option<u64>,
    /// The schedule rule it concerns, if one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<u64>,
    /// The event as a sentence, e.g. `14 files, 12.3 GB`.
    pub details: String,
}

impl HistoryEvent {
    /// An event for now.
    pub(crate) fn new(
        kind: HistoryKind,
        download_id: Option<u64>,
        rule_id: Option<u64>,
        details: impl Into<String>,
    ) -> Self {
        let mut details = details.into();
        if details.len() > MAX_DETAILS {
            let mut end = MAX_DETAILS;
            while !details.is_char_boundary(end) {
                end -= 1;
            }
            details.truncate(end);
            details.push('…');
        }
        Self {
            at: now_ms(),
            kind,
            download_id,
            rule_id,
            details,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The event store: every event in memory, appended to `path` as it happens.
pub(crate) struct History {
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
    cap: u64,
}

struct Inner {
    events: Vec<HistoryEvent>,
    /// Bytes the file holds (or would).
    bytes: u64,
}

fn line_of(event: &HistoryEvent) -> Option<String> {
    serde_json::to_string(event).ok().map(|j| j + "\n")
}

impl History {
    /// Read `path` (`None`: memory only), dropping events older than
    /// `max_age_ms` at `now_ms`, ones about a download that is gone (an id
    /// below `next_id` not in `live`), and the oldest until the rest fit
    /// `cap`. A missing or damaged file just means fewer events.
    pub(crate) fn open(
        path: Option<PathBuf>,
        live: &HashSet<u64>,
        next_id: u64,
        now_ms: u64,
        max_age_ms: u64,
        cap: u64,
    ) -> Self {
        let text = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let total = text.lines().count();
        let mut events: Vec<HistoryEvent> = text
            .lines()
            .filter_map(|l| serde_json::from_str::<HistoryEvent>(l).ok())
            .filter(|e| now_ms.saturating_sub(e.at) <= max_age_ms)
            .filter(|e| {
                e.download_id
                    .is_none_or(|id| id >= next_id || live.contains(&id))
            })
            .collect();
        let mut bytes: u64 = events
            .iter()
            .filter_map(line_of)
            .map(|l| l.len() as u64)
            .sum();
        let dropped = fit(&mut events, &mut bytes, cap);
        let history = Self {
            path,
            inner: Mutex::new(Inner { events, bytes }),
            cap,
        };
        if dropped || history.inner_len() != total || !text.is_empty() && !text.ends_with('\n') {
            let _ = history.rewrite(&lock(&history.inner));
        }
        history
    }

    fn inner_len(&self) -> usize {
        lock(&self.inner).events.len()
    }

    /// Add `event`, saving it too.
    pub(crate) fn record(&self, event: HistoryEvent) {
        let mut inner = lock(&self.inner);
        let Some(line) = line_of(&event) else { return };
        inner.bytes += line.len() as u64;
        inner.events.push(event);
        if inner.bytes > self.cap {
            let Inner { events, bytes } = &mut *inner;
            fit(events, bytes, self.cap);
            let _ = self.rewrite(&inner);
        } else if let Some(path) = &self.path {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| f.write_all(line.as_bytes()));
        }
    }

    /// Drop every event about download `id`, as its log goes.
    pub(crate) fn forget_download(&self, id: u64) {
        let mut inner = lock(&self.inner);
        let before = inner.events.len();
        inner.events.retain(|e| e.download_id != Some(id));
        if inner.events.len() == before {
            return;
        }
        inner.bytes = inner
            .events
            .iter()
            .filter_map(line_of)
            .map(|l| l.len() as u64)
            .sum();
        let _ = self.rewrite(&inner);
    }

    /// Every event, oldest first.
    pub(crate) fn all(&self) -> Vec<HistoryEvent> {
        lock(&self.inner).events.clone()
    }

    /// Download `id`'s events, oldest first.
    pub(crate) fn for_download(&self, id: u64) -> Vec<HistoryEvent> {
        lock(&self.inner)
            .events
            .iter()
            .filter(|e| e.download_id == Some(id))
            .cloned()
            .collect()
    }

    /// The rule-level events of rule `id` (not its downloads'), oldest first.
    pub(crate) fn for_rule(&self, id: u64) -> Vec<HistoryEvent> {
        lock(&self.inner)
            .events
            .iter()
            .filter(|e| e.rule_id == Some(id) && e.download_id.is_none())
            .cloned()
            .collect()
    }

    /// Replace the file with the events held.
    fn rewrite(&self, inner: &Inner) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let text: String = inner.events.iter().filter_map(line_of).collect();
        let tmp = path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }
}

/// Drop the oldest of `events` until their `bytes` fit `cap`; true if any went.
fn fit(events: &mut Vec<HistoryEvent>, bytes: &mut u64, cap: u64) -> bool {
    if *bytes <= cap {
        return false;
    }
    let mut cut = 0;
    while *bytes > cap && cut < events.len() {
        *bytes -= line_of(&events[cut]).map_or(0, |l| l.len() as u64);
        cut += 1;
    }
    events.drain(..cut);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: HistoryKind, download: Option<u64>, rule: Option<u64>, at: u64) -> HistoryEvent {
        HistoryEvent {
            at,
            kind,
            download_id: download,
            rule_id: rule,
            details: "x".into(),
        }
    }

    fn open(path: &std::path::Path, live: &[u64], now: u64, cap: u64) -> History {
        History::open(
            Some(path.to_path_buf()),
            &live.iter().copied().collect(),
            100,
            now,
            HISTORY_MAX_AGE_MS,
            cap,
        )
    }

    #[test]
    fn events_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let h = open(&path, &[], 1000, HISTORY_CAP);
        h.record(event(HistoryKind::Opened, None, Some(1), 1000));
        h.record(event(HistoryKind::Held, Some(7), Some(1), 1000));
        drop(h);
        let h = open(&path, &[7], 1000, HISTORY_CAP);
        assert_eq!(h.for_rule(1).len(), 1, "rule history leaves out downloads");
        assert_eq!(h.for_download(7)[0].kind, HistoryKind::Held);
    }

    #[test]
    fn old_orphaned_and_torn_lines_go_at_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let day = 24 * 60 * 60 * 1000;
        let now = 200 * day;
        let mut text = String::new();
        for e in [
            event(HistoryKind::Opened, None, Some(1), now - 91 * day),
            event(HistoryKind::Opened, None, Some(1), now - 89 * day),
            event(HistoryKind::Held, Some(5), None, now),
            event(HistoryKind::Held, Some(6), None, now),
        ] {
            text.push_str(&line_of(&e).unwrap());
        }
        text.push_str("{torn");
        std::fs::write(&path, text).unwrap();
        // 5 is gone (below next_id 100, not live); 6 is live.
        let h = open(&path, &[6], now, HISTORY_CAP);
        assert_eq!(h.for_rule(1).len(), 1);
        assert!(h.for_download(5).is_empty());
        assert_eq!(h.for_download(6).len(), 1);
        let kept = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            kept.lines().count(),
            2,
            "the file is rewritten without them"
        );
    }

    #[test]
    fn the_oldest_events_go_first_to_fit_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let one = line_of(&event(HistoryKind::Opened, None, Some(1), 1000))
            .unwrap()
            .len() as u64;
        let h = open(&path, &[], 1000, one * 3);
        for i in 0..5 {
            h.record(event(HistoryKind::Opened, None, Some(1), 1000 + i));
        }
        let kept = h.for_rule(1);
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[0].at, 1002);
        drop(h);
        let h = open(&path, &[], 1004, one * 3);
        assert_eq!(h.for_rule(1).len(), 3);
    }

    #[test]
    fn forgetting_a_download_removes_its_events_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let h = open(&path, &[], 1000, HISTORY_CAP);
        h.record(event(HistoryKind::TagAdded, Some(1), None, 1000));
        h.record(event(HistoryKind::TagAdded, Some(2), None, 1000));
        h.forget_download(1);
        assert!(h.for_download(1).is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 1);
    }

    #[test]
    fn long_text_is_cut_on_a_char_boundary() {
        let e = HistoryEvent::new(HistoryKind::Command, None, None, "é".repeat(3000));
        assert!(e.details.len() <= MAX_DETAILS + '…'.len_utf8());
        assert!(e.details.ends_with('…'));
    }
}
