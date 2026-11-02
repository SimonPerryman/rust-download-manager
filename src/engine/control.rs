//! Control hooks the chunk loop consults each chunk: cancel and pause before
//! each write, and a rate-limit throttle after it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use crate::engine::RateLimiter;

/// Longest a throttle sleeps before re-checking pause/cancel and the live rate.
const THROTTLE_SLICE: Duration = Duration::from_millis(50);

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
    limiters: Vec<Arc<RateLimiter>>,
}

impl Control {
    /// A fresh control handle that permits the download to run to completion,
    /// unthrottled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Throttle the download by every given limiter at once (it runs at the
    /// slowest one's pace). Limiters may be shared with other downloads.
    ///
    /// # Panics
    /// A builder step: panics if this handle has already been cloned.
    pub fn with_limiters(mut self, limiters: impl IntoIterator<Item = Arc<RateLimiter>>) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("Control::with_limiters must be called before the handle is cloned")
            .limiters
            .extend(limiters);
        self
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

    /// Awaited after each chunk is written: charge it to every limiter, then
    /// wait until the most indebted one is repaid. Returns early on
    /// pause/cancel (the debt stays owed).
    pub(crate) async fn throttle(&self, chunk_len: usize) {
        let limiters = &self.inner.limiters;
        let n = chunk_len as u64;
        // Waiting on the latest deadline (not each in turn) keeps the pace at
        // the slowest limiter rather than the sum of their waits.
        let mut deadline = limiters.iter().filter_map(|l| l.charge(n)).max();
        while let Some(until) = deadline {
            if self.poll() != Flow::Continue {
                return;
            }
            tokio::time::sleep_until(until.min(Instant::now() + THROTTLE_SLICE)).await;
            // Re-peek rather than trust `until`: a live rate change may have
            // moved it.
            deadline = limiters.iter().filter_map(|l| l.charge(0)).max();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;

    fn limited(rates: &[u64]) -> (Control, Vec<Arc<RateLimiter>>) {
        let limiters: Vec<_> = rates
            .iter()
            .map(|&r| Arc::new(RateLimiter::new(Some(r))))
            .collect();
        (Control::new().with_limiters(limiters.clone()), limiters)
    }

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

    #[test]
    #[should_panic(expected = "before the handle is cloned")]
    fn with_limiters_rejects_a_cloned_handle() {
        let c = Control::new();
        let _clone = c.clone();
        let _ = c.with_limiters([Arc::new(RateLimiter::new(Some(MIB)))]);
    }

    #[tokio::test(start_paused = true)]
    async fn unlimited_throttle_never_sleeps() {
        let start = Instant::now();
        Control::new().throttle(usize::MAX).await;
        let unlimited = Control::new().with_limiters([Arc::new(RateLimiter::new(None))]);
        unlimited.throttle(100 * MIB as usize).await;
        assert_eq!(Instant::now(), start);
    }

    #[tokio::test(start_paused = true)]
    async fn throttle_debits_each_limiter_exactly_once() {
        let (control, limiters) = limited(&[MIB, 2 * MIB]);
        control.throttle(10 * KIB as usize).await;
        for l in &limiters {
            assert_eq!(l.balance(), (l.burst() - 10 * KIB) as f64);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn throttle_waits_for_the_slowest_limiter_not_the_sum() {
        // 768 KiB: 0.5 s of debt at 1 MiB/s (burst 256 KiB), 1.25 s at
        // 512 KiB/s (burst 128 KiB). Paying in turn would take 1.75 s.
        let (control, _limiters) = limited(&[MIB, 512 * KIB]);
        let start = Instant::now();
        control.throttle(768 * KIB as usize).await;
        let waited = (Instant::now() - start).as_secs_f64();
        assert!((1.25..1.26).contains(&waited), "waited {waited}s");
    }

    #[tokio::test(start_paused = true)]
    async fn pause_interrupts_a_throttle_within_one_slice() {
        let (control, _limiters) = limited(&[MIB]);
        let throttled = control.clone();
        let task = tokio::spawn(async move {
            throttled.throttle(10 * MIB as usize).await; // ~10 s of debt
            Instant::now()
        });
        tokio::time::sleep(Duration::from_millis(120)).await;
        let paused_at = Instant::now();
        control.pause();
        let returned_at = task.await.unwrap();
        assert!(returned_at - paused_at <= THROTTLE_SLICE);
    }

    #[tokio::test(start_paused = true)]
    async fn live_rate_change_reshapes_a_throttle_sleep() {
        let (control, limiters) = limited(&[MIB]);
        let start = Instant::now();
        let raise = async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            limiters[0].set_rate(Some(100 * MIB));
        };
        tokio::join!(control.throttle(10 * MIB as usize), raise);
        let waited = Instant::now() - start;
        assert!(waited < Duration::from_millis(400), "waited {waited:?}");

        // Going unlimited ends a sleep within one slice.
        limiters[0].set_rate(Some(MIB));
        let start = Instant::now();
        let lift = async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            limiters[0].set_rate(None);
        };
        tokio::join!(control.throttle(10 * MIB as usize), lift);
        assert!(Instant::now() - start <= Duration::from_millis(200) + THROTTLE_SLICE);
    }
}
