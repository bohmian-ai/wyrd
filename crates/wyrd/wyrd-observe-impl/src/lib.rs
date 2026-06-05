//! Bridge from Wyrd observer state into Skald agent runs.

#![deny(missing_docs)]

use std::sync::{Arc, OnceLock};

use skald_agent::{Observer, ObserverProvider};

#[cfg(feature = "python")]
/// Python initialization hook.
pub mod python;
#[cfg(feature = "python")]
pub use python::{PythonObserver, python_register};

/// Observer provider installed into `skald-agent`.
pub struct WyrdObserverProvider;

impl ObserverProvider for WyrdObserverProvider {
    fn current(&self) -> Arc<dyn Observer> {
        wyrd_observe::current()
    }
}

static INIT: OnceLock<()> = OnceLock::new();

/// Install the Wyrd observer provider into `skald-agent`.
///
/// Initialization is idempotent. The first call installs the provider and later
/// calls are silent no-ops.
pub fn init() {
    if INIT.set(()).is_err() {
        return;
    }
    skald_agent::set_observer_provider(Box::new(WyrdObserverProvider));
}
