//! Post-download actions as subscribers of the Manager's lifecycle events:
//! the command run after each download, and the power action once everything
//! is done. The download code only announces events; it knows nothing of
//! what these do.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::broadcast::error::RecvError;

use super::{Inner, Manager, ManagerState};
use crate::engine::actions::{
    run_command, ActionsStatus, LifecycleEvent, PostActions, PowerAction, PowerBackend,
    PowerStatus, WaitReason, POWER_COUNTDOWN_SECS,
};
use crate::engine::history::{HistoryEvent, HistoryKind};
use crate::engine::lock::lock;
use crate::engine::schedule::StartAt;
use crate::engine::store::Status;

/// Where a power action stands in its countdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum PowerPhase {
    #[default]
    Idle,
    Countdown {
        left: u64,
    },
}

/// What the actions remember between events.
#[derive(Debug, Default)]
pub(super) struct ActionsState {
    /// Commands still running.
    pub(super) commands_running: usize,
    /// The queue emptied and the power action is due once nothing is in its
    /// way; dropped the moment a download shows up again.
    pub(super) armed: bool,
    pub(super) phase: PowerPhase,
    /// Why the machine refused the last power action.
    pub(super) failure: Option<String>,
}

impl ManagerState {
    /// What downloads are still in play, most pressing first: running ones,
    /// waiting ones (or ones the closed queue window holds), then ones held
    /// for a start time.
    fn downloads_in_play(&self) -> Option<WaitReason> {
        let mut queued = false;
        let mut startable = false;
        let mut scheduled: Option<StartAt> = None;
        for e in self.entries.values() {
            if e.task.is_some() {
                return Some(WaitReason::Running);
            }
            if e.record.status != Status::Queued {
                continue;
            }
            match e.record.start_at {
                Some(at) => {
                    scheduled = Some(scheduled.map_or(at, |s| if at.0 < s.0 { at } else { s }))
                }
                None => {
                    queued = true;
                    startable |= e.gate_open;
                }
            }
        }
        if queued {
            return Some(if startable {
                WaitReason::Queued
            } else {
                WaitReason::Window
            });
        }
        scheduled.map(|at| WaitReason::Scheduled { at })
    }

    /// Nothing running, waiting or scheduled.
    pub(super) fn is_idle(&self) -> bool {
        self.downloads_in_play().is_none()
    }

    fn power_status(&self) -> PowerStatus {
        let power = self.settings.actions.power;
        if !power.enabled {
            return PowerStatus::Off;
        }
        if let PowerPhase::Countdown { left } = self.actions.phase {
            return PowerStatus::Countdown {
                seconds_left: left,
                action: power.action,
            };
        }
        if let Some(message) = &self.actions.failure {
            return PowerStatus::Failed {
                message: message.clone(),
            };
        }
        let reason = self
            .downloads_in_play()
            .or(if self.actions.commands_running > 0 {
                Some(WaitReason::Command)
            } else {
                None
            });
        reason.map_or(PowerStatus::Ready, |reason| PowerStatus::Waiting { reason })
    }
}

impl Inner {
    /// Settle the power action against the state as it is now, advancing a
    /// countdown by `elapsed` seconds. Returns the action to carry out, if its
    /// countdown ran out. Call with the state lock held; publishes the status.
    pub(super) fn actions_eval(&self, st: &mut ManagerState, elapsed: u64) -> Option<PowerAction> {
        let power = st.settings.actions.power;
        let mut fire = None;
        if !power.enabled {
            st.actions.armed = false;
            st.actions.phase = PowerPhase::Idle;
            st.actions.failure = None;
        } else if st.downloads_in_play().is_some() {
            // Something new (or still) in the way: this round is dropped.
            st.actions.armed = false;
            st.actions.phase = PowerPhase::Idle;
        } else if st.actions.commands_running == 0 {
            if st.actions.armed && st.actions.phase == PowerPhase::Idle {
                st.actions.failure = None;
                st.actions.phase = PowerPhase::Countdown {
                    left: POWER_COUNTDOWN_SECS,
                };
            } else if let PowerPhase::Countdown { left } = st.actions.phase {
                let left = left.saturating_sub(elapsed);
                if left == 0 {
                    st.actions.armed = false;
                    st.actions.phase = PowerPhase::Idle;
                    fire = Some(power.action);
                } else {
                    st.actions.phase = PowerPhase::Countdown { left };
                }
            }
        }
        self.publish_actions_status(st);
        fire
    }

    /// Tell subscribers where the power action stands, if that changed.
    pub(super) fn publish_actions_status(&self, st: &ManagerState) {
        let status = ActionsStatus {
            power: st.power_status(),
        };
        self.actions_tx.send_if_modified(|current| {
            let changed = *current != status;
            *current = status;
            changed
        });
    }

    /// [`actions_eval`](Self::actions_eval), then carry out what it says.
    pub(super) fn actions_step(self: &Arc<Self>, elapsed: u64) {
        let fire = self.actions_eval(&mut lock(&self.state), elapsed);
        let Some(action) = fire else { return };
        let backend = lock(&self.power).clone();
        let result = backend.perform(action);
        let details = match &result {
            Ok(()) => format!("Ran {} after the last download", action.name()),
            Err(message) => format!("Couldn't {}: {message}", action.name()),
        };
        self.history
            .record(HistoryEvent::new(HistoryKind::Power, None, None, details));
        if let Err(message) = result {
            tracing::warn!("Couldn't {}: {message}", action.name());
            let st = &mut *lock(&self.state);
            st.actions.failure = Some(message);
            self.actions_eval(st, 0);
        }
    }

    /// Seconds to count on, from the real clock unless a test holds it.
    pub(super) fn real_tick(self: &Arc<Self>) {
        let held = self.actions_clock_held.load(Ordering::SeqCst);
        self.actions_step(u64::from(!held));
    }

    /// Run the subscribers of the event channel until the Manager is dropped.
    pub(super) fn spawn_actions(self: &Arc<Self>) {
        let mut events = self.events_tx.subscribe();
        let weak = Arc::downgrade(self);
        self.handle.spawn(async move {
            loop {
                let event = events.recv().await;
                let Some(inner) = weak.upgrade() else { break };
                match event {
                    Ok(LifecycleEvent::Finished { id, path, name }) => {
                        inner.run_command_for(id, path, name);
                    }
                    Ok(LifecycleEvent::QueueEmptied) => {
                        let st = &mut *lock(&inner.state);
                        st.actions.armed = st.settings.actions.power.enabled;
                        let _ = inner.actions_eval(st, 0);
                    }
                    // Missed some: the state says where things stand.
                    Err(RecvError::Lagged(_)) => inner.actions_step(0),
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }

    /// Start the user's command for a finished download, on its own thread.
    /// A failure shows on the download's row; it never fails the download.
    fn run_command_for(self: &Arc<Self>, id: u64, path: std::path::PathBuf, name: String) {
        let command = {
            let st = &mut *lock(&self.state);
            let command = st.settings.actions.command.clone();
            if !command.enabled || command.program.is_empty() {
                return;
            }
            if command.wait_for_exit {
                st.actions.commands_running += 1;
                let _ = self.actions_eval(st, 0);
            }
            command
        };
        let inner = self.clone();
        let waited = command.wait_for_exit;
        let program = std::path::Path::new(&command.program)
            .file_name()
            .map_or_else(
                || command.program.clone(),
                |n| n.to_string_lossy().into_owned(),
            );
        self.handle.spawn(async move {
            let outcome = tokio::task::spawn_blocking(move || run_command(&command, &path, &name))
                .await
                .unwrap_or_else(|e| Err(format!("the command's thread failed: {e}")));
            if inner.closed.load(Ordering::SeqCst) {
                return;
            }
            inner.history.record(HistoryEvent::new(
                HistoryKind::Command,
                Some(id),
                None,
                match &outcome {
                    Ok(()) => format!("{program} ran"),
                    Err(message) => format!("{program} failed: {message}"),
                },
            ));
            {
                let st = &mut *lock(&inner.state);
                if waited {
                    st.actions.commands_running = st.actions.commands_running.saturating_sub(1);
                }
                if let (Err(message), Some(e)) = (outcome, st.entries.get_mut(&id)) {
                    tracing::warn!("The command for download {id}: {message}");
                    e.command_warning = Some(message);
                }
                inner.publish(st);
            }
            inner.actions_step(0);
        });
    }
}

impl Manager {
    /// The command and power action settings.
    pub fn post_actions(&self) -> PostActions {
        self.settings().actions
    }

    /// Replace the command and power action settings. Saved with the
    /// settings. Switching the power action off cancels a countdown.
    pub fn set_post_actions(&self, actions: PostActions) {
        self.update_settings(|s| s.actions = actions);
    }

    /// Subscribe to the lifecycle events: a download finishing, and the queue
    /// emptying. The actions are subscribers of this channel like any other.
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<LifecycleEvent> {
        self.inner.events_tx.subscribe()
    }

    /// Where the power action stands (what it waits for, or its countdown).
    pub fn actions_status(&self) -> ActionsStatus {
        self.inner.actions_tx.borrow().clone()
    }

    /// Subscribe to [`ActionsStatus`] changes; the receiver holds the latest.
    pub fn subscribe_actions(&self) -> tokio::sync::watch::Receiver<ActionsStatus> {
        self.inner.actions_tx.subscribe()
    }

    /// Cancel a power action that is counting down. It stays switched on and
    /// runs again after the next time everything is done.
    pub fn cancel_power(&self) {
        let st = &mut *lock(&self.inner.state);
        st.actions.armed = false;
        st.actions.phase = PowerPhase::Idle;
        let _ = self.inner.actions_eval(st, 0);
    }

    /// Replace what carries out power actions. Frontends install the real
    /// one; a Manager that is never given one refuses to power anything down.
    pub fn set_power_backend(&self, backend: Arc<dyn PowerBackend>) {
        *lock(&self.inner.power) = backend;
    }

    /// For tests: stop counting countdown seconds from the real clock, so the
    /// countdown only moves with [`advance_actions_clock`](Self::advance_actions_clock).
    #[doc(hidden)]
    pub fn hold_actions_clock(&self) {
        self.inner.actions_clock_held.store(true, Ordering::SeqCst);
    }

    /// For tests: move the countdown on by `secs` seconds.
    #[doc(hidden)]
    pub fn advance_actions_clock(&self, secs: u64) {
        self.inner.actions_step(secs);
    }
}
