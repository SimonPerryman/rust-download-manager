//! Start-at schedules: downloads held out of the queue until a local time,
//! and the ones whose time passed while rdm was closed. A schedule is the
//! downloads given one start time together (one add, one batch, one
//! selection); it is only a shared identity on their records.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use super::commands::requeue;
use super::{Inner, Manager, ManagerState};
use crate::engine::lock::lock;
use crate::engine::schedule::StartAt;
use crate::engine::store::Status;

/// One start schedule: when it starts and which downloads wait for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StartSchedule {
    /// Its identity, for [`Manager::start_schedule_now`] and the like.
    pub group: u64,
    /// The local time it starts, or was due to if `missed`.
    pub at: StartAt,
    /// The time passed while rdm was closed, so nothing started by itself.
    pub missed: bool,
    /// The downloads waiting on it, in list order.
    pub downloads: Vec<u64>,
}

/// How many schedules were missed and how many downloads they hold, for the
/// notice ("3 scheduled starts were missed (50 downloads)").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct MissedSummary {
    /// Missed schedules.
    pub schedules: usize,
    /// Downloads in them.
    pub downloads: usize,
}

impl ManagerState {
    /// The schedules in force, soonest first.
    fn start_schedules(&self) -> Vec<StartSchedule> {
        let mut groups: BTreeMap<u64, StartSchedule> = BTreeMap::new();
        for id in &self.order {
            let Some(e) = self.entries.get(id).filter(|e| e.is_held()) else {
                continue;
            };
            let (Some(at), Some(group)) = (e.record.start_at, e.record.start_group) else {
                continue;
            };
            groups
                .entry(group)
                .or_insert_with(|| StartSchedule {
                    group,
                    at,
                    missed: e.missed,
                    downloads: Vec::new(),
                })
                .downloads
                .push(*id);
        }
        let mut all: Vec<_> = groups.into_values().collect();
        all.sort_by_key(|s| (s.at.0, s.group));
        all
    }

    /// The held downloads of `groups`, with the position each has in the
    /// list.
    fn held_in(&self, groups: &[u64]) -> Vec<(StartAt, u64, usize, u64)> {
        let mut held = Vec::new();
        for (pos, id) in self.order.iter().enumerate() {
            let Some(e) = self.entries.get(id).filter(|e| e.is_held()) else {
                continue;
            };
            if let (Some(at), Some(group)) = (e.record.start_at, e.record.start_group) {
                if groups.contains(&group) {
                    held.push((at, group, pos, *id));
                }
            }
        }
        held
    }

    /// Release the downloads of `groups` into the queue, oldest due first:
    /// they take each other's places in the list, in due order, so the queue
    /// (which starts in list order) feeds them under its usual cap.
    fn release(&mut self, groups: &[u64]) {
        let mut held = self.held_in(groups);
        let mut slots: Vec<usize> = held.iter().map(|&(_, _, pos, _)| pos).collect();
        slots.sort_unstable();
        held.sort_by_key(|&(at, group, pos, _)| (at.0, group, pos));
        for (slot, (.., id)) in slots.into_iter().zip(held) {
            self.order[slot] = id;
            if let Some(e) = self.entries.get_mut(&id) {
                e.clear_start();
            }
        }
    }
}

impl Inner {
    /// Release every schedule whose time has come while rdm ran (a missed one
    /// waits for the user). True if any were. Call with the state lock held.
    pub(super) fn fire_due_starts(self: &Arc<Self>, st: &mut ManagerState) -> bool {
        let now = self.clock.now();
        let due: Vec<u64> = st
            .start_schedules()
            .into_iter()
            .filter(|s| !s.missed && s.at.is_due(now))
            .map(|s| s.group)
            .collect();
        if due.is_empty() {
            return false;
        }
        st.release(&due);
        true
    }
}

impl Manager {
    /// The start schedules waiting now, soonest first, missed ones included.
    pub fn start_schedules(&self) -> Vec<StartSchedule> {
        lock(&self.inner.state).start_schedules()
    }

    /// How many schedules were missed (their time passed while rdm was
    /// closed) and how many downloads they hold.
    pub fn missed_schedules(&self) -> MissedSummary {
        let missed: Vec<_> = self
            .start_schedules()
            .into_iter()
            .filter(|s| s.missed)
            .collect();
        MissedSummary {
            schedules: missed.len(),
            downloads: missed.iter().map(|s| s.downloads.len()).sum(),
        }
    }

    /// Read a start time typed by a person against the clock's now (see
    /// [`StartAt::parse`]); refused as
    /// [`Error::InvalidRequest`](crate::engine::Error::InvalidRequest).
    pub fn parse_start_at(&self, text: &str) -> crate::engine::Result<StartAt> {
        StartAt::parse(text, self.inner.clock.now())
            .map_err(|e| crate::engine::Error::InvalidRequest(e.to_string()))
    }

    /// Hold `ids` out of the queue until `at` as one new schedule, taking
    /// them out of any schedule they were in. Waiting, paused, failed and
    /// stopped downloads can be scheduled; running, finished or unknown ones
    /// are skipped. Returns how many were. A time already past starts them
    /// on the next look at the clock.
    pub fn schedule_start(&self, ids: &[u64], at: StartAt) -> usize {
        let st = &mut *lock(&self.inner.state);
        let group = st.new_group();
        let mut count = 0;
        for &id in ids {
            let queued = |st: &ManagerState| {
                st.entries
                    .get(&id)
                    .is_some_and(|e| e.task.is_none() && e.record.status == Status::Queued)
            };
            if !queued(st) && !requeue(st, id) {
                continue;
            }
            if let Some(e) = st.entries.get_mut(&id) {
                e.record.start_at = Some(at);
                e.record.start_group = Some(group);
                e.missed = false;
                count += 1;
            }
        }
        self.finish_change(st);
        count
    }

    /// Move whole schedules to a new time (one schedule if several), as when
    /// a missed one is rescheduled. Unknown groups are ignored.
    pub fn reschedule(&self, groups: &[u64], at: StartAt) {
        let st = &mut *lock(&self.inner.state);
        let target = st.new_group();
        for e in st.entries.values_mut().filter(|e| e.is_held()) {
            if e.record.start_group.is_some_and(|g| groups.contains(&g)) {
                e.record.start_at = Some(at);
                e.record.start_group = Some(target);
                e.missed = false;
            }
        }
        self.finish_change(st);
    }

    /// Start schedules now (Start now, Start all): their downloads join the
    /// normal queue, oldest due first, under the usual cap.
    pub fn start_schedule_now(&self, groups: &[u64]) {
        let st = &mut *lock(&self.inner.state);
        st.release(groups);
        self.finish_change(st);
    }

    /// Drop schedules without starting anything (Dismiss): their downloads
    /// stay in the list as ordinary paused downloads, to resume when wanted.
    pub fn dismiss_schedules(&self, groups: &[u64]) {
        let st = &mut *lock(&self.inner.state);
        for (.., id) in st.held_in(groups) {
            if let Some(e) = st.entries.get_mut(&id) {
                e.clear_start();
                e.record.status = Status::Paused;
            }
        }
        self.finish_change(st);
    }

    fn finish_change(&self, st: &mut ManagerState) {
        self.inner.schedule(st);
        self.inner.persist(st);
        self.inner.publish(st);
    }
}
