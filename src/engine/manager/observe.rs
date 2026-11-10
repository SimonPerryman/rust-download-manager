//! What a frontend sees: snapshots, coalesced progress, and each download's log.

use std::sync::atomic::Ordering;
use std::time::Instant;

use tokio::sync::watch;
use tracing::Subscriber;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use super::{DownloadSnapshot, Entry, Inner, Manager, ManagerState, PUBLISH_INTERVAL};
use crate::engine::download_log::{capture_filter, LogLayer, LogLine};
use crate::engine::lock::lock;
use crate::engine::Progress;

impl Inner {
    pub(super) fn snapshots(&self, st: &ManagerState) -> Vec<DownloadSnapshot> {
        st.order
            .iter()
            .filter_map(|id| st.entries.get(id))
            .map(Entry::snapshot)
            .collect()
    }

    /// Publish the current snapshot immediately (used on lifecycle changes).
    pub(super) fn publish(&self, st: &ManagerState) {
        let _ = self.tx.send(self.snapshots(st));
        *lock(&self.last_publish) = Instant::now();
    }

    /// Per-chunk progress callback: update counters cheaply, publish at most
    /// once per [`PUBLISH_INTERVAL`].
    pub(super) fn on_progress(&self, id: u64, p: Progress) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let st = &mut *lock(&self.state);
        if let Some(e) = st.entries.get_mut(&id) {
            // The engine threw the partial away and started over (e.g. the
            // server ignored the resume range), so the clock starts over too.
            if p.downloaded < e.record.downloaded {
                e.record.active_ms = Some(0);
                e.started = Some(Instant::now());
                e.peak_lanes = 0;
            }
            e.record.downloaded = p.downloaded;
            if p.total.is_some() {
                e.record.total = p.total;
            }
            e.live.bytes_per_sec = p.bytes_per_sec;
            e.live.eta_secs = p.eta.map(|d| d.as_secs());
            e.live.connections = p.connections;
            e.segments = p.segments;
            e.lanes = p.lanes;
            e.peak_lanes = e.peak_lanes.max(p.connections);
        } else {
            return;
        }
        let now = Instant::now();
        let mut last = lock(&self.last_publish);
        if now.duration_since(*last) >= PUBLISH_INTERVAL {
            *last = now;
            let _ = self.tx.send(self.snapshots(st));
        }
    }
}

impl Manager {
    /// A tracing layer that keeps the last [`LOG_LINES`](crate::engine::LOG_LINES)
    /// INFO-and-up lines of each download, plus rdm's own recent DEBUG detail,
    /// for [`download_log`](Self::download_log); a frontend adds it to its
    /// subscriber. It has its own level filter, so it works whatever
    /// `RUST_LOG` says.
    pub fn log_layer<S>(&self) -> impl Layer<S> + Send + Sync + 'static
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        LogLayer {
            logs: self.inner.logs.clone(),
        }
        .with_filter(capture_filter())
    }

    /// Download `id`'s recent log lines, oldest first: empty for an unknown id
    /// or one that hasn't run since the manager loaded (lines aren't saved).
    pub fn download_log(&self, id: u64) -> Vec<LogLine> {
        self.inner.logs.lines(id)
    }

    /// A one-shot snapshot of every download, in display order.
    pub fn snapshot(&self) -> Vec<DownloadSnapshot> {
        let st = lock(&self.inner.state);
        self.inner.snapshots(&st)
    }

    /// Subscribe to live snapshots. The receiver always holds the latest value
    /// (intermediate updates are coalesced), so a slow consumer never falls
    /// behind — it just sees the most recent state.
    pub fn subscribe(&self) -> watch::Receiver<Vec<DownloadSnapshot>> {
        self.inner.tx.subscribe()
    }
}
