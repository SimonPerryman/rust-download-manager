//! The saved download list: restoring it at load, and saving it back.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use tokio::sync::watch;

use super::files::owns_partial;
use super::window::QueueState;
use super::{Entry, Inner, Manager, ManagerState};
use crate::engine::actions::{ActionsStatus, NoPower, PowerStatus};
use crate::engine::download_log::{prune, DownloadLogs, LOG_MAX_AGE, LOG_TOTAL_CAP};
use crate::engine::lock::lock;
use crate::engine::schedule::{Clock, SystemClock};
use crate::engine::store::{ChecksumState, Status, Store, StoreData, SCHEMA_VERSION};
use crate::engine::{single_segment, state, tags, DownloadState, RateLimiter};

impl Manager {
    /// Build a manager over `store`, spawning download tasks onto `handle`,
    /// and restore any previously persisted downloads. Downloads that were
    /// mid-flight at the last shutdown are restored as [`Status::Paused`]
    /// (resumable) rather than auto-resumed, so launching the app never kicks
    /// off surprise network activity; one still downloading (the process
    /// ended without [`shutdown`](Self::shutdown)) has its active time marked
    /// unknown. A checksum hash cut short by the shutdown is restored as
    /// unchecked. A restart into an update
    /// ([`shutdown_for_restart`](Self::shutdown_for_restart)) is no different:
    /// nothing resumes on its own, and the downloads it marked stay marked
    /// (see [`paused_for_update`](Self::paused_for_update)).
    ///
    /// Taking an explicit [`tokio::runtime::Handle`] (instead of relying on an
    /// ambient runtime) lets the manager be driven from a synchronous caller
    /// such as a GUI command handler. Inside an async context,
    /// `tokio::runtime::Handle::current()` is the handle to pass.
    pub fn load(
        store: Box<dyn Store>,
        handle: tokio::runtime::Handle,
    ) -> crate::engine::Result<Self> {
        Self::load_with_clock(store, handle, Arc::new(SystemClock::new()))
    }

    /// [`load`](Self::load) with the clock the schedule rules are judged
    /// against, so a test can move time by hand.
    pub fn load_with_clock(
        store: Box<dyn Store>,
        handle: tokio::runtime::Handle,
        clock: Arc<dyn Clock>,
    ) -> crate::engine::Result<Self> {
        let data = store.load()?;
        let mut entries = HashMap::new();
        let mut order = Vec::new();
        let mut batch_limiters: HashMap<u64, Arc<RateLimiter>> = HashMap::new();
        for mut rec in data.downloads {
            // A download that was running at shutdown has no task now; restore
            // it paused so the user can resume it deliberately.
            // A queued download held for its start time stays so: it is
            // not running, only waiting for the clock.
            if rec.status == Status::Queued && rec.start_at.is_some() {
                rec.start_group.get_or_insert(rec.id);
            } else {
                rec.start_at = None;
                rec.start_group = None;
            }
            let held = rec.start_at.is_some();
            if rec.status.is_running() && !held {
                // Still marked running: the process died mid-run (a quit
                // would have saved it paused), so that run's time is unknown.
                if rec.status == Status::Downloading {
                    rec.active_ms = None;
                }
                rec.status = Status::Paused;
            }
            if rec.checksum_state == ChecksumState::Pending {
                rec.checksum_state = ChecksumState::None;
            }
            // Only a finished download is archived; a hand-edited store
            // can't hide a resumable one from every working view.
            if !rec.status.is_terminal() {
                rec.archived = false;
            }
            // Tags follow the add_tags rules, so a hand-edited record can't
            // carry ones that would block tagging it.
            rec.tags = tags::sanitized(&rec.tags);
            // The on-disk state is authoritative for how far we got (a
            // multi-segment `.part` is pre-allocated, so not its length), but
            // only for a record that owns it: a stopped one whose partial was
            // deleted may share its target with a newer download.
            let mut segments = Vec::new();
            if owns_partial(&rec) {
                if let Some(downloaded) = state::downloaded_bytes(&rec.target) {
                    rec.downloaded = downloaded;
                }
                segments = match DownloadState::load(&state::sidecar_path(&rec.target)) {
                    Ok(DownloadState {
                        segments: Some(segments),
                        ..
                    }) => segments,
                    Ok(single) => single_segment(rec.downloaded, single.total),
                    Err(_) => Vec::new(),
                };
            }
            let limiter = Arc::new(RateLimiter::new(rec.limit));
            rec.limit = limiter.rate();
            order.push(rec.id);
            let batch_limiter = rec.batch.zip(rec.batch_limit).map(|(batch, limit)| {
                batch_limiters
                    .entry(batch)
                    .or_insert_with(|| Arc::new(RateLimiter::new(Some(limit))))
                    .clone()
            });
            let mut entry = Entry::new(rec, limiter);
            // A start time that passed while rdm was closed never fires
            // silently; the user decides.
            entry.missed = entry.record.start_at.is_some_and(|s| s.is_due(clock.now()));
            entry.batch_limiter = batch_limiter;
            entry.segments = segments;
            entries.insert(entry.record.id, entry);
        }
        let next_id = data.next_id.max(1);
        let logs = Arc::new(DownloadLogs::new(store.logs_dir()));
        for &id in &order {
            logs.open(id);
        }
        if let Some(dir) = store.logs_dir() {
            // Off the caller's thread: it walks the folder.
            let live: HashSet<u64> = order.iter().copied().collect();
            handle.spawn_blocking(move || {
                prune(
                    &dir,
                    &live,
                    next_id,
                    SystemTime::now(),
                    LOG_MAX_AGE,
                    LOG_TOTAL_CAP,
                );
            });
        }
        let settings = data.settings.clamped();
        let queue = QueueState::at(&settings.schedule, clock.now());
        let global = Arc::new(RateLimiter::new(queue.limit.or(settings.global_limit)));
        let next_group = entries
            .values()
            .filter_map(|e| e.record.start_group)
            .max()
            .map_or(1, |g| g + 1);
        let st = ManagerState {
            next_group,
            next_id,
            order,
            entries,
            scope_limiters: Default::default(),
            actions: Default::default(),
            settings,
        };
        let (tx, _rx) = watch::channel(Vec::new());
        let (queue_tx, _rx) = watch::channel(queue);
        let (actions_tx, _rx) = watch::channel(ActionsStatus {
            power: PowerStatus::Off,
        });
        let inner = Arc::new(Inner {
            tx,
            store,
            last_publish: Mutex::new(Instant::now()),
            handle,
            closed: AtomicBool::new(false),
            global,
            logs,
            missing_tx: tokio::sync::broadcast::channel(64).0,
            missing_busy: Mutex::default(),
            find_tx: tokio::sync::broadcast::channel(2048).0,
            find_results: Mutex::default(),
            finds: Mutex::default(),
            next_find: std::sync::atomic::AtomicU64::new(1),
            probes: Arc::new(tokio::sync::Semaphore::new(
                crate::engine::PROBE_CONCURRENCY,
            )),
            clock,
            queue_tx,
            events_tx: tokio::sync::broadcast::channel(256).0,
            actions_tx,
            power: Mutex::new(Arc::new(NoPower)),
            actions_clock_held: AtomicBool::new(false),
            state: Mutex::new(st),
        });
        inner.spawn_window_clock();
        inner.spawn_actions();
        // Prime the channel with the restored list.
        {
            let st = lock(&inner.state);
            let _ = inner.tx.send(inner.snapshots(&st));
        }
        Ok(Self { inner })
    }
}

impl Inner {
    /// Persist the whole document. Best-effort: a save failure must not take
    /// down an in-flight download.
    pub(super) fn persist(&self, st: &ManagerState) {
        let data = StoreData {
            version: SCHEMA_VERSION,
            next_id: st.next_id,
            downloads: st
                .order
                .iter()
                .filter_map(|id| st.entries.get(id))
                .map(|e| e.record.clone())
                .collect(),
            settings: st.settings.clone(),
        };
        let _ = self.store.save(&data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::manager::tests::{record, FixedStore};
    use crate::engine::store::{DownloadRecord, Settings};

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
        let st = lock(&mgr.inner.state);
        for (id, want) in [(1, Some(256 * 1024)), (2, None)] {
            let e = &st.entries[&id];
            assert_eq!(e.limiter.rate(), want, "download {id}'s limiter");
            assert_eq!(e.snapshot().limit, want, "download {id}'s shown limit");
        }
    }

    /// A hash cut short by quitting can't still be running after a restart.
    #[tokio::test]
    async fn load_resets_a_pending_checksum() {
        let pending = DownloadRecord {
            status: Status::Completed,
            checksum: Some("sha256:ab".into()),
            checksum_state: ChecksumState::Pending,
            ..record(1, None)
        };
        let verified = DownloadRecord {
            id: 2,
            checksum_state: ChecksumState::Verified,
            ..pending.clone()
        };
        let data = StoreData {
            next_id: 3,
            downloads: vec![pending, verified],
            ..StoreData::default()
        };
        let mgr = Manager::load(
            Box::new(FixedStore(data)),
            tokio::runtime::Handle::current(),
        )
        .unwrap();
        let states: Vec<_> = mgr.snapshot().iter().map(|s| s.checksum_state).collect();
        assert_eq!(states, [ChecksumState::None, ChecksumState::Verified]);
    }

    /// A store can't load a resumable download as archived, which would hide
    /// it from every working view.
    #[tokio::test]
    async fn load_keeps_only_finished_downloads_archived() {
        let paused = DownloadRecord {
            archived: true,
            ..record(1, None)
        };
        let done = DownloadRecord {
            id: 2,
            status: Status::Completed,
            ..paused.clone()
        };
        let data = StoreData {
            next_id: 3,
            downloads: vec![paused, done],
            ..StoreData::default()
        };
        let mgr = Manager::load(
            Box::new(FixedStore(data)),
            tokio::runtime::Handle::current(),
        )
        .unwrap();
        let archived: Vec<_> = mgr.snapshot().iter().map(|s| s.archived).collect();
        assert_eq!(archived, [false, true]);
    }
}
