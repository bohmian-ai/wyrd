//! Fixed-cardinality operational telemetry for Forge maintenance.

use std::collections::BTreeMap;
use std::time::Duration;

use metrics::{Counter, Gauge, Histogram};
use num_traits::ToPrimitive;
use uuid::Uuid;
use vala_sql::row_types::forge_tasks::{ForgeTaskState, ForgeTaskStrategy};
use wyrd_spec::vala::api::StoragePath;

use crate::catalog::TenantTableBinding;

use super::error::ForgeError;
use super::orphan_gc::OrphanGcOutcome;
use super::path::catalog_path_to_object_key;
use super::{Forge, ForgeScheduleOutcome};

/// Validated object-key contract for one rewrite source.
#[derive(Debug, Clone, Copy)]
pub(super) enum RewritePathContract<'binding> {
    /// Live Iceberg audit paths are catalog URIs rooted at the table location.
    Catalog {
        /// Validated physical tenant/table binding that owns the catalog root.
        binding: &'binding TenantTableBinding,
        /// Catalog table location used by the retained-manifest observation.
        table_location: &'binding str,
    },
}

impl RewritePathContract<'_> {
    /// Converts one typed rewrite path into a binding-validated object key.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when a staging path escapes its
    /// binding or a catalog URI does not belong to the validated table root.
    fn object_key(self, store: &opendal::Operator, path: &str) -> Result<String, ForgeError> {
        match self {
            Self::Catalog {
                binding,
                table_location,
            } => catalog_path_to_object_key(table_location, binding, store, path),
        }
    }
}

/// Durable-data source that produced a Forge operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeMetricSource {
    /// Server-side staging file-list state.
    Staging,
    /// Current Iceberg snapshot state.
    Iceberg,
}

impl ForgeMetricSource {
    /// Every source label registered for Forge operation and rewrite series.
    const ALL: [Self; 2] = [Self::Staging, Self::Iceberg];

    /// Returns the only metric label value emitted for this source.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Iceberg => "iceberg",
        }
    }
}

/// Closed maintenance stage inventory used by duration and failure series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeMetricStage {
    /// Promote already-published Scribe hot objects into the table unchanged.
    ScribePromotion,
    /// Reconcile staging audit operations.
    ReconcileStaging,
    /// Reconcile current-snapshot replacement operations.
    ReconcileIceberg,
    /// Discover current-snapshot rewrite groups.
    ManifestDiscovery,
    /// Replace one current-snapshot group.
    IcebergRewrite,
    /// Rewrite fragmented Iceberg manifests.
    ManifestRewrite,
    /// Reconcile and expire old snapshots.
    SnapshotExpiry,
    /// Reconcile and remove proven orphan objects.
    OrphanGc,
}

impl ForgeMetricStage {
    /// Every stage label registered for Forge duration and failure series.
    const ALL: [Self; 8] = [
        Self::ScribePromotion,
        Self::ReconcileStaging,
        Self::ReconcileIceberg,
        Self::ManifestDiscovery,
        Self::IcebergRewrite,
        Self::ManifestRewrite,
        Self::SnapshotExpiry,
        Self::OrphanGc,
    ];

    /// Returns the only metric label value emitted for this stage.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::ScribePromotion => "scribe_promotion",
            Self::ReconcileStaging => "reconcile_staging",
            Self::ReconcileIceberg => "reconcile_iceberg",
            Self::ManifestDiscovery => "manifest_discovery",
            Self::IcebergRewrite => "iceberg_rewrite",
            Self::ManifestRewrite => "manifest_rewrite",
            Self::SnapshotExpiry => "snapshot_expiry",
            Self::OrphanGc => "orphan_gc",
        }
    }
}

/// Closed strategy inventory for authoritative Forge catalog commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForgeCatalogCommitStrategy {
    /// A promotion fast-appends already-published Scribe objects unchanged.
    ScribePromotion,
    /// A live small-file rewrite replaces current Iceberg data files.
    SmallFiles,
}

impl ForgeCatalogCommitStrategy {
    /// Return the stable span value for this catalog commit strategy.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::ScribePromotion => "scribe_promotion",
            Self::SmallFiles => "small_files",
        }
    }
}

/// Closed terminal classification for staging and Iceberg operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeOperationResult {
    /// A new durable catalog effect committed.
    Committed,
    /// A previously uncertain effect was proven committed.
    Recovered,
    /// A prepared operation was proven abandoned and reset.
    Reset,
    /// No durable work was necessary.
    Noop,
    /// The tick-wide budget deferred work.
    Budget,
    /// The owner lost its lease fence.
    FenceLost,
    /// The owning operation returned an error.
    Failed,
}

impl ForgeOperationResult {
    /// Every result label registered for Forge operation series.
    const ALL: [Self; 7] = [
        Self::Committed,
        Self::Recovered,
        Self::Reset,
        Self::Noop,
        Self::Budget,
        Self::FenceLost,
        Self::Failed,
    ];

    /// Returns the only metric label value emitted for this result.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::Recovered => "recovered",
            Self::Reset => "reset",
            Self::Noop => "noop",
            Self::Budget => "budget",
            Self::FenceLost => "fence_lost",
            Self::Failed => "failed",
        }
    }
}

/// Closed lease-boundary result inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeLeaseResult {
    /// Another owner retains a live lease.
    Contention,
    /// An expired different owner was replaced.
    Takeover,
    /// A formerly held lease fence was lost.
    FenceLost,
}

impl ForgeLeaseResult {
    /// Every result label registered for Forge lease-boundary series.
    const ALL: [Self; 3] = [Self::Contention, Self::Takeover, Self::FenceLost];

    /// Returns the only metric label value emitted for this result.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Contention => "contention",
            Self::Takeover => "takeover",
            Self::FenceLost => "fence_lost",
        }
    }
}

/// Closed strategy labels retained by the task-duration and spill metric schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeTaskMetricStrategy {
    /// Promote already-published Scribe hot objects unchanged.
    ScribePromotion,
    /// Compact current Iceberg small files.
    SmallFiles,
    /// Rewrite fragmented Iceberg manifests.
    ManifestRewrite,
    /// Expire retained Iceberg snapshots.
    SnapshotExpiry,
}

impl ForgeTaskMetricStrategy {
    /// Every strategy label eagerly registered for task-duration and spill series.
    const ALL: [Self; 4] = [
        Self::ScribePromotion,
        Self::SmallFiles,
        Self::ManifestRewrite,
        Self::SnapshotExpiry,
    ];

    /// Returns the stable task metric label for this strategy.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::ScribePromotion => "scribe_promotion",
            Self::SmallFiles => "small_files",
            Self::ManifestRewrite => "manifest_rewrite",
            Self::SnapshotExpiry => "snapshot_expiry",
        }
    }
}

impl TryFrom<ForgeTaskStrategy> for ForgeTaskMetricStrategy {
    type Error = ForgeTaskStrategy;

    /// Converts executable durable strategies to the fixed task-metric vocabulary.
    ///
    /// Full-identity repair and cleanup rows deliberately have no task-duration
    /// series: the worker rejects them before execution. The
    /// explicit error preserves that invariant instead of silently discarding an
    /// unexpected durable strategy.
    fn try_from(strategy: ForgeTaskStrategy) -> Result<Self, Self::Error> {
        match strategy {
            ForgeTaskStrategy::ScribePromotion => Ok(Self::ScribePromotion),
            ForgeTaskStrategy::SmallFiles => Ok(Self::SmallFiles),
            ForgeTaskStrategy::ManifestRewrite => Ok(Self::ManifestRewrite),
            ForgeTaskStrategy::SnapshotExpiry => Ok(Self::SnapshotExpiry),
            ForgeTaskStrategy::FullIdentity
            | ForgeTaskStrategy::ExpiredCleanup
            | ForgeTaskStrategy::OrphanCleanup => Err(strategy),
        }
    }
}

/// Closed terminal result labels retained by the task-duration metric schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeTaskTerminalResult {
    /// The task committed or otherwise terminalized successfully.
    Succeeded,
    /// The task remains eligible for a later attempt.
    Retryable,
    /// The task failed permanently.
    Failed,
    /// The task was superseded or cancelled before a durable effect.
    Cancelled,
    /// The task permanently exceeded configured execution capacity.
    Unschedulable,
}

impl ForgeTaskTerminalResult {
    /// Every terminal result label eagerly registered for task-duration series.
    const ALL: [Self; 5] = [
        Self::Succeeded,
        Self::Retryable,
        Self::Failed,
        Self::Cancelled,
        Self::Unschedulable,
    ];

    /// Returns the stable task metric label for this terminal result.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Retryable => "retryable",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Unschedulable => "unschedulable",
        }
    }
}

impl TryFrom<ForgeTaskState> for ForgeTaskTerminalResult {
    type Error = ForgeTaskState;

    /// Converts terminal durable task states to metric labels.
    ///
    /// # Errors
    ///
    /// Returns the original non-terminal state when a caller attempts to
    /// publish task-duration telemetry before terminalization.
    fn try_from(state: ForgeTaskState) -> Result<Self, Self::Error> {
        match state {
            ForgeTaskState::Succeeded => Ok(Self::Succeeded),
            ForgeTaskState::Retryable => Ok(Self::Retryable),
            ForgeTaskState::Failed => Ok(Self::Failed),
            ForgeTaskState::Cancelled => Ok(Self::Cancelled),
            ForgeTaskState::Unschedulable => Ok(Self::Unschedulable),
            ForgeTaskState::Ready
            | ForgeTaskState::Claimed
            | ForgeTaskState::Running
            | ForgeTaskState::Prepared => Err(state),
        }
    }
}

/// Closed durable conflict labels emitted at Forge rejection boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeConflictKind {
    /// Another owner retains the table lease.
    LeaseContention,
    /// The executing owner lost its lease fence.
    FenceLost,
    /// The task's base snapshot no longer matches the current table snapshot.
    SnapshotChanged,
}

impl ForgeConflictKind {
    /// Every conflict label eagerly registered for the conflict counter.
    const ALL: [Self; 3] = [
        Self::LeaseContention,
        Self::FenceLost,
        Self::SnapshotChanged,
    ];

    /// Returns the stable conflict metric label for this durable rejection.
    const fn as_str(self) -> &'static str {
        match self {
            Self::LeaseContention => "lease_contention",
            Self::FenceLost => "fence_lost",
            Self::SnapshotChanged => "snapshot_changed",
        }
    }
}

/// Closed cleanup provenance labels retained by the cleanup-duration schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum ForgeCleanupKind {
    /// Cleanup proved one expired object can be removed.
    Expired,
    /// Cleanup proved one never-published object can be removed.
    Orphan,
}

impl ForgeCleanupKind {
    /// Every provenance label eagerly registered for cleanup-duration series.
    const ALL: [Self; 2] = [Self::Expired, Self::Orphan];

    /// Returns the stable cleanup metric label for this provenance.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Expired => "expired",
            Self::Orphan => "orphan",
        }
    }
}

/// Exact object counts and bytes for one terminal replacement.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct ForgeRewriteVolume {
    /// Exact replaced input object count.
    pub(super) input_files: usize,
    /// Exact replaced input bytes.
    pub(super) input_bytes: u64,
    /// Exact committed output object count.
    pub(super) output_files: usize,
    /// Exact committed output bytes.
    pub(super) output_bytes: u64,
}

/// Closed resource kinds owned by one Forge rewrite attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ForgeAttemptResource {
    /// Resident memory granted through the attempt-local `DataFusion` pool.
    Memory,
    /// Disposable sort and pending-output scratch granted to the attempt.
    Scratch,
}

impl ForgeAttemptResource {
    /// Every resource label registered for attempt lifecycle telemetry.
    const ALL: [Self; 2] = [Self::Memory, Self::Scratch];

    /// Returns the stable metric label for this resource.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Scratch => "scratch",
        }
    }
}

/// Closed observation points for a Forge attempt resource envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ForgeResourceObservationKind {
    /// The authoritative resource amount persisted by planning.
    Planned,
    /// The exact resource amount granted atomically by the root governor.
    Acquired,
    /// The largest conservative resource ownership observed during execution.
    Peak,
}

impl ForgeResourceObservationKind {
    /// Every observation label registered for attempt resource telemetry.
    const ALL: [Self; 3] = [Self::Planned, Self::Acquired, Self::Peak];

    /// Returns the stable metric label for this observation point.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Acquired => "acquired",
            Self::Peak => "peak",
        }
    }
}

/// Exact resource lifecycle observation emitted once during attempt finalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ForgeResourceObservation {
    /// Persisted resident-memory request.
    pub(crate) planned_memory: u64,
    /// Exact resident-memory grant.
    pub(crate) acquired_memory: u64,
    /// Largest attempt-local resident reservation.
    pub(crate) peak_memory: u64,
    /// Persisted aggregate scratch request.
    pub(crate) planned_scratch: u64,
    /// Exact aggregate scratch grant.
    pub(crate) acquired_scratch: u64,
    /// Conservative bounded peak of sort spill plus pending-output ownership.
    pub(crate) peak_scratch: u64,
}

/// Closed execution phase for a typed Forge capacity refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ForgeCapacityRefusalPhase {
    /// Live root capacity was unavailable before execution or input IO.
    Admission,
    /// An admitted attempt exhausted one persisted execution term.
    Execution,
}

impl ForgeCapacityRefusalPhase {
    /// Every refusal phase registered in the fixed metric inventory.
    const ALL: [Self; 2] = [Self::Admission, Self::Execution];

    /// Returns the stable metric label for this refusal phase.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::Execution => "execution",
        }
    }
}

/// Closed durable progress effects emitted when a Forge task succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ForgeProgressEffect {
    /// The task changed durable table state or completed non-expiry work.
    Changed,
    /// Snapshot-expiry maintenance honestly acknowledged a no-op trigger.
    AcknowledgedNoop,
}

impl ForgeProgressEffect {
    /// Every durable progress effect registered in the metric inventory.
    const ALL: [Self; 2] = [Self::Changed, Self::AcknowledgedNoop];

    /// Returns the stable metric label for this progress effect.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::AcknowledgedNoop => "acknowledged_noop",
        }
    }
}

/// Closed outcomes for one completed durable hint-persistence attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ForgeHintPersistenceResult {
    /// The hint was committed as durable planning demand.
    Succeeded,
    /// Durable demand persistence returned an error.
    Failed,
}

impl ForgeHintPersistenceResult {
    /// Every result registered in the fixed-cardinality persistence inventory.
    const ALL: [Self; 2] = [Self::Succeeded, Self::Failed];

    /// Return the stable metric and span label for this outcome.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

/// Closed outcomes for one completed planning-demand transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ForgeDemandTransitionResult {
    /// Candidate-free discovery acknowledged the exact demand generation.
    Drained,
    /// Successful task completion atomically created successor demand.
    Continued,
    /// A concurrent producer advanced the demand before acknowledgement.
    GenerationChanged,
    /// A completed transition returned an error while retaining recoverable work.
    Failed,
}

impl ForgeDemandTransitionResult {
    /// Every fixed-cardinality transition result registered at owner creation.
    const ALL: [Self; 4] = [
        Self::Drained,
        Self::Continued,
        Self::GenerationChanged,
        Self::Failed,
    ];

    /// Return the stable metric label for this transition result.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Drained => "drained",
            Self::Continued => "continued",
            Self::GenerationChanged => "generation_changed",
            Self::Failed => "failed",
        }
    }
}

/// Registered Forge metric handles retained by one Forge owner.
pub struct ForgeTelemetry {
    /// Worker settlement outcomes by the six durable failure classes.
    failure_classes: BTreeMap<super::error::ForgeFailureClass, Counter>,
    /// Planned, acquired, and peak byte observations by closed resource kind.
    attempt_resource_bytes:
        BTreeMap<(ForgeAttemptResource, ForgeResourceObservationKind), Histogram>,
    /// Exactly-once resource finalization outcomes by resource and result.
    attempt_resource_releases: BTreeMap<(ForgeAttemptResource, &'static str), Counter>,
    /// Execution-envelope failures by the resource whose persisted term was exhausted.
    execution_envelope_failures: BTreeMap<ForgeAttemptResource, Counter>,
    /// Audited legacy-envelope settlement outcomes.
    legacy_envelopes_superseded: BTreeMap<&'static str, Counter>,
    /// Retried durable failures by closed failure class.
    retries: BTreeMap<super::error::ForgeFailureClass, Counter>,
    /// Terminally poisoned work by closed failure class.
    terminal_poisons: BTreeMap<super::error::ForgeFailureClass, Counter>,
    /// Capacity refusals split between admission and execution.
    capacity_refusals: BTreeMap<ForgeCapacityRefusalPhase, Counter>,
    /// Current durable compaction debt by files and bytes.
    compaction_debt: BTreeMap<&'static str, Gauge>,
    /// Successful task progress effects by closed outcome.
    progress_effects: BTreeMap<ForgeProgressEffect, Counter>,
    /// Current durable quarantine state for this worker process.
    quarantine_state: Gauge,
    /// Completed planning-demand transitions by closed result.
    demand_transitions: BTreeMap<ForgeDemandTransitionResult, Counter>,
    /// Candidate file counts observed at the current-snapshot planning boundary.
    discovered_candidate_files: BTreeMap<ForgeTaskMetricStrategy, Histogram>,
    /// Candidate bytes observed at the current-snapshot planning boundary.
    discovered_candidate_bytes: BTreeMap<ForgeTaskMetricStrategy, Histogram>,
    /// Completed hint-persistence counters by closed result.
    hint_persistence: BTreeMap<ForgeHintPersistenceResult, Counter>,
    /// Completed hint-persistence durations by closed result.
    hint_persistence_seconds: BTreeMap<ForgeHintPersistenceResult, Histogram>,
    /// Complete scheduler-owned oldest-backlog publication.
    oldest_backlog: Gauge,
    /// Complete scheduler-owned fairness-lag publication.
    fairness_lag: Gauge,
    /// Count of complete scheduler gauge publications.
    complete_gauge_publications: Counter,
    /// Terminal operation counters for every source/result combination.
    operations: BTreeMap<(ForgeMetricSource, ForgeOperationResult), Counter>,
    /// Lease-boundary counters for every closed result.
    lease_events: BTreeMap<ForgeLeaseResult, Counter>,
    /// Failure counters for every owning stage.
    stage_failures: BTreeMap<ForgeMetricStage, Counter>,
    /// Duration histograms for every owning stage.
    stage_seconds: BTreeMap<ForgeMetricStage, Histogram>,
    /// Committed/recovered replacement input-file counters.
    rewrite_input_files: BTreeMap<ForgeMetricSource, Counter>,
    /// Committed/recovered replacement input-byte counters.
    rewrite_input_bytes: BTreeMap<ForgeMetricSource, Counter>,
    /// Committed/recovered replacement output-file counters.
    rewrite_output_files: BTreeMap<ForgeMetricSource, Counter>,
    /// Committed/recovered replacement output-byte counters.
    rewrite_output_bytes: BTreeMap<ForgeMetricSource, Counter>,
    /// One terminal duration series for each closed strategy/result pair.
    task_duration: BTreeMap<(ForgeTaskMetricStrategy, ForgeTaskTerminalResult), Histogram>,
    /// Final spill observations partitioned by closed Forge strategy.
    task_spill: BTreeMap<ForgeTaskMetricStrategy, Histogram>,
    /// Conflict counters emitted exactly at durable rejection boundaries.
    conflicts: BTreeMap<ForgeConflictKind, Counter>,
    /// Completed cleanup obligation duration by closed cleanup provenance.
    cleanup_duration: BTreeMap<ForgeCleanupKind, Histogram>,
    /// Scheduler pass duration retained by the injected production handle.
    scheduler_duration: Histogram,
}

impl ForgeTelemetry {
    /// Registers the complete fixed-cardinality Forge metric inventory.
    #[must_use]
    pub fn new() -> Self {
        Self {
            failure_classes: failure_class_counters("bifrost_forge_task_failures_total"),
            attempt_resource_bytes: attempt_resource_histograms(),
            attempt_resource_releases: attempt_resource_release_counters(),
            execution_envelope_failures: attempt_resource_counters(),
            legacy_envelopes_superseded: ["replanned", "failed"]
                .into_iter()
                .map(|result| {
                    (
                        result,
                        metrics::counter!(
                            "bifrost_forge_legacy_envelopes_superseded_total",
                            "result" => result
                        ),
                    )
                })
                .collect(),
            retries: failure_class_counters("bifrost_forge_retries_total"),
            terminal_poisons: failure_class_counters("bifrost_forge_terminal_poisons_total"),
            capacity_refusals: capacity_refusal_counters(),
            compaction_debt: compaction_debt_gauges(),
            progress_effects: progress_effect_counters(),
            quarantine_state: metrics::gauge!("bifrost_forge_worker_quarantined"),
            demand_transitions: demand_transition_counters(),
            discovered_candidate_files: strategy_histograms(
                "bifrost_forge_discovered_candidate_files",
            ),
            discovered_candidate_bytes: strategy_histograms(
                "bifrost_forge_discovered_candidate_bytes",
            ),
            hint_persistence: ForgeHintPersistenceResult::ALL
                .into_iter()
                .map(|result| {
                    (
                        result,
                        metrics::counter!(
                            "bifrost_forge_hint_persistence_total",
                            "result" => result.as_str()
                        ),
                    )
                })
                .collect(),
            hint_persistence_seconds: ForgeHintPersistenceResult::ALL
                .into_iter()
                .map(|result| {
                    (
                        result,
                        metrics::histogram!(
                            "bifrost_forge_hint_persistence_seconds",
                            "result" => result.as_str()
                        ),
                    )
                })
                .collect(),
            oldest_backlog: metrics::gauge!("bifrost_forge_oldest_backlog_seconds"),
            fairness_lag: metrics::gauge!("bifrost_forge_fairness_lag_tasks"),
            complete_gauge_publications: metrics::counter!(
                "bifrost_forge_complete_gauge_publications_total"
            ),
            operations: operation_counters(),
            lease_events: lease_counters(),
            stage_failures: stage_failure_counters(),
            stage_seconds: stage_histograms(),
            rewrite_input_files: source_counters("bifrost_forge_rewrite_input_files_total"),
            rewrite_input_bytes: source_counters("bifrost_forge_rewrite_input_bytes_total"),
            rewrite_output_files: source_counters("bifrost_forge_rewrite_output_files_total"),
            rewrite_output_bytes: source_counters("bifrost_forge_rewrite_output_bytes_total"),
            task_duration: task_duration_histograms(),
            task_spill: strategy_histograms("bifrost_forge_task_spill_bytes"),
            conflicts: conflict_counters(),
            cleanup_duration: cleanup_histograms(),
            scheduler_duration: metrics::histogram!("bifrost_forge_scheduler_duration_seconds"),
        }
    }

    /// Record one completed durable hint-persistence attempt.
    pub(super) fn record_hint_persistence(
        &self,
        result: ForgeHintPersistenceResult,
        elapsed: Duration,
    ) {
        self.hint_persistence[&result].increment(1);
        self.hint_persistence_seconds[&result].record(elapsed.as_secs_f64());
    }

    /// Record one completed planning-demand transition.
    pub(super) fn record_demand_transition(&self, result: ForgeDemandTransitionResult) {
        self.demand_transitions[&result].increment(1);
    }

    /// Record one deterministic candidate at the production discovery boundary.
    pub(super) fn record_discovered_candidate(
        &self,
        strategy: ForgeTaskMetricStrategy,
        files: usize,
        bytes: u64,
    ) {
        self.discovered_candidate_files[&strategy].record(files.to_f64().unwrap_or(f64::MAX));
        self.discovered_candidate_bytes[&strategy].record(bytes.to_f64().unwrap_or(f64::MAX));
    }

    /// Record one finished scheduler pass through the production metric owner.
    ///
    /// Complete-only gauges remain published at the scheduler's authoritative
    /// roster-status query rather than inferred from this aggregate outcome.
    pub(crate) fn record_scheduler_pass(&self, _outcome: &ForgeScheduleOutcome, elapsed: Duration) {
        self.scheduler_duration.record(elapsed.as_secs_f64());
    }

    /// Records one terminal worker attempt through its typed label inventory.
    pub(super) fn record_task_terminal(
        &self,
        strategy: ForgeTaskMetricStrategy,
        result: ForgeTaskTerminalResult,
        elapsed: Duration,
    ) {
        self.task_duration[&(strategy, result)].record(elapsed.as_secs_f64());
    }

    /// Record final `DataFusion` spill accounting for one completed Forge attempt.
    pub(super) fn record_task_spill(&self, strategy: ForgeTaskMetricStrategy, bytes: u64) {
        self.task_spill[&strategy].record(bytes.to_f64().unwrap_or(f64::MAX));
    }

    /// Record one exact durable conflict before its caller returns the rejection.
    pub(super) fn record_conflict(&self, kind: ForgeConflictKind) {
        self.conflicts[&kind].increment(1);
    }

    /// Record completion of one durable cleanup obligation.
    pub(super) fn record_cleanup(&self, kind: ForgeCleanupKind, elapsed: Duration) {
        self.cleanup_duration[&kind].record(elapsed.as_secs_f64());
    }

    /// Records one completed orphan-GC run through its closed label inventory.
    ///
    /// Emits the run's wall-clock duration into
    /// `bifrost_forge_cleanup_duration_seconds{kind="orphan"}`, its deleted and
    /// retained counts into the staging operation counters, and — when a per-run
    /// bound ended the run before its candidate set was exhausted — one `Budget`
    /// staging operation so a Partial run is observable. Counts route through the
    /// existing operation surface with no per-tenant, per-table, or per-path
    /// label. The duration is recorded unconditionally so an empty run still
    /// advances the cleanup series; the count increments are no-ops when zero.
    pub(super) fn record_orphan_gc(&self, outcome: &OrphanGcOutcome, elapsed: Duration) {
        self.record_cleanup(ForgeCleanupKind::Orphan, elapsed);
        self.record_operation(
            ForgeMetricSource::Staging,
            ForgeOperationResult::Committed,
            outcome.deleted,
        );
        self.record_operation(
            ForgeMetricSource::Staging,
            ForgeOperationResult::Noop,
            outcome.skipped,
        );
        if outcome.partial {
            self.record_operation(ForgeMetricSource::Staging, ForgeOperationResult::Budget, 1);
        }
    }

    /// Publish the complete scheduler-owned backlog and fairness state.
    ///
    /// This boundary is called only after the scheduler has read a complete
    /// durable status page, so neither gauge can expose a partial roster.
    pub(crate) fn record_planning_status(&self, backlog_age: Duration, fairness_lag: usize) {
        self.oldest_backlog.set(backlog_age.as_secs_f64());
        self.fairness_lag
            .set(fairness_lag.to_f64().unwrap_or(f64::MAX));
        self.complete_gauge_publications.increment(1);
    }

    /// Construct the shared closed-schema span for one catalog commit future.
    ///
    /// Only the strategy, result, role, and scrubbed durable task UUIDs are
    /// owner-authored. Tenant, table, SQL, object-path, and error details must
    /// never be added by callers.
    pub(super) fn catalog_commit_span(
        strategy: ForgeCatalogCommitStrategy,
        task_identity: Option<(Uuid, Uuid)>,
    ) -> tracing::Span {
        let span = tracing::info_span!(
            "bifrost.forge.catalog.commit",
            strategy = strategy.as_str(),
            result = tracing::field::Empty,
            role = "forge_worker",
            task_id = tracing::field::Empty,
            attempt_id = tracing::field::Empty,
        );
        if let Some((task_id, attempt_id)) = task_identity {
            span.record("task_id", tracing::field::display(task_id));
            span.record("attempt_id", tracing::field::display(attempt_id));
        }
        span
    }

    /// Records one typed terminal operation event.
    pub(super) fn record_operation(
        &self,
        source: ForgeMetricSource,
        result: ForgeOperationResult,
        count: usize,
    ) {
        self.operations[&(source, result)].increment(count as u64);
    }

    /// Records one typed lease-boundary event.
    pub(super) fn record_lease(&self, result: ForgeLeaseResult) {
        self.lease_events[&result].increment(1);
    }

    /// Records one owning-stage return and whether it failed.
    pub(super) fn record_stage(&self, stage: ForgeMetricStage, elapsed: Duration, failed: bool) {
        self.stage_seconds[&stage].record(elapsed.as_secs_f64());
        if failed {
            self.stage_failures[&stage].increment(1);
        }
    }

    /// Records committed replacement volume from an authoritative outcome.
    pub(super) fn record_rewrite_volume(
        &self,
        source: ForgeMetricSource,
        input_files: usize,
        input_bytes: u64,
        output_files: usize,
        output_bytes: u64,
    ) {
        self.rewrite_input_files[&source].increment(input_files as u64);
        self.rewrite_input_bytes[&source].increment(input_bytes);
        self.rewrite_output_files[&source].increment(output_files as u64);
        self.rewrite_output_bytes[&source].increment(output_bytes);
    }

    /// Records one closed durable failure classification at settlement.
    pub(super) fn record_failure_class(&self, class: super::error::ForgeFailureClass) {
        self.failure_classes[&class].increment(1);
    }

    /// Records one complete attempt resource envelope at its sole finalizer.
    pub(crate) fn record_attempt_resources(&self, observation: ForgeResourceObservation) {
        for (resource, planned, acquired, peak) in [
            (
                ForgeAttemptResource::Memory,
                observation.planned_memory,
                observation.acquired_memory,
                observation.peak_memory,
            ),
            (
                ForgeAttemptResource::Scratch,
                observation.planned_scratch,
                observation.acquired_scratch,
                observation.peak_scratch,
            ),
        ] {
            for (kind, bytes) in [
                (ForgeResourceObservationKind::Planned, planned),
                (ForgeResourceObservationKind::Acquired, acquired),
                (ForgeResourceObservationKind::Peak, peak),
            ] {
                self.attempt_resource_bytes[&(resource, kind)]
                    .record(bytes.to_f64().unwrap_or(f64::MAX));
            }
        }
    }

    /// Records the exactly-once release result for both resources in one lease.
    pub(crate) fn record_attempt_release(
        &self,
        result: crate::resources::ForgeResourceReleaseResult,
    ) {
        let result = match result {
            crate::resources::ForgeResourceReleaseResult::Released => "released",
            crate::resources::ForgeResourceReleaseResult::Poisoned => "poisoned",
        };
        for resource in ForgeAttemptResource::ALL {
            self.attempt_resource_releases[&(resource, result)].increment(1);
        }
    }

    /// Records one typed capacity refusal at its actual phase boundary.
    pub(super) fn record_capacity_refusal(&self, phase: ForgeCapacityRefusalPhase) {
        self.capacity_refusals[&phase].increment(1);
    }

    /// Records an admitted execution term exhausting its persisted envelope.
    pub(super) fn record_execution_envelope_failure(&self, resource: ForgeAttemptResource) {
        self.execution_envelope_failures[&resource].increment(1);
    }

    /// Records whether audited legacy settlement created replacement demand.
    pub(super) fn record_legacy_supersession(&self, replanned: bool) {
        self.legacy_envelopes_superseded[if replanned { "replanned" } else { "failed" }]
            .increment(1);
    }

    /// Records one retry selected by the closed durable settlement policy.
    pub(super) fn record_retry(&self, class: super::error::ForgeFailureClass) {
        self.retries[&class].increment(1);
    }

    /// Records one terminal failure selected by the closed durable settlement policy.
    pub(super) fn record_terminal_poison(&self, class: super::error::ForgeFailureClass) {
        self.terminal_poisons[&class].increment(1);
    }

    /// Publishes the complete current compaction debt snapshot.
    pub(super) fn record_compaction_debt(&self, files: u64, bytes: u64) {
        self.compaction_debt["files"].set(files.to_f64().unwrap_or(f64::MAX));
        self.compaction_debt["bytes"].set(bytes.to_f64().unwrap_or(f64::MAX));
    }

    /// Records one successful durable progress effect.
    pub(super) fn record_progress_effect(&self, effect: ForgeProgressEffect) {
        self.progress_effects[&effect].increment(1);
    }

    /// Publishes whether the local worker is durably quarantined.
    pub(super) fn record_quarantine_state(&self, quarantined: bool) {
        self.quarantine_state
            .set(if quarantined { 1.0 } else { 0.0 });
    }
}

/// Registers resource observations for every resource and lifecycle point.
fn attempt_resource_histograms()
-> BTreeMap<(ForgeAttemptResource, ForgeResourceObservationKind), Histogram> {
    let mut histograms = BTreeMap::new();
    for resource in ForgeAttemptResource::ALL {
        for observation in ForgeResourceObservationKind::ALL {
            histograms.insert(
                (resource, observation),
                metrics::histogram!(
                    "bifrost_forge_attempt_resource_bytes",
                    "resource" => resource.as_str(),
                    "observation" => observation.as_str()
                ),
            );
        }
    }
    histograms
}

/// Registers exactly-once release counters for every resource and result.
fn attempt_resource_release_counters() -> BTreeMap<(ForgeAttemptResource, &'static str), Counter> {
    let mut counters = BTreeMap::new();
    for resource in ForgeAttemptResource::ALL {
        for result in [
            crate::resources::ForgeResourceReleaseResult::Released,
            crate::resources::ForgeResourceReleaseResult::Poisoned,
        ] {
            let label = match result {
                crate::resources::ForgeResourceReleaseResult::Released => "released",
                crate::resources::ForgeResourceReleaseResult::Poisoned => "poisoned",
            };
            counters.insert(
                (resource, label),
                metrics::counter!(
                    "bifrost_forge_attempt_resource_releases_total",
                    "resource" => resource.as_str(),
                    "result" => label
                ),
            );
        }
    }
    counters
}

/// Registers one counter for each durable failure class without string fallback.
fn failure_class_counters(
    name: &'static str,
) -> BTreeMap<super::error::ForgeFailureClass, Counter> {
    [
        super::error::ForgeFailureClass::DataRefusal,
        super::error::ForgeFailureClass::TransientObjectStore,
        super::error::ForgeFailureClass::TransientCoordination,
        super::error::ForgeFailureClass::StorageHealth,
        super::error::ForgeFailureClass::CapacityRefused,
        super::error::ForgeFailureClass::InternalInvariant,
    ]
    .into_iter()
    .map(|class| {
        (
            class,
            metrics::counter!(name, "failure_class" => class.as_str()),
        )
    })
    .collect()
}

/// Registers execution-envelope counters for every attempt resource.
fn attempt_resource_counters() -> BTreeMap<ForgeAttemptResource, Counter> {
    ForgeAttemptResource::ALL.into_iter().map(|resource| (resource, metrics::counter!("bifrost_forge_execution_envelope_failures_total", "resource" => resource.as_str()))).collect()
}

/// Registers capacity-refusal counters for every admission phase.
fn capacity_refusal_counters() -> BTreeMap<ForgeCapacityRefusalPhase, Counter> {
    ForgeCapacityRefusalPhase::ALL.into_iter().map(|phase| (phase, metrics::counter!("bifrost_forge_capacity_refusals_total", "phase" => phase.as_str()))).collect()
}

/// Registers the two exact live-compaction debt gauges.
fn compaction_debt_gauges() -> BTreeMap<&'static str, Gauge> {
    ["files", "bytes"]
        .into_iter()
        .map(|unit| {
            (
                unit,
                metrics::gauge!("bifrost_forge_compaction_debt", "unit" => unit),
            )
        })
        .collect()
}

/// Registers progress counters for every durable effect classification.
fn progress_effect_counters() -> BTreeMap<ForgeProgressEffect, Counter> {
    ForgeProgressEffect::ALL.into_iter().map(|effect| (effect, metrics::counter!("bifrost_forge_progress_effects_total", "effect" => effect.as_str()))).collect()
}

/// Registers demand-transition counters for every closed transition result.
fn demand_transition_counters() -> BTreeMap<ForgeDemandTransitionResult, Counter> {
    ForgeDemandTransitionResult::ALL.into_iter().map(|result| (result, metrics::counter!("bifrost_forge_demand_transitions_total", "result" => result.as_str()))).collect()
}

impl Default for ForgeTelemetry {
    /// Register the standard production Forge metric inventory.
    fn default() -> Self {
        Self::new()
    }
}

/// Registers every closed strategy/result task-duration series eagerly.
fn task_duration_histograms()
-> BTreeMap<(ForgeTaskMetricStrategy, ForgeTaskTerminalResult), Histogram> {
    let mut histograms = BTreeMap::new();
    for strategy in ForgeTaskMetricStrategy::ALL {
        for result in ForgeTaskTerminalResult::ALL {
            histograms.insert(
                (strategy, result),
                metrics::histogram!(
                    "bifrost_forge_task_duration_seconds",
                    "strategy" => strategy.as_str(),
                    "result" => result.as_str()
                ),
            );
        }
    }
    histograms
}

/// Register one histogram for every strategy accepted by the v1 worker.
fn strategy_histograms(name: &'static str) -> BTreeMap<ForgeTaskMetricStrategy, Histogram> {
    ForgeTaskMetricStrategy::ALL
        .into_iter()
        .map(|strategy| {
            (
                strategy,
                metrics::histogram!(name, "strategy" => strategy.as_str()),
            )
        })
        .collect()
}

/// Register one counter for every stable Forge conflict classification.
fn conflict_counters() -> BTreeMap<ForgeConflictKind, Counter> {
    ForgeConflictKind::ALL
        .into_iter()
        .map(|kind| {
            (
                kind,
                metrics::counter!("bifrost_forge_conflicts_total", "kind" => kind.as_str()),
            )
        })
        .collect()
}

/// Register cleanup duration histograms for the closed cleanup provenance set.
fn cleanup_histograms() -> BTreeMap<ForgeCleanupKind, Histogram> {
    ForgeCleanupKind::ALL
        .into_iter()
        .map(|kind| {
            (
                kind,
                metrics::histogram!("bifrost_forge_cleanup_duration_seconds", "kind" => kind.as_str()),
            )
        })
        .collect()
}

impl Forge {
    /// Measures exact durable object volume before appending a recovery terminal.
    ///
    /// Reading metadata before the terminal audit makes replay deduplication
    /// exact: a successful terminal append removes the operation from the open
    /// reconciliation page, while a metadata failure leaves it retryable.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when a staging or catalog path is
    /// malformed, foreign, or outside its tenant/table binding; an object-store
    /// error when metadata stat fails; or [`ForgeError::InvalidConfig`] when
    /// byte accumulation overflows.
    pub(super) async fn measure_rewrite_volume(
        &self,
        contract: RewritePathContract<'_>,
        input_paths: &[StoragePath],
        output_paths: &[StoragePath],
    ) -> Result<ForgeRewriteVolume, ForgeError> {
        let input_bytes = self.measure_path_bytes(contract, input_paths).await?;
        let output_bytes = self.measure_path_bytes(contract, output_paths).await?;
        Ok(ForgeRewriteVolume {
            input_files: input_paths.len(),
            input_bytes,
            output_files: output_paths.len(),
            output_bytes,
        })
    }

    /// Sums exact object sizes for one validated path set.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when a staging or catalog path is
    /// malformed, foreign, or outside its tenant/table binding; an object-store
    /// error when metadata stat fails; or [`ForgeError::InvalidConfig`] when
    /// the total exceeds `u64`.
    async fn measure_path_bytes(
        &self,
        contract: RewritePathContract<'_>,
        paths: &[StoragePath],
    ) -> Result<u64, ForgeError> {
        let mut bytes = 0_u64;
        for path in paths {
            let object_key = contract.object_key(&self.core.staging, path.as_str())?;
            let size = self
                .core
                .object_store
                .stat(&object_key)
                .await
                .map_err(ForgeError::ObjectStore)?
                .content_length();
            bytes = bytes
                .checked_add(size)
                .ok_or_else(|| ForgeError::InvalidConfig {
                    detail: "recovered rewrite volume exceeds u64".to_owned(),
                })?;
        }
        Ok(bytes)
    }
}

/// Production-owner telemetry sensitivity tests.
mod tests {
    #[cfg(test)]
    use std::collections::BTreeMap;
    #[cfg(test)]
    use std::sync::Arc;
    #[cfg(test)]
    use std::time::Duration;

    #[cfg(test)]
    use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool};
    #[cfg(test)]
    use vala_sql::row_types::forge_tasks::{ForgeTaskState, ForgeTaskStrategy};
    #[cfg(test)]
    use wyrd_bench::{BenchmarkMetricSnapshot, BenchmarkRecorder};

    #[cfg(test)]
    use super::{
        ForgeAttemptResource, ForgeCapacityRefusalPhase, ForgeCleanupKind, ForgeConflictKind,
        ForgeHintPersistenceResult, ForgeMetricSource, ForgeProgressEffect,
        ForgeResourceObservation, ForgeResourceObservationKind, ForgeTaskMetricStrategy,
        ForgeTaskTerminalResult, ForgeTelemetry, OrphanGcOutcome,
    };
    #[cfg(test)]
    use datafusion::execution::memory_pool::GreedyMemoryPool;

    /// Report families whose values are projected by the Forge maintenance report.
    #[cfg(test)]
    const REPORT_FAMILIES: [&str; 10] = [
        "bifrost_forge_rewrite_output_bytes_total",
        "bifrost_forge_task_duration_seconds",
        "bifrost_forge_oldest_backlog_seconds",
        "bifrost_resource_current_bytes",
        "bifrost_forge_task_spill_bytes",
        "bifrost_forge_conflicts_total",
        "bifrost_forge_fairness_lag_tasks",
        "bifrost_forge_cleanup_duration_seconds",
        "bifrost_forge_role_processes",
        "bifrost_forge_role_process_started_total",
    ];

    /// Return whether one report family changed between recorder snapshots.
    #[cfg(test)]
    fn family_changed(
        before: &BenchmarkMetricSnapshot,
        after: &BenchmarkMetricSnapshot,
        family: &str,
    ) -> bool {
        let counters = |snapshot: &BenchmarkMetricSnapshot| {
            snapshot
                .counters
                .iter()
                .filter(|(name, _)| {
                    name.as_str() == family || name.starts_with(&format!("{family}{{"))
                })
                .map(|(name, value)| (name.clone(), *value))
                .collect::<BTreeMap<_, _>>()
        };
        let gauges = |snapshot: &BenchmarkMetricSnapshot| {
            snapshot
                .gauges
                .iter()
                .filter(|(name, _)| {
                    name.as_str() == family || name.starts_with(&format!("{family}{{"))
                })
                .map(|(name, value)| (name.clone(), *value))
                .collect::<BTreeMap<_, _>>()
        };
        let histograms = |snapshot: &BenchmarkMetricSnapshot| {
            snapshot
                .histograms
                .iter()
                .filter(|(name, _)| {
                    name.as_str() == family || name.starts_with(&format!("{family}{{"))
                })
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        };
        counters(before) != counters(after)
            || gauges(before) != gauges(after)
            || histograms(before) != histograms(after)
    }

    /// Assert one authoritative owner transition changes only its mapped report family.
    #[cfg(test)]
    fn assert_owner_transition(
        recorder: &BenchmarkRecorder,
        expected: &str,
        transition: impl FnOnce(),
    ) {
        let before = recorder.snapshot();
        transition();
        let after = recorder.snapshot();
        let changed = REPORT_FAMILIES
            .into_iter()
            .filter(|family| family_changed(&before, &after, family))
            .collect::<Vec<_>>();
        assert_eq!(changed, vec![expected]);
    }

    /// Proves an orphan-GC outcome routes its counts and cleanup duration through metrics.
    ///
    /// A Complete outcome moves its deleted count onto the `committed` staging
    /// operation series, its retained count onto `noop`, no `budget` movement,
    /// and advances the orphan cleanup-duration family. A Partial outcome moves
    /// its deleted count onto `committed` and emits exactly one `budget` staging
    /// operation, so a bounded stop is observable without any per-path label.
    ///
    /// # Panics
    ///
    /// Panics when any staging-operation delta or the cleanup-duration family
    /// movement does not match the outcome the telemetry owner was handed.
    #[cfg(test)]
    fn assert_orphan_gc_operation_emission(
        recorder: &BenchmarkRecorder,
        telemetry: &ForgeTelemetry,
    ) {
        let staging_operations = |snapshot: &BenchmarkMetricSnapshot, result_label: &str| {
            snapshot
                .counters
                .iter()
                .filter(|(name, _)| {
                    name.starts_with("bifrost_forge_operations{")
                        && name.contains("source=\"staging\"")
                        && name.contains(&format!("result=\"{result_label}\""))
                })
                .map(|(_, value)| *value)
                .sum::<u64>()
        };

        let before_complete = recorder.snapshot();
        telemetry.record_orphan_gc(
            &OrphanGcOutcome {
                deleted: 3,
                skipped: 2,
                partial: false,
                ..OrphanGcOutcome::default()
            },
            Duration::from_millis(5),
        );
        let after_complete = recorder.snapshot();
        assert_eq!(
            staging_operations(&after_complete, "committed")
                - staging_operations(&before_complete, "committed"),
            3
        );
        assert_eq!(
            staging_operations(&after_complete, "noop")
                - staging_operations(&before_complete, "noop"),
            2
        );
        assert_eq!(
            staging_operations(&after_complete, "budget")
                - staging_operations(&before_complete, "budget"),
            0
        );
        assert!(family_changed(
            &before_complete,
            &after_complete,
            "bifrost_forge_cleanup_duration_seconds"
        ));

        let before_partial = recorder.snapshot();
        telemetry.record_orphan_gc(
            &OrphanGcOutcome {
                deleted: 1,
                skipped: 0,
                partial: true,
                ..OrphanGcOutcome::default()
            },
            Duration::from_millis(5),
        );
        let after_partial = recorder.snapshot();
        assert_eq!(
            staging_operations(&after_partial, "committed")
                - staging_operations(&before_partial, "committed"),
            1
        );
        assert_eq!(
            staging_operations(&after_partial, "budget")
                - staging_operations(&before_partial, "budget"),
            1
        );
    }

    /// Asserts each Forge metric family emitted exactly its full label product.
    ///
    /// The caller has just driven every strategy, result, conflict, cleanup, and
    /// resource variant once, so each family's series count must equal the
    /// product of the enums that label it. A count below the product means a
    /// variant emits nothing; above it means a label escaped its closed set.
    ///
    /// # Panics
    ///
    /// Panics if any family's observed series count differs from its expected
    /// label product.
    #[cfg(test)]
    fn assert_metric_label_cardinality(recorder: &BenchmarkRecorder) {
        let snapshot = recorder.snapshot();
        let task_durations = snapshot
            .histograms
            .keys()
            .filter(|name| name.starts_with("bifrost_forge_task_duration_seconds{"))
            .count();
        let spills = snapshot
            .histograms
            .keys()
            .filter(|name| name.starts_with("bifrost_forge_task_spill_bytes{"))
            .count();
        let cleanups = snapshot
            .histograms
            .keys()
            .filter(|name| name.starts_with("bifrost_forge_cleanup_duration_seconds{"))
            .count();
        let conflicts = snapshot
            .counters
            .keys()
            .filter(|name| name.starts_with("bifrost_forge_conflicts_total{"))
            .count();
        let resource_observations = snapshot
            .histograms
            .keys()
            .filter(|name| name.starts_with("bifrost_forge_attempt_resource_bytes{"))
            .count();
        let resource_releases = snapshot
            .counters
            .keys()
            .filter(|name| name.starts_with("bifrost_forge_attempt_resource_releases_total{"))
            .count();
        assert_eq!(
            task_durations,
            ForgeTaskMetricStrategy::ALL.len() * ForgeTaskTerminalResult::ALL.len()
        );
        assert_eq!(spills, ForgeTaskMetricStrategy::ALL.len());
        assert_eq!(cleanups, ForgeCleanupKind::ALL.len());
        assert_eq!(conflicts, ForgeConflictKind::ALL.len());
        assert_eq!(
            resource_observations,
            ForgeAttemptResource::ALL.len() * ForgeResourceObservationKind::ALL.len()
        );
        assert_eq!(resource_releases, ForgeAttemptResource::ALL.len() * 2);
    }

    /// Asserts each owner-emitted family moves when its owner records again.
    ///
    /// Cardinality alone cannot distinguish a live metric from one that emitted
    /// once and froze, so every family is driven a second time through its real
    /// owner and required to change. Each conflict kind is checked individually
    /// because they share one counter family and a single mis-wired kind would
    /// otherwise hide behind its siblings.
    ///
    /// # Panics
    ///
    /// Panics if any family does not change when its owner records again.
    #[cfg(test)]
    fn assert_owner_transitions(recorder: &BenchmarkRecorder, telemetry: &ForgeTelemetry) {
        assert_owner_transition(recorder, "bifrost_forge_rewrite_output_bytes_total", || {
            telemetry.record_rewrite_volume(ForgeMetricSource::Staging, 1, 64, 1, 32);
        });
        assert_owner_transition(recorder, "bifrost_forge_task_duration_seconds", || {
            telemetry.record_task_terminal(
                ForgeTaskMetricStrategy::SmallFiles,
                ForgeTaskTerminalResult::Succeeded,
                Duration::from_millis(2),
            );
        });
        assert_owner_transition(recorder, "bifrost_forge_oldest_backlog_seconds", || {
            telemetry.record_planning_status(Duration::from_secs(2), 1);
        });
        assert_owner_transition(recorder, "bifrost_forge_fairness_lag_tasks", || {
            telemetry.record_planning_status(Duration::from_secs(2), 2);
        });
        assert_owner_transition(recorder, "bifrost_forge_task_spill_bytes", || {
            telemetry.record_task_spill(ForgeTaskMetricStrategy::SmallFiles, 4096);
        });
        for kind in ForgeConflictKind::ALL {
            assert_owner_transition(recorder, "bifrost_forge_conflicts_total", || {
                telemetry.record_conflict(kind);
            });
        }
        assert_owner_transition(recorder, "bifrost_forge_cleanup_duration_seconds", || {
            telemetry.record_cleanup(ForgeCleanupKind::Expired, Duration::from_millis(3));
        });
    }

    /// Drive each real telemetry owner boundary and pin independent report sensitivity.
    #[test]
    fn forge_production_telemetry_contract() {
        let recorder = BenchmarkRecorder::new();
        metrics::with_local_recorder(&recorder, || {
            let telemetry = ForgeTelemetry::new();
            telemetry.record_rewrite_volume(ForgeMetricSource::Staging, 0, 0, 0, 0);
            for strategy in ForgeTaskMetricStrategy::ALL {
                for result in ForgeTaskTerminalResult::ALL {
                    telemetry.record_task_terminal(strategy, result, Duration::ZERO);
                }
                telemetry.record_task_spill(strategy, 0);
            }
            for kind in ForgeConflictKind::ALL {
                telemetry.record_conflict(kind);
            }
            for kind in ForgeCleanupKind::ALL {
                telemetry.record_cleanup(kind, Duration::ZERO);
            }
            telemetry.record_planning_status(Duration::from_secs(1), 1);
            telemetry.record_attempt_resources(ForgeResourceObservation {
                planned_memory: 64,
                acquired_memory: 64,
                peak_memory: 32,
                planned_scratch: 128,
                acquired_scratch: 128,
                peak_scratch: 96,
            });
            telemetry
                .record_attempt_release(crate::resources::ForgeResourceReleaseResult::Released);
            telemetry
                .record_attempt_release(crate::resources::ForgeResourceReleaseResult::Poisoned);
            for resource in ForgeAttemptResource::ALL {
                telemetry.record_execution_envelope_failure(resource);
            }
            telemetry.record_legacy_supersession(true);
            telemetry.record_legacy_supersession(false);
            for class in [
                super::super::error::ForgeFailureClass::DataRefusal,
                super::super::error::ForgeFailureClass::TransientObjectStore,
                super::super::error::ForgeFailureClass::TransientCoordination,
                super::super::error::ForgeFailureClass::StorageHealth,
                super::super::error::ForgeFailureClass::CapacityRefused,
                super::super::error::ForgeFailureClass::InternalInvariant,
            ] {
                telemetry.record_retry(class);
                telemetry.record_terminal_poison(class);
            }
            telemetry.record_capacity_refusal(ForgeCapacityRefusalPhase::Admission);
            telemetry.record_capacity_refusal(ForgeCapacityRefusalPhase::Execution);
            telemetry.record_compaction_debt(7, 4096);
            telemetry.record_progress_effect(ForgeProgressEffect::Changed);
            telemetry.record_progress_effect(ForgeProgressEffect::AcknowledgedNoop);
            let pool: Arc<dyn MemoryPool> = Arc::new(GreedyMemoryPool::new(1024 * 1024 * 1024));
            let reservation = MemoryConsumer::new("forge-owner-proof").register(&pool);
            reservation.try_grow(0).expect("zero-sized Forge reserve");

            assert_metric_label_cardinality(&recorder);

            assert_owner_transitions(&recorder, &telemetry);

            assert_orphan_gc_operation_emission(&recorder, &telemetry);

            reservation.try_grow(4096).expect("Forge memory reserve");
            assert!(recorder.snapshot().gauges.keys().all(|family| {
                !family.starts_with("bifrost_memory_reserved_bytes")
                    && !family.starts_with("bifrost_memory_limit_bytes")
            }));
        });
    }

    /// Hint persistence exposes one counter and duration observation per outcome.
    #[test]
    fn forge_hint_persistence_records_success_and_failure() {
        let recorder = BenchmarkRecorder::new();
        metrics::with_local_recorder(&recorder, || {
            let telemetry = ForgeTelemetry::new();
            telemetry.record_hint_persistence(
                ForgeHintPersistenceResult::Succeeded,
                Duration::from_millis(2),
            );
            telemetry.record_hint_persistence(
                ForgeHintPersistenceResult::Failed,
                Duration::from_millis(3),
            );
        });
        let snapshot = recorder.snapshot();
        for result in ["succeeded", "failed"] {
            assert_eq!(
                snapshot.counters.get(&format!(
                    "bifrost_forge_hint_persistence_total{{result=\"{result}\"}}"
                )),
                Some(&1)
            );
            assert_eq!(
                snapshot
                    .histograms
                    .get(&format!(
                        "bifrost_forge_hint_persistence_seconds{{result=\"{result}\"}}"
                    ))
                    .map(|values| values.count),
                Some(1)
            );
        }
    }

    /// Maps every durable Forge strategy and task state without a string fallback.
    #[test]
    fn forge_durable_labels_are_exhaustive() {
        assert_eq!(
            ForgeTaskTerminalResult::ALL.map(ForgeTaskTerminalResult::as_str),
            [
                "succeeded",
                "retryable",
                "failed",
                "cancelled",
                "unschedulable",
            ]
        );
        assert_eq!(
            ForgeTaskMetricStrategy::try_from(ForgeTaskStrategy::SmallFiles),
            Ok(ForgeTaskMetricStrategy::SmallFiles)
        );
        assert_eq!(
            ForgeTaskMetricStrategy::try_from(ForgeTaskStrategy::ManifestRewrite),
            Ok(ForgeTaskMetricStrategy::ManifestRewrite)
        );
        assert_eq!(
            ForgeTaskMetricStrategy::try_from(ForgeTaskStrategy::SnapshotExpiry),
            Ok(ForgeTaskMetricStrategy::SnapshotExpiry)
        );
        for strategy in [
            ForgeTaskStrategy::FullIdentity,
            ForgeTaskStrategy::ExpiredCleanup,
            ForgeTaskStrategy::OrphanCleanup,
        ] {
            assert_eq!(ForgeTaskMetricStrategy::try_from(strategy), Err(strategy));
        }
        for (state, result) in [
            (
                ForgeTaskState::Succeeded,
                ForgeTaskTerminalResult::Succeeded,
            ),
            (
                ForgeTaskState::Retryable,
                ForgeTaskTerminalResult::Retryable,
            ),
            (ForgeTaskState::Failed, ForgeTaskTerminalResult::Failed),
            (
                ForgeTaskState::Cancelled,
                ForgeTaskTerminalResult::Cancelled,
            ),
            (
                ForgeTaskState::Unschedulable,
                ForgeTaskTerminalResult::Unschedulable,
            ),
        ] {
            assert_eq!(ForgeTaskTerminalResult::try_from(state), Ok(result));
        }
        for state in [
            ForgeTaskState::Ready,
            ForgeTaskState::Claimed,
            ForgeTaskState::Running,
            ForgeTaskState::Prepared,
        ] {
            assert_eq!(ForgeTaskTerminalResult::try_from(state), Err(state));
        }
    }
}

/// Registers every closed source/result operation counter.
fn operation_counters() -> BTreeMap<(ForgeMetricSource, ForgeOperationResult), Counter> {
    let mut counters = BTreeMap::new();
    for source in ForgeMetricSource::ALL {
        for result in ForgeOperationResult::ALL {
            counters.insert(
                (source, result),
                metrics::counter!(
                    "bifrost_forge_operations",
                    "source" => source.as_str(),
                    "result" => result.as_str()
                ),
            );
        }
    }
    counters
}

/// Registers every closed lease-result counter.
fn lease_counters() -> BTreeMap<ForgeLeaseResult, Counter> {
    ForgeLeaseResult::ALL
        .into_iter()
        .map(|result| {
            (
                result,
                metrics::counter!(
                    "bifrost_forge_lease_events_total",
                    "result" => result.as_str()
                ),
            )
        })
        .collect()
}

/// Registers failure counters for every closed owning stage.
fn stage_failure_counters() -> BTreeMap<ForgeMetricStage, Counter> {
    ForgeMetricStage::ALL
        .into_iter()
        .map(|stage| {
            (
                stage,
                metrics::counter!(
                    "bifrost_forge_stage_failures_total",
                    "stage" => stage.as_str()
                ),
            )
        })
        .collect()
}

/// Registers duration histograms for every closed owning stage.
fn stage_histograms() -> BTreeMap<ForgeMetricStage, Histogram> {
    ForgeMetricStage::ALL
        .into_iter()
        .map(|stage| {
            (
                stage,
                metrics::histogram!(
                    "bifrost_forge_stage_seconds",
                    "stage" => stage.as_str()
                ),
            )
        })
        .collect()
}

/// Registers one counter handle for each closed source value.
fn source_counters(name: &'static str) -> BTreeMap<ForgeMetricSource, Counter> {
    ForgeMetricSource::ALL
        .into_iter()
        .map(|source| (source, metrics::counter!(name, "source" => source.as_str())))
        .collect()
}
