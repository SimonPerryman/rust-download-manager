//! Checking a finished file against a checksum, off the async runtime.

use std::sync::atomic::Ordering;

use super::files::finished_file;
use super::{Inner, Manager};
use crate::engine::lock::lock;
use crate::engine::store::ChecksumState;
use crate::engine::{panic_message, Checksum, Error};

impl Manager {
    /// Hash a completed download's file against `checksum`, off the async
    /// runtime: the snapshot's `checksum_state` goes `pending`, then `verified`
    /// or `mismatch`. Errors unless `id` is completed and its file is on disk.
    pub fn verify_checksum(&self, id: u64, checksum: Checksum) -> crate::engine::Result<()> {
        let st = &mut *lock(&self.inner.state);
        let e = st
            .entries
            .get_mut(&id)
            .ok_or_else(|| Error::InvalidRequest(format!("no download with id {id}")))?;
        let path = finished_file(&e.record)?;
        e.record.checksum = Some(checksum.to_string());
        e.record.checksum_state = ChecksumState::Pending;
        e.verify_gen += 1;
        let generation = e.verify_gen;
        self.inner.persist(st);
        self.inner.publish(st);

        let inner = self.inner.clone();
        self.inner.handle.spawn_blocking(move || {
            // A panic must still end the check, or it stays pending for good.
            let digest = std::panic::catch_unwind(|| checksum.digest_file(&path));
            let matched = digest
                .unwrap_or_else(|panic| Err(std::io::Error::other(panic_message(&*panic))))
                .map(|hex| hex == checksum.hex);
            inner.finish_verify(id, generation, matched);
        });
        Ok(())
    }
}

impl Inner {
    /// A [`Manager::verify_checksum`] hash has ended: `Ok(matched)`, or an I/O
    /// error if the file became unreadable mid-hash.
    fn finish_verify(&self, id: u64, generation: u64, result: std::io::Result<bool>) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let st = &mut *lock(&self.state);
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        if e.verify_gen != generation {
            return;
        }
        e.record.checksum_state = match result {
            Ok(true) => ChecksumState::Verified,
            Ok(false) => ChecksumState::Mismatch,
            Err(err) => {
                tracing::warn!(id, error = %err, "checksum verification could not read the file");
                ChecksumState::None
            }
        };
        self.persist(st);
        self.publish(st);
    }
}
