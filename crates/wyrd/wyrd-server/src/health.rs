//! Server readiness checking and liveness routes.

use serde::Serialize;

/// Snapshot of the server's readiness state, published by the background
/// `readiness_loop` task and consumed lock-free by `/readyz` handlers.
#[derive(Debug, Clone, Serialize)]
pub struct ReadinessSnapshot {
    /// Overall readiness status.
    pub ready: bool,
    /// Per-component status messages.
    pub components: Vec<ComponentStatus>,
}

/// Per-component readiness status.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentStatus {
    /// Component name.
    pub name: String,
    /// Whether this component is healthy.
    pub ok: bool,
    /// Reason code for unhealthy components.
    pub reason: Option<String>,
}

impl ReadinessSnapshot {
    /// Initial pre-boot snapshot: not ready (probes have not yet run).
    #[must_use]
    pub fn initial() -> Self {
        Self {
            ready: false,
            components: Vec::new(),
        }
    }

    /// Returns true when all components report healthy.
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.ready && self.components.iter().all(|c| c.ok)
    }
}
