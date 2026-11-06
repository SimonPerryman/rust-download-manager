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
//! Downloads beyond [`Settings::max_concurrent`] wait as [`Status::Queued`]
//! and start in display order as slots free. Each download is throttled by
//! its own [`RateLimiter`] and a manager-wide one, both owned here so live
//! limit changes reach a running download.
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
use tracing::Instrument;

use crate::engine::store::{DownloadRecord, Settings, Status, Store, StoreData};
use crate::engine::{
    download, filename, state, Checksum, Control, DownloadRequest, Error, Progress, RateLimiter,
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
    /// Per-download speed cap in bytes/s; `None` is unlimited.
    pub limit: Option<u64>,
    /// Connections open, including ones still connecting; zero unless
    /// actively downloading.
    pub connections: u8,
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
            limit: self.record.limit,
            connections: self.live.connections,
        }
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
    /// Set true once dropped, so late task callbacks become no-ops.
    closed: AtomicBool,
    /// Shared by every download; unlimited until a global limit is set.
    global: Arc<RateLimiter>,
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
            settings: st.settings,
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
            e.live.connections = p.connections;
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

    /// Start queued downloads, in display order, while fewer than
    /// `max_concurrent` are running. Called with the state lock held.
    fn schedule(self: &Arc<Self>, st: &mut ManagerState) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let cap = st.settings.max_concurrent;
        let connections = st.settings.max_connections;
        let running = st.entries.values().filter(|e| e.task.is_some()).count();
        // A lowered cap can leave `running > cap`; nothing is preempted.
        let free = cap.saturating_sub(running);
        let waiting: Vec<u64> = st
            .order
            .iter()
            .copied()
            .filter(|id| {
                st.entries
                    .get(id)
                    .is_some_and(|e| e.record.status == Status::Queued && e.task.is_none())
            })
            .take(free)
            .collect();
        for (running, id) in (running..).zip(waiting) {
            debug_assert!(running < cap, "promoted past the concurrency cap");
            let e = st.entries.get_mut(&id).expect("collected above");
            // A fresh control each start: `cancel` latches and can't be
            // cleared, and the `.part` + sidecar, not the control, carry the
            // resume position. `with_limiters` must precede any clone.
            let control = Control::new().with_limiters([e.limiter.clone(), self.global.clone()]);
            e.control = control.clone();
            e.record.status = Status::Downloading;
            e.task = Some(spawn_task(self.clone(), &e.record, control, connections));
        }
    }

    /// A download task has ended; record its terminal status and persist.
    fn finish(self: &Arc<Self>, id: u64, result: crate::engine::Result<crate::engine::Summary>) {
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
        self.schedule(st);
        self.persist(st);
        self.publish(st);
    }
}

/// Coordinates many concurrent downloads over the engine's [`download`].
///
/// Cheap to clone-by-`Arc` internally; a frontend holds one `Manager` and
/// drives it with [`add`](Manager::add), [`pause`](Manager::pause),
/// [`resume`](Manager::resume), [`cancel`](Manager::cancel) and
/// [`remove`](Manager::remove), tunes it with the [`Settings`] setters, and
/// observes via [`subscribe`](Manager::subscribe) or
/// [`snapshot`](Manager::snapshot).
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
            // The on-disk state is authoritative for how far we got (a
            // multi-segment `.part` is pre-allocated, so not its length).
            if !rec.target_provisional {
                if let Some(downloaded) = state::downloaded_bytes(&rec.target) {
                    rec.downloaded = downloaded;
                }
            }
            let limiter = Arc::new(RateLimiter::new(rec.limit));
            rec.limit = limiter.rate();
            order.push(rec.id);
            entries.insert(
                rec.id,
                Entry {
                    limiter,
                    record: rec,
                    control: Control::new(),
                    live: Live::default(),
                    task: None,
                },
            );
        }
        let next_id = data.next_id.max(1);
        let settings = data.settings.clamped();

        let st = ManagerState {
            next_id,
            order,
            entries,
            settings,
        };
        let (tx, _rx) = watch::channel(Vec::new());
        let inner = Arc::new(Inner {
            tx,
            store,
            last_publish: Mutex::new(Instant::now()),
            handle,
            closed: AtomicBool::new(false),
            global: Arc::new(RateLimiter::new(settings.global_limit)),
            state: Mutex::new(st),
        });
        // Prime the channel with the restored list.
        {
            let st = inner.state.lock().unwrap();
            let _ = inner.tx.send(inner.snapshots(&st));
        }
        Ok(Self { inner })
    }

    /// Add a new download, starting it now if a slot is free or queueing it
    /// otherwise. `dir` is the destination folder; `out`, when given, is an
    /// explicit full target path that overrides the derived name. Returns the
    /// new download's id.
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
        let options = AddOptions {
            out,
            checksum,
            ..AddOptions::default()
        };
        self.add_with(url, dir, options)
    }

    /// [`add`](Self::add) with every option, including a per-download speed
    /// limit. Returns the new download's id.
    pub fn add_with(
        &self,
        url: impl Into<String>,
        dir: impl Into<PathBuf>,
        options: AddOptions,
    ) -> u64 {
        let url = url.into();
        let st = &mut *self.inner.state.lock().unwrap();

        let id = st.next_id;
        st.next_id += 1;

        let (target, target_provisional) = match options.out {
            Some(out) => (unique_target(st, out), false),
            None => (dir.into().join(filename::resolve(None, &url, None)), true),
        };
        let limiter = Arc::new(RateLimiter::new(options.limit));
        let record = DownloadRecord {
            id,
            url,
            target,
            target_provisional,
            final_path: None,
            checksum: options.checksum.map(|c| c.to_string()),
            status: Status::Queued,
            downloaded: 0,
            total: None,
            error: None,
            added_at: now_secs(),
            finished_at: None,
            limit: limiter.rate(),
        };
        let entry = Entry {
            record,
            control: Control::new(),
            limiter,
            live: Live::default(),
            task: None,
        };

        st.order.push(id);
        st.entries.insert(id, entry);
        self.inner.schedule(st);
        self.inner.persist(st);
        self.inner.publish(st);
        id
    }

    /// Request a pause. A running task stops at the next chunk boundary,
    /// keeping the `.part` + sidecar; the status flips to [`Status::Paused`]
    /// once the task observes it. A queued download leaves the queue at once.
    /// No-op otherwise.
    pub fn pause(&self, id: u64) {
        let st = &mut *self.inner.state.lock().unwrap();
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        if e.task.is_some() {
            e.control.pause();
        } else if e.record.status == Status::Queued {
            e.record.status = Status::Paused;
            self.inner.schedule(st);
            self.inner.persist(st);
            self.inner.publish(st);
        }
    }

    /// Start, resume, or retry a download: resumes a paused one from its
    /// `.part`, retries a failed one, or restarts a cancelled one from scratch.
    /// It rejoins the queue, starting now if a slot is free. No-op if it's
    /// already running, queued or completed.
    pub fn resume(&self, id: u64) {
        let st = &mut *self.inner.state.lock().unwrap();
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        if e.task.is_some() || !e.record.status.is_resumable() {
            return;
        }
        e.record.status = Status::Queued;
        e.record.error = None;
        e.record.finished_at = None;
        self.inner.schedule(st);
        self.inner.persist(st);
        self.inner.publish(st);
    }

    /// Cancel a running, queued or paused download and discard its partial
    /// data. No-op for downloads that have already finished.
    pub fn cancel(&self, id: u64) {
        let st = &mut *self.inner.state.lock().unwrap();
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        if e.task.is_some() {
            // The live task observes the cancel, removes `.part` + sidecar
            // itself, and reports back through `finish`.
            e.control.cancel();
            return;
        }
        match e.record.status {
            // No task to observe it: discard the partial here and now (a
            // resumed-then-queued download may own a `.part`).
            Status::Queued | Status::Paused => {
                remove_partial(&e.record);
                e.record.status = Status::Cancelled;
                e.record.downloaded = 0;
                e.record.finished_at = now_secs();
                self.inner.schedule(st);
                self.inner.persist(st);
                self.inner.publish(st);
            }
            Status::Downloading | Status::Completed | Status::Failed | Status::Cancelled => {}
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
        self.inner.schedule(st);
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

    /// The current manager-wide settings.
    pub fn settings(&self) -> Settings {
        self.inner.state.lock().unwrap().settings
    }

    /// Set how many downloads may stream at once (at least 1). Raising it
    /// starts queued downloads; lowering it never interrupts running ones.
    pub fn set_max_concurrent(&self, n: usize) {
        self.update_settings(|s| s.max_concurrent = n);
    }

    /// Cap the aggregate speed of all downloads in bytes/s (`None` or
    /// `Some(0)` is unlimited). Applies live, including to running downloads.
    pub fn set_global_limit(&self, limit: Option<u64>) {
        self.update_settings(|s| s.global_limit = limit);
    }

    /// Set the max parallel connections per download (clamped to 1..=16). Applies
    /// from each download's next start, not to ones already running.
    pub fn set_max_connections(&self, n: u8) {
        self.update_settings(|s| s.max_connections = n);
    }

    /// Cap one download's speed in bytes/s (`None` or `Some(0)` is
    /// unlimited). Applies live if it's running, and persists across restarts.
    pub fn set_limit(&self, id: u64, limit: Option<u64>) {
        let st = &mut *self.inner.state.lock().unwrap();
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        e.limiter.set_rate(limit);
        e.record.limit = e.limiter.rate();
        self.inner.persist(st);
        self.inner.publish(st);
    }

    fn update_settings(&self, change: impl FnOnce(&mut Settings)) {
        let st = &mut *self.inner.state.lock().unwrap();
        change(&mut st.settings);
        st.settings = st.settings.clamped();
        self.inner.global.set_rate(st.settings.global_limit);
        self.inner.schedule(st);
        self.inner.persist(st);
        self.inner.publish(st);
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
/// Runs under the state lock, so it must never take it.
fn spawn_task(
    inner: Arc<Inner>,
    record: &DownloadRecord,
    control: Control,
    connections: u8,
) -> JoinHandle<()> {
    let id = record.id;
    // Error level, like the engine's own span, so the tags survive any filter
    // that still shows errors. A provisional name is only a guess until claimed.
    let span = tracing::error_span!("task", id, file = tracing::field::Empty);
    if !record.target_provisional {
        span.record("file", tracing::field::display(file_name(&record.target)));
    }
    let req = request_for(&inner, record, &control, connections, &span);
    let handle = inner.handle.clone();
    handle.spawn(
        async move {
            let progress = inner.clone();
            let result = download(&req, &control, move |p| progress.on_progress(id, p)).await;
            inner.finish(id, result);
        }
        .instrument(span),
    )
}

/// `path`'s file name, for the task span's `file` field.
fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Build the engine request for `record`. A provisional target is left for
/// the engine to derive from the response and claimed back via `inner`; a
/// claimed one is passed explicitly so the engine resumes its `.part`.
fn request_for(
    inner: &Arc<Inner>,
    record: &DownloadRecord,
    control: &Control,
    connections: u8,
    span: &tracing::Span,
) -> DownloadRequest {
    let mut req = if record.target_provisional {
        let dir = record
            .target
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        let (inner, id, control) = (inner.clone(), record.id, control.clone());
        let span = span.clone();
        DownloadRequest::new(&record.url, dir).with_claim_target(move |proposed| {
            let target = inner.claim_target(id, proposed, &control);
            span.record("file", tracing::field::display(file_name(&target)));
            tracing::debug!(path = %target.display(), "target claimed");
            target
        })
    } else {
        DownloadRequest::to_path(&record.url, &record.target)
    };
    if let Some(cs) = record.checksum.as_deref().and_then(|s| s.parse().ok()) {
        let cs: Checksum = cs;
        req = req.with_checksum(cs);
    }
    req.with_connections(connections)
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

/// Best-effort removal of a record's `.part`, `.rdm` sidecar and any stale
/// sidecar temp files. A provisional target has none (and may name another
/// download's), so it is left alone.
fn remove_partial(record: &DownloadRecord) {
    if record.target_provisional {
        return;
    }
    let _ = std::fs::remove_file(state::part_path(&record.target));
    let sidecar = state::sidecar_path(&record.target);
    let _ = std::fs::remove_file(&sidecar);
    state::remove_stale_temps(&sidecar);
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

    /// A [`Store`] that serves a fixed document and discards saves.
    struct FixedStore(StoreData);

    impl Store for FixedStore {
        fn load(&self) -> crate::engine::Result<StoreData> {
            Ok(self.0.clone())
        }

        fn save(&self, _: &StoreData) -> crate::engine::Result<()> {
            Ok(())
        }
    }

    fn record(id: u64, limit: Option<u64>) -> DownloadRecord {
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
        }
    }

    /// Restored limits are enforced, not just displayed: each download's
    /// limiter and the global one run at the persisted rates, and a stored
    /// zero limit restores as unlimited.
    #[tokio::test]
    async fn load_restores_enforced_limits() {
        let data = StoreData {
            version: 1,
            next_id: 3,
            downloads: vec![record(1, Some(256 * 1024)), record(2, Some(0))],
            settings: Settings {
                global_limit: Some(1024 * 1024),
                ..Settings::default()
            },
        };
        let mgr = Manager::load(
            Box::new(FixedStore(data)),
            tokio::runtime::Handle::current(),
        )
        .unwrap();

        assert_eq!(mgr.inner.global.rate(), Some(1024 * 1024));
        let st = mgr.inner.state.lock().unwrap();
        for (id, want) in [(1, Some(256 * 1024)), (2, None)] {
            let e = &st.entries[&id];
            assert_eq!(e.limiter.rate(), want, "download {id}'s limiter");
            assert_eq!(e.snapshot().limit, want, "download {id}'s shown limit");
        }
    }
}
