//! Commands on one download: add, pause, resume, stop (or cancel) and remove; and
//! the shutdown that stops them all.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::files::{discard_files, owned_elsewhere, owns_partial, unique_target};
use super::{now_secs, AddOptions, Entry, Manager};
use crate::engine::store::{ChecksumState, DownloadRecord, Status};
use crate::engine::{filename, tags, Checksum, RateLimiter};

impl Manager {
    /// Add a new download, starting it now if a slot is free or queueing it
    /// otherwise. `dir` is the destination folder; `out`, when given, is an
    /// explicit full target path that overrides the derived name. Returns the
    /// new download's id.
    ///
    /// Without `out`, the target shows a URL-derived placeholder until the first
    /// response, when the server's `Content-Disposition` (or URL) name is
    /// claimed, deduplicated against other downloads and files on disk.
    pub fn add(
        &self,
        url: impl Into<String>,
        dir: impl Into<PathBuf>,
        out: Option<PathBuf>,
        checksum: Option<Checksum>,
    ) -> u64 {
        let options = AddOptions {
            out,
            checksum,
            ..AddOptions::default()
        };
        self.insert(url.into(), dir.into(), options)
    }

    /// [`add`](Self::add) with every option, including a per-download speed
    /// limit and tags. Returns the new download's id, or
    /// [`Error::InvalidRequest`](crate::engine::Error::InvalidRequest) for a tag the [`tags`] rules reject.
    pub fn add_with(
        &self,
        url: impl Into<String>,
        dir: impl Into<PathBuf>,
        mut options: AddOptions,
    ) -> crate::engine::Result<u64> {
        options.tags = tags::merged(&[], &tags::normalize(&options.tags)?)?;
        Ok(self.insert(url.into(), dir.into(), options))
    }

    /// Add a download whose options are already valid.
    fn insert(&self, url: String, dir: PathBuf, options: AddOptions) -> u64 {
        let st = &mut *self.inner.state.lock().unwrap();

        let id = st.next_id;
        st.next_id += 1;

        let (target, target_provisional) = match options.out {
            Some(out) => (unique_target(st, out), false),
            None => {
                let name = filename::resolve(None, &url, None);
                (filename::join_in(&dir, &name), true)
            }
        };
        let limiter = Arc::new(RateLimiter::new(options.limit));
        let record = DownloadRecord {
            id,
            url,
            target,
            target_provisional,
            final_path: None,
            checksum: options.checksum.map(|c| c.to_string()),
            status: Status::Queued,
            downloaded: 0,
            total: None,
            error: None,
            added_at: now_secs(),
            finished_at: None,
            limit: limiter.rate(),
            partial_kept: false,
            checksum_state: ChecksumState::None,
            active_ms: Some(0),
            archived: false,
            tags: options.tags,
        };
        let entry = Entry::new(record, limiter);

        st.order.push(id);
        st.entries.insert(id, entry);
        self.inner.schedule(st);
        self.inner.persist(st);
        self.inner.publish(st);
        id
    }

    /// Request a pause. A running task stops at the next chunk boundary,
    /// keeping the `.part` + sidecar; the status flips to [`Status::Paused`]
    /// once the task observes it. A queued download leaves the queue at once.
    /// No-op otherwise.
    pub fn pause(&self, id: u64) {
        let st = &mut *self.inner.state.lock().unwrap();
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        if e.task.is_some() {
            e.control.pause();
        } else if e.record.status == Status::Queued {
            e.record.status = Status::Paused;
            self.inner.schedule(st);
            self.inner.persist(st);
            self.inner.publish(st);
        }
    }

    /// Start, resume, or retry a download: resumes a paused one (or one
    /// stopped with its partial kept) from its `.part`, retries a failed one,
    /// or restarts a cancelled one from scratch. A restart whose partial was
    /// deleted moves to a free name (e.g. `name (1).ext`) if another download
    /// has since claimed its target. It rejoins the queue, starting now if a
    /// slot is free, and leaves the archive if it was archived (only finished
    /// downloads are). No-op if it's already running, queued or completed.
    pub fn resume(&self, id: u64) {
        let st = &mut *self.inner.state.lock().unwrap();
        // A row that owns nothing on disk may have had its target claimed by a
        // newer download since; restarting onto it would share one `.part`.
        let retarget = st
            .entries
            .get(&id)
            .filter(|e| !e.record.target_provisional && !owns_partial(&e.record))
            .map(|e| unique_target(st, e.record.target.clone()));
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        if e.task.is_some() || !e.record.status.is_resumable() {
            return;
        }
        if let Some(target) = retarget {
            e.record.target = target;
        }
        e.record.status = Status::Queued;
        e.record.archived = false;
        e.record.error = None;
        e.record.finished_at = None;
        e.record.partial_kept = false;
        e.record.checksum_state = ChecksumState::None;
        self.inner.schedule(st);
        self.inner.persist(st);
        self.inner.publish(st);
    }

    /// Cancel a running, queued or paused download and discard its partial
    /// data; the same as [`stop`](Self::stop) with `keep_partial = false`.
    pub fn cancel(&self, id: u64) {
        self.stop(id, false);
    }

    /// Stop a running, queued or paused download as [`Status::Cancelled`].
    /// With `keep_partial` its `.part` + sidecar stay on disk and
    /// [`resume`](Self::resume) continues from them; without, they're deleted
    /// (also for a download stopped earlier with its partial kept). No-op once
    /// completed or failed.
    pub fn stop(&self, id: u64, keep_partial: bool) {
        let st = &mut *self.inner.state.lock().unwrap();
        let shared = owned_elsewhere(st, id);
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        if e.task.is_some() {
            // A pause makes the live task flush and checkpoint (`finish` turns
            // it into a kept cancel); a cancel makes it remove `.part` + sidecar
            // itself. Either way it reports back through `finish`.
            e.stop_keeping = keep_partial;
            if keep_partial {
                e.control.pause();
            } else {
                e.control.cancel();
            }
            return;
        }
        match e.record.status {
            Status::Queued | Status::Paused if keep_partial => e.cancel_keeping(),
            // No task to observe it: discard the partial here and now (a
            // resumed-then-queued download may own a `.part`).
            Status::Queued | Status::Paused => {
                e.discard_partial(shared);
                e.record.status = Status::Cancelled;
                e.record.finished_at = now_secs();
            }
            Status::Cancelled if !keep_partial && e.record.partial_kept => {
                e.discard_partial(shared);
            }
            Status::Downloading | Status::Completed | Status::Failed | Status::Cancelled => return,
        }
        self.inner.schedule(st);
        self.inner.persist(st);
        self.inner.publish(st);
    }

    /// Remove a download from the list entirely, archived or not. The
    /// downloaded file is never deleted, nor is a partial that
    /// [`stop`](Self::stop) kept; any other partial (a failed, paused or
    /// running download's `.part` + sidecar) is, and a running task is stopped.
    pub fn remove(&self, id: u64) {
        let st = &mut *self.inner.state.lock().unwrap();
        let Some(e) = st.entries.remove(&id) else {
            return;
        };
        st.order.retain(|&x| x != id);
        if let Some(task) = e.task {
            task.abort();
        }
        self.inner.logs.forget(id);
        discard_files(st, &e.record);
        self.inner.schedule(st);
        self.inner.persist(st);
        self.inner.publish(st);
    }

    /// Stop every running download ahead of the process exiting, recording
    /// each one's active time and saving it as paused, so a quit doesn't
    /// lose the time the way a crash does. Call it last: no download starts or
    /// reports progress afterwards.
    pub fn shutdown(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        let st = &mut *self.inner.state.lock().unwrap();
        for e in st.entries.values_mut() {
            if let Some(task) = e.task.take() {
                task.abort();
            }
            e.record.active_ms = e.active_ms();
            e.started = None;
            if e.record.status == Status::Downloading {
                e.record.status = Status::Paused;
            }
        }
        self.inner.persist(st);
    }
}
