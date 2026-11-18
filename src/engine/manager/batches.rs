//! Batches: adding several links as one named group, and retrying a group's
//! failures. A batch is only an identity on its downloads' records (their
//! shared [`DownloadRecord::batch`] id and label); each item is an ordinary
//! download that queues, throttles and resumes like any other.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{AddOptions, Manager, ManagerState};
use crate::engine::lock::lock;
use crate::engine::store::Status;
use crate::engine::{filename, tags, Error, RateLimiter, Result};

/// Longest batch name, in characters (after trimming).
pub const MAX_BATCH_LABEL_LEN: usize = 64;

/// What to do with a batch: its name, and whether its items go in a folder of
/// that name inside the destination (else straight into the destination).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchSpec {
    /// The batch's name; trimmed, 1 to [`MAX_BATCH_LABEL_LEN`] characters.
    pub label: String,
    /// Put the items in a folder named after the batch.
    pub folder: bool,
    /// Speed cap in bytes/s shared by all the batch's items combined
    /// (`None` or `Some(0)` is unlimited). It adds to each item's own limit
    /// and the global one; the slowest applies.
    pub limit: Option<u64>,
}

/// A batch as stamped on one new download: identity plus the limiter its
/// items share.
pub(super) struct BatchTag<'a> {
    pub id: u64,
    pub label: &'a str,
    pub limiter: Option<Arc<RateLimiter>>,
}

/// How [`Manager::add_batch`] went. A link that couldn't be added never stops
/// the rest.
#[derive(Debug, Default)]
pub struct BatchAdded {
    /// The batch's id; `None` when no batch was made (a single link, or no
    /// name), in which case the links are ordinary downloads.
    pub batch: Option<u64>,
    /// The folder the items went in, when the batch made one.
    pub folder: Option<PathBuf>,
    /// The new downloads' ids, in link order.
    pub added: Vec<u64>,
    /// Links that weren't added, with why.
    pub failed: Vec<(String, Error)>,
}

impl Manager {
    /// Add `urls` in order, as one batch when there are two or more and `spec`
    /// names it, else as ordinary downloads in `dir`. `options` applies to each
    /// link (`out` is ignored: a batch names its own files).
    ///
    /// A batch gets an id (its first download's) and the name on every item. With
    /// `spec.folder` the items go in a folder of that name inside `dir`; one that
    /// already exists, on disk or as another download's folder, gets the usual
    /// `name (1)` suffix instead of being shared.
    pub fn add_batch(
        &self,
        urls: Vec<String>,
        dir: impl Into<PathBuf>,
        spec: Option<BatchSpec>,
        mut options: AddOptions,
    ) -> Result<BatchAdded> {
        options.tags = tags::merged(&[], &tags::normalize(&options.tags)?)?;
        options.out = None;
        let dir = dir.into();
        let spec = match spec {
            Some(spec) if urls.len() >= 2 => Some(checked(spec)?),
            _ => None,
        };
        let mut out = BatchAdded::default();
        if urls.is_empty() {
            return Ok(out);
        }

        let st = &mut *lock(&self.inner.state);
        let dir = match &spec {
            Some(s) if s.folder => {
                let folder = unique_folder(st, &dir, &s.label);
                out.folder = Some(folder.clone());
                folder
            }
            _ => dir,
        };
        let limiter = spec
            .as_ref()
            .and_then(|s| s.limit)
            .filter(|&l| l > 0)
            .map(|l| Arc::new(RateLimiter::new(Some(l))));
        out.batch = spec.as_ref().map(|_| st.next_id);
        let group = st.new_group();
        let mut probes = Vec::new();
        for url in urls {
            let tag = spec.as_ref().zip(out.batch).map(|(s, id)| BatchTag {
                id,
                label: &s.label,
                limiter: limiter.clone(),
            });
            let (id, probe_url) =
                self.push_entry(st, url, dir.clone(), options.clone(), tag, group);
            out.added.push(id);
            probes.push((id, probe_url));
        }
        self.inner.schedule(st);
        self.inner.persist(st);
        self.inner.publish(st);
        for (id, url) in probes {
            self.inner.spawn_probe(id, url, st.settings.proxy.clone());
        }
        Ok(out)
    }

    /// The folder batch `batch`'s files went into: the deepest folder holding
    /// all their targets (the batch folder, or the destination when none was
    /// made). `None` for an unknown batch. For a frontend's Open folder.
    pub fn batch_folder(&self, batch: u64) -> Option<PathBuf> {
        let st = lock(&self.inner.state);
        let mut dirs = st
            .entries
            .values()
            .filter(|e| e.record.batch == Some(batch))
            .filter_map(|e| e.record.target.parent().map(Path::to_path_buf));
        let first = dirs.next()?;
        dirs.try_fold(first, |common, dir| {
            let shared: PathBuf = common
                .components()
                .zip(dir.components())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect();
            Some(shared)
        })
    }

    /// Batch `batch`'s shared speed cap in bytes/s, or `None` when it has no
    /// cap (or is unknown).
    pub fn batch_limit(&self, batch: u64) -> Option<u64> {
        let st = lock(&self.inner.state);
        st.entries
            .values()
            .find(|e| e.record.batch == Some(batch))
            .and_then(|e| e.batch_limiter.as_ref()?.rate())
    }

    /// Retry the failed downloads of batch `batch` (those not archived), and
    /// no others. Returns how many were retried.
    pub fn retry_batch(&self, batch: u64) -> usize {
        let ids: Vec<u64> = {
            let st = lock(&self.inner.state);
            st.order
                .iter()
                .filter_map(|id| st.entries.get(id))
                .filter(|e| {
                    e.record.batch == Some(batch)
                        && e.record.status == Status::Failed
                        && !e.record.archived
                })
                .map(|e| e.record.id)
                .collect()
        };
        for &id in &ids {
            self.resume(id);
        }
        ids.len()
    }
}

fn checked(spec: BatchSpec) -> Result<BatchSpec> {
    let label = spec.label.trim().to_string();
    if label.is_empty() {
        return Err(Error::InvalidRequest("a batch name can't be empty".into()));
    }
    if label.chars().count() > MAX_BATCH_LABEL_LEN {
        return Err(Error::InvalidRequest(format!(
            "the batch name is longer than {MAX_BATCH_LABEL_LEN} characters"
        )));
    }
    Ok(BatchSpec { label, ..spec })
}

/// `dir/<label>` made safe as a folder name, or `dir/<label> (1)`, `(2)`, …
/// when that exists or already holds another download's file.
fn unique_folder(st: &ManagerState, dir: &Path, label: &str) -> PathBuf {
    let plain: String = label.replace(['/', '\\'], "_");
    let name = Some(filename::sanitize(&plain))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "batch".to_string());
    let used = |p: &Path| {
        p.exists()
            || st
                .entries
                .values()
                .any(|e| e.record.target.parent() == Some(p))
    };
    let first = dir.join(&name);
    if !used(&first) {
        return first;
    }
    (1u32..)
        .map(|n| dir.join(format!("{name} ({n})")))
        .find(|p| !used(p))
        .expect("the range is effectively unbounded")
}
