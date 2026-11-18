//! The queue window: pausing running downloads when it closes, starting the
//! waiting ones when it opens, and telling a frontend which it is.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

use super::{Entry, Inner, Manager, ManagerState};
use crate::engine::lock::lock;
use crate::engine::schedule::{LocalDateTime, Schedule, Scope, Subject};
use crate::engine::store::Status;
use crate::engine::RateLimiter;

/// How often the Manager looks at the clock. A window edge is on a minute, so
/// a second's lag is invisible; it also notices the clock being changed.
const POLL: Duration = Duration::from_secs(1);

/// Whether the queue is open and which scheduled speed limit applies, and
/// when each next changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueState {
    /// Downloads may start. True when no enabled all-downloads queue window
    /// exists; windows scoped to a tag, host or batch hold only their own
    /// downloads and don't show here.
    pub open: bool,
    /// The local time the queue next opens (if closed) or closes (if open);
    /// `None` if it never will.
    pub until: Option<LocalDateTime>,
    /// The speed limit in bytes/s an all-downloads schedule entry imposes now,
    /// replacing the global one; `None` when the global limit applies.
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
    /// Bring the downloads in line with the windows as of now: a running
    /// download whose windows have closed pauses if it can resume, and
    /// waiting ones whose window has opened start. Call with the state lock
    /// held, after any change to the rules and on every clock tick.
    pub(super) fn apply_window(self: &Arc<Self>, st: &mut ManagerState) {
        if self.fire_due_starts(st) {
            self.schedule(st);
            self.persist(st);
            self.publish(st);
        }
        let now = self.clock.now();
        let state = QueueState::at(&st.settings.schedule, now);
        // The limiter is shared by running downloads, so this takes effect
        // mid-stream. Only touched on a change: set_rate settles the bucket.
        let rate = state.limit.or(st.settings.global_limit);
        if self.global.rate() != rate {
            self.global.set_rate(rate);
        }
        sync_scope_limiters(st, now);
        let mut opened = false;
        let schedule = st.settings.schedule.clone();
        for e in st.entries.values_mut() {
            let open = schedule.queue_open_for(now, &subject(e));
            opened |= open && !e.gate_open;
            e.gate_open = open;
            if !open {
                pause_for_window(&schedule, e);
            }
        }
        if opened {
            self.schedule(st);
        }
        self.queue_tx.send_if_modified(|current| {
            let changed = *current != state;
            *current = state;
            changed
        });
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

    /// The cap in bytes/s a running download is held to now: its slowest
    /// limiter (its own, the global and any scoped or batch cap). `None` when
    /// unlimited, unknown, or not running.
    pub fn running_speed_cap(&self, id: u64) -> Option<u64> {
        let st = lock(&self.inner.state);
        let e = st.entries.get(&id).filter(|e| e.task.is_some())?;
        e.control.rate_limit()
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

pub(super) fn subject(e: &Entry) -> Subject<'_> {
    Subject {
        tags: &e.record.tags,
        url: &e.record.url,
        batch: e.record.batch,
    }
}

/// Pause `e` if it runs, can resume and its closed window doesn't merely stop
/// new starts. One whose server can't resume is let finish, since a pause
/// would cost its bytes; a window flagged `finish_running` does the same for
/// the downloads it applies to. A paused download goes back to waiting (see
/// `finish`), so it resumes by itself at open.
fn pause_for_window(schedule: &Schedule, e: &mut Entry) {
    let s = subject(e);
    let only_stops_starting = schedule
        .queue_windows
        .iter()
        .any(|r| r.enabled && r.finish_running && r.scope.matches(&s));
    let can_resume = e.tally.resumable() != Some(false);
    if !only_stops_starting
        && e.task.is_some()
        && e.record.status == Status::Downloading
        && can_resume
    {
        e.window_pause = true;
        e.control.pause();
    }
}

/// Keep one limiter per scope a speed entry names at that scope's cap now
/// (unlimited outside its windows), and hand it to every running download the
/// scope matches. The limiter is shared, so the matching downloads together
/// stay under the cap, and a download in two scopes runs at the slower one.
/// A scope no entry names any more is set unlimited and forgotten; a running
/// download keeps holding it, harmlessly.
pub(super) fn sync_scope_limiters(st: &mut ManagerState, now: LocalDateTime) {
    let scopes = st.settings.schedule.speed_scopes();
    st.scope_limiters.retain(|scope, limiter| {
        let kept = scopes.contains(scope);
        if !kept {
            limiter.set_rate(None);
        }
        kept
    });
    for scope in &scopes {
        let rate = st.settings.schedule.scoped_speed_limit(now, scope);
        let limiter = st
            .scope_limiters
            .entry(scope.clone())
            .or_insert_with(|| Arc::new(RateLimiter::new(rate)));
        if limiter.rate() != rate {
            limiter.set_rate(rate);
        }
    }
    for e in st.entries.values().filter(|e| e.task.is_some()) {
        attach_scope_limiters(&st.scope_limiters, e);
    }
}

/// Give `e`'s control the limiter of each scope it matches.
pub(super) fn attach_scope_limiters(limiters: &HashMap<Scope, Arc<RateLimiter>>, e: &Entry) {
    let s = subject(e);
    for (scope, limiter) in limiters {
        if scope.matches(&s) {
            e.control.add_limiter(limiter);
        }
    }
}
