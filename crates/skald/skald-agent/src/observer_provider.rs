//! Process-global observer provider indirection.

use std::sync::{Arc, OnceLock};

use crate::observer::{NoopObserver, Observer};

/// Provider for the observer active at agent run start.
pub trait ObserverProvider: Send + Sync + 'static {
    /// Returns the observer to capture for one agent run.
    fn current(&self) -> Arc<dyn Observer>;
}

static OBSERVER_PROVIDER: OnceLock<Box<dyn ObserverProvider>> = OnceLock::new();

/// Installs the process-wide observer provider.
///
/// The first call wins. Later calls are silent no-ops so higher-level
/// initialization can be idempotent.
pub fn set_observer_provider(provider: Box<dyn ObserverProvider>) {
    let _ = OBSERVER_PROVIDER.set(provider);
}

/// Returns the current observer, falling back to [`NoopObserver`].
#[must_use]
pub fn current_observer() -> Arc<dyn Observer> {
    match OBSERVER_PROVIDER.get() {
        Some(provider) => provider.current(),
        None => Arc::new(NoopObserver),
    }
}
