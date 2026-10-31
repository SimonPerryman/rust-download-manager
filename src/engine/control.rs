//! Control hooks consulted by the engine's chunk loop.
//!
//! The download loop checks a [`Control`] once per chunk and calls its throttle
//! hook after each write. In M0 the only behaviour wired up is **cancel**;
//! pause and rate-limiting are deliberately left as structural hooks (the call
//! sites exist in the loop) so M1/M4 can add them without reshaping the loop.

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
}

/// A cheap, cloneable handle used to steer an in-flight download.
///
/// Clones share one state, so a frontend can hold a clone and call
/// [`Control::cancel`] from elsewhere while the engine owns another.
#[derive(Clone, Default)]
pub struct Control {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    cancelled: AtomicBool,
}

impl Control {
    /// A fresh control handle that permits the download to run to completion.
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation. The engine notices at the next chunk boundary,
    /// removes the partial file, and returns [`Error::Cancelled`](crate::Error::Cancelled).
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// Consulted by the loop before each chunk is written.
    pub(crate) fn poll(&self) -> Flow {
        if self.is_cancelled() {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// Throttle hook, awaited after each chunk is written. In M0 this returns
    /// immediately; M4's rate limiter will sleep here to enforce a byte/sec
    /// cap. It lives in the loop now so adding throttling is a local change.
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
    }

    #[test]
    fn cancel_is_observed_through_clones() {
        let a = Control::new();
        let b = a.clone();
        a.cancel();
        assert!(b.is_cancelled());
        assert_eq!(b.poll(), Flow::Cancel);
    }
}
