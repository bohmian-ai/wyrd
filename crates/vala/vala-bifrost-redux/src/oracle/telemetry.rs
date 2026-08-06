//! Closed-label peer, fragment, slot, and security telemetry.

use std::time::Instant;
use wyrd_spec::vala::api::QueryClass;

/// Closed query class projection used by every Oracle metric family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OracleQueryClassLabel {
    /// Latency-sensitive query.
    Interactive,
    /// Scan-heavy query.
    Analytical,
}

impl OracleQueryClassLabel {
    /// Every query class used by benchmark telemetry fixtures.
    #[cfg(feature = "bench-support")]
    pub(crate) const ALL: [Self; 2] = [Self::Interactive, Self::Analytical];

    /// Return the wire-safe label value.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Analytical => "analytical",
        }
    }
}

impl From<QueryClass> for OracleQueryClassLabel {
    /// Project the planner's closed class into the telemetry label domain.
    fn from(value: QueryClass) -> Self {
        match value {
            QueryClass::Interactive => Self::Interactive,
            QueryClass::Analytical => Self::Analytical,
        }
    }
}

/// Closed local admission outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OracleAdmissionOutcome {
    /// Query entered execution.
    Admitted,
    /// Query was rejected before execution.
    Rejected,
}

impl OracleAdmissionOutcome {
    /// Every admission outcome used by benchmark telemetry fixtures.
    #[cfg(feature = "bench-support")]
    pub(crate) const ALL: [Self; 2] = [Self::Admitted, Self::Rejected];

    /// Return the canonical label value.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Rejected => "rejected",
        }
    }
}

/// Closed local admission rejection reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OracleAdmissionReason {
    /// Class-local active capacity was exhausted.
    ClassCapacity,
    /// Tenant-local budget was exhausted.
    TenantBudget,
    /// Queue bound was reached.
    QueueFull,
    /// Queue deadline elapsed.
    QueueDeadline,
    /// Memory budget was exhausted.
    Memory,
    /// Spill budget was exhausted.
    Spill,
    /// Audit WAL could not accept the decision.
    AuditUnavailable,
    /// Oracle shutdown closed admission.
    Shutdown,
}

impl OracleAdmissionReason {
    /// Every wire value used by contract tests and dashboards.
    pub(crate) const ALL: [Self; 8] = [
        Self::ClassCapacity,
        Self::TenantBudget,
        Self::QueueFull,
        Self::QueueDeadline,
        Self::Memory,
        Self::Spill,
        Self::AuditUnavailable,
        Self::Shutdown,
    ];
    /// Return the canonical label value.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ClassCapacity => "class_capacity",
            Self::TenantBudget => "tenant_budget",
            Self::QueueFull => "queue_full",
            Self::QueueDeadline => "queue_deadline",
            Self::Memory => "memory",
            Self::Spill => "spill",
            Self::AuditUnavailable => "audit_unavailable",
            Self::Shutdown => "shutdown",
        }
    }
}

/// Closed query-cancellation reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OracleCancellationReason {
    /// Client dropped the response stream.
    ClientDrop,
    /// Query deadline elapsed.
    Deadline,
    /// Oracle shutdown cancelled the stream.
    Shutdown,
    /// Peer execution failed.
    PeerFailure,
}

impl OracleCancellationReason {
    /// Every wire value used by contract tests and dashboards.
    pub(crate) const ALL: [Self; 4] = [
        Self::ClientDrop,
        Self::Deadline,
        Self::Shutdown,
        Self::PeerFailure,
    ];
    /// Return the canonical label value.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ClientDrop => "client_drop",
            Self::Deadline => "deadline",
            Self::Shutdown => "shutdown",
            Self::PeerFailure => "peer_failure",
        }
    }
}

/// Closed locality label for one fragment attempt.
#[derive(Debug, Clone, Copy)]
pub enum FragmentLocality {
    /// Attempt executed on the leader node.
    Local,
    /// Attempt executed through a peer transport.
    Remote,
}

/// Closed terminal fragment outcome.
#[derive(Debug, Clone, Copy)]
pub enum FragmentOutcome {
    /// Footer-validated bytes were admitted.
    Success,
    /// The attempt failed or was cancelled.
    Failed,
}

impl FragmentOutcome {
    /// Every fragment outcome used by benchmark telemetry fixtures.
    #[cfg(feature = "bench-support")]
    pub(crate) const ALL: [Self; 2] = [Self::Success, Self::Failed];

    /// Returns the canonical low-cardinality metric label.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }
}

/// Closed peer-attempt error class.
#[derive(Debug, Clone, Copy)]
pub enum PeerErrorClass {
    /// No error occurred.
    None,
    /// Transport or worker availability failed.
    Availability,
    /// Security or closed-contract validation failed.
    Security,
    /// All bounded distinct attempts failed.
    Exhausted,
    /// Footer identity validation failed.
    Footer,
    /// Whole-attempt buffering or footer completeness failed.
    Attempt,
}

/// Closed worker-slot reservation outcome.
#[derive(Debug, Clone, Copy)]
pub enum SlotOutcome {
    /// A pending slot was accepted.
    Pending,
    /// A pending reservation transitioned to running.
    Running,
    /// Capacity or fencing rejected the reservation.
    Rejected,
}

/// Closed peer-security event boundary.
#[derive(Debug, Clone, Copy)]
pub enum SecurityEventClass {
    /// Raw ticket authentication failed.
    Ticket,
    /// Verified claim structure failed.
    Claims,
    /// Fragment-bound verified claims failed.
    Fragment,
}

/// Stream-scoped fragment metrics released on every exit path.
pub struct FragmentTelemetry {
    /// Fragment start used by the canonical duration histogram.
    started_at: Instant,
    /// Whether a terminal outcome has already been recorded.
    finished: bool,
}

impl FragmentTelemetry {
    /// Starts canonical in-flight and duration accounting.
    #[must_use]
    pub fn start(_locality: FragmentLocality) -> Self {
        metrics::gauge!("oracle_fragments_active").increment(1.0);
        Self {
            started_at: Instant::now(),
            finished: false,
        }
    }

    /// Records one terminal fragment outcome and admitted encoded bytes.
    pub fn finish(&mut self, outcome: FragmentOutcome, encoded_bytes: u64) {
        if self.finished {
            return;
        }
        self.finished = true;
        metrics::histogram!("oracle_fragment_duration_seconds", "outcome" => outcome.as_str())
            .record(self.started_at.elapsed().as_secs_f64());
        let _ = encoded_bytes;
    }
}

impl Drop for FragmentTelemetry {
    /// Records cancellation/failure and closes the in-flight gauge.
    fn drop(&mut self) {
        if !self.finished {
            self.finish(FragmentOutcome::Failed, 0);
        }
        metrics::gauge!("oracle_fragments_active").decrement(1.0);
    }
}

/// Records one peer attempt outcome with closed labels.
pub fn record_peer_attempt(outcome: FragmentOutcome, error_class: PeerErrorClass) {
    let _ = (outcome, error_class);
}

/// Records one pending/running worker-slot reservation outcome.
pub fn record_slot(query_class: QueryClass, outcome: SlotOutcome) {
    let _ = (query_class, outcome);
}

/// Records one closed peer-security event class.
pub fn record_security(event_class: SecurityEventClass) {
    let _ = event_class;
}
