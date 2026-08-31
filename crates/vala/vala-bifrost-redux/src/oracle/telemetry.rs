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
    /// No live local Oracle membership may accept new work.
    Membership,
    /// Oracle shutdown closed admission.
    Shutdown,
}

impl OracleAdmissionReason {
    /// Every wire value used by contract tests and dashboards.
    pub(crate) const ALL: [Self; 9] = [
        Self::ClassCapacity,
        Self::TenantBudget,
        Self::QueueFull,
        Self::QueueDeadline,
        Self::Memory,
        Self::Spill,
        Self::AuditUnavailable,
        Self::Membership,
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
            Self::Membership => "membership",
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

/// Closed private stage-operation label for the Analytical execution path.
///
/// Both halves of the distributed protocol are separately observable: a stage
/// that fails to set its plan and a stage that fails to execute its task are
/// different operational problems, and their tickets carry different signing
/// domains and nonces, so their telemetry must not collapse into one series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticalStageOperation {
    /// The coordinator installed a stage's subplan on a follower.
    SetPlan,
    /// The coordinator asked a follower to execute a partition range.
    ExecuteTask,
}

impl AnalyticalStageOperation {
    /// Every stage operation, used to pre-register series at zero.
    pub(crate) const ALL: [Self; 2] = [Self::SetPlan, Self::ExecuteTask];

    /// Returns the canonical low-cardinality metric label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SetPlan => "set_plan",
            Self::ExecuteTask => "execute_task",
        }
    }
}

/// Closed outcome of one stage-operation authority decision.
///
/// `Authorized` is emitted only after every binding, body digest, deadline, and
/// replay check has passed, so the ratio of these series is what tells an
/// operator whether follower work is being refused before it can decode a plan,
/// touch the task cache, or issue object I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticalStageAuthorityOutcome {
    /// The stage operation is authorized to proceed to decode and execution.
    Authorized,
    /// The ticket did not authenticate against the deployment key.
    Signature,
    /// A bound identity, node, fence, stage, task, attempt, or digest mismatched.
    Binding,
    /// The bounded raw body did not match its signed digest, or exceeded bounds.
    Body,
    /// The absolute deadline or the ticket's own expiry had elapsed.
    Expired,
    /// The single-use nonce had already been consumed.
    Replay,
}

impl AnalyticalStageAuthorityOutcome {
    /// Every authority outcome, used to pre-register series at zero.
    pub(crate) const ALL: [Self; 6] = [
        Self::Authorized,
        Self::Signature,
        Self::Binding,
        Self::Body,
        Self::Expired,
        Self::Replay,
    ];

    /// Returns the canonical low-cardinality metric label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authorized => "authorized",
            Self::Signature => "signature",
            Self::Binding => "binding",
            Self::Body => "body",
            Self::Expired => "expired",
            Self::Replay => "replay",
        }
    }
}

/// Closed terminal outcome of one Analytical attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticalAttemptOutcome {
    /// The attempt drained and produced its complete result.
    Success,
    /// The attempt was superseded by the one permitted pre-egress retry.
    Retried,
    /// The attempt was cancelled, by client drop, deadline, or shutdown.
    Cancelled,
    /// The attempt failed terminally.
    Failed,
}

impl AnalyticalAttemptOutcome {
    /// Every attempt outcome, used to pre-register series at zero.
    pub(crate) const ALL: [Self; 4] = [Self::Success, Self::Retried, Self::Cancelled, Self::Failed];

    /// Returns the canonical low-cardinality metric label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Retried => "retried",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

/// Pre-registers every Analytical series at zero.
///
/// Wyrd's convention is that a closed-label family exists in the registry
/// before it is first incremented, so a dashboard panel and a contract test can
/// both distinguish "no Analytical work happened" from "this build does not
/// emit Analytical telemetry at all". Called once from Oracle telemetry
/// construction, alongside the Interactive families.
pub(crate) fn register_analytical_series() {
    for operation in AnalyticalStageOperation::ALL {
        for outcome in AnalyticalStageAuthorityOutcome::ALL {
            metrics::counter!(
                "bifrost_oracle_analytical_stage_authority_total",
                "operation" => operation.as_str(),
                "outcome" => outcome.as_str()
            )
            .increment(0);
        }
        metrics::counter!(
            "bifrost_oracle_analytical_stage_operations_total",
            "operation" => operation.as_str()
        )
        .increment(0);
    }
    for outcome in AnalyticalAttemptOutcome::ALL {
        metrics::counter!(
            "bifrost_oracle_analytical_attempts_total",
            "outcome" => outcome.as_str()
        )
        .increment(0);
    }
    metrics::gauge!("bifrost_oracle_analytical_attempts_active").set(0.0);
    metrics::gauge!("bifrost_oracle_analytical_exchanges_active").set(0.0);
    metrics::counter!("bifrost_oracle_analytical_exchange_batches_total").increment(0);
    metrics::counter!("bifrost_oracle_analytical_exchange_bytes_total").increment(0);
}

/// Records one stage-operation authority decision with closed labels.
///
/// Emits `bifrost_oracle_analytical_stage_authority_total{operation,outcome}`
/// and, for a refusal, one `bifrost_oracle_security_events_total` observation on
/// the same family Oracle's tenant tripwire already uses, so a stage refusal is
/// visible on the existing Bifrost security panel without a new family. Every
/// label is a closed enum, so a forged identity cannot expand the label space.
pub fn record_stage_authority(
    operation: AnalyticalStageOperation,
    outcome: AnalyticalStageAuthorityOutcome,
) {
    metrics::counter!(
        "bifrost_oracle_analytical_stage_authority_total",
        "operation" => operation.as_str(),
        "outcome" => outcome.as_str()
    )
    .increment(1);
    if !matches!(outcome, AnalyticalStageAuthorityOutcome::Authorized) {
        metrics::counter!(
            "bifrost_oracle_security_events_total",
            "event_class" => "analytical_stage"
        )
        .increment(1);
        tracing::warn!(
            operation = operation.as_str(),
            outcome = outcome.as_str(),
            "Oracle analytical stage operation refused before decode, cache, or IO"
        );
    }
}

/// Records one accepted stage operation reaching decode and execution.
pub fn record_stage_operation(operation: AnalyticalStageOperation) {
    metrics::counter!(
        "bifrost_oracle_analytical_stage_operations_total",
        "operation" => operation.as_str()
    )
    .increment(1);
}

/// Attempt-scoped Analytical metrics released on every terminal path.
///
/// Mirrors [`FragmentTelemetry`]: the in-flight gauge and duration histogram are
/// owned by the guard, and `Drop` records a terminal outcome when a caller exits
/// through cancellation, a deadline, an error, or a panic. That is what makes
/// "zero retained attempts" an assertion a journey can make about the gauge
/// rather than about a caller's discipline.
pub struct AnalyticalAttemptTelemetry {
    /// Attempt start used by the canonical duration histogram.
    started_at: Instant,
    /// Whether a terminal outcome has already been recorded.
    finished: bool,
}

impl AnalyticalAttemptTelemetry {
    /// Starts in-flight and duration accounting for one Analytical attempt.
    ///
    /// The attempt's identities are recorded as structured `tracing` fields
    /// rather than metric labels: both are UUIDs, and putting them in the label
    /// space would make the series cardinality unbounded.
    #[must_use]
    pub fn start(public_query_id: &str, datafusion_query_id: &str, attempt: u8) -> Self {
        metrics::gauge!("bifrost_oracle_analytical_attempts_active").increment(1.0);
        tracing::debug!(
            public_query_id,
            datafusion_query_id,
            attempt,
            "Oracle analytical attempt started"
        );
        Self {
            started_at: Instant::now(),
            finished: false,
        }
    }

    /// Records one terminal attempt outcome exactly once.
    ///
    /// Repeat calls after the first terminal outcome are ignored, so the `Drop`
    /// fallback never double-counts an attempt a caller already finished.
    pub fn finish(&mut self, outcome: AnalyticalAttemptOutcome) {
        if self.finished {
            return;
        }
        self.finished = true;
        metrics::counter!(
            "bifrost_oracle_analytical_attempts_total",
            "outcome" => outcome.as_str()
        )
        .increment(1);
        metrics::histogram!(
            "bifrost_oracle_analytical_attempt_duration_seconds",
            "outcome" => outcome.as_str()
        )
        .record(self.started_at.elapsed().as_secs_f64());
    }
}

impl Drop for AnalyticalAttemptTelemetry {
    /// Records a failed terminal attempt and closes the in-flight gauge.
    fn drop(&mut self) {
        if !self.finished {
            self.finish(AnalyticalAttemptOutcome::Failed);
        }
        metrics::gauge!("bifrost_oracle_analytical_attempts_active").decrement(1.0);
    }
}

/// Records one streamed exchange observation for the Analytical path.
///
/// Exchange volume is the evidence that follower stages actually exchanged data
/// rather than collapsing onto the leader, so it is counted separately from
/// query-level row and byte families.
pub fn record_exchange_transfer(batches: u64, bytes: u64) {
    metrics::counter!("bifrost_oracle_analytical_exchange_batches_total").increment(batches);
    metrics::counter!("bifrost_oracle_analytical_exchange_bytes_total").increment(bytes);
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
    /// A refused stage operation lands on both the closed authority family and
    /// the existing Bifrost security family.
    ///
    /// # Panics
    ///
    /// Panics when either series is missing its single increment.
    #[test]
    fn record_stage_authority_emits_closed_and_security_counters() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let guard = metrics::set_default_local_recorder(&recorder);
        record_stage_authority(
            AnalyticalStageOperation::ExecuteTask,
            AnalyticalStageAuthorityOutcome::Binding,
        );
        drop(guard);

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get(
                    "bifrost_oracle_analytical_stage_authority_total{operation=\"execute_task\",outcome=\"binding\"}"
                )
                .copied(),
            Some(1),
            "{snapshot:?}",
        );
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_oracle_security_events_total{event_class=\"analytical_stage\"}")
                .copied(),
            Some(1),
            "{snapshot:?}",
        );
    }

    /// An authorized stage operation raises no security event.
    ///
    /// # Panics
    ///
    /// Panics when an accepted operation is counted as a security event.
    #[test]
    fn record_stage_authority_authorized_raises_no_security_event() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let guard = metrics::set_default_local_recorder(&recorder);
        record_stage_authority(
            AnalyticalStageOperation::SetPlan,
            AnalyticalStageAuthorityOutcome::Authorized,
        );
        drop(guard);

        let snapshot = recorder.snapshot();
        assert!(
            !snapshot.counters.contains_key(
                "bifrost_oracle_security_events_total{event_class=\"analytical_stage\"}"
            ),
            "{snapshot:?}",
        );
    }

    /// An attempt guard dropped without a terminal outcome records a failure.
    ///
    /// # Panics
    ///
    /// Panics when the `Drop` fallback does not record exactly one failure.
    #[test]
    fn analytical_attempt_drop_without_finish_records_failure_once() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let guard = metrics::set_default_local_recorder(&recorder);
        drop(AnalyticalAttemptTelemetry::start("public", "private", 0));
        drop(guard);

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_oracle_analytical_attempts_total{outcome=\"failed\"}")
                .copied(),
            Some(1),
            "{snapshot:?}",
        );
    }
}
