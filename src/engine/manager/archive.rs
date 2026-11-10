//! The archive: hiding finished downloads from the working views, and back.

use super::{Inner, Manager, ManagerState};
use crate::engine::lock::lock;
use crate::engine::store::Status;

impl Inner {
    /// Set or clear the archive flag on each listed finished download whose
    /// flag differs, then persist and publish if any changed. Returns the ids
    /// that changed, in the order given.
    fn set_archived(&self, st: &mut ManagerState, ids: &[u64], archived: bool) -> Vec<u64> {
        let mut changed = Vec::new();
        for &id in ids {
            let Some(e) = st.entries.get_mut(&id) else {
                continue;
            };
            if e.record.archived != archived && e.record.status.is_terminal() {
                e.record.archived = archived;
                changed.push(id);
            }
        }
        if !changed.is_empty() {
            self.persist(st);
            self.publish(st);
        }
        changed
    }
}

impl Manager {
    /// Archive the listed downloads: hide them from the working views but
    /// keep their records. Only finished (completed, failed or cancelled)
    /// downloads are archived; any other id is left unchanged. Returns the ids
    /// it archived, in the order given, so exactly those can be unarchived to
    /// undo it.
    pub fn archive(&self, ids: &[u64]) -> Vec<u64> {
        let st = &mut *lock(&self.inner.state);
        self.inner.set_archived(st, ids, true)
    }

    /// Return the listed archived downloads to the working views. Returns how
    /// many changed.
    pub fn unarchive(&self, ids: &[u64]) -> usize {
        let st = &mut *lock(&self.inner.state);
        self.inner.set_archived(st, ids, false).len()
    }

    /// Archive every finished, not yet archived download whose status is in
    /// `statuses` and, with `only`, whose id is listed. Returns the ids it
    /// archived, in display order, so they can be unarchived to undo it.
    pub fn archive_finished(&self, statuses: &[Status], only: Option<&[u64]>) -> Vec<u64> {
        let st = &mut *lock(&self.inner.state);
        let ids: Vec<u64> = st
            .order
            .iter()
            .filter_map(|id| st.entries.get(id))
            .filter(|e| statuses.contains(&e.record.status))
            .filter(|e| only.is_none_or(|ids| ids.contains(&e.record.id)))
            .map(|e| e.record.id)
            .collect();
        self.inner.set_archived(st, &ids, true)
    }
}
