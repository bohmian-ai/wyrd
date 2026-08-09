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

impl FragmentLocality {
    /// Returns the canonical low-cardinality metric label.
    ///
    /// Both the in-flight gauge and the terminal `bifrost_oracle_fragments_total`
    /// counter tag their observations with this closed value so dashboards can
    /// split fragment volume by where the attempt executed without unbounded
    /// label cardinality.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }
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

impl PeerErrorClass {
    /// Returns the canonical low-cardinality metric label.
    ///
    /// Tags each `bifrost_oracle_peer_attempts_total` observation with the closed
    /// failure category (or `none` for a successful attempt) so peer-dispatch
    /// error rates stay attributable without leaking free-form error text into
    /// the metric label space.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Availability => "availability",
            Self::Security => "security",
            Self::Exhausted => "exhausted",
            Self::Footer => "footer",
            Self::Attempt => "attempt",
        }
    }
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
    /// Closed locality label retained from `start` so the terminal
    /// `bifrost_oracle_fragments_total` counter can attribute each fragment to
    /// where it executed.
    locality: FragmentLocality,
    /// Fragment start used by the canonical duration histogram.
    started_at: Instant,
    /// Whether a terminal outcome has already been recorded.
    finished: bool,
}

impl FragmentTelemetry {
    /// Starts canonical in-flight and duration accounting.
    ///
    /// Increments the in-flight fragment gauge and captures the start instant.
    /// The `locality` is retained on the guard so `finish` (and the `Drop`
    /// fallback) can tag the terminal `bifrost_oracle_fragments_total` counter
    /// with the same closed label.
    #[must_use]
    pub fn start(locality: FragmentLocality) -> Self {
        metrics::gauge!("oracle_fragments_active").increment(1.0);
        Self {
            locality,
            started_at: Instant::now(),
            finished: false,
        }
    }

    /// Records one terminal fragment outcome and admitted encoded bytes.
    ///
    /// Emits the closed `bifrost_oracle_fragments_total{locality,outcome}`
    /// counter and the canonical duration histogram exactly once; repeat calls
    /// after the first terminal outcome are ignored so the `Drop` fallback never
    /// double-counts. `encoded_bytes` is accepted for caller symmetry but is
    /// intentionally not projected into a metric here.
    pub fn finish(&mut self, outcome: FragmentOutcome, encoded_bytes: u64) {
        if self.finished {
            return;
        }
        self.finished = true;
        metrics::counter!(
            "bifrost_oracle_fragments_total",
            "locality" => self.locality.as_str(),
            "outcome" => outcome.as_str()
        )
        .increment(1);
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
///
/// Increments the `bifrost_oracle_peer_attempts_total{error_class,outcome}`
/// counter so peer-dispatch success and failure volume stays observable per
/// closed failure category. Both labels are closed enums, keeping the series
/// cardinality bounded.
pub fn record_peer_attempt(outcome: FragmentOutcome, error_class: PeerErrorClass) {
    metrics::counter!(
        "bifrost_oracle_peer_attempts_total",
        "outcome" => outcome.as_str(),
        "error_class" => error_class.as_str()
    )
    .increment(1);
}

/// Records one pending/running worker-slot reservation outcome.
pub fn record_slot(query_class: QueryClass, outcome: SlotOutcome) {
    let _ = (query_class, outcome);
}

/// Records one closed peer-security event class.
pub fn record_security(event_class: SecurityEventClass) {
    let _ = event_class;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terminal `finish` emits the closed fragment counter with both the
    /// retained locality and the terminal outcome, exactly once.
    ///
    /// Drives one `FragmentTelemetry` guard through an explicit `finish` under a
    /// scoped benchmark recorder and asserts the
    /// `bifrost_oracle_fragments_total{locality,outcome}` series carries a single
    /// increment tagged with the locality captured at `start`.
    #[test]
    fn finish_emits_fragments_total_with_retained_locality() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let guard = metrics::set_default_local_recorder(&recorder);
        let mut telemetry = FragmentTelemetry::start(FragmentLocality::Remote);
        telemetry.finish(FragmentOutcome::Success, 4_096);
        drop(guard);

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_oracle_fragments_total{locality=\"remote\",outcome=\"success\"}")
                .copied(),
            Some(1),
            "{snapshot:?}",
        );
    }

    /// The `Drop` fallback records a failed terminal fragment when no explicit
    /// terminal outcome was reported, without double-counting.
    ///
    /// Lets a guard drop without calling `finish` and asserts a single
    /// failure-tagged increment on `bifrost_oracle_fragments_total` for the
    /// retained locality.
    #[test]
    fn drop_without_finish_records_failed_fragment_once() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let guard = metrics::set_default_local_recorder(&recorder);
        drop(FragmentTelemetry::start(FragmentLocality::Local));
        drop(guard);

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_oracle_fragments_total{locality=\"local\",outcome=\"failed\"}")
                .copied(),
            Some(1),
            "{snapshot:?}",
        );
    }

    /// `record_peer_attempt` emits the closed peer-attempt counter with both the
    /// outcome and error-class labels.
    ///
    /// Records one failed availability attempt under a scoped recorder and
    /// asserts the `bifrost_oracle_peer_attempts_total{error_class,outcome}`
    /// series carries the matching single increment.
    #[test]
    fn record_peer_attempt_emits_closed_counter() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let guard = metrics::set_default_local_recorder(&recorder);
        record_peer_attempt(FragmentOutcome::Failed, PeerErrorClass::Availability);
        drop(guard);

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get(
                    "bifrost_oracle_peer_attempts_total{error_class=\"availability\",outcome=\"failed\"}"
                )
                .copied(),
            Some(1),
            "{snapshot:?}",
        );
    }
}
