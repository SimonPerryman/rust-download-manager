//! The queue window: pausing running downloads when it closes, starting the
//! waiting ones when it opens, and telling a frontend which it is.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

use super::{Inner, Manager, ManagerState};
use crate::engine::lock::lock;
use crate::engine::schedule::{LocalDateTime, Schedule};
use crate::engine::store::Status;

/// How often the Manager looks at the clock. A window edge is on a minute, so
/// a second's lag is invisible; it also notices the clock being changed.
const POLL: Duration = Duration::from_secs(1);

/// Whether the queue is open and which scheduled speed limit applies, and
/// when each next changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueState {
    /// Downloads may start. True when no enabled queue window exists.
    pub open: bool,
    /// The local time the queue next opens (if closed) or closes (if open);
    /// `None` if it never will.
    pub until: Option<LocalDateTime>,
    /// The speed limit in bytes/s a schedule entry imposes now, replacing the
    /// global one; `None` when the global limit applies.
    pub limit: Option<u64>,
    /// The local time the scheduled limit next changes; `None` if it never will.
    pub limit_until: Option<LocalDateTime>,
    /// The scheduled limit from `limit_until` on (`None`: the global one).
    pub limit_then: Option<u64>,
}

impl QueueState {
    pub(super) fn at(schedule: &Schedule, now: LocalDateTime) -> Self {
        let limit_until = schedule.next_speed_change(now);
        Self {
            open: schedule.queue_open(now),
            until: schedule.next_queue_change(now),
            limit: schedule.speed_limit(now),
            limit_until,
            limit_then: limit_until.and_then(|t| schedule.speed_limit(t)),
        }
    }
}

impl Inner {
    /// Whether the queue window lets downloads start now.
    pub(super) fn queue_open(&self, st: &ManagerState) -> bool {
        st.settings.schedule.queue_open(self.clock.now())
    }

    /// Bring the downloads in line with the window as of now: when it has
    /// just closed, pause the running ones that can resume; when it has just
    /// opened, start the waiting ones. Call with the state lock held, after
    /// any change to the rules and on every clock tick.
    pub(super) fn apply_window(self: &Arc<Self>, st: &mut ManagerState) {
        if self.fire_due_starts(st) {
            self.schedule(st);
            self.persist(st);
            self.publish(st);
        }
        let state = QueueState::at(&st.settings.schedule, self.clock.now());
        // The limiter is shared by running downloads, so this takes effect
        // mid-stream. Only touched on a change: set_rate settles the bucket.
        let rate = state.limit.or(st.settings.global_limit);
        if self.global.rate() != rate {
            self.global.set_rate(rate);
        }
        let was_open = std::mem::replace(&mut st.queue_open, state.open);
        if was_open && !state.open {
            self.pause_for_window(st);
        } else if !was_open && state.open {
            self.schedule(st);
        }
        self.queue_tx.send_if_modified(|current| {
            let changed = *current != state;
            *current = state;
            changed
        });
    }

    /// Pause each running download that a pause won't cost its bytes. One
    /// whose server can't resume is let finish, and so is every running one
    /// when a window says it only stops new starts. A paused download goes
    /// back to waiting (see `finish`), so it resumes by itself at open.
    fn pause_for_window(&self, st: &mut ManagerState) {
        let only_stops_starting = st
            .settings
            .schedule
            .queue_windows
            .iter()
            .any(|r| r.enabled && r.finish_running);
        if only_stops_starting {
            return;
        }
        for e in st.entries.values_mut() {
            let can_resume = e.tally.resumable() != Some(false);
            if e.task.is_some() && e.record.status == Status::Downloading && can_resume {
                e.window_pause = true;
                e.control.pause();
            }
        }
    }

    /// Look at the clock every [`POLL`] until the Manager is dropped.
    pub(super) fn spawn_window_clock(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        self.handle.spawn(async move {
            loop {
                tokio::time::sleep(POLL).await;
                let Some(inner) = weak.upgrade() else { break };
                if inner.closed.load(Ordering::SeqCst) {
                    break;
                }
                inner.apply_window(&mut lock(&inner.state));
                inner.real_tick();
            }
        });
    }
}

impl Manager {
    /// The aggregate cap in bytes/s in force now: a speed entry's limit
    /// inside its window, else the global limit; `None` is unlimited.
    pub fn effective_global_limit(&self) -> Option<u64> {
        self.inner.global.rate()
    }

    /// Whether the queue is open and when that next changes, as of now.
    pub fn queue_state(&self) -> QueueState {
        QueueState::at(
            &lock(&self.inner.state).settings.schedule,
            self.inner.clock.now(),
        )
    }

    /// Subscribe to [`QueueState`] changes. The receiver always holds the
    /// latest value.
    pub fn subscribe_queue(&self) -> tokio::sync::watch::Receiver<QueueState> {
        self.inner.queue_tx.subscribe()
    }

    /// Look at the clock now and act on a window edge it has passed, instead
    /// of waiting for the next tick (a second). For tests that move a fake
    /// clock.
    pub fn check_schedule(&self) {
        self.inner.apply_window(&mut lock(&self.inner.state));
    }
}
