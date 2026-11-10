//! Tags on downloads: adding and removing them.

use std::collections::HashSet;

use super::{Manager, ManagerState};
use crate::engine::lock::lock;
use crate::engine::tags;

impl Manager {
    /// Add `tags` to each listed download, skipping ones it already has
    /// (ignoring case) and keeping the order added. A tag is trimmed and 1 to
    /// [`MAX_TAG_LEN`](tags::MAX_TAG_LEN) characters, and a download holds at
    /// most [`MAX_TAGS`](tags::MAX_TAGS); breaking either is
    /// [`Error::InvalidRequest`](crate::engine::Error::InvalidRequest) and changes nothing. Returns how many
    /// downloads changed.
    pub fn add_tags(&self, ids: &[u64], tags: &[String]) -> crate::engine::Result<usize> {
        let added = tags::normalize(tags)?;
        let st = &mut *lock(&self.inner.state);
        let mut updates = Vec::new();
        for id in unique(ids) {
            if let Some(e) = st.entries.get(&id) {
                let merged = tags::merged(&e.record.tags, &added)?;
                if merged != e.record.tags {
                    updates.push((id, merged));
                }
            }
        }
        Ok(self.apply_tags(st, updates))
    }

    /// Remove `tags` (trimmed, matched ignoring case) from each listed
    /// download. Returns how many downloads changed.
    pub fn remove_tags(&self, ids: &[u64], tags: &[String]) -> usize {
        let removed: Vec<String> = tags.iter().map(|t| t.trim().to_string()).collect();
        let st = &mut *lock(&self.inner.state);
        let updates = unique(ids)
            .into_iter()
            .filter_map(|id| st.entries.get(&id).map(|e| (id, &e.record.tags)))
            .filter_map(|(id, current)| {
                let kept: Vec<String> = current
                    .iter()
                    .filter(|t| !tags::contains(&removed, t))
                    .cloned()
                    .collect();
                (kept.len() != current.len()).then_some((id, kept))
            })
            .collect();
        self.apply_tags(st, updates)
    }

    /// Store each `(id, tags)` pair, then persist and publish if any.
    fn apply_tags(&self, st: &mut ManagerState, updates: Vec<(u64, Vec<String>)>) -> usize {
        let changed = updates.len();
        for (id, tags) in updates {
            if let Some(e) = st.entries.get_mut(&id) {
                e.record.tags = tags;
            }
        }
        if changed > 0 {
            self.inner.persist(st);
            self.inner.publish(st);
        }
        changed
    }
}

/// `ids` without repeats, in first-seen order.
fn unique(ids: &[u64]) -> Vec<u64> {
    let mut seen = HashSet::new();
    ids.iter().copied().filter(|id| seen.insert(*id)).collect()
}
