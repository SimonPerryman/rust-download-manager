//! Control hooks the chunk loop consults each chunk. M0 wires up cancel; M1
//! adds pause; rate-limiting is a structural hook (the call site exists) for M4.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// What the chunk loop should do before writing the next chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep downloading.
    Continue,
    /// Stop now; the engine discards the partial file and returns
    /// [`Error::Cancelled`](crate::Error::Cancelled).
    Cancel,
    /// Stop now but keep the `.part` + sidecar; the engine returns
    /// [`Error::Paused`](crate::Error::Paused) so the download can be resumed.
    Pause,
}

/// A cheap, cloneable handle used to steer an in-flight download.
///
/// Clones share one state, so a frontend can hold a clone and call
/// [`Control::cancel`] or [`Control::pause`] from elsewhere while the engine
/// owns another.
#[derive(Clone, Default)]
pub struct Control {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    cancelled: AtomicBool,
    paused: AtomicBool,
}

impl Control {
    /// A fresh control handle that permits the download to run to completion.
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation. The engine notices at the next chunk boundary,
    /// removes the partial file + sidecar, and returns [`Error::Cancelled`](crate::Error::Cancelled).
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// Request a pause. The engine notices at the next chunk boundary, flushes
    /// and keeps the `.part` + sidecar, and returns [`Error::Paused`](crate::Error::Paused).
    pub fn pause(&self) {
        self.inner.paused.store(true, Ordering::SeqCst);
    }

    /// Whether a pause has been requested.
    pub fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::SeqCst)
    }

    /// Clear a pending pause so the handle can drive a fresh/resumed download.
    pub fn resume(&self) {
        self.inner.paused.store(false, Ordering::SeqCst);
    }

    /// Consulted by the loop before each chunk is written. Cancel takes
    /// priority over pause.
    pub(crate) fn poll(&self) -> Flow {
        if self.is_cancelled() {
            Flow::Cancel
        } else if self.is_paused() {
            Flow::Pause
        } else {
            Flow::Continue
        }
    }

    /// Throttle hook awaited after each chunk. No-op today; M4's rate limiter
    /// sleeps here to enforce a byte/sec cap.
    pub(crate) async fn throttle(&self, _chunk_len: usize) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_control_continues() {
        let c = Control::new();
        assert_eq!(c.poll(), Flow::Continue);
        assert!(!c.is_cancelled());
        assert!(!c.is_paused());
    }

    #[test]
    fn cancel_is_observed_through_clones() {
        let a = Control::new();
        let b = a.clone();
        a.cancel();
        assert!(b.is_cancelled());
        assert_eq!(b.poll(), Flow::Cancel);
    }

    #[test]
    fn pause_is_observed_through_clones_and_cleared_by_resume() {
        let a = Control::new();
        let b = a.clone();
        a.pause();
        assert!(b.is_paused());
        assert_eq!(b.poll(), Flow::Pause);

        a.resume();
        assert!(!b.is_paused());
        assert_eq!(b.poll(), Flow::Continue);
    }

    #[test]
    fn cancel_takes_priority_over_pause() {
        let c = Control::new();
        c.pause();
        c.cancel();
        assert_eq!(c.poll(), Flow::Cancel);
    }
}
