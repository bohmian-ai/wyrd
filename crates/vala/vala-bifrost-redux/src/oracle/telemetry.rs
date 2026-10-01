//! Closed-label peer, fragment, slot, and security telemetry.

use std::time::Instant;
use wyrd_spec::vala::api::QueryClass;

/// Carries the spawning task's span into every task `DataFusion` spawns.
///
/// `DataFusion` runs partitions, repartitioning, and coalescing on its own
/// spawned tasks, which start with no span. Without this, work a query plan
/// does on those tasks, such as a remote peer fragment, would be a separate
/// trace root rather than child work of the query that caused it.
struct QuerySpanJoinSetTracer;

impl datafusion::common::runtime::JoinSetTracer for QuerySpanJoinSetTracer {
    /// Instruments a spawned future with the span current at spawn time.
    fn trace_future(
        &self,
        future: futures_util::future::BoxFuture<'static, Box<dyn std::any::Any + Send>>,
    ) -> futures_util::future::BoxFuture<'static, Box<dyn std::any::Any + Send>> {
        use tracing::Instrument as _;
        Box::pin(future.instrument(tracing::Span::current()))
    }

    /// Runs a spawned blocking closure inside the span current at spawn time.
    fn trace_block(
        &self,
        block: Box<dyn FnOnce() -> Box<dyn std::any::Any + Send> + Send>,
    ) -> Box<dyn FnOnce() -> Box<dyn std::any::Any + Send> + Send> {
        let span = tracing::Span::current();
        Box::new(move || span.in_scope(block))
    }
}

/// Installs [`QuerySpanJoinSetTracer`] as `DataFusion`'s process tracer.
///
/// The tracer is process-global and can be set once; every Oracle owner in
/// the process calls this, and a later call finding it already set is the
/// expected outcome rather than a failure, because the installed tracer is
/// this same stateless one.
pub(crate) fn install_query_span_propagation() {
    static TRACER: QuerySpanJoinSetTracer = QuerySpanJoinSetTracer;
    let _already_installed = datafusion::common::runtime::set_join_set_tracer(&TRACER);
}

/// Returns the closed metric label every Oracle metric family uses for a class.
pub(crate) const fn query_class_label(class: QueryClass) -> &'static str {
    match class {
        QueryClass::Interactive => "interactive",
        QueryClass::Analytical => "analytical",
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

/// Closed local admission reason.
///
/// Only reasons an admission branch actually reaches are listed; a label value
/// no code path can emit would be a permanently zero series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OracleAdmissionReason {
    /// Class-local active capacity granted the query, or the class has none.
    ClassCapacity,
    /// Queue bound was reached.
    QueueFull,
    /// Queue deadline elapsed.
    QueueDeadline,
    /// No live local Oracle membership may accept new work.
    Membership,
    /// Oracle shutdown closed admission.
    Shutdown,
}

impl OracleAdmissionReason {
    /// Every wire value, projected by the benchmark label-domain contract.
    #[cfg(feature = "bench-support")]
    pub(crate) const ALL: [Self; 5] = [
        Self::ClassCapacity,
        Self::QueueFull,
        Self::QueueDeadline,
        Self::Membership,
        Self::Shutdown,
    ];

    /// Every `(outcome, reason)` pair an admission branch can emit.
    ///
    /// Oracle pre-registers exactly these series at zero, so an impossible
    /// combination such as an admitted query refused for a full queue never
    /// appears on a dashboard.
    pub(crate) const DECISIONS: [(OracleAdmissionOutcome, Self); 6] = [
        (OracleAdmissionOutcome::Admitted, Self::ClassCapacity),
        (OracleAdmissionOutcome::Rejected, Self::ClassCapacity),
        (OracleAdmissionOutcome::Rejected, Self::QueueFull),
        (OracleAdmissionOutcome::Rejected, Self::QueueDeadline),
        (OracleAdmissionOutcome::Rejected, Self::Membership),
        (OracleAdmissionOutcome::Rejected, Self::Shutdown),
    ];

    /// Return the canonical label value.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ClassCapacity => "class_capacity",
            Self::QueueFull => "queue_full",
            Self::QueueDeadline => "queue_deadline",
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
    /// Oracle shutdown cancelled the stream.
    Shutdown,
}

impl OracleCancellationReason {
    /// Every wire value used by contract tests and dashboards.
    pub(crate) const ALL: [Self; 2] = [Self::ClientDrop, Self::Shutdown];

    /// Return the canonical label value.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ClientDrop => "client_drop",
            Self::Shutdown => "shutdown",
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
        }
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
/// `Authorized` is emitted only after every bound, binding, body digest, and
/// deadline check has passed, so the ratio of these series is what tells an
/// operator whether follower work is being refused before it can decode a plan,
/// touch the task cache, or issue object I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticalStageAuthorityOutcome {
    /// The stage operation is authorized to proceed to decode and execution.
    Authorized,
    /// The context was absent, oversized, or did not decode.
    Malformed,
    /// A bound identity, node, fence, stage, task, attempt, or digest mismatched.
    Binding,
    /// The bounded raw body did not match its signed digest, or exceeded bounds.
    Body,
    /// The absolute deadline or the context's own expiry had elapsed.
    Expired,
}

impl AnalyticalStageAuthorityOutcome {
    /// Every authority outcome, used to pre-register series at zero.
    pub(crate) const ALL: [Self; 5] = [
        Self::Authorized,
        Self::Malformed,
        Self::Binding,
        Self::Body,
        Self::Expired,
    ];

    /// Returns the canonical low-cardinality metric label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authorized => "authorized",
            Self::Malformed => "malformed",
            Self::Binding => "binding",
            Self::Body => "body",
            Self::Expired => "expired",
        }
    }
}

/// Closed terminal outcome of one Analytical attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticalAttemptOutcome {
    /// The attempt drained and produced its complete result.
    Success,
    /// The attempt was cancelled, by client drop, deadline, or shutdown.
    Cancelled,
    /// The attempt failed terminally.
    Failed,
}

impl AnalyticalAttemptOutcome {
    /// Every attempt outcome, used to pre-register series at zero.
    pub(crate) const ALL: [Self; 3] = [Self::Success, Self::Cancelled, Self::Failed];

    /// Returns the canonical low-cardinality metric label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
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
    metrics::counter!("bifrost_oracle_analytical_exchange_batches_total").increment(0);
    metrics::counter!("bifrost_oracle_analytical_exchange_bytes_total").increment(0);
    metrics::counter!("bifrost_oracle_analytical_output_sort_spills_total").increment(0);
    metrics::counter!("bifrost_oracle_analytical_output_sort_spilled_bytes_total").increment(0);
    metrics::counter!("bifrost_oracle_analytical_output_sort_spilled_rows_total").increment(0);
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
/// The in-flight gauge and duration histogram are owned by the guard, and
/// `Drop` records a terminal outcome when a caller exits through cancellation,
/// a deadline, an error, or a panic. That is what makes
/// "zero retained attempts" an assertion a journey can make about the gauge
/// rather than about a caller's discipline.
pub struct AnalyticalAttemptTelemetry {
    /// Attempt start used by the canonical duration histogram.
    started_at: Instant,
    /// Whether a terminal outcome has already been recorded.
    finished: bool,
    /// Production span carrying this attempt's scrubbed identity and outcome.
    ///
    /// Held rather than entered: the attempt outlives any single `await`, so
    /// what the span provides is a correlatable record of one attempt's
    /// lifetime and terminal outcome, not an ambient context for the work.
    /// It closes when the guard drops, which is the same moment the in-flight
    /// gauge returns to its baseline.
    span: tracing::Span,
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
        let span = tracing::info_span!(
            "bifrost.oracle.analytical.attempt",
            public_query_id,
            datafusion_query_id,
            attempt,
            outcome = tracing::field::Empty
        );
        Self {
            started_at: Instant::now(),
            finished: false,
            span,
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
        let elapsed = self.started_at.elapsed();
        metrics::counter!(
            "bifrost_oracle_analytical_attempts_total",
            "outcome" => outcome.as_str()
        )
        .increment(1);
        metrics::histogram!(
            "bifrost_oracle_analytical_attempt_duration_seconds",
            "outcome" => outcome.as_str()
        )
        .record(elapsed.as_secs_f64());
        self.span.record("outcome", outcome.as_str());
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

/// Publishes one settled query's own output-sort spill evidence.
///
/// Emitted from the graph's settlement rather than from the result stream,
/// because a `SortExec` registers its metric set during execution and only
/// stops changing once that stream is dropped. The three families are
/// unlabelled: the operator identity that makes the evidence attributable is a
/// per-query fact and belongs on the attempt span, not in a metric label
/// domain a query shape could expand.
pub fn record_output_sort_spill(spills: u64, bytes: u64, rows: u64) {
    metrics::counter!("bifrost_oracle_analytical_output_sort_spills_total").increment(spills);
    metrics::counter!("bifrost_oracle_analytical_output_sort_spilled_bytes_total").increment(bytes);
    metrics::counter!("bifrost_oracle_analytical_output_sort_spilled_rows_total").increment(rows);
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
