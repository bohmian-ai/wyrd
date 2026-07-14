//! Current observer resolution.

use std::sync::{Arc, OnceLock};

use crate::{NoopObserver, Observer};

use crate::scoped::SCOPED_OBSERVER;

static GLOBAL_OBSERVER: OnceLock<Arc<dyn Observer>> = OnceLock::new();

/// Register a process-wide default observer.
///
/// The first call wins. Later calls are silent no-ops, which keeps host
/// initialization idempotent while still allowing tests and advanced users to
/// install a default before or after the Skald bridge is initialized.
pub fn set_global(observer: Arc<dyn Observer>) {
    let _ = GLOBAL_OBSERVER.set(observer);
}

/// Resolve the observer active for the current task.
///
/// Precedence is scoped task-local override, process-wide global observer, then
/// [`NoopObserver`].
#[must_use]
pub fn current() -> Arc<dyn Observer> {
    SCOPED_OBSERVER
        .try_with(Arc::clone)
        .ok()
        .or_else(|| GLOBAL_OBSERVER.get().cloned())
        .unwrap_or_else(|| Arc::new(NoopObserver))
}
