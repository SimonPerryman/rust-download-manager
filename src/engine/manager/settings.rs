//! Manager-wide settings, and each download's own speed limit.

use super::Manager;
use crate::engine::lock::lock;
use crate::engine::store::Settings;
use crate::engine::ProxyMode;

impl Manager {
    /// The current manager-wide settings.
    pub fn settings(&self) -> Settings {
        lock(&self.inner.state).settings.clone()
    }

    /// Set how many downloads may stream at once (at least 1). Raising it
    /// starts queued downloads; lowering it never interrupts running ones.
    pub fn set_max_concurrent(&self, n: usize) {
        self.update_settings(|s| s.max_concurrent = n);
    }

    /// Cap the aggregate speed of all downloads in bytes/s (`None` or
    /// `Some(0)` is unlimited). Applies live, including to running downloads.
    pub fn set_global_limit(&self, limit: Option<u64>) {
        self.update_settings(|s| s.global_limit = limit);
    }

    /// Set the max parallel connections per download (clamped to 1..=16). Applies
    /// from each download's next start, not to ones already running.
    pub fn set_max_connections(&self, n: u8) {
        self.update_settings(|s| s.max_connections = n);
    }

    /// Choose the proxy downloads use. Applies from each download's next
    /// start or resume, not to ones already running.
    pub fn set_proxy(&self, proxy: ProxyMode) {
        self.update_settings(|s| s.proxy = proxy);
    }

    /// Cap one download's speed in bytes/s (`None` or `Some(0)` is
    /// unlimited). Applies live if it's running, and persists across restarts.
    pub fn set_limit(&self, id: u64, limit: Option<u64>) {
        let st = &mut *lock(&self.inner.state);
        let Some(e) = st.entries.get_mut(&id) else {
            return;
        };
        e.limiter.set_rate(limit);
        e.record.limit = e.limiter.rate();
        self.inner.persist(st);
        self.inner.publish(st);
    }

    fn update_settings(&self, change: impl FnOnce(&mut Settings)) {
        let st = &mut *lock(&self.inner.state);
        change(&mut st.settings);
        st.settings = std::mem::take(&mut st.settings).clamped();
        self.inner.global.set_rate(st.settings.global_limit);
        self.inner.schedule(st);
        self.inner.persist(st);
        self.inner.publish(st);
    }
}
