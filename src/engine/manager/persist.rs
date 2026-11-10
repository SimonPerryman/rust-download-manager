//! The saved download list: restoring it at load, and saving it back.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::watch;

use super::files::owns_partial;
use super::{Entry, Inner, Manager, ManagerState};
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
        let data = store.load()?;
        let mut entries = HashMap::new();
        let mut order = Vec::new();
        for mut rec in data.downloads {
            // A download that was running at shutdown has no task now; restore
            // it paused so the user can resume it deliberately.
            if rec.status.is_running() {
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
            let mut entry = Entry::new(rec, limiter);
            entry.segments = segments;
            entries.insert(entry.record.id, entry);
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
            logs: Arc::default(),
            state: Mutex::new(st),
        });
        // Prime the channel with the restored list.
        {
            let st = inner.state.lock().unwrap();
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
            settings: st.settings,
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
        let st = mgr.inner.state.lock().unwrap();
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
