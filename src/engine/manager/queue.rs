//! The queue: starting waiting downloads as slots free, and reordering them.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use super::task::spawn_task;
use super::{Inner, Manager, ManagerState};
use crate::engine::lock::lock;
use crate::engine::store::Status;
use crate::engine::Control;

impl Inner {
    /// Start queued downloads, in display order, while fewer than
    /// `max_concurrent` are running. Called with the state lock held.
    pub(super) fn schedule(self: &Arc<Self>, st: &mut ManagerState) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let cap = st.settings.max_concurrent;
        let settings = st.settings.clone();
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
            if e.record.downloaded == 0 {
                e.record.active_ms = Some(0);
                e.tally = Arc::default();
            }
            e.started = Some(Instant::now());
            e.task = Some(spawn_task(
                self.clone(),
                &e.record,
                control,
                &settings,
                e.tally.clone(),
            ));
        }
    }
}

impl Manager {
    /// Move a waiting (queued, not yet started) download to position `index`
    /// among the waiting ones (clamped to the last): the order they start in.
    /// Persists. No-op for any other download.
    pub fn move_queued(&self, id: u64, index: usize) {
        let st = &mut *lock(&self.inner.state);
        let entries = &st.entries;
        let waiting = |id: u64| {
            entries
                .get(&id)
                .is_some_and(|e| e.record.status == Status::Queued && e.task.is_none())
        };
        if !waiting(id) {
            return;
        }
        move_among(&mut st.order, waiting, id, index);
        self.inner.persist(st);
        self.inner.publish(st);
    }
}

/// Move `id` to position `index` (clamped) among the entries of `order` that
/// are `waiting`, leaving every other entry where it is.
fn move_among(order: &mut [u64], waiting: impl Fn(u64) -> bool, id: u64, index: usize) {
    let (slots, mut ids): (Vec<usize>, Vec<u64>) = order
        .iter()
        .enumerate()
        .filter(|&(_, &other)| waiting(other))
        .map(|(slot, &other)| (slot, other))
        .unzip();
    let Some(from) = ids.iter().position(|&other| other == id) else {
        return;
    };
    let moved = ids.remove(from);
    ids.insert(index.min(ids.len()), moved);
    for (slot, id) in slots.into_iter().zip(ids) {
        order[slot] = id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_among_permutes_only_the_waiting_entries() {
        let waiting = |id: u64| id.is_multiple_of(2);
        let moved = |id, index| {
            let mut order = vec![1, 2, 3, 4, 5, 6, 8];
            move_among(&mut order, waiting, id, index);
            order
        };
        assert_eq!(moved(8, 0), [1, 8, 3, 2, 5, 4, 6]);
        assert_eq!(moved(2, 2), [1, 4, 3, 6, 5, 2, 8]);
        assert_eq!(moved(2, 99), [1, 4, 3, 6, 5, 8, 2], "clamped to the last");
        assert_eq!(moved(6, 2), [1, 2, 3, 4, 5, 6, 8], "already there");
        assert_eq!(moved(3, 0), [1, 2, 3, 4, 5, 6, 8], "not waiting");
    }
}
