//! A registry that runs many downloads at once on top of [`download`](crate::engine::download).
//!
//! M1's [`download`](crate::engine::download) is one async call per download; a GUI needs to juggle
//! several and steer each independently. [`Manager`] wraps the engine without
//! changing it: every download runs the existing [`download`](crate::engine::download) on its own tokio
//! task holding a [`Control`] clone, and the manager tracks each one's
//! [`Status`] and latest [`Progress`](crate::engine::Progress). It owns the persistent [`Store`] and
//! exposes a [`watch`] channel of [`DownloadSnapshot`]s so a frontend can
//! render live state without any download logic of its own.
//!
//! Downloads beyond [`Settings::max_concurrent`] wait as [`Status::Queued`]
//! and start in display order as slots free. Each download is throttled by
//! its own [`RateLimiter`] and a manager-wide one, both owned here so live
//! limit changes reach a running download.
//!
//! Observation is coalesced: per-chunk progress updates in-memory counters
//! cheaply and only publishes a fresh snapshot at ~10 Hz, so UI update rate is
//! decoupled from chunk rate. Lifecycle transitions publish immediately.

mod archive;
mod commands;
mod files;
mod observe;
mod persist;
mod queue;
mod settings;
mod tagging;
mod task;
mod verify;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::engine::download_log::DownloadLogs;
use crate::engine::store::{ChecksumState, DownloadRecord, Settings, Status, Store};
use crate::engine::tally::Tally;
use crate::engine::{Checksum, Control, RateLimiter, Segment};
use files::{has_partial, remove_partial};

/// Minimum wall-clock gap between progress-driven snapshot publishes.
const PUBLISH_INTERVAL: Duration = Duration::from_millis(100);

/// A read-only, UI-facing view of one download at a point in time.
///
/// Serializable so a frontend (e.g. a Tauri event payload) can consume it
/// directly. Combines the persisted [`DownloadRecord`] with live throughput.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DownloadSnapshot {
    /// Stable identifier for this download.
    pub id: u64,
    /// The source URL.
    pub url: String,
    /// Display filename (the target's file name).
    pub filename: String,
    /// Planned target path.
    pub target: PathBuf,
    /// Final path once completed.
    pub final_path: Option<PathBuf>,
    /// Coarse lifecycle status.
    pub status: Status,
    /// Bytes downloaded so far.
    pub downloaded: u64,
    /// Expected total, when known.
    pub total: Option<u64>,
    /// Current throughput estimate (bytes/sec); zero unless actively streaming.
    pub bytes_per_sec: f64,
    /// Estimated seconds remaining, when total and speed are known.
    pub eta_secs: Option<u64>,
    /// Failure reason when `status == Failed`.
    pub error: Option<String>,
    /// Unix-epoch seconds when added.
    pub added_at: Option<u64>,
    /// Unix-epoch seconds when it reached a terminal status.
    pub finished_at: Option<u64>,
    /// Per-download speed cap in bytes/s; `None` is unlimited.
    pub limit: Option<u64>,
    /// Connections open, including ones still connecting; zero unless
    /// actively downloading.
    pub connections: u8,
    /// Stopped with its `.part` + sidecar kept, so resuming continues them.
    pub partial_kept: bool,
    /// The expected checksum as `"sha256:<hex>"`, if one was given.
    pub checksum: Option<String>,
    /// Where the checksum check of the finished file stands.
    pub checksum_state: ChecksumState,
    /// The byte ranges being fetched and how far each has got (the whole
    /// tiling of the file); kept while paused, failed or stopped with the
    /// partial kept (reloaded from the sidecar on restart), and once done for
    /// this session only.
    pub segments: Vec<Segment>,
    /// For each of `segments`, the lane fetching it now (numbered from 1, as
    /// the log names it), if any; empty unless downloading. Live only, never
    /// saved.
    pub lanes: Vec<Option<u8>>,
    /// The most lanes it has had open at once since it last started from zero
    /// bytes, this session (0 until it runs), for a finish summary such as "up
    /// to 3 lanes". Counted at every progress report, not just the published
    /// snapshots, so a quick ramp isn't missed.
    pub peak_lanes: u8,
    /// Milliseconds spent running (not waiting or paused) since it last
    /// started from zero bytes, so a frontend can say how long it took; `None`
    /// when unknown (an older build's record, or a run cut short by a crash).
    pub active_ms: Option<u64>,
    /// Archived: hidden from the working views, kept as history.
    pub archived: bool,
    /// User labels, in the order added.
    pub tags: Vec<String>,
}

/// Optional extras for [`Manager::add_with`].
#[derive(Debug, Clone, Default)]
pub struct AddOptions {
    /// Explicit full target path, overriding the derived name.
    pub out: Option<PathBuf>,
    /// Integrity check for the finished file.
    pub checksum: Option<Checksum>,
    /// Speed cap in bytes/s (`None` or `Some(0)` is unlimited).
    pub limit: Option<u64>,
    /// Tags to start with, under the [`tags`](crate::engine::tags) rules.
    pub tags: Vec<String>,
}

/// Live, non-persisted throughput for one download.
#[derive(Debug, Clone, Copy, Default)]
struct Live {
    bytes_per_sec: f64,
    eta_secs: Option<u64>,
    connections: u8,
}

/// One managed download: its persisted record, control handle, rate limiter,
/// live speed, and the running task (if any).
struct Entry {
    record: DownloadRecord,
    control: Control,
    /// Outlives each start's `Control`, so a limit set while queued or paused
    /// still applies.
    limiter: Arc<RateLimiter>,
    live: Live,
    task: Option<JoinHandle<()>>,
    /// The running task was asked to stop keeping its partial; it reports
    /// back as a pause, which `finish` turns into a kept cancel.
    stop_keeping: bool,
    /// Bumped per [`Manager::verify_checksum`] call, so a superseded hash's
    /// result is dropped.
    verify_gen: u64,
    /// The latest segment tiling; see [`DownloadSnapshot::segments`].
    segments: Vec<Segment>,
    /// Each segment's lane; see [`DownloadSnapshot::lanes`].
    lanes: Vec<Option<u8>>,
    /// See [`DownloadSnapshot::peak_lanes`].
    peak_lanes: u8,
    /// When the running task started, for [`DownloadRecord::active_ms`].
    started: Option<Instant>,
    /// Totals for the finish summary, across this download's runs here.
    tally: Arc<Tally>,
}

impl Entry {
    fn new(record: DownloadRecord, limiter: Arc<RateLimiter>) -> Self {
        Self {
            record,
            control: Control::new(),
            limiter,
            live: Live::default(),
            task: None,
            stop_keeping: false,
            verify_gen: 0,
            segments: Vec::new(),
            lanes: Vec::new(),
            peak_lanes: 0,
            started: None,
            tally: Arc::default(),
        }
    }

    fn snapshot(&self) -> DownloadSnapshot {
        DownloadSnapshot {
            id: self.record.id,
            url: self.record.url.clone(),
            filename: self
                .record
                .target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            target: self.record.target.clone(),
            final_path: self.record.final_path.clone(),
            status: self.record.status,
            downloaded: self.record.downloaded,
            total: self.record.total,
            bytes_per_sec: self.live.bytes_per_sec,
            eta_secs: self.live.eta_secs,
            error: self.record.error.clone(),
            added_at: self.record.added_at,
            finished_at: self.record.finished_at,
            limit: self.record.limit,
            connections: self.live.connections,
            partial_kept: self.record.partial_kept,
            checksum: self.record.checksum.clone(),
            checksum_state: self.record.checksum_state,
            segments: self.segments.clone(),
            lanes: if self.record.status == Status::Downloading {
                self.lanes.clone()
            } else {
                Vec::new()
            },
            peak_lanes: self.peak_lanes,
            active_ms: self.active_ms(),
            archived: self.record.archived,
            tags: self.record.tags.clone(),
        }
    }

    /// The recorded active time plus the running task's time so far.
    fn active_ms(&self) -> Option<u64> {
        let running = self.started.map_or(0, |t| millis(t.elapsed()));
        self.record.active_ms.map(|ms| ms.saturating_add(running))
    }

    /// Mark this download stopped, its partial kept if there is one on disk.
    fn cancel_keeping(&mut self) {
        self.record.status = Status::Cancelled;
        self.record.partial_kept = has_partial(&self.record);
        self.record.finished_at = now_secs();
    }

    /// Discard this download's partial data and the progress it stood for.
    /// With `shared`, another download owns the target's files, so they stay.
    fn discard_partial(&mut self, shared: bool) {
        if !shared {
            remove_partial(&self.record);
        }
        self.record.partial_kept = false;
        self.record.downloaded = 0;
        self.segments.clear();
        self.peak_lanes = 0;
    }
}

/// Mutable registry state, guarded by a single lock.
struct ManagerState {
    next_id: u64,
    order: Vec<u64>,
    entries: HashMap<u64, Entry>,
    settings: Settings,
}

/// Shared manager internals, held behind an `Arc` so download tasks can report
/// progress and completion back.
struct Inner {
    state: Mutex<ManagerState>,
    store: Box<dyn Store>,
    tx: watch::Sender<Vec<DownloadSnapshot>>,
    last_publish: Mutex<Instant>,
    /// Runtime the download tasks are spawned onto. Held explicitly (rather
    /// than relying on an ambient runtime) so the manager works when driven
    /// from a non-async caller, e.g. a GUI's synchronous command handler.
    handle: tokio::runtime::Handle,
    /// Set by [`Manager::shutdown`] or drop, so late task callbacks become no-ops.
    closed: AtomicBool,
    /// Shared by every download; unlimited until a global limit is set.
    global: Arc<RateLimiter>,
    /// Each download's recent log lines, filled by [`Manager::log_layer`].
    logs: Arc<DownloadLogs>,
}

/// Coordinates many concurrent downloads over the engine's [`download`](crate::engine::download).
///
/// Cheap to clone-by-`Arc` internally; a frontend holds one `Manager` and
/// drives it with [`add`](Manager::add), [`pause`](Manager::pause),
/// [`resume`](Manager::resume), [`stop`](Manager::stop),
/// [`remove`](Manager::remove) and the like, tunes it with the [`Settings`]
/// setters, and observes via [`subscribe`](Manager::subscribe) or
/// [`snapshot`](Manager::snapshot).
pub struct Manager {
    inner: Arc<Inner>,
}

impl Drop for Manager {
    fn drop(&mut self) {
        // Stop late task callbacks from touching freed state, and abort any
        // still-running downloads so they don't outlive the manager.
        self.inner.closed.store(true, Ordering::SeqCst);
        if let Ok(st) = self.inner.state.lock() {
            for e in st.entries.values() {
                if let Some(task) = &e.task {
                    task.abort();
                }
            }
        }
    }
}

/// `d` in whole milliseconds, saturating.
fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Current Unix-epoch seconds, or `None` if the clock is before the epoch.
fn now_secs() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::store::StoreData;

    /// A [`Store`] that serves a fixed document and discards saves.
    pub(super) struct FixedStore(pub(super) StoreData);

    impl Store for FixedStore {
        fn load(&self) -> crate::engine::Result<StoreData> {
            Ok(self.0.clone())
        }

        fn save(&self, _: &StoreData) -> crate::engine::Result<()> {
            Ok(())
        }
    }

    pub(super) fn record(id: u64, limit: Option<u64>) -> DownloadRecord {
        DownloadRecord {
            id,
            url: format!("http://example.invalid/{id}"),
            target: PathBuf::from(format!("{id}.bin")),
            target_provisional: false,
            final_path: None,
            checksum: None,
            status: Status::Paused,
            downloaded: 0,
            total: None,
            error: None,
            added_at: None,
            finished_at: None,
            limit,
            partial_kept: false,
            checksum_state: ChecksumState::None,
            active_ms: None,
            archived: false,
            tags: Vec::new(),
        }
    }
}
