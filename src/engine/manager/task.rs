//! Running one download on its own task: the engine request, the target
//! claim, and recording how it ended.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;
use tokio::task::JoinHandle;
use tracing::Instrument;

use super::files::{owned_elsewhere, unique_target};
use super::{now_secs, Inner, Live};
use crate::engine::download_log::TASK_SPAN;
use crate::engine::lock::lock;
use crate::engine::store::{ChecksumState, DownloadRecord, Settings, Status};
use crate::engine::tally::Tally;
use crate::engine::{download, panic_message, Checksum, Control, DownloadRequest, Error};

impl Inner {
    /// Engine claim hook for a provisional target: dedup the derived `proposed`
    /// path, adopt it as the record's target, and hand it back to the engine.
    /// Called from the download task (never with the state lock held).
    fn claim_target(&self, id: u64, proposed: &Path, control: &Control) -> PathBuf {
        let st = &mut *lock(&self.state);
        if let Some(e) = st.entries.get(&id) {
            if !e.record.target_provisional {
                return e.record.target.clone();
            }
        }
        let target = unique_target(st, proposed.to_path_buf());
        let closed = self.closed.load(Ordering::SeqCst);
        match st.entries.get_mut(&id) {
            Some(e) if !closed => {
                e.record.target = target.clone();
                e.record.target_provisional = false;
                self.persist(st);
                self.publish(st);
            }
            // Removed (or manager dropped) before the first response: steer the
            // engine to a free path and cancel, so it never touches another
            // download's `.part`.
            _ => control.cancel(),
        }
        target
    }

    /// A download task has ended; record its terminal status and persist.
    fn finish(self: &Arc<Self>, id: u64, result: crate::engine::Result<crate::engine::Summary>) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let st = &mut *lock(&self.state);
        let shared = owned_elsewhere(st, id);
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        e.task = None;
        e.live = Live::default();
        e.record.active_ms = e.active_ms();
        e.started = None;
        let stop_keeping = std::mem::take(&mut e.stop_keeping);
        match result {
            Ok(summary) => {
                e.record.status = Status::Completed;
                e.record.downloaded = summary.bytes;
                e.record.total = e.record.total.or(Some(summary.bytes));
                e.record.final_path = Some(summary.path);
                e.record.error = None;
                e.record.finished_at = now_secs();
                e.record.checksum_state = if e.record.checksum.is_some() {
                    ChecksumState::Verified
                } else {
                    ChecksumState::None
                };
            }
            // A delete-stop landed after the task had already seen the pause.
            Err(Error::Paused) if e.control.is_cancelled() => {
                e.discard_partial(shared);
                e.record.status = Status::Cancelled;
                e.record.finished_at = now_secs();
            }
            Err(Error::Paused) if stop_keeping => e.cancel_keeping(),
            Err(Error::Paused) => {
                e.record.status = Status::Paused;
            }
            // The engine already removed `.part` + sidecar; this is a no-op
            // sweep that also zeroes the progress they stood for.
            Err(Error::Cancelled) => {
                e.discard_partial(shared);
                e.record.status = Status::Cancelled;
                e.record.finished_at = now_secs();
            }
            Err(other) => {
                if let Error::Internal(message) = &other {
                    tracing::error!("Internal error (a bug in rdm, please report it): {message}");
                }
                // Known-bad bytes: a retry must re-download, not re-finalize them.
                if matches!(other, Error::ChecksumMismatch(_)) {
                    e.record.checksum_state = ChecksumState::Mismatch;
                    e.discard_partial(shared);
                    if !shared {
                        tracing::info!("Discarded the downloaded file");
                    }
                }
                e.record.status = Status::Failed;
                e.record.error = Some(crate::engine::error_detail(&other));
                e.record.finished_at = now_secs();
            }
        }
        self.schedule(st);
        self.persist(st);
        self.publish(st);
    }
}

/// Spawn the engine `download` for one record on its own task, reporting
/// progress and completion back through `inner`.
/// Runs under the state lock, so it must never take it.
pub(super) fn spawn_task(
    inner: Arc<Inner>,
    record: &DownloadRecord,
    control: Control,
    settings: &Settings,
    tally: Arc<Tally>,
) -> JoinHandle<()> {
    let id = record.id;
    inner.logs.open(id);
    // Error level, like the engine's own span, so the tags survive any filter
    // that still shows errors. A provisional name is only a guess until claimed.
    let span = tracing::error_span!(TASK_SPAN, id, file = tracing::field::Empty);
    if !record.target_provisional {
        span.record("file", tracing::field::display(file_name(&record.target)));
    }
    let active_before = Duration::from_millis(record.active_ms.unwrap_or(0));
    let req =
        request_for(&inner, record, &control, settings, &span).with_history(tally, active_before);
    let handle = inner.handle.clone();
    handle.spawn(
        async move {
            let progress = inner.clone();
            let run = download(&req, &control, move |p| progress.on_progress(id, p));
            // A panic anywhere in the download (segment workers included: they
            // are polled on this task) fails just this download, so its slot
            // frees and the rest of the app carries on.
            let result = AssertUnwindSafe(run)
                .catch_unwind()
                .await
                .unwrap_or_else(|panic| Err(Error::Internal(panic_message(&*panic))));
            inner.finish(id, result);
        }
        .instrument(span),
    )
}

/// `path`'s file name, for the task span's `file` field.
fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Build the engine request for `record`. A provisional target is left for
/// the engine to derive from the response and claimed back via `inner`; a
/// claimed one is passed explicitly so the engine resumes its `.part`.
fn request_for(
    inner: &Arc<Inner>,
    record: &DownloadRecord,
    control: &Control,
    settings: &Settings,
    span: &tracing::Span,
) -> DownloadRequest {
    let mut req = if record.target_provisional {
        let dir = record
            .target
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        let (inner, id, control) = (inner.clone(), record.id, control.clone());
        let span = span.clone();
        DownloadRequest::new(&record.url, dir).with_claim_target(move |proposed| {
            let target = inner.claim_target(id, proposed, &control);
            span.record("file", tracing::field::display(file_name(&target)));
            tracing::debug!("Saving as {}", target.display());
            target
        })
    } else {
        DownloadRequest::to_path(&record.url, &record.target)
    };
    if let Some(cs) = record.checksum.as_deref().and_then(|s| s.parse().ok()) {
        let cs: Checksum = cs;
        req = req.with_checksum(cs);
    }
    req.with_proxy(settings.proxy.clone())
        .with_connections(settings.max_connections)
}
