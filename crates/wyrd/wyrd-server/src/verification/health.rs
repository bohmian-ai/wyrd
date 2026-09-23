//! Liveness of the verification runtime's capability tasks.
//!
//! [`VerificationHealth`] is shared between the runtime, which marks each
//! capability required when it is composed and up or down as its task starts
//! and exits, and the readiness loop, which reports the server not ready while
//! any required capability is absent.

use std::sync::atomic::{AtomicBool, Ordering};

/// One capability slot of the verification runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCapability {
    /// Turns due binding schedules into runs.
    Scheduler,
    /// Claims, executes, publishes, and settles Verifier runs.
    Runner,
    /// Delivers Operator dispatches; composed by a later change.
    OperatorWorker,
}

impl RuntimeCapability {
    /// Every capability, in slot order.
    pub const ALL: [Self; 3] = [Self::Scheduler, Self::Runner, Self::OperatorWorker];

    /// Stable telemetry label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scheduler => "scheduler",
            Self::Runner => "runner",
            Self::OperatorWorker => "operator_worker",
        }
    }

    /// Slot index into the health arrays.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Scheduler => 0,
            Self::Runner => 1,
            Self::OperatorWorker => 2,
        }
    }
}

/// Shared required/up state of every capability slot.
///
/// A capability that this process never composes is not required and never
/// degrades health; a composed capability is degraded from the moment its task
/// exits until its restart is running again.
#[derive(Debug, Default)]
pub struct VerificationHealth {
    /// Whether this process composed each capability.
    required: [AtomicBool; 3],
    /// Whether each capability's task is currently running.
    up: [AtomicBool; 3],
}

impl VerificationHealth {
    /// Record that this process runs `capability`.
    pub fn require(&self, capability: RuntimeCapability) {
        self.required[capability.index()].store(true, Ordering::SeqCst);
    }

    /// Record whether `capability`'s task is running.
    pub fn set_up(&self, capability: RuntimeCapability, up: bool) {
        self.up[capability.index()].store(up, Ordering::SeqCst);
        metrics::gauge!(
            crate::app::metrics::VERIFICATION_CAPABILITY_UP,
            "capability" => capability.as_str()
        )
        .set(if up { 1.0 } else { 0.0 });
    }

    /// Whether `capability`'s task is running.
    #[must_use]
    pub fn is_up(&self, capability: RuntimeCapability) -> bool {
        self.up[capability.index()].load(Ordering::SeqCst)
    }

    /// Whether this process composed any capability at all.
    #[must_use]
    pub fn is_composed(&self) -> bool {
        self.required
            .iter()
            .any(|required| required.load(Ordering::SeqCst))
    }

    /// Whether any required capability is absent.
    #[must_use]
    pub fn is_degraded(&self) -> bool {
        RuntimeCapability::ALL.iter().any(|capability| {
            self.required[capability.index()].load(Ordering::SeqCst) && !self.is_up(*capability)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An uncomposed runtime is never degraded; a composed capability is
    /// degraded exactly while its task is down.
    ///
    /// # Panics
    /// Panics when degradation does not track the required capability.
    #[test]
    fn degradation_tracks_required_capabilities() {
        let health = VerificationHealth::default();
        assert!(!health.is_composed());
        assert!(!health.is_degraded());
        health.require(RuntimeCapability::Scheduler);
        assert!(health.is_degraded(), "a required capability starts absent");
        health.set_up(RuntimeCapability::Scheduler, true);
        assert!(!health.is_degraded());
        assert!(
            !health.is_up(RuntimeCapability::OperatorWorker),
            "an uncomposed slot is never required"
        );
        health.set_up(RuntimeCapability::Scheduler, false);
        assert!(health.is_degraded());
    }
}
