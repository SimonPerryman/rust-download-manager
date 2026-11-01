//! A registry that runs many downloads at once on top of [`download`].
//!
//! M1's [`download`] is one async call per download; a GUI needs to juggle
//! several and steer each independently. [`Manager`] wraps the engine without
//! changing it: every download runs the existing [`download`] on its own tokio
//! task holding a [`Control`] clone, and the manager tracks each one's
//! [`Status`] and latest [`Progress`]. It owns the persistent [`Store`] and
//! exposes a [`watch`] channel of [`DownloadSnapshot`]s so a frontend can
//! render live state without any download logic of its own.
//!
//! Observation is coalesced: per-chunk progress updates in-memory counters
//! cheaply and only publishes a fresh snapshot at ~10 Hz, so UI update rate is
//! decoupled from chunk rate. Lifecycle transitions publish immediately.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::engine::store::{DownloadRecord, Status, Store, StoreData};
use crate::engine::{
    download, filename, state, Checksum, Control, DownloadRequest, Error, Progress,
};

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
}

/// Live, non-persisted throughput for one download.
#[derive(Debug, Clone, Copy, Default)]
struct Live {
    bytes_per_sec: f64,
    eta_secs: Option<u64>,
}

/// One managed download: its persisted record, control handle, live speed, and
/// the running task (if any).
struct Entry {
    record: DownloadRecord,
    control: Control,
    live: Live,
    task: Option<JoinHandle<()>>,
}

impl Entry {
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
        }
    }
}

/// Mutable registry state, guarded by a single lock.
struct ManagerState {
    next_id: u64,
    order: Vec<u64>,
    entries: HashMap<u64, Entry>,
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
    /// Set true once dropped, so late task callbacks become no-ops.
    closed: AtomicBool,
}

impl Inner {
    fn snapshots(&self, st: &ManagerState) -> Vec<DownloadSnapshot> {
        st.order
            .iter()
            .filter_map(|id| st.entries.get(id))
            .map(Entry::snapshot)
            .collect()
    }

    /// Publish the current snapshot immediately (used on lifecycle changes).
    fn publish(&self, st: &ManagerState) {
        let _ = self.tx.send(self.snapshots(st));
        *self.last_publish.lock().unwrap() = Instant::now();
    }

    /// Persist the whole document. Best-effort: a save failure must not take
    /// down an in-flight download.
    fn persist(&self, st: &ManagerState) {
        let data = StoreData {
            version: 1,
            next_id: st.next_id,
            downloads: st
                .order
                .iter()
                .filter_map(|id| st.entries.get(id))
                .map(|e| e.record.clone())
                .collect(),
        };
        let _ = self.store.save(&data);
    }

    /// Per-chunk progress callback: update counters cheaply, publish at most
    /// once per [`PUBLISH_INTERVAL`].
    fn on_progress(&self, id: u64, p: Progress) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let st = &mut *self.state.lock().unwrap();
        if let Some(e) = st.entries.get_mut(&id) {
            e.record.downloaded = p.downloaded;
            if p.total.is_some() {
                e.record.total = p.total;
            }
            e.live.bytes_per_sec = p.bytes_per_sec;
            e.live.eta_secs = p.eta.map(|d| d.as_secs());
        } else {
            return;
        }
        let now = Instant::now();
        let mut last = self.last_publish.lock().unwrap();
        if now.duration_since(*last) >= PUBLISH_INTERVAL {
            *last = now;
            let _ = self.tx.send(self.snapshots(st));
        }
    }

    /// Engine claim hook for a provisional target: dedup the derived `proposed`
    /// path, adopt it as the record's target, and hand it back to the engine.
    /// Called from the download task (never with the state lock held).
    fn claim_target(&self, id: u64, proposed: &Path, control: &Control) -> PathBuf {
        let st = &mut *self.state.lock().unwrap();
        if let Some(e) = st.entries.get(&id) {
            if !e.record.target_provisional {
                return e.record.target.clone();
            }
        }
        let target = unique_target(st, proposed.to_path_buf());
        let closed = self.closed.load(Ordering::SeqCst);
        match st.entries.get_mut(&id) {
            Some(e) if !closed => {
                e.record.target = target.clone();
                e.record.target_provisional = false;
                self.persist(st);
                self.publish(st);
            }
            // Removed (or manager dropped) before the first response: steer the
            // engine to a free path and cancel, so it never touches another
            // download's `.part`.
            _ => control.cancel(),
        }
        target
    }

    /// A download task has ended; record its terminal status and persist.
    fn finish(&self, id: u64, result: crate::engine::Result<crate::engine::Summary>) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let st = &mut *self.state.lock().unwrap();
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        e.task = None;
        e.live = Live::default();
        match result {
            Ok(summary) => {
                e.record.status = Status::Completed;
                e.record.downloaded = summary.bytes;
                e.record.total = e.record.total.or(Some(summary.bytes));
                e.record.final_path = Some(summary.path);
                e.record.error = None;
                e.record.finished_at = now_secs();
            }
            Err(Error::Paused) => {
                e.record.status = Status::Paused;
            }
            Err(Error::Cancelled) => {
                e.record.status = Status::Cancelled;
                e.record.finished_at = now_secs();
            }
            Err(other) => {
                e.record.status = Status::Failed;
                e.record.error = Some(crate::engine::error_detail(&other));
                e.record.finished_at = now_secs();
            }
        }
        self.persist(st);
        self.publish(st);
    }
}

/// Coordinates many concurrent downloads over the engine's [`download`].
///
/// Cheap to clone-by-`Arc` internally; a frontend holds one `Manager` and
/// drives it with [`add`](Manager::add), [`pause`](Manager::pause),
/// [`resume`](Manager::resume), [`cancel`](Manager::cancel) and
/// [`remove`](Manager::remove), observing via [`subscribe`](Manager::subscribe)
/// or [`snapshot`](Manager::snapshot).
pub struct Manager {
    inner: Arc<Inner>,
}

impl Manager {
    /// Build a manager over `store`, spawning download tasks onto `handle`,
    /// and restore any previously persisted downloads. Downloads that were
    /// mid-flight at the last shutdown are restored as [`Status::Paused`]
    /// (resumable) rather than auto-resumed, so launching the app never kicks
    /// off surprise network activity.
    ///
    /// Taking an explicit [`tokio::runtime::Handle`] (instead of relying on an
    /// ambient runtime) lets the manager be driven from a synchronous caller
    /// such as a GUI command handler. Inside an async context,
    /// `tokio::runtime::Handle::current()` is the handle to pass.
    pub fn load(
        store: Box<dyn Store>,
        handle: tokio::runtime::Handle,
    ) -> crate::engine::Result<Self> {
        let data = store.load()?;
        let mut entries = HashMap::new();
        let mut order = Vec::new();
        for mut rec in data.downloads {
            // A download that was running at shutdown has no task now; restore
            // it paused so the user can resume it deliberately.
            if rec.status.is_running() {
                rec.status = Status::Paused;
            }
            // The `.part` on disk is authoritative for how far we got.
            if !rec.target_provisional {
                if let Ok(meta) = std::fs::metadata(state::part_path(&rec.target)) {
                    rec.downloaded = meta.len();
                }
            }
            order.push(rec.id);
            entries.insert(
                rec.id,
                Entry {
                    record: rec,
                    control: Control::new(),
                    live: Live::default(),
                    task: None,
                },
            );
        }
        let next_id = data.next_id.max(1);

        let st = ManagerState {
            next_id,
            order,
            entries,
        };
        let (tx, _rx) = watch::channel(Vec::new());
        let inner = Arc::new(Inner {
            tx,
            store,
            last_publish: Mutex::new(Instant::now()),
            handle,
            closed: AtomicBool::new(false),
            state: Mutex::new(st),
        });
        // Prime the channel with the restored list.
        {
            let st = inner.state.lock().unwrap();
            let _ = inner.tx.send(inner.snapshots(&st));
        }
        Ok(Self { inner })
    }

    /// Add a new download and start it immediately. `dir` is the destination
    /// folder; `out`, when given, is an explicit full target path that
    /// overrides the derived name. Returns the new download's id.
    ///
    /// Without `out`, the target shows a URL-derived placeholder until the first
    /// response, when the server's `Content-Disposition` (or URL) name is
    /// claimed, deduplicated against other downloads and files on disk.
    pub fn add(
        &self,
        url: impl Into<String>,
        dir: impl Into<PathBuf>,
        out: Option<PathBuf>,
        checksum: Option<Checksum>,
    ) -> u64 {
        let url = url.into();
        let st = &mut *self.inner.state.lock().unwrap();

        let id = st.next_id;
        st.next_id += 1;

        let (target, target_provisional) = match out {
            Some(out) => (unique_target(st, out), false),
            None => (dir.into().join(filename::resolve(None, &url, None)), true),
        };
        let control = Control::new();
        let record = DownloadRecord {
            id,
            url,
            target,
            target_provisional,
            final_path: None,
            checksum: checksum.map(|c| c.to_string()),
            status: Status::Downloading,
            downloaded: 0,
            total: None,
            error: None,
            added_at: now_secs(),
            finished_at: None,
        };
        let task = spawn_task(self.inner.clone(), &record, control.clone());
        let entry = Entry {
            record,
            control,
            live: Live::default(),
            task: Some(task),
        };

        st.order.push(id);
        st.entries.insert(id, entry);
        self.inner.persist(st);
        self.inner.publish(st);
        id
    }

    /// Request a pause. The running task stops at the next chunk boundary,
    /// keeping the `.part` + sidecar; the status flips to [`Status::Paused`]
    /// once the task observes it. No-op if the download isn't running.
    pub fn pause(&self, id: u64) {
        let st = self.inner.state.lock().unwrap();
        if let Some(e) = st.entries.get(&id) {
            if e.record.status.is_running() {
                e.control.pause();
            }
        }
    }

    /// Start, resume, or retry a download: resumes a paused one from its
    /// `.part`, retries a failed one, or restarts a cancelled one from scratch.
    /// No-op if it's already running or already completed.
    pub fn resume(&self, id: u64) {
        let st = &mut *self.inner.state.lock().unwrap();
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        if e.task.is_some() || !e.record.status.is_resumable() {
            return;
        }
        // A fresh control each (re)start: `cancel` latches and can't be
        // cleared, and the `.part` + sidecar — not the control — carry resume
        // position, so a new handle is always safe.
        let control = Control::new();
        e.control = control.clone();
        e.record.status = Status::Downloading;
        e.record.error = None;
        e.record.finished_at = None;
        e.task = Some(spawn_task(self.inner.clone(), &e.record, control));
        self.inner.persist(st);
        self.inner.publish(st);
    }

    /// Cancel a running or paused download and discard its partial data. No-op
    /// for downloads that have already finished.
    pub fn cancel(&self, id: u64) {
        let st = &mut *self.inner.state.lock().unwrap();
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        match e.record.status {
            // A live task observes the cancel, removes `.part` + sidecar itself,
            // and reports back through `finish`.
            Status::Downloading | Status::Queued => e.control.cancel(),
            // No task to observe it — discard the partial here and now.
            Status::Paused => {
                remove_partial(&e.record);
                e.record.status = Status::Cancelled;
                e.record.downloaded = 0;
                e.record.finished_at = now_secs();
                self.inner.persist(st);
                self.inner.publish(st);
            }
            Status::Completed | Status::Failed | Status::Cancelled => {}
        }
    }

    /// Remove a download from the list entirely. Any running task is stopped
    /// and partial data discarded; a completed download's finished file is left
    /// on disk untouched.
    pub fn remove(&self, id: u64) {
        let st = &mut *self.inner.state.lock().unwrap();
        let Some(e) = st.entries.remove(&id) else {
            return;
        };
        st.order.retain(|&x| x != id);
        if let Some(task) = e.task {
            task.abort();
        }
        if e.record.status != Status::Completed {
            remove_partial(&e.record);
        }
        self.inner.persist(st);
        self.inner.publish(st);
    }

    /// A one-shot snapshot of every download, in display order.
    pub fn snapshot(&self) -> Vec<DownloadSnapshot> {
        let st = self.inner.state.lock().unwrap();
        self.inner.snapshots(&st)
    }

    /// Subscribe to live snapshots. The receiver always holds the latest value
    /// (intermediate updates are coalesced), so a slow consumer never falls
    /// behind — it just sees the most recent state.
    pub fn subscribe(&self) -> watch::Receiver<Vec<DownloadSnapshot>> {
        self.inner.tx.subscribe()
    }
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

/// Spawn the engine `download` for one record on its own task, reporting
/// progress and completion back through `inner`.
fn spawn_task(inner: Arc<Inner>, record: &DownloadRecord, control: Control) -> JoinHandle<()> {
    let id = record.id;
    let req = request_for(&inner, record, &control);
    let handle = inner.handle.clone();
    handle.spawn(async move {
        let progress = inner.clone();
        let result = download(&req, &control, move |p| progress.on_progress(id, p)).await;
        inner.finish(id, result);
    })
}

/// Build the engine request for `record`. A provisional target is left for
/// the engine to derive from the response and claimed back via `inner`; a
/// claimed one is passed explicitly so the engine resumes its `.part`.
fn request_for(inner: &Arc<Inner>, record: &DownloadRecord, control: &Control) -> DownloadRequest {
    let mut req = if record.target_provisional {
        let dir = record
            .target
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        let (inner, id, control) = (inner.clone(), record.id, control.clone());
        DownloadRequest::new(&record.url, dir)
            .with_claim_target(move |proposed| inner.claim_target(id, proposed, &control))
    } else {
        DownloadRequest::to_path(&record.url, &record.target)
    };
    if let Some(cs) = record.checksum.as_deref().and_then(|s| s.parse().ok()) {
        let cs: Checksum = cs;
        req = req.with_checksum(cs);
    }
    req
}

/// Choose a target path that no other *active* download and no file on disk is
/// already using, so concurrent `.part` files never collide. Mirrors the
/// `name (1).ext` scheme the engine uses at finalize. Provisional targets own
/// nothing on disk, so they never block a name.
fn unique_target(st: &ManagerState, base: PathBuf) -> PathBuf {
    let active: HashSet<&Path> = st
        .entries
        .values()
        .filter(|e| !e.record.target_provisional)
        .filter(|e| e.record.status.is_running() || e.record.status == Status::Paused)
        .map(|e| e.record.target.as_path())
        .collect();

    let taken = |p: &Path| active.contains(p) || p.exists() || state::part_path(p).exists();
    if !taken(&base) {
        return base;
    }

    let parent = base.parent();
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download")
        .to_string();
    let ext = base
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_string);
    for n in 1u32.. {
        let name = match &ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = match parent {
            Some(p) if !p.as_os_str().is_empty() => p.join(&name),
            _ => PathBuf::from(&name),
        };
        if !taken(&candidate) {
            return candidate;
        }
    }
    unreachable!("the range is effectively unbounded")
}

/// Best-effort removal of a record's `.part` and `.rdm` sidecar. A provisional
/// target has none (and may name another download's), so it is left alone.
fn remove_partial(record: &DownloadRecord) {
    if record.target_provisional {
        return;
    }
    let _ = std::fs::remove_file(state::part_path(&record.target));
    let _ = std::fs::remove_file(state::sidecar_path(&record.target));
}

/// Current Unix-epoch seconds, or `None` if the clock is before the epoch.
fn now_secs() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}
