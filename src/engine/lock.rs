//! Poison-tolerant locking. A panic while a lock is held (e.g. inside a
//! download task, which the Manager contains) must not turn every later
//! `lock()` into a panic that takes the whole frontend down with it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Set once the first poisoned lock has been reported, so a lock taken on
/// every chunk can't flood the log.
static WARNED: AtomicBool = AtomicBool::new(false);

/// Lock `mutex`, recovering it if an earlier panic poisoned it: the data is
/// still used (every critical section here leaves it consistent between
/// statements), and the poison is cleared so the warning is logged only once.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            // Release before warning: the per-download log layer takes its
            // own lock (which may be this one) to record the event.
            drop(poisoned.into_inner());
            mutex.clear_poison();
            if !WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!("Recovered a lock poisoned by an earlier panic; carrying on");
            }
            mutex.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poisoned_lock_is_recovered_with_its_data() {
        let mutex = std::sync::Arc::new(Mutex::new(41));
        let held = mutex.clone();
        let _ = std::thread::spawn(move || {
            let mut guard = lock(&held);
            *guard += 1;
            panic!("poison it");
        })
        .join();
        assert!(mutex.is_poisoned());

        assert_eq!(*lock(&mutex), 42);
        assert!(!mutex.is_poisoned(), "recovery clears the poison");
        *lock(&mutex) += 1;
        assert_eq!(*lock(&mutex), 43);
    }
}
