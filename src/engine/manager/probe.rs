//! Sizing queued downloads: a probe per add, off the queue and off the add's
//! critical path.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::{Entry, Inner};
use crate::engine::filename;
use crate::engine::lock::lock;
use crate::engine::probe::{probe, Probed};
use crate::engine::store::Status;
use crate::engine::{build_client, ProxyMode};

/// Whether a probe's answer can still be taken: the download waits (queued or
/// paused) and hasn't started a task. Once the first GET has run, its numbers
/// win and a late probe is ignored.
fn awaiting_start(e: &Entry) -> bool {
    e.task.is_none() && matches!(e.record.status, Status::Queued | Status::Paused)
}

impl Inner {
    /// Probe download `id`'s `url` on its own task, never delaying the caller
    /// and never failing it: a refused or unanswered probe leaves the row as
    /// it was. At most [`PROBE_CONCURRENCY`](crate::engine::PROBE_CONCURRENCY)
    /// run at once; none uses a download slot. A download that has already
    /// started by the time the probe's turn comes is not probed at all (its
    /// own first response says the same).
    pub(super) fn spawn_probe(self: &Arc<Self>, id: u64, url: String, proxy: ProxyMode) {
        let inner = self.clone();
        self.handle.spawn(async move {
            let Ok(_turn) = inner.probes.clone().acquire_owned().await else {
                return;
            };
            if !inner.wants_probe(id) {
                return;
            }
            let Ok(client) = build_client(&proxy) else {
                return;
            };
            if let Some(probed) = probe(&client, &url).await {
                inner.apply_probe(id, probed);
            }
        });
    }

    fn wants_probe(&self, id: u64) -> bool {
        !self.closed.load(Ordering::SeqCst)
            && lock(&self.state)
                .entries
                .get(&id)
                .is_some_and(awaiting_start)
    }

    /// Record what a probe learned on download `id`: its size, if unknown,
    /// and its name, while the target is still a URL-derived placeholder.
    /// Ignored for a download that was removed, has started, or has finished.
    pub(super) fn apply_probe(&self, id: u64, probed: Probed) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let st = &mut *lock(&self.state);
        let Some(e) = st.entries.get_mut(&id).filter(|e| awaiting_start(e)) else {
            return;
        };
        let mut changed = false;
        if let (None, Some(size)) = (e.record.total, probed.size) {
            e.record.total = Some(size);
            changed = true;
        }
        if e.record.target_provisional {
            if let Some(dir) = e.record.target.parent() {
                let target = filename::join_in(dir, &probed.filename);
                if target != e.record.target {
                    e.record.target = target;
                    changed = true;
                }
            }
        }
        if changed {
            self.persist(st);
            self.publish(st);
        }
    }
}
