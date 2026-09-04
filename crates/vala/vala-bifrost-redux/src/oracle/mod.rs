//! Local Oracle query owner and deterministic planning primitives.
//!
//! The module keeps query-floor validation, admission classification, and
//! source reconciliation in the Redux crate so later serving adapters cannot
//! bypass the engine's invariants.  IO-backed execution is intentionally
//! composed around these small owners.
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use arrow::array::Array;
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use datafusion::catalog::{CatalogProvider, MemoryCatalogProvider, MemorySchemaProvider};
use datafusion::common::TableReference;
use datafusion::dataframe::DataFrame;
use datafusion::datasource::MemTable;
use datafusion::datasource::TableProvider;
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::{ExecutionPlan, SendableRecordBatchStream, execute_stream};
use datafusion::sql::parser::{DFParser, Statement as DfStatement};
use datafusion::sql::resolve::resolve_table_references;
use datafusion::sql::sqlparser::ast::Statement as SqlStatement;
use futures_util::{Stream, StreamExt};
use sha2::{Digest as _, Sha256};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use vala_sql::ValaPostgres;
use wyrd_runtime::Principal;
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError;
#[cfg(feature = "test-support")]
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult};
use wyrd_spec::vala::api::{
    AuditDetail, AuthMethod, BifrostQueryRequest, BifrostSecurityPhase,
    BifrostSecurityViolationKind, NodeId, PersistedFileDescriptor, QueryAuditDigest,
    QueryBatchFrame, QueryClass, QueryExecutionMode, QueryExecutionPath, QueryFreshness, QueryId,
    QuerySchemaFrame, QuerySource, QueryStreamFrame, QueryTerminalErrorCode, QueryTerminalFrame,
    QueryTerminalOutcome, SourceCompletion, SourceCompletionOutcome, VisibilityMode,
};

use crate::catalog::{BifrostCatalog, BifrostCatalogError, PinnedSealedTable, TableRef};
use crate::cluster::{ClusterRegistry, ClusterSnapshot, RegisteredRole};
use crate::schema::SchemaFingerprint;
use crate::scribe::tail_rpc::{TAIL_PROTOCOL_VERSION, TailReadTransport};

mod admission;
pub mod analytical;
pub mod analytical_scan;
pub mod analytical_supervisor;
pub mod analytical_transport;
pub mod attempt;
mod bindings;
pub mod codec;
pub mod dispatcher;
pub(crate) mod exec;

/// Test-only observation of the column closure each Iceberg physical scan is
/// built with.
///
/// Re-exported here because [`exec`] is crate-private while the Oracle journeys
/// that need this evidence live outside the crate. Present only under
/// `test-support`.
#[cfg(feature = "test-support")]
pub use exec::iceberg_projection_probe;
#[cfg(feature = "test-support")]
pub use exec::{remote_partition_attempts_for_test, reset_remote_partition_attempts_for_test};
pub mod follower;
mod ownership;
mod participant_cut;
pub mod peer;
mod planner;
pub(crate) mod pruning;
mod query_stream;
mod running;
mod spill;

/// One stable aggregate-visible degraded partition entry.
#[derive(Debug, Clone)]
pub(super) struct DegradedPartition {
    /// Selected participant ordinal; `u32::MAX` denotes a non-partition live-tail loss.
    pub(super) ordinal: u32,
    /// Closed stable reason label.
    pub(super) reason: &'static str,
    /// Exact source tiers affected by the partition-local condition.
    pub(super) sources: Vec<QuerySource>,
}

/// Query-scoped ordered degradation accumulated by distributed partitions.
pub(super) type DegradedSourceAccumulator = Arc<std::sync::Mutex<Vec<DegradedPartition>>>;
pub use spill::OracleSpillRuntime;

/// Return the process-local query lifecycle observer used by test journeys.
#[cfg(feature = "test-support")]
#[must_use]
pub fn query_lifecycle_observer_for_test() -> std::sync::Arc<query_stream::QueryLifecycleObserver> {
    query_stream::query_lifecycle_observer_for_test()
}
mod tail_fence;
pub mod telemetry;

use telemetry::{
    OracleAdmissionOutcome, OracleAdmissionReason, OracleCancellationReason, OracleQueryClassLabel,
};

use admission::AdmittedQueryGuard;
pub use admission::OracleAdmission;
#[cfg(feature = "test-support")]
pub use admission::{QueryResourceProbe, QueryResourceSnapshot};
pub use exec::TenantTripwireExec;
use exec::{HotFileSource, OracleQueryScanStats, OracleTableInputs, OracleTableProvider};
pub use ownership::{
    DEFAULT_DELEGATED_ALLOCATION_UNITS, DelegatedAdmissionBlock, DelegatedAdmissionRequest,
    DelegatedOracleAdmission, DelegatedOracleAdmissionConfig, DelegatedOracleAdmissionError,
    DelegatedOracleAdmissionGrant, DelegatedOracleAdmissionWorker,
};
pub use participant_cut::{
    OracleQueryAttemptCut, OracleQueryAttemptCutError, OracleQueryAttemptRoster,
    OracleQueryParticipant,
};
pub use planner::OraclePlanner;
pub use query_stream::OracleQueryStream;
pub use query_stream::QueryStreamLifecycle;
pub use query_stream::{
    ORACLE_IPC_FRAMING_SCRATCH_BYTES, QueryIpcDecodeError, QueryIpcDecoder, QueryIpcEncoder,
};
use query_stream::{QueryStreamInput, RunningQueryTerminalOwner};
pub use running::{RunningQueryEntry, RunningQueryRegistry, RunningQuerySettlement};

/// Builds one test stream through the production telemetry terminal owner.
#[cfg(test)]
fn test_query_stream_from_physical(
    telemetry: &Arc<OracleTelemetry>,
    schema: &SchemaRef,
    batches: SendableRecordBatchStream,
    scan_stats: OracleQueryScanStats,
) -> OracleQueryStream {
    query_stream::OracleQueryStream::test_from_physical(telemetry, schema, batches, scan_stats)
}

pub use tail_fence::{DiscoveredTailRoute, TailStreamDiscovery};
use tail_fence::{DrainedTails, TailFenceDrainer, TailFenceDrainerConfig};

/// Default maximum SQL request size accepted by the synchronous query floor.
pub const DEFAULT_MAX_SQL_BYTES: usize = 64 * 1024;
/// Authenticated caller context used by the engine before a server adapter
/// adds transport-specific metadata.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuthorizedQueryContext {
    /// Authenticated principal identity.
    pub principal: Principal,
    /// Tenant selected by authentication and authorization.
    pub data_tenant_id: DataTenantId,
    /// `UUIDv7` correlator retained by the mandatory audit event.
    pub request_id: RequestId,
    /// Distributed trace identifier, when one was verified at the request boundary.
    pub trace_id: Option<String>,
    /// Verified authentication method retained by the mandatory audit event.
    pub auth_method: AuthMethod,
    /// Effective permission checked before the query entered Oracle.
    pub permission: String,
}

/// Builds one valid pinned-Iceberg descriptor for assignment-shape tests.
///
/// Tests that only care about which object paths an assignment carries still
/// need a descriptor that satisfies [`PersistedFileDescriptor::is_valid`], so
/// this fixture supplies the smallest legal identity for `path`: a positive
/// size and snapshot, one row, and absent event-time bounds.
#[cfg(test)]
pub(crate) fn test_persisted_descriptor(path: &str) -> PersistedFileDescriptor {
    PersistedFileDescriptor::Iceberg(wyrd_spec::vala::api::IcebergFileDescriptor {
        path: path.to_owned(),
        size_bytes: 1,
        row_count: 1,
        snapshot_id: 1,
        min_event_time_micros: None,
        max_event_time_micros: None,
    })
}

/// Classifies only parent-contract-eligible live-tail loss as degraded.
#[cfg(test)]
fn follower_source_loss_degrades(
    error: &dispatcher::DispatchError,
    freshness: wyrd_spec::vala::api::FreshnessPolicy,
    sources: &[QuerySource],
) -> bool {
    freshness == wyrd_spec::vala::api::FreshnessPolicy::AllowDegraded
        && matches!(error, dispatcher::DispatchError::EligibleSourceLoss { .. })
        && sources == [QuerySource::LiveTail]
}

/// The one persisted source, and for Iceberg the one pinned snapshot, an
/// assignment's descriptors name.
///
/// A scan id is a leader-chosen label carried on the wire; it is not authority
/// for what a follower opens. The descriptor variant is, because it is what
/// preflight validated and what the assignment-authority digest covers, so it
/// is the only thing either side classifies an assignment by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssignedPersistedSource {
    /// The assignment carries no persisted file and resolves to an empty leaf.
    Empty,
    /// Every descriptor names staged hot Parquet described by `vala.file_list`.
    Hot,
    /// Every descriptor names a data file in one pinned Iceberg snapshot.
    Iceberg {
        /// The snapshot every descriptor in the assignment was pinned to.
        snapshot_id: i64,
    },
}

/// Why an assignment's descriptor list does not name one resolvable source.
///
/// Both refusals exist because the follower resolves the whole list through a
/// single reader: there is no correct leaf for a list that needs two, and
/// picking one would return a signed, digest-clean result missing the other's
/// rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum AssignedSourceRejection {
    /// The list mixes hot and Iceberg descriptors, which are read under
    /// different authority by different readers.
    #[error("assignment names more than one persisted source")]
    MixedSources,
    /// The list pins more than one Iceberg snapshot, so no single snapshot
    /// binding can serve it.
    #[error("assignment names more than one pinned snapshot")]
    MixedSnapshots,
}

impl AssignedPersistedSource {
    /// Classifies one assignment's ordered descriptor list.
    ///
    /// # Errors
    /// Returns [`AssignedSourceRejection`] when the list mixes hot and Iceberg
    /// descriptors, or pins more than one Iceberg snapshot.
    pub(crate) fn classify(
        files: &[PersistedFileDescriptor],
    ) -> Result<Self, AssignedSourceRejection> {
        let mut source = Self::Empty;
        for file in files {
            let observed = match file {
                PersistedFileDescriptor::Hot(_) => Self::Hot,
                PersistedFileDescriptor::Iceberg(iceberg) => Self::Iceberg {
                    snapshot_id: iceberg.snapshot_id,
                },
            };
            match (source, observed) {
                (Self::Empty, _) => source = observed,
                (Self::Hot, Self::Hot) => {}
                (Self::Iceberg { snapshot_id: seen }, Self::Iceberg { snapshot_id })
                    if seen == snapshot_id => {}
                (Self::Iceberg { .. }, Self::Iceberg { .. }) => {
                    return Err(AssignedSourceRejection::MixedSnapshots);
                }
                _ => return Err(AssignedSourceRejection::MixedSources),
            }
        }
        Ok(source)
    }
}

impl AuthorizedQueryContext {
    /// Creates a query context after proving that authorization selected the
    /// same tenant carried by the authenticated principal.
    ///
    /// # Errors
    ///
    /// Returns the tenant invariant error when the independently supplied
    /// tenant does not match the authenticated principal.
    pub fn try_new(
        principal: Principal,
        data_tenant_id: DataTenantId,
        request_id: RequestId,
        trace_id: Option<String>,
        auth_method: AuthMethod,
        permission: impl Into<String>,
    ) -> Result<Self, BifrostError> {
        if principal.tenant_id != data_tenant_id {
            return Err(BifrostError::QueryTenantInvariant);
        }
        Ok(Self {
            principal,
            data_tenant_id,
            request_id,
            trace_id,
            auth_method,
            permission: permission.into(),
        })
    }
}

/// Options for a lowered, already-parsed logical query.
#[derive(Debug, Clone, Copy)]
pub struct QueryOptions {
    /// Visibility mode represented by the lowered plan.
    pub visibility: VisibilityMode,
    /// Maximum execution duration.
    pub deadline: Instant,
}

/// Oracle memory and spill resources shared with Scribe's parent governor.
#[derive(Debug, Clone)]
pub struct OracleMemoryResources {
    /// Narrow Oracle capability issued by the one production composition.
    pub resources: crate::resources::OracleResources,
    /// Maximum bytes reserved by one query for reconciliation state.
    pub reconciliation_limit_bytes: usize,
}

/// Point-in-time readiness inputs used by role-separated lifecycle journeys.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OracleReadinessSnapshot {
    /// Whether Oracle startup readiness completed and remains uncancelled.
    pub startup_reconciled: bool,
    /// Number of live Oracle memberships in the local immutable cluster snapshot.
    pub live_oracles: usize,
    /// Configured local running slot capacity available to query admission.
    pub running_capacity: usize,
}

/// Read-only aggregate owned by local Oracle admission and peer reservations.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OracleRuntimeInspection {
    /// Queries currently holding local class and tenant grants.
    pub active_queries: u64,
    /// Waiters currently queued for a local grant.
    pub queued_queries: u64,
    /// Memory bytes reserved by active queries.
    pub reserved_memory_bytes: u64,
    /// Spill bytes reserved by active queries.
    pub reserved_spill_bytes: u64,
    /// Peer pending reservations held by this Oracle.
    pub peer_pending: u64,
    /// Peer running reservations held by this Oracle.
    pub peer_running: u64,
}

/// Bounded local admission slots for one Oracle process.
#[derive(Debug)]
pub struct OracleSlotManager {
    /// Semaphore bounding requests waiting to enter pod-local admission.
    pending: Arc<Semaphore>,
    /// Semaphore representing local running slot units.
    running: Arc<Semaphore>,
    /// Immutable configured pending capacity used for readiness diagnostics.
    pending_limit: usize,
    /// Immutable configured running capacity used for placement calculations.
    running_limit: usize,
}

/// Per-phase stopwatch for one SQL attempt after planning completes.
///
/// A single attempt total cannot distinguish real execution from time spent
/// queued for admission, and those two call for opposite fixes: queueing is an
/// admission-sizing problem, execution is a planning or parallelism problem.
/// Splitting admit, audit-and-drain, and execute makes the dominant cost
/// attributable from a single log line.
///
/// Elapsed values are cumulative from construction; each phase method converts
/// its slice by subtracting the phases already recorded.
struct AttemptPhaseTimer {
    /// Monotonic origin, taken once planning has produced its cuts.
    started_at: Instant,
    /// Milliseconds spent acquiring admission.
    admit_ms: u128,
    /// Milliseconds spent auditing the cut and draining live tails.
    drained_ms: u128,
}

impl AttemptPhaseTimer {
    /// Starts the stopwatch for one attempt.
    fn started() -> Self {
        Self {
            started_at: Instant::now(),
            admit_ms: 0,
            drained_ms: 0,
        }
    }

    /// Records the admission phase as complete.
    fn admitted(&mut self) {
        self.admit_ms = self.started_at.elapsed().as_millis();
    }

    /// Records the audit-and-drain phase as complete.
    fn drained(&mut self) {
        self.drained_ms = self
            .started_at
            .elapsed()
            .as_millis()
            .saturating_sub(self.admit_ms);
    }

    /// Emits the three phase durations, deriving execution from the remainder.
    fn emit(&self) {
        tracing::debug!(
            admit_ms = self.admit_ms,
            drained_ms = self.drained_ms,
            execute_ms = self
                .started_at
                .elapsed()
                .as_millis()
                .saturating_sub(self.admit_ms)
                .saturating_sub(self.drained_ms),
            "Oracle SQL attempt phase timings"
        );
    }
}

/// Production metric owner for one retained local Oracle.
///
/// The owner keeps the process-local slot and memory accounting needed to
/// update gauges from real guard lifetimes. It emits through the recorder
/// installed by the server and never installs an exporter or subscriber.
#[derive(Debug)]
struct OracleTelemetry {
    /// Immutable local slot capacity reported alongside query activity.
    slots: Arc<OracleSlotManager>,
    /// Oracle-owned parent-memory bytes retained by live reservations.
    memory_bytes: AtomicU64,
}

impl OracleTelemetry {
    /// Adds one live Oracle memory owner to the canonical gauge accounting.
    fn charge_memory(&self, bytes: usize, query_class: QueryClass, memory_kind: OracleMemoryKind) {
        let total = self
            .memory_bytes
            .fetch_add(bytes as u64, Ordering::AcqRel)
            .saturating_add(bytes as u64);
        let _ = (total, query_class, memory_kind);
    }

    /// Creates telemetry around the same slot owner used by admission.
    #[must_use]
    fn new(slots: Arc<OracleSlotManager>) -> Self {
        let _ = (OracleAdmissionReason::ALL, OracleCancellationReason::ALL);
        for query_class in [QueryClass::Interactive, QueryClass::Analytical] {
            let class = query_class_label(query_class);
            metrics::gauge!("oracle_queries_active", "class" => query_class_label(query_class))
                .set(0.0);
            metrics::gauge!("oracle_queries_queued", "class" => query_class_label(query_class))
                .set(0.0);
            metrics::gauge!(
                "oracle_tenant_budget_pressure",
                "class" => query_class_label(query_class)
            )
            .set(0.0);
            for family in [
                "oracle_query_rows_total",
                "oracle_query_logical_bytes_selected_total",
                "oracle_query_bytes_scanned_total",
                "oracle_query_bytes_returned_total",
                "oracle_query_files_scanned_total",
                "oracle_query_partitions_scanned_total",
                "oracle_query_row_groups_scanned_total",
                "oracle_query_row_groups_pruned_total",
                "oracle_query_spill_bytes_total",
                "oracle_query_spill_files_total",
            ] {
                metrics::counter!(family, "class" => class).increment(0);
            }
            for outcome in ["success", "error", "cancelled"] {
                metrics::counter!(
                    "oracle_query_spill_queries_total",
                    "class" => class,
                    "outcome" => outcome
                )
                .increment(0);
            }
            for outcome in [
                OracleAdmissionOutcome::Admitted,
                OracleAdmissionOutcome::Rejected,
            ] {
                for reason in OracleAdmissionReason::ALL {
                    metrics::counter!(
                        "oracle_admission_total",
                        "class" => class,
                        "outcome" => outcome.as_str(),
                        "reason" => reason.as_str()
                    )
                    .increment(0);
                }
            }
        }
        for reason in OracleCancellationReason::ALL {
            metrics::counter!("oracle_query_cancellations_total", "reason" => reason.as_str())
                .increment(0);
        }
        telemetry::register_analytical_series();
        Self {
            slots,
            memory_bytes: AtomicU64::new(0),
        }
    }

    /// Starts production accounting for one classified logical query.
    #[must_use]
    fn start_query(
        self: &Arc<Self>,
        _visibility: VisibilityMode,
        query_class: QueryClass,
    ) -> QueryTelemetryGuard {
        let _ = self.slots.running_capacity();
        metrics::gauge!(
            "oracle_queries_active",
            "class" => query_class_label(query_class)
        )
        .increment(1.0);
        QueryTelemetryGuard {
            query_class,
            started_at: Instant::now(),
            source_span: Some(tracing::info_span!(
                "bifrost.oracle.source",
                outcome = tracing::field::Empty
            )),
            first_batch_recorded: false,
            stream_started: false,
            finalization: QueryTelemetryFinalization::Open,
            emitted_rows: 0,
            emitted_bytes: 0,
            scan_stats: OracleQueryScanStats::default(),
            explicit_cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Records one admission decision with the closed D65 label domains.
    fn record_admission(
        query_class: QueryClass,
        outcome: OracleAdmissionOutcome,
        reason: OracleAdmissionReason,
    ) {
        metrics::counter!(
            "oracle_admission_total",
            "class" => query_class_label(query_class),
            "outcome" => outcome.as_str(),
            "reason" => reason.as_str()
        )
        .increment(1);
        // A refusal is the one admission outcome an operator has to explain,
        // and the counter alone cannot say which of a node's queries lost. The
        // event is emitted here, at the single place every reason converges,
        // so no refusal branch can be added later without becoming visible.
        if matches!(outcome, OracleAdmissionOutcome::Rejected) {
            tracing::debug!(
                target: "wyrd::oracle::admission",
                class = query_class_label(query_class),
                reason = reason.as_str(),
                "oracle admission refused"
            );
        }
    }

    /// Records one query cancellation without identity-bearing labels.
    fn record_cancellation(reason: OracleCancellationReason) {
        metrics::counter!("oracle_query_cancellations_total", "reason" => reason.as_str())
            .increment(1);
    }

    /// Starts a bounded admission-waiter gauge and duration observation.
    #[must_use]
    fn start_admission_waiter(query_class: QueryClass) -> AdmissionWaitTelemetryGuard {
        metrics::gauge!("oracle_queries_queued", "class" => query_class_label(query_class))
            .increment(1.0);
        AdmissionWaitTelemetryGuard {
            query_class,
            started_at: Instant::now(),
            finished: false,
        }
    }

    /// Couples a nested query-pool reservation to canonical Oracle gauges.
    #[must_use]
    fn account_query_memory(
        self: &Arc<Self>,
        reservation: crate::resources::OracleQueryMemoryReservation,
        query_class: QueryClass,
        memory_kind: OracleMemoryKind,
    ) -> AccountedMemoryReservation {
        let bytes = reservation.bytes();
        self.charge_memory(bytes, query_class, memory_kind);
        AccountedMemoryReservation {
            reservation: Some(OracleGovernorReservation::Query(reservation)),
            owner: Arc::clone(self),
            query_class,
            memory_kind,
            bytes,
        }
    }

    /// Releases gauge accounting before the governor reservation is dropped.
    ///
    /// # Errors
    ///
    /// Returns `Err(())` when the wrapper total cannot cover `bytes`; its
    /// caller poisons the coupled governor because ownership no longer agrees.
    fn release_memory(
        &self,
        query_class: QueryClass,
        memory_kind: OracleMemoryKind,
        bytes: usize,
    ) -> Result<(), ()> {
        let bytes = u64::try_from(bytes).map_err(|_| ())?;
        let mut current = self.memory_bytes.load(Ordering::Acquire);
        loop {
            let Some(total) = current.checked_sub(bytes) else {
                return Err(());
            };
            match self.memory_bytes.compare_exchange(
                current,
                total,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    let _ = (total, query_class, memory_kind, bytes);
                    return Ok(());
                }
                Err(observed) => current = observed,
            }
        }
    }
}

/// Closed Oracle memory purpose used by the canonical class gauge.
#[derive(Debug, Clone, Copy)]
enum OracleMemoryKind {
    /// Encoded or decoded source buffers.
    Source,
    /// Fenced live-tail batches retained through query completion.
    Tail,
}

/// Closed state machine for one query's terminal metric emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryTelemetryFinalization {
    /// No terminal outcome has been emitted yet.
    Open,
    /// Terminal accounting is in progress or has completed.
    Closed,
}

/// Query-lifetime accounting that emits one terminal outcome on every drop.
struct QueryTelemetryGuard {
    /// Immutable admission class label.
    query_class: QueryClass,
    /// Query start used by duration and first-batch histograms.
    started_at: Instant,
    /// Production source span retained until the first physical batch or terminal result.
    source_span: Option<tracing::Span>,
    /// Whether the first yielded batch was already observed.
    first_batch_recorded: bool,
    /// Whether a public stream was successfully constructed.
    stream_started: bool,
    /// Explicit terminal-accounting state preventing duplicate emission.
    finalization: QueryTelemetryFinalization,
    /// Number of rows carried by client-visible batch frames.
    emitted_rows: u64,
    /// Number of bytes in client-visible Arrow IPC schema and batch payloads.
    emitted_bytes: u64,
    /// Physical and logical scan evidence retained until terminal emission.
    scan_stats: OracleQueryScanStats,
    /// Shared marker set only by an explicit stream-owner cancellation.
    explicit_cancelled: Arc<AtomicBool>,
}

impl QueryTelemetryGuard {
    /// Marks that stream construction completed and stream outcomes now apply.
    fn start_stream(&mut self) {
        self.stream_started = true;
    }

    /// Records time to the first actual batch exactly once.
    fn first_batch(&mut self) {
        if self.first_batch_recorded {
            return;
        }
        self.first_batch_recorded = true;
        self.finish_source_span("success");
        metrics::histogram!(
            "oracle_query_time_to_first_batch_seconds",
            "class" => query_class_label(self.query_class)
        )
        .record(self.started_at.elapsed().as_secs_f64());
    }

    /// Closes the source-to-first-batch span exactly once.
    fn finish_source_span(&mut self, outcome: &'static str) {
        if let Some(span) = self.source_span.take() {
            span.record("outcome", outcome);
        }
    }

    /// Returns the explicit-cancellation marker shared with the stream owner.
    #[must_use]
    fn cancellation_marker(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.explicit_cancelled)
    }

    /// Accounts one client-visible Arrow IPC payload.
    fn record_payload(&mut self, rows: u64, bytes: usize) {
        self.emitted_rows = self.emitted_rows.saturating_add(rows);
        self.emitted_bytes = self
            .emitted_bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
    }

    /// Retains the final physical-plan scan evidence for terminal emission.
    fn record_scan_stats(&mut self, scan_stats: OracleQueryScanStats) {
        self.scan_stats = scan_stats;
    }

    /// Emits the final query and stream outcome exactly once.
    fn finish(&mut self, outcome: &'static str, _freshness: &'static str) {
        if self.finalization == QueryTelemetryFinalization::Closed {
            return;
        }
        self.finalization = QueryTelemetryFinalization::Closed;
        self.finish_source_span(outcome);
        self.scan_stats.finalize();
        metrics::counter!(
            "oracle_query_logical_bytes_selected_total",
            "class" => query_class_label(self.query_class)
        )
        .increment(self.scan_stats.logical_bytes_selected);
        metrics::counter!(
            "oracle_query_files_scanned_total",
            "class" => query_class_label(self.query_class)
        )
        .increment(self.scan_stats.files_scanned);
        metrics::counter!(
            "oracle_query_partitions_scanned_total",
            "class" => query_class_label(self.query_class)
        )
        .increment(self.scan_stats.partitions_scanned);
        metrics::counter!(
            "oracle_query_row_groups_scanned_total",
            "class" => query_class_label(self.query_class)
        )
        .increment(self.scan_stats.row_groups_scanned);
        metrics::counter!(
            "oracle_query_row_groups_pruned_total",
            "class" => query_class_label(self.query_class)
        )
        .increment(self.scan_stats.row_groups_pruned);
        if let Some(bytes) = self.scan_stats.physical_bytes_scanned {
            metrics::counter!(
                "oracle_query_bytes_scanned_total",
                "class" => query_class_label(self.query_class)
            )
            .increment(bytes);
        }
        metrics::histogram!(
            "oracle_query_duration_seconds",
            "class" => query_class_label(self.query_class),
            "outcome" => outcome
        )
        .record(self.started_at.elapsed().as_secs_f64());
        let result = match outcome {
            "success" => "success",
            "rejected" => "rejected",
            _ => "failed",
        };
        metrics::histogram!("bifrost_query_duration_seconds", "result" => result)
            .record(self.started_at.elapsed().as_secs_f64());
        if self.stream_started {
            metrics::counter!("oracle_query_rows_total", "class" => query_class_label(self.query_class))
            .increment(self.emitted_rows);
            metrics::counter!("oracle_query_bytes_returned_total", "class" => query_class_label(self.query_class))
            .increment(self.emitted_bytes);
        }
    }
}

impl Drop for QueryTelemetryGuard {
    /// Records cancellation or a pre-stream failure and closes in-flight state.
    fn drop(&mut self) {
        if self.finalization == QueryTelemetryFinalization::Open {
            let outcome = if self.explicit_cancelled.load(Ordering::Acquire) {
                OracleTelemetry::record_cancellation(OracleCancellationReason::Shutdown);
                "cancelled"
            } else if self.stream_started {
                OracleTelemetry::record_cancellation(OracleCancellationReason::ClientDrop);
                "client_drop"
            } else {
                "failed"
            };
            self.finish(outcome, "complete");
        }
        metrics::gauge!("oracle_queries_active", "class" => query_class_label(self.query_class))
            .decrement(1.0);
    }
}

/// Admission-wait gauge guard with one canonical duration outcome.
struct AdmissionWaitTelemetryGuard {
    /// Immutable query class.
    query_class: QueryClass,
    /// Admission-wait start.
    started_at: Instant,
    /// Whether an explicit outcome was emitted.
    finished: bool,
}

impl AdmissionWaitTelemetryGuard {
    /// Records the final durable scope and outcome for this waiter.
    fn finish(&mut self, _scope: &'static str, _outcome: &'static str) {
        if self.finished {
            return;
        }
        self.finished = true;
        metrics::histogram!("oracle_admission_queue_duration_seconds", "class" => query_class_label(self.query_class))
            .record(self.started_at.elapsed().as_secs_f64());
    }
}

impl Drop for AdmissionWaitTelemetryGuard {
    /// Closes the waiter gauge and records unexpected exits as failures.
    fn drop(&mut self) {
        if !self.finished {
            self.finish("cluster", "failed");
        }
        metrics::gauge!("oracle_queries_queued", "class" => query_class_label(self.query_class))
            .decrement(1.0);
    }
}

/// Governor reservation coupled to canonical Oracle memory gauges.
enum OracleGovernorReservation {
    /// Nested ownership inside the complete query envelope.
    Query(crate::resources::OracleQueryMemoryReservation),
}

impl OracleGovernorReservation {
    /// Poison the shared governor after wrapper-accounting corruption.
    fn poison(&self) {
        match self {
            Self::Query(reservation) => reservation.poison(),
        }
    }
}

/// Governor reservation coupled to canonical Oracle memory gauges.
struct AccountedMemoryReservation {
    /// Governor reservation released before the gauges are decremented.
    reservation: Option<OracleGovernorReservation>,
    /// Retained process-local telemetry owner.
    owner: Arc<OracleTelemetry>,
    /// Query class charged for the reservation.
    query_class: QueryClass,
    /// Closed memory purpose.
    memory_kind: OracleMemoryKind,
    /// Exact reserved bytes.
    bytes: usize,
}

impl Drop for AccountedMemoryReservation {
    /// Releases checked wrapper accounting, then drops parent capacity.
    fn drop(&mut self) {
        if self
            .owner
            .release_memory(self.query_class, self.memory_kind, self.bytes)
            .is_err()
        {
            if let Some(reservation) = &self.reservation {
                reservation.poison();
            }
            tracing::error!("Oracle memory wrapper cleanup poisoned accounting");
        }
        self.reservation.take();
    }
}

impl OracleSlotManager {
    /// Creates bounded pending and running slot guards.
    #[must_use]
    pub fn new(pending: usize, running: usize) -> Self {
        Self {
            pending: Arc::new(Semaphore::new(pending)),
            running: Arc::new(Semaphore::new(running)),
            pending_limit: pending,
            running_limit: running,
        }
    }

    /// Returns the currently configured pending capacity.
    #[must_use]
    pub fn pending_capacity(&self) -> usize {
        self.pending_limit
    }

    /// Returns the number of pending worker units currently reserved locally.
    #[must_use]
    pub(crate) fn pending_in_use(&self) -> u64 {
        self.pending_limit
            .saturating_sub(self.pending.available_permits()) as u64
    }

    /// Returns the currently configured running capacity.
    #[must_use]
    pub fn running_capacity(&self) -> usize {
        self.running_limit
    }

    /// Returns the number of running worker units currently reserved locally.
    #[must_use]
    pub(crate) fn running_in_use(&self) -> u64 {
        self.running_limit
            .saturating_sub(self.running.available_permits()) as u64
    }

    /// Tries to reserve one bounded reservation-waiter slot.
    ///
    /// Peer reservation is allowed to wait out momentary running-slot
    /// saturation, and this bound caps how many such waits may be in flight at
    /// once. It is deliberately not a dispatch gate: holding it grants no right
    /// to execute, only the right to wait for the running gate that does. That
    /// separation is what keeps a leader's completed fan-out reservation a real
    /// guarantee rather than an optimistic one.
    ///
    /// # Errors
    ///
    /// Returns admission rejection when the local waiter bound is full.
    pub(crate) fn try_pending(&self) -> Result<OwnedSemaphorePermit, BifrostError> {
        Arc::clone(&self.pending)
            .try_acquire_owned()
            .map_err(|_| BifrostError::QueryAdmissionRejected)
    }

    /// Tries to reserve the local running units required by one admitted query.
    ///
    /// # Errors
    ///
    /// Returns admission rejection when local running capacity changed since placement.
    pub(crate) fn try_running(&self, demand: u32) -> Result<OwnedSemaphorePermit, BifrostError> {
        Arc::clone(&self.running)
            .try_acquire_many_owned(demand)
            .map_err(|_| BifrostError::QueryAdmissionRejected)
    }
}

/// Canonical key for one table and optional Scribe stream identity.
type TailTransportKey = (String, Option<uuid::Uuid>, Option<uuid::Uuid>);

/// Canonical key for independently discovered live streams by table and node.
type LiveStreamKey = (String, Option<uuid::Uuid>);

/// Transport lookup for table-local live-tail fences.
#[derive(Default)]
pub struct TailTransportDirectory {
    /// Canonical table-to-transport map owned by the serving composition root.
    transports: RwLock<HashMap<TailTransportKey, Arc<dyn TailReadTransport>>>,
    /// Independently discovered live streams, including tables with no sealed file.
    live_streams: RwLock<HashMap<LiveStreamKey, Vec<LiveTailRoute>>>,
}

/// One independently discovered live Scribe stream for a table/day.
#[derive(Clone)]
struct LiveTailRoute {
    /// Stable Scribe node identity.
    node_id: uuid::Uuid,
    /// Current writer epoch for this Scribe boot.
    writer_epoch: u64,
    /// UTC event day served by the registered live stream.
    time_partition: wyrd_spec::vala::api::TimePartitionWire,
    /// Transport reaching the exact stream owner.
    transport: Arc<dyn TailReadTransport>,
}

impl std::fmt::Debug for TailTransportDirectory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TailTransportDirectory")
            .finish_non_exhaustive()
    }
}

impl TailTransportDirectory {
    /// Registers or replaces the transport for one canonical table key.
    pub fn insert(&self, table: impl Into<String>, transport: Arc<dyn TailReadTransport>) {
        if let Ok(mut transports) = self.transports.write() {
            transports.insert((table.into(), None, None), transport);
        }
    }

    /// Registers or replaces the transport for one canonical table and Scribe stream.
    pub fn insert_for_stream(
        &self,
        table: impl Into<String>,
        node_id: wyrd_spec::vala::api::NodeId,
        transport: Arc<dyn TailReadTransport>,
    ) {
        if let Ok(mut transports) = self.transports.write() {
            transports.insert((table.into(), Some(node_id.as_uuid()), None), transport);
        }
    }

    /// Registers an independently discovered live stream and its exact day.
    ///
    /// This metadata is deliberately independent of `file_list`: a newly
    /// registered table can have live memtable rows before its first seal, and
    /// Fused visibility must still fence that interval.
    pub fn insert_live_stream(
        &self,
        table: impl Into<String>,
        node_id: wyrd_spec::vala::api::NodeId,
        writer_epoch: u64,
        time_partition: wyrd_spec::vala::api::TimePartitionWire,
        transport: Arc<dyn TailReadTransport>,
    ) {
        self.insert_live_stream_route(
            table.into(),
            None,
            node_id,
            writer_epoch,
            time_partition,
            transport,
        );
    }

    /// Registers a tenant-scoped independently discovered live stream.
    ///
    /// Tenant-scoped routes prevent one tenant's authenticated Scribe channel
    /// from being selected for another tenant that uses the same logical table.
    pub fn insert_live_stream_for_tenant(
        &self,
        tenant: wyrd_spec::DataTenantId,
        table: impl Into<String>,
        node_id: wyrd_spec::vala::api::NodeId,
        writer_epoch: u64,
        time_partition: wyrd_spec::vala::api::TimePartitionWire,
        transport: Arc<dyn TailReadTransport>,
    ) {
        self.insert_live_stream_route(
            table.into(),
            Some(tenant.as_uuid()),
            node_id,
            writer_epoch,
            time_partition,
            transport,
        );
    }

    /// Stores one generic or tenant-scoped live-stream route.
    fn insert_live_stream_route(
        &self,
        table: String,
        tenant: Option<uuid::Uuid>,
        node_id: wyrd_spec::vala::api::NodeId,
        writer_epoch: u64,
        time_partition: wyrd_spec::vala::api::TimePartitionWire,
        transport: Arc<dyn TailReadTransport>,
    ) {
        if let Ok(mut transports) = self.transports.write() {
            transports.insert(
                (table.clone(), Some(node_id.as_uuid()), tenant),
                Arc::clone(&transport),
            );
        }
        if let Ok(mut streams) = self.live_streams.write() {
            let routes = streams.entry((table, tenant)).or_default();
            routes.retain(|route| {
                route.node_id != node_id.as_uuid()
                    || route.writer_epoch != writer_epoch
                    || route.time_partition != time_partition
            });
            routes.push(LiveTailRoute {
                node_id: node_id.as_uuid(),
                writer_epoch,
                time_partition,
                transport,
            });
            routes.sort_by(|left, right| {
                (left.node_id, left.writer_epoch, left.time_partition).cmp(&(
                    right.node_id,
                    right.writer_epoch,
                    right.time_partition,
                ))
            });
        }
    }

    /// Looks up a table-local tail transport without taking ownership of it.
    #[must_use]
    pub fn get(&self, table: &str) -> Option<Arc<dyn TailReadTransport>> {
        self.transports
            .read()
            .ok()
            .and_then(|transports| transports.get(&(table.to_owned(), None, None)).cloned())
    }

    /// Looks up a stream-specific transport, falling back to the local table route.
    #[must_use]
    fn get_for_stream(
        &self,
        table: &str,
        node_id: uuid::Uuid,
        tenant: wyrd_spec::DataTenantId,
    ) -> Option<Arc<dyn TailReadTransport>> {
        self.transports.read().ok().and_then(|transports| {
            transports
                .get(&(table.to_owned(), Some(node_id), Some(tenant.as_uuid())))
                .or_else(|| transports.get(&(table.to_owned(), Some(node_id), None)))
                .or_else(|| transports.get(&(table.to_owned(), None, None)))
                .cloned()
        })
    }

    /// Returns tenant-scoped and generic independently discovered live streams.
    #[must_use]
    fn live_streams(&self, table: &str, tenant: wyrd_spec::DataTenantId) -> Vec<LiveTailRoute> {
        let Ok(streams) = self.live_streams.read() else {
            return Vec::new();
        };
        let mut routes = streams
            .get(&(table.to_owned(), None))
            .cloned()
            .unwrap_or_default();
        routes.extend(
            streams
                .get(&(table.to_owned(), Some(tenant.as_uuid())))
                .cloned()
                .unwrap_or_default(),
        );
        routes
    }
}

/// Narrow audit collaborator owned by the serving composition root.
#[async_trait]
pub trait OracleAudit: Send + Sync {
    /// Commits the immutable read-decision detail before row access.
    ///
    /// # Errors
    ///
    /// Returns audit unavailable when the mandatory immutable event cannot commit.
    async fn append_read_decision(
        &self,
        context: &AuthorizedQueryContext,
        decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError>;

    /// Commits a tenant-tripwire security event.
    ///
    /// # Errors
    ///
    /// Returns audit unavailable when the mandatory security event cannot commit.
    async fn append_security_violation(
        &self,
        context: VerifiedSecurityContext,
        violation: BifrostSecurityViolation,
    ) -> Result<(), BifrostError>;
}

/// Locked T1 projection of one immutable local Oracle read decision.
#[derive(Debug, Clone, PartialEq)]
pub struct BifrostQueryReadDecision {
    /// Exact validated T1 detail appended to the tenant audit chain.
    detail: AuditDetail,
}

impl BifrostQueryReadDecision {
    /// Validates and retains the exact T1 read-decision projection.
    ///
    /// # Errors
    ///
    /// Returns audit unavailable when the projection violates the locked
    /// binding, topology, retry, slot, or deadline bounds.
    pub fn try_new(detail: AuditDetail) -> Result<Self, BifrostError> {
        if !matches!(detail, AuditDetail::BifrostQueryReadDecision { .. }) {
            return Err(BifrostError::QueryAuditUnavailable);
        }
        detail
            .validate()
            .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        Ok(Self { detail })
    }

    /// Consumes the decision into the exact T1 audit detail.
    #[must_use]
    pub fn into_detail(self) -> AuditDetail {
        self.detail
    }
}

/// Trusted, authenticated context used to append a security violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSecurityContext {
    /// Authenticated query context whose tenant binding was already checked.
    pub query: AuthorizedQueryContext,
    /// Trusted normalized query digest, when the failure is query-associated.
    pub query_digest: Option<QueryAuditDigest>,
}

/// Locked T1 projection of one tenant or peer security violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BifrostSecurityViolation {
    /// Closed violation class.
    pub violation: BifrostSecurityViolationKind,
    /// Verified boundary where the violation occurred.
    pub phase: BifrostSecurityPhase,
}

/// Audit writer that accepts every decision without persisting it.
///
/// Peer transport and fencing proofs assert on routing, reservation, and frame
/// behavior, not on the audit chain. Binding this writer keeps the worker's
/// real fail-closed audit call on the path while removing the Postgres
/// dependency those proofs do not need. Any test that asserts audit content
/// must use a writer that actually records.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct AcceptingOracleAudit;

#[cfg(any(test, feature = "test-support"))]
#[async_trait]
impl OracleAudit for AcceptingOracleAudit {
    /// Accepts the read decision so the worker proceeds to serve rows.
    ///
    /// # Errors
    /// Never returns an error.
    async fn append_read_decision(
        &self,
        _context: &AuthorizedQueryContext,
        _decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError> {
        Ok(())
    }

    /// Accepts the security violation so refusal reporting is not masked by an
    /// audit failure.
    ///
    /// # Errors
    /// Never returns an error.
    async fn append_security_violation(
        &self,
        _context: VerifiedSecurityContext,
        _violation: BifrostSecurityViolation,
    ) -> Result<(), BifrostError> {
        Ok(())
    }
}

/// SQL-backed audit writer used by the T3 Postgres integration harness.
///
/// Production composition may provide a broader audit owner, while this
/// implementation deliberately exercises the canonical `append_audit`
/// transaction and tenant RLS boundary without inventing a parallel sink.
#[cfg(feature = "test-support")]
#[derive(Clone)]
pub struct TestPostgresOracleAudit {
    /// Tenant-scoped Vala SQL root used for each independent audit transaction.
    vala: ValaPostgres,
}

#[cfg(feature = "test-support")]
impl TestPostgresOracleAudit {
    /// Creates the integration audit writer around the managed Postgres owner.
    #[must_use]
    pub fn new(vala: ValaPostgres) -> Self {
        Self { vala }
    }

    /// Appends and commits one exact audit event under tenant RLS.
    ///
    /// The caller selects the locked result while the authenticated,
    /// authorized query decision remains `Allow`: reads record `Success` and
    /// source security violations record `Failure`.
    ///
    /// # Errors
    ///
    /// Returns audit unavailable when tenant acquisition, append, or commit
    /// fails. No caller-visible read may begin after this operation fails.
    async fn commit(
        &self,
        context: &AuthorizedQueryContext,
        operation: &str,
        result: AuditResult,
        detail: AuditDetail,
    ) -> Result<(), BifrostError> {
        let event = AuditEvent::new(
            context.request_id.clone(),
            context.trace_id.clone(),
            operation.to_owned(),
            "bifrost.query".to_owned(),
            context.principal.card_ref().cloned(),
            context.principal.id,
            context.principal.kind.tag(),
            context.auth_method,
            context.permission.clone(),
            AuditDecision::Allow,
            result,
            "scrubbed Bifrost query decision".to_owned(),
        )
        .with_detail(detail);
        let mut conn = self
            .vala
            .tenant_conn(context.data_tenant_id)
            .await
            .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        vala_sql::queries::audit_outbox::append_audit(&mut conn, &event)
            .await
            .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        conn.commit()
            .await
            .map_err(|_| BifrostError::QueryAuditUnavailable)
    }
}

#[cfg(feature = "test-support")]
#[async_trait]
impl OracleAudit for TestPostgresOracleAudit {
    /// Commits one locked read decision to the tenant audit chain.
    ///
    /// # Errors
    ///
    /// Returns audit unavailable when the transaction cannot commit.
    async fn append_read_decision(
        &self,
        context: &AuthorizedQueryContext,
        decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError> {
        self.commit(
            context,
            "bifrost.query.read_decision",
            AuditResult::Success,
            decision.into_detail(),
        )
        .await
    }

    /// Commits one locked security violation to the tenant audit chain.
    ///
    /// # Errors
    ///
    /// Returns audit unavailable when the transaction cannot commit.
    async fn append_security_violation(
        &self,
        context: VerifiedSecurityContext,
        violation: BifrostSecurityViolation,
    ) -> Result<(), BifrostError> {
        self.commit(
            &context.query,
            "bifrost.query.security_violation",
            AuditResult::Failure,
            AuditDetail::BifrostSecurityViolation {
                violation: violation.violation,
                phase: violation.phase,
                query_digest: context.query_digest,
            },
        )
        .await
    }
}

/// Inputs required to retain one local Oracle owner.
pub struct OracleBuildConfig {
    /// One process-wide shutdown token shared by every Bifrost lifecycle owner.
    pub shutdown: CancellationToken,
    /// Redux Iceberg/catalog owner.
    pub catalog: Arc<BifrostCatalog>,
    /// Tenant-scoped SQL owner.
    pub vala: ValaPostgres,
    /// Cross-tenant operator pool used only by delegated admission background work.
    pub operator_pool: vala_sql::OperatorPool,
    /// Immutable membership registry.
    pub cluster: Arc<ClusterRegistry>,
    /// Fenced local Oracle role.
    pub local_role: RegisteredRole,
    /// Local pending/running slot guards.
    pub local_slots: Arc<OracleSlotManager>,
    /// Parent memory and spill resources.
    pub memory: OracleMemoryResources,
    /// Process-lifetime owner of pod-local Oracle query scratch.
    pub spill_runtime: Arc<OracleSpillRuntime>,
    /// Table-local tail transports.
    pub tails: Arc<TailTransportDirectory>,
    /// Read/security audit collaborator.
    pub audit: Arc<dyn OracleAudit>,
    /// Server-owned narrow peer-ticket authority.
    pub peer_ticket_minter: Arc<dyn peer::PeerTicketMinter>,
    /// Reservation owner this node's fragment and graph paths both charge against.
    ///
    /// One registry per node, shared with the peer worker that accepts
    /// reservations, so a graph lease can only ever be activated from a
    /// reservation this same node actually granted.
    pub reservations: Arc<dispatcher::ReservationRegistry>,
    /// Server-owned east-west stage authority for the inactive Analytical path.
    ///
    /// Absent on a deployment whose Oracle role cannot serve stage operations.
    /// The inactive Analytical owners are only composed when it is present, so
    /// a node without it has no follower ingress to mount and no leader handle
    /// to execute through.
    pub stage_authority: Option<Arc<dyn peer::OracleStageAuthority>>,
    /// Immutable Bifrost peer identity every east-west Oracle channel dials with.
    ///
    /// Absent only on a deployment whose target does not serve the peer plane.
    /// The Analytical owners are composed only when it is present, because a
    /// coordinator that cannot present the peer client identity cannot reach a
    /// follower at all.
    pub peer_tls: Option<dispatcher::BifrostPeerTls>,
    /// Workload credential this node presents on every east-west peer request.
    ///
    /// Absent only on a deployment whose target does not serve the peer plane.
    /// The private listener authenticates the workload credential before it
    /// polls a request body, so a coordinator without one cannot reach a
    /// follower even when it holds a valid peer certificate.
    pub peer_credentials: Option<Arc<dyn dispatcher::OraclePeerCredentials>>,
    /// Server-owned domain-separated Scribe-tail ticket signer.
    pub tail_ticket_minter: Option<Arc<dyn crate::scribe::tail_rpc::TailTicketMinter>>,
    /// Query-scoped live Scribe discovery owner.
    pub tail_discovery: Option<Arc<dyn tail_fence::TailStreamDiscovery>>,
    /// Optional node-aware local/tonic directory used for immutable sealed leaves.
    pub peer_transports: Option<Arc<dispatcher::OraclePeerTransportDirectory>>,
    /// Engine limits and lifecycle values.
    pub config: OracleConfig,
    /// Validated delegated policy-capacity lifecycle settings.
    pub delegated_admission_config: DelegatedOracleAdmissionConfig,
}

/// Local Oracle limits and bounded lifecycle settings.
#[derive(Debug, Clone, Copy)]
pub struct OracleConfig {
    /// Maximum SQL bytes accepted before metadata access.
    pub max_sql_bytes: usize,
    /// Default query deadline.
    pub default_deadline: Duration,
    /// Maximum concurrent sealed planning operations.
    pub planning_permits: usize,
    /// Tenant ceiling for interactive slot units.
    pub tenant_interactive_slots: u32,
    /// Tenant ceiling for analytical slot units.
    pub tenant_analytical_slots: u32,
    /// Maximum remote workers selected per query; leader is additional.
    pub max_workers_per_query: usize,
    /// Maximum sealed files represented by one micro-fragment.
    pub fragment_max_files: usize,
    /// Hard byte ceiling for one complete worker attempt.
    pub attempt_max_bytes: usize,
    /// In-memory attempt threshold before permission-restricted spill.
    pub attempt_memory_bytes: usize,
    /// Interactive class capacity.
    pub interactive_slots: u32,
    /// Analytical class capacity.
    pub analytical_slots: u32,
    /// Single-tenant local ceiling.
    pub single_tenant_ceiling: u32,
    /// Multi-tenant local ceiling.
    pub multi_tenant_ceiling: u32,
    /// Maximum queued waiters.
    pub queue_capacity: u32,
    /// Absolute queue wait cap.
    pub max_queue_wait: Duration,
    /// Bytes one Analytical attempt may retain as spill scratch.
    pub analytical_scratch_bytes: u64,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            max_sql_bytes: DEFAULT_MAX_SQL_BYTES,
            default_deadline: Duration::from_secs(30),
            planning_permits: 16,
            tenant_interactive_slots: 8,
            tenant_analytical_slots: 4,
            max_workers_per_query: 2,
            fragment_max_files: 16,
            attempt_max_bytes: 64 * 1024 * 1024,
            attempt_memory_bytes: 8 * 1024 * 1024,
            interactive_slots: 8,
            analytical_slots: 4,
            single_tenant_ceiling: 8,
            multi_tenant_ceiling: 4,
            queue_capacity: 64,
            max_queue_wait: Duration::from_millis(250),
            analytical_scratch_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Internal execution outcome that preserves the sole whole-query replan signal.
#[derive(Debug, thiserror::Error)]
enum OracleExecutionError {
    /// A stable public failure that must cross the transport boundary unchanged.
    #[error(transparent)]
    Public(#[from] BifrostError),
}

/// One physical root retained with the exact config it was built from.
///
/// The config travels with the root because execution must reuse it verbatim:
/// it carries the planning-time partition, batch, and spill choices the root
/// was optimized for, plus the `OnceLock` extension identity every planned leaf
/// resolves its post-admission bindings through.
struct RetainedPhysicalPlan {
    /// Exact `SessionConfig` the root was planned with.
    config: datafusion::prelude::SessionConfig,
    /// The single physical root this query executes.
    root: Arc<dyn ExecutionPlan>,
}

/// Everything the one retained root needs to execute exactly once.
struct RetainedExecutionInput<'a> {
    /// The single physical root and the config it was planned with.
    retained: RetainedPhysicalPlan,
    /// Immutable selected-file bytes used for logical scan telemetry.
    logical_bytes_selected: u64,
    /// Class derived from the retained root and admitted under.
    query_class: QueryClass,
    /// Authenticated request context.
    context: &'a AuthorizedQueryContext,
    /// Admitted owner whose envelope an Analytical root moves onto its graph.
    admitted: &'a mut AdmittedQueryGuard,
    /// Signed, role-fenced participant cut this root executes under.
    participant_cut: &'a OracleQueryAttemptCut,
    /// Absolute execution deadline.
    deadline: Instant,
    /// Scannable work the pinned cut offers, used to shape graph parallelism.
    work_units: usize,
    /// Immutable Analytical attempt identity, present only for an Analytical root.
    analytical: Option<&'a analytical::AnalyticalAttemptContext>,
    /// The public running-query owner, moved onto the graph if Analytical.
    running_query: &'a mut Option<RunningQueryTerminalOwner>,
}

/// One executed cut's output together with the path it was executed on.
///
/// The path is produced by execution rather than chosen by the caller: only
/// the code that saw a real `DistributedExec` survive can say the query became
/// Analytical, so it travels out with the stream it describes.
struct CutExecution {
    /// Output schema of the executed root.
    schema: SchemaRef,
    /// Undrained result stream of the executed root.
    batches: SendableRecordBatchStream,
    /// Scan telemetry derived from the executed root.
    scan_stats: OracleQueryScanStats,
    /// Shared accumulator recording ordered degradation reasons.
    degraded_sources: DegradedSourceAccumulator,
    /// Path this cut irreversibly selected before its stream opened.
    execution_path: QueryExecutionPath,
}

/// Pinned tables and class selected during one retry's planning phase.
///
/// Pinned once per query and carried straight into the physical build, so the
/// catalog is read once rather than once to size the query and again to run it.
pub struct PlannedSqlCut {
    /// Exact immutable table cuts.
    pub(crate) cuts: Vec<PinnedSealedTable>,
    /// Fraction of pinned sealed bytes in the local hot tier.
    pub(crate) local_ratio: f64,
}

/// Inputs for live-fence acquisition, mandatory audit, and bounded drain.
struct CutAuditInput<'a> {
    /// Authenticated request context.
    context: &'a AuthorizedQueryContext,
    /// Original validated query request.
    request: &'a BifrostQueryRequest,
    /// Pinned tables being authorized.
    cuts: &'a [PinnedSealedTable],
    /// Server-derived query class.
    query_class: QueryClass,
    /// Absolute query deadline.
    deadline: Instant,
    /// Admitted query owner supplying cancellation and durable query identity.
    admitted: &'a AdmittedQueryGuard,
}

/// Immutable inputs shared by one complete SQL execution attempt.
struct SqlAttemptInput<'a> {
    /// Authenticated request context.
    context: &'a AuthorizedQueryContext,
    /// Original validated query request.
    request: &'a BifrostQueryRequest,
    /// Tables parsed from the validated SQL statement.
    tables: &'a [TableRef],
    /// Absolute whole-query deadline.
    deadline: Instant,
    /// Class-neutral membership frozen before any class existed.
    ///
    /// It is finalized into the signed participant cut only after the physical
    /// root has been built and its class derived, so no topology or source
    /// refresh can occur between the class and the cut it is signed into.
    roster: participant_cut::OracleQueryAttemptRoster,
    /// Catalog snapshot already pinned in this process for this attempt.
    prepared: Option<PlannedSqlCut>,
    /// Inactive Analytical attempt identity, present only on the harness entry.
    analytical: Option<&'a analytical::AnalyticalAttemptContext>,
}

/// Inputs for the pre-admission half of one attempt.
struct ClassifyInput<'a> {
    /// Authenticated request context.
    context: &'a AuthorizedQueryContext,
    /// Original validated query request.
    request: &'a BifrostQueryRequest,
    /// Tables parsed from the validated SQL statement.
    tables: &'a [TableRef],
    /// Absolute whole-query deadline.
    deadline: Instant,
    /// Class-neutral membership frozen before any class existed.
    roster: participant_cut::OracleQueryAttemptRoster,
    /// Catalog snapshot already pinned in this process for this attempt.
    prepared: Option<PlannedSqlCut>,
    /// Inactive Analytical attempt identity, present only on the harness entry.
    analytical: Option<&'a analytical::AnalyticalAttemptContext>,
    /// Telemetry slot opened once the class is known.
    query_telemetry: &'a mut Option<QueryTelemetryGuard>,
}

/// The one cut, root, and class every later step of an attempt reads.
struct ClassifiedAttempt {
    /// The single immutable source cut this attempt pinned.
    planned: PlannedSqlCut,
    /// The one physical root and the exact config it was built with.
    retained: RetainedPhysicalPlan,
    /// Class derived from that root alone.
    query_class: QueryClass,
    /// The participant cut signed with that class and the frozen roster.
    participant_cut: participant_cut::OracleQueryAttemptCut,
    /// Analytical attempt identity, present only for an Analytical root.
    attempt: Option<analytical::AnalyticalAttemptContext>,
    /// Scannable work units the frozen cut selected.
    work_units: usize,
}

/// Retained local query engine owner.
pub struct Oracle {
    /// Planner and floor configuration.
    planner: OraclePlanner,
    /// Admission state and local guards.
    admission: Arc<OracleAdmission>,
    /// Cached-only policy admission owner bound to the exact local role fence.
    delegated_admission: Arc<DelegatedOracleAdmission>,
    /// Typed continuity-loss handoff retained for the distributed query consumer.
    delegated_loss: Mutex<
        Option<tokio::sync::mpsc::Receiver<wyrd_spec::vala::api::OracleAdmissionContinuityLost>>,
    >,
    /// Immutable membership registry retained for planning and worker selection.
    cluster: Arc<ClusterRegistry>,
    /// Reservation owner this node's fragment and graph paths both charge against.
    reservations: Arc<dispatcher::ReservationRegistry>,
    /// One process-local lifecycle registry shared with private controls.
    running_queries: Arc<RunningQueryRegistry>,
    /// Tenant-qualified catalog and SQL owners retained for query execution.
    catalog: Arc<BifrostCatalog>,
    /// Tenant SQL handle retained for the Oracle lifecycle boundary.
    vala: ValaPostgres,
    /// Parent-governed query memory and spill configuration.
    memory: OracleMemoryResources,
    /// Process-lifetime owner used to construct bounded query disk managers.
    spill_runtime: Arc<OracleSpillRuntime>,
    /// Process-owned planning-only runtime shared by every physical build.
    ///
    /// It has an unbounded pool and no disk manager because planning performs
    /// no row IO and retains no query data; a query's real runtime is created
    /// from its own admitted grant after the class is derived.
    planning_runtime: Arc<datafusion::execution::runtime_env::RuntimeEnv>,
    /// Table-local Scribe tail transport directory.
    tails: Arc<TailTransportDirectory>,
    /// Query-scoped Scribe-tail ticket signer, when the server has a Scribe role.
    tail_ticket_minter: Option<Arc<dyn crate::scribe::tail_rpc::TailTicketMinter>>,
    /// Query-scoped live Scribe discovery owner.
    tail_discovery: Option<Arc<dyn tail_fence::TailStreamDiscovery>>,
    /// Test-tier switch that routes fused reads through explicitly registered local transports.
    #[cfg(feature = "test-support")]
    prefer_local_tail_routes: std::sync::atomic::AtomicBool,
    /// Test-tier one-shot refusal armed immediately before the distributed build.
    ///
    /// Selection has two distinct pre-selection refusals that both fall back to
    /// Interactive — the closed support predicate and the pinned distributed
    /// planner — and only an injected planner refusal can tell them apart from
    /// outside the process. The flag is consumed by the attempt that observes
    /// it, so a statement the predicate already refused leaves it armed, which
    /// is itself the proof that validation ran first.
    #[cfg(feature = "test-support")]
    fail_next_analytical_plan: std::sync::atomic::AtomicBool,
    /// Mandatory immutable read/security audit collaborator.
    audit: Arc<dyn OracleAudit>,
    /// Optional distributed fragment owner assembled from server capabilities.
    fragment_dispatcher: Option<Arc<dispatcher::FragmentDispatcher>>,
    /// Production-unreachable Analytical owners, composed but never routed to.
    ///
    /// Present only when the server supplied a stage authority. Nothing in the
    /// query path reads this field; it exists so the distributed path can be
    /// mounted and exercised before it is ever selectable.
    analytical: Option<Arc<analytical::AnalyticalExecutionHandle>>,
    /// Production metrics owner shared by query execution and admission.
    telemetry: Arc<OracleTelemetry>,
    /// Lifecycle cancellation token.
    shutdown: CancellationToken,
    /// Whether local startup readiness completed.
    ready: Arc<AtomicBool>,
    /// One-shot startup result consumed by the server activation boundary.
    startup_result: Mutex<Option<StartupResultReceiver>>,
    /// Cancellation-bound local admission lifecycle task.
    maintenance: Mutex<Option<JoinHandle<()>>>,
    /// Background-only `PostgreSQL` allocator and renewal task.
    delegated_maintenance: Mutex<Option<JoinHandle<()>>>,
    /// Test-tier one-shot pause after immutable worker selection.
    #[cfg(feature = "test-support")]
    topology_probe: Mutex<Option<Arc<OracleTopologyProbe>>>,
}

/// Notification-backed test seam for a topology change after worker selection.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub struct OracleTopologyProbe {
    /// Ensures exactly one query attempt pauses at the selected-candidate seam.
    claimed: AtomicBool,
    /// Wakes the journey once the first attempt has selected its immutable cut.
    selected: tokio::sync::Notify,
    /// Records permission for the paused attempt to continue dispatch.
    resumed: AtomicBool,
    /// Wakes the paused attempt after the fixture changes membership.
    resume: tokio::sync::Notify,
    /// Remote worker selected by the paused immutable assignment.
    target: Mutex<Option<NodeId>>,
}

#[cfg(feature = "test-support")]
impl OracleTopologyProbe {
    /// Waits until the first query attempt has selected its worker candidates.
    pub async fn wait_selected(&self) {
        while !self.claimed.load(Ordering::Acquire) {
            self.selected.notified().await;
        }
    }

    /// Returns the remote worker selected by the paused first attempt.
    #[must_use]
    pub fn selected_worker(&self) -> Option<NodeId> {
        self.target.lock().ok().and_then(|target| *target)
    }

    /// Releases the selected attempt after the fixture changes membership.
    pub fn resume(&self) {
        self.resumed.store(true, Ordering::Release);
        self.resume.notify_waiters();
    }
}

/// One-shot startup result consumed exactly once by activation.
type StartupResultReceiver = tokio::sync::oneshot::Receiver<Result<(), BifrostError>>;

impl std::fmt::Debug for Oracle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Oracle")
            .field("ready", &self.ready.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

/// Validates the synchronous Oracle construction limits before owners start.
///
/// # Errors
///
/// Returns [`BifrostError::Internal`] when fragment, SQL, planning, or tenant
/// limits cannot safely admit work.
fn validate_oracle_config(config: OracleConfig) -> Result<(), BifrostError> {
    if config.max_workers_per_query > 63
        || config.fragment_max_files == 0
        || config.attempt_max_bytes == 0
        || config.attempt_memory_bytes == 0
        || config.attempt_memory_bytes > config.attempt_max_bytes
    {
        return Err(BifrostError::Internal {
            detail: "Oracle fragment configuration is invalid".to_owned(),
        });
    }
    if config.max_sql_bytes == 0 {
        return Err(BifrostError::Internal {
            detail: "Oracle SQL byte limit must be positive".to_owned(),
        });
    }
    if config.planning_permits == 0
        || config.tenant_interactive_slots == 0
        || config.tenant_analytical_slots == 0
    {
        return Err(BifrostError::Internal {
            detail: "Oracle planning and tenant limits must be positive".to_owned(),
        });
    }
    Ok(())
}

/// One node's composed delegated-admission owner and its maintenance task.
///
/// Returned together because the worker only means anything alongside the
/// admission owner it drains for, and the loss receiver only alongside the
/// channel that owner was built with.
struct ComposedDelegatedAdmission {
    /// Delegated admission owner shared with the rest of Oracle.
    delegated_admission: Arc<DelegatedOracleAdmission>,
    /// Background worker draining this node's delegated demand.
    delegated_maintenance: tokio::task::JoinHandle<()>,
    /// Receiver signalling that delegated leadership was lost.
    loss_rx: tokio::sync::mpsc::Receiver<wyrd_spec::vala::api::OracleAdmissionContinuityLost>,
}

/// Builds this node's delegated-admission owner and starts its worker.
///
/// The demand channel is sized from the queue capacity so a node cannot buffer
/// more delegated demand than it would ever admit, and the worker is spawned on
/// the caller's runtime rather than an ad hoc one so it shuts down with the
/// process that composed it.
///
/// # Errors
///
/// Returns [`BifrostError::Internal`] when the delegated owner rejects its
/// configuration, or when Oracle is being constructed outside a Tokio runtime.
fn compose_delegated_admission(
    admission: &Arc<OracleAdmission>,
    delegated_config: DelegatedOracleAdmissionConfig,
    operator_pool: vala_sql::OperatorPool,
    queue_capacity: u32,
    shutdown: &CancellationToken,
) -> Result<ComposedDelegatedAdmission, BifrostError> {
    let (demand_tx, demand_rx) = tokio::sync::mpsc::channel(queue_capacity.max(1) as usize);
    let (loss_tx, loss_rx) = tokio::sync::mpsc::channel(1);
    let delegated_admission = Arc::new(
        DelegatedOracleAdmission::new(
            admission.local_role.key.node_id,
            admission.local_role.fencing_token,
            delegated_config,
            demand_tx,
            loss_tx,
        )
        .map_err(|error| BifrostError::Internal {
            detail: error.to_string(),
        })?,
    );
    let delegated_maintenance = tokio::runtime::Handle::try_current()
        .map_err(|_| BifrostError::Internal {
            detail: "Oracle construction requires an active Tokio runtime".to_owned(),
        })?
        .spawn(
            DelegatedOracleAdmissionWorker::new(
                operator_pool,
                Arc::clone(&delegated_admission),
                demand_rx,
                shutdown.clone(),
            )
            .run(),
        );
    Ok(ComposedDelegatedAdmission {
        delegated_admission,
        delegated_maintenance,
        loss_rx,
    })
}

/// Everything one node needs to compose its Analytical execution handle.
///
/// Grouped rather than passed positionally because every field is either an
/// identity this node signs with or a resource owner the handle must share with
/// the rest of Oracle; naming them at the call site is what keeps the
/// composition auditable.
struct AnalyticalCompositionInputs {
    /// Signing and verifying authority for every stage ticket this node uses.
    authority: Arc<dyn peer::OracleStageAuthority>,
    /// This node's Oracle identity.
    node_id: NodeId,
    /// This node's Oracle fencing token.
    fence: u64,
    /// Catalog the follower leaf binding resolves sources through.
    catalog: Arc<BifrostCatalog>,
    /// Audit owner every refused stage message records through.
    audit: Arc<dyn OracleAudit>,
    /// Reservation owner both the fragment and graph paths charge against.
    reservations: Arc<dispatcher::ReservationRegistry>,
    /// Directory this leader reserves each graph participant's envelope through.
    peer_transports: Option<Arc<dispatcher::OraclePeerTransportDirectory>>,
    /// Process-owned spill runtime every query-owned runtime is built from.
    spill: Arc<OracleSpillRuntime>,
    /// Per-attempt scratch share.
    scratch_bytes: u64,
    /// Immutable peer identity every east-west channel is dialed through.
    peer_tls: dispatcher::BifrostPeerTls,
    /// Workload credential every east-west request presents.
    peer_credentials: Arc<dyn dispatcher::OraclePeerCredentials>,
}

/// Builds one node's Analytical execution handle from its composed owners.
///
/// The egress resolver, the follower ingress, and the leader handle all need
/// the same identity, authority, and resource owners, so they are composed in
/// one place rather than threaded through `Oracle::new`. Nothing here starts
/// work: the returned handle is unreachable from routing until a caller leases
/// an attempt through it.
fn compose_analytical_handle(
    inputs: AnalyticalCompositionInputs,
) -> Arc<analytical::AnalyticalExecutionHandle> {
    let AnalyticalCompositionInputs {
        authority,
        node_id,
        fence,
        catalog,
        audit,
        reservations,
        peer_transports,
        spill,
        scratch_bytes,
        peer_tls,
        peer_credentials,
    } = inputs;
    let supervisor = Arc::new(analytical::AnalyticalSupervisor::new());
    let leaf = codec::AnalyticalLeafBinding::new(
        wyrd_spec::vala::api::ClusterRole::Oracle,
        Arc::new(follower::OracleCatalogResolver::new(catalog)),
        audit,
    );
    let egress = Arc::new(analytical::AnalyticalStageEgress::new(
        Arc::clone(&authority),
        node_id,
        fence,
        ANALYTICAL_STAGE_TICKET_TTL,
        peer_tls.clone(),
        Arc::clone(&peer_credentials),
    ));
    let worker =
        analytical::AnalyticalStageIngress::new(analytical::AnalyticalStageIngressConfig {
            node_id,
            oracle_fence: fence,
            authority: Arc::clone(&authority),
            supervisor: Arc::clone(&supervisor),
            reservations,
            spill: Arc::clone(&spill),
            leaf: leaf.clone(),
            egress,
        });
    Arc::new(analytical::AnalyticalExecutionHandle::new(
        analytical::AnalyticalExecutionOwners {
            worker: Arc::clone(&worker),
            authority,
            supervisor,
            spill,
            peer_transports,
        },
        analytical::AnalyticalExecutionConfig {
            node_id,
            oracle_fence: fence,
            ticket_ttl: ANALYTICAL_STAGE_TICKET_TTL,
            scratch_bytes,
            peer_tls,
            peer_credentials,
        },
        leaf,
    ))
}

/// Lifetime of every Analytical stage ticket this node mints.
///
/// Short enough that a captured ticket is useless long before a query's own
/// deadline, and long enough to cover one coordinator-to-follower dispatch on a
/// loaded cluster. Ticket expiry is checked in addition to the query deadline,
/// never instead of it.
const ANALYTICAL_STAGE_TICKET_TTL: chrono::Duration = chrono::Duration::seconds(30);

impl Oracle {
    /// Reports whether the injected process shutdown token reached the engine.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn process_shutdown_observed_for_test(&self) -> bool {
        self.shutdown.is_cancelled()
    }

    /// Borrow the serving composition's live-tail transport directory.
    ///
    /// Test and server composition roots use this handle to register transports
    /// discovered after the Oracle owner was constructed; query callers never
    /// receive or bypass this directory.
    #[must_use]
    pub fn tail_transports(&self) -> &TailTransportDirectory {
        &self.tails
    }

    /// Injects a private Scribe discovery outage for test-tier journeys.
    #[cfg(feature = "test-support")]
    pub fn set_tail_discovery_unavailable_for_test(&self, unavailable: bool) {
        if let Some(discovery) = &self.tail_discovery {
            discovery.set_unavailable_for_test(unavailable);
        }
    }

    /// Selects explicitly registered local tail routes for one-process language journeys.
    #[cfg(feature = "test-support")]
    pub fn prefer_local_tail_routes_for_test(&self) {
        self.prefer_local_tail_routes
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Arms one refusal of the pinned distributed physical build.
    ///
    /// The refusal is checked immediately before the second
    /// `create_physical_plan` call and returns the same mapped planning error
    /// that call would have returned. It neither replaces nor wraps the pinned
    /// planner, so an armed flag that is still armed afterwards proves the
    /// pinned planner was never reached.
    #[cfg(feature = "test-support")]
    pub fn fail_next_analytical_plan_for_test(&self) {
        self.fail_next_analytical_plan
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Reports whether an armed distributed-planning refusal is still unconsumed.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn analytical_plan_failure_armed_for_test(&self) -> bool {
        self.fail_next_analytical_plan
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// Consumes an armed one-shot distributed-planning refusal, if any.
    fn take_analytical_plan_failure(&self) -> bool {
        #[cfg(feature = "test-support")]
        {
            self.fail_next_analytical_plan
                .swap(false, std::sync::atomic::Ordering::AcqRel)
        }
        #[cfg(not(feature = "test-support"))]
        {
            false
        }
    }

    /// Returns the query-scoped discovery owner unless a test selected local routes.
    fn tail_discovery_for_query(&self) -> Option<Arc<dyn tail_fence::TailStreamDiscovery>> {
        #[cfg(feature = "test-support")]
        if self
            .prefer_local_tail_routes
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return None;
        }
        self.tail_discovery.clone()
    }

    /// Constructs a retained Oracle owner from explicit dependency handles.
    ///
    /// # Errors
    /// Returns [`BifrostError::Internal`] when the configured SQL floor is zero.
    pub fn new(config: OracleBuildConfig) -> Result<Self, BifrostError> {
        validate_oracle_config(config.config)?;
        let planner = OraclePlanner::new(config.config);
        let telemetry = Arc::new(OracleTelemetry::new(Arc::clone(&config.local_slots)));
        let cluster = Arc::clone(&config.cluster);
        let running_queries = Arc::new(RunningQueryRegistry::new());
        let membership_available = !cluster.snapshot().live_oracles().is_empty();
        let admission = Arc::new(OracleAdmission::with_config(
            config.local_slots,
            config.local_role,
            membership_available,
            admission::OracleAdmissionConfig {
                interactive_slots: config.config.interactive_slots,
                analytical_slots: config.config.analytical_slots,
                single_tenant_ceiling: config.config.single_tenant_ceiling,
                multi_tenant_ceiling: config.config.multi_tenant_ceiling,
                queue_capacity: config.config.queue_capacity,
                max_queue_wait: config.config.max_queue_wait,
            },
            config.memory.resources.clone(),
        ));
        let initial_snapshot = cluster.snapshot();
        admission.refresh(&initial_snapshot);
        let shutdown = config.shutdown;
        let ComposedDelegatedAdmission {
            delegated_admission,
            delegated_maintenance,
            loss_rx,
        } = compose_delegated_admission(
            &admission,
            config.delegated_admission_config,
            config.operator_pool,
            config.config.queue_capacity,
            &shutdown,
        )?;
        let ready = Arc::new(AtomicBool::new(false));
        let (maintenance, startup_result) =
            OracleAdmission::start_maintenance(shutdown.clone(), Arc::clone(&ready))?;
        let fragment_dispatcher = config.peer_transports.as_ref().map(|transports| {
            Arc::new(dispatcher::FragmentDispatcher::new(
                Arc::clone(&config.peer_ticket_minter),
                Arc::clone(transports),
            ))
        });
        // Both owners are required together: the authority proves a stage
        // operation, and the peer identity is the only way to deliver one.
        let analytical = config
            .stage_authority
            .zip(config.peer_tls)
            .zip(config.peer_credentials)
            .map(|((authority, peer_tls), peer_credentials)| {
                compose_analytical_handle(AnalyticalCompositionInputs {
                    authority,
                    peer_tls,
                    peer_credentials,
                    node_id: admission.local_role.key.node_id,
                    fence: admission.local_role.fencing_token,
                    catalog: Arc::clone(&config.catalog),
                    audit: Arc::clone(&config.audit),
                    reservations: Arc::clone(&config.reservations),
                    peer_transports: config.peer_transports.as_ref().map(Arc::clone),
                    spill: Arc::clone(&config.spill_runtime),
                    scratch_bytes: config.config.analytical_scratch_bytes,
                })
            });
        Ok(Self {
            planner,
            admission,
            delegated_admission,
            delegated_loss: Mutex::new(Some(loss_rx)),
            cluster,
            reservations: Arc::clone(&config.reservations),
            running_queries,
            catalog: config.catalog,
            vala: config.vala,
            memory: config.memory,
            spill_runtime: config.spill_runtime,
            planning_runtime: Arc::new(
                datafusion::execution::runtime_env::RuntimeEnvBuilder::new()
                    .build()
                    .map_err(|error| map_datafusion_error(&error))?,
            ),
            tails: config.tails,
            tail_ticket_minter: config.tail_ticket_minter,
            tail_discovery: config.tail_discovery,
            #[cfg(feature = "test-support")]
            prefer_local_tail_routes: std::sync::atomic::AtomicBool::new(false),
            #[cfg(feature = "test-support")]
            fail_next_analytical_plan: std::sync::atomic::AtomicBool::new(false),
            audit: config.audit,
            fragment_dispatcher,
            analytical,
            telemetry,
            shutdown,
            ready,
            startup_result: Mutex::new(Some(startup_result)),
            maintenance: Mutex::new(Some(maintenance)),
            delegated_maintenance: Mutex::new(Some(delegated_maintenance)),
            #[cfg(feature = "test-support")]
            topology_probe: Mutex::new(None),
        })
    }

    /// Borrows the one process-local registry used by execution and lifecycle controls.
    #[must_use]
    pub fn running_queries(&self) -> &Arc<RunningQueryRegistry> {
        &self.running_queries
    }

    /// Returns the production distributed fragment dispatcher when configured.
    #[must_use]
    pub fn fragment_dispatcher(&self) -> Option<Arc<dispatcher::FragmentDispatcher>> {
        self.fragment_dispatcher.as_ref().map(Arc::clone)
    }

    /// Binds a one-shot topology selection probe for a test-tier query.
    #[cfg(feature = "test-support")]
    pub fn bind_topology_probe_for_test(&self, probe: Arc<OracleTopologyProbe>) {
        if let Ok(mut current) = self.topology_probe.lock() {
            *current = Some(probe);
        }
    }

    /// Validates the query floor before any asynchronous metadata operation.
    ///
    /// # Errors
    /// Returns a stable public query error when the SQL is empty, oversized, or
    /// contains more than one statement or a non-`SELECT` leading keyword.
    pub fn validate_query(&self, request: &BifrostQueryRequest) -> Result<(), BifrostError> {
        self.planner.validate_query(request)
    }

    /// Resolves one tenant-qualified table into a schema-only typed-plan
    /// `DataFrame`.
    ///
    /// The returned logical plan carries no executable provider. Oracle installs
    /// the authenticated immutable-cut provider during [`Self::query_plan`],
    /// after audit and visibility decisions are committed.
    ///
    /// # Errors
    ///
    /// Returns a stable catalog or execution failure when the table namespace,
    /// provider, empty builtin fallback, or `DataFrame` cannot be constructed.
    pub async fn typed_dataframe(
        &self,
        tenant: DataTenantId,
        fqn: &str,
    ) -> Result<DataFrame, BifrostError> {
        let (namespace, name) = fqn
            .rsplit_once('.')
            .ok_or(BifrostError::QueryExecutionFailed)?;
        let namespace = namespace.strip_prefix("vala.").unwrap_or(namespace);
        let namespace = crate::namespaces::BifrostNamespace::from_domain_namespace(namespace)
            .ok_or(BifrostError::QueryExecutionFailed)?;
        let table = TableRef::new(namespace, name);
        let session = SessionContext::new();
        // Typed plans are schema-only authoring artifacts. The executable
        // Oracle provider is installed later by `query_plan` after it freezes
        // a tenant-bound visibility cut and commits its read decision.
        let schema: Arc<Schema> = match self.catalog.pin_sealed_table(&table, tenant).await {
            Ok(cut) => Arc::new(
                iceberg::arrow::schema_to_arrow_schema(
                    cut.iceberg_table.metadata().current_schema(),
                )
                .map_err(|_| BifrostError::QueryExecutionFailed)?,
            ),
            Err(BifrostCatalogError::TableNotFound(_)) => {
                let definition = crate::tables::builtin_table(namespace.as_str(), name)
                    .ok_or(BifrostError::QueryExecutionFailed)?;
                (definition.schema)()
            }
            Err(error) => return Err(error.into_public()),
        };
        let public_fields = schema
            .fields()
            .iter()
            .filter(|field| field.name() != "data_tenant_id")
            .cloned()
            .collect::<Vec<_>>();
        let provider = MemTable::try_new(Arc::new(Schema::new(public_fields)), vec![Vec::new()])
            .map_err(|error| map_datafusion_error(&error))?;
        session
            .register_table(TableReference::bare(fqn), Arc::new(provider))
            .map_err(|error| map_datafusion_error(&error))?;
        session
            .table(TableReference::bare(fqn))
            .await
            .map_err(|error| map_datafusion_error(&error))
    }

    /// Returns this node's inactive Analytical follower ingress, when composed.
    ///
    /// The server mounts the upstream worker service behind
    /// [`AnalyticalStageAuthLayer`] over this owner. It is `None` on a
    /// deployment that supplied no stage authority, in which case no worker
    /// service is mounted at all and the node cannot serve stage operations.
    ///
    /// [`AnalyticalStageAuthLayer`]: analytical_transport::AnalyticalStageAuthLayer
    #[must_use]
    pub fn analytical_worker(&self) -> Option<Arc<analytical::AnalyticalStageIngress>> {
        self.analytical
            .as_ref()
            .map(|handle| Arc::clone(handle.worker()))
    }

    /// Returns this node's inactive Analytical execution handle, when composed.
    ///
    /// Nothing in the query path calls this. It exists so test-support can
    /// drive the distributed path that production routing never selects.
    #[must_use]
    #[cfg(feature = "test-support")]
    pub fn analytical_execution(&self) -> Option<&Arc<analytical::AnalyticalExecutionHandle>> {
        self.analytical.as_ref()
    }

    /// Starts a query through the retained owner.
    ///
    /// # Errors
    ///
    /// Returns stable query, catalog, admission, visibility, audit, timeout, or
    /// execution errors before any public frame is returned.
    pub async fn query_sql(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
    ) -> Result<OracleQueryStream, BifrostError> {
        let (roster, planned) = self.prepare_query_attempt(&context, &request).await?;
        self.run_sql_query(context, request, roster, Some(planned), None)
            .await
    }

    /// Starts one raw-SQL query on the production-unreachable Analytical path.
    ///
    /// Everything up to the execution lease is the production path unchanged:
    /// the same validation, classification, participant cut, providers, audit,
    /// admission, and terminal stream owner. Only the leased session differs,
    /// and that difference is what makes the plan distribute across followers
    /// through the real signed private transport rather than execute on the
    /// leader.
    ///
    /// Nothing in routing calls this. It exists so the distributed path can be
    /// proved from raw SQL to drained result before it is ever selectable.
    ///
    /// # Errors
    ///
    /// Returns the same stable query, catalog, admission, visibility, audit,
    /// timeout, or execution errors as [`Self::query_sql`], plus
    /// [`BifrostError::OracleRoleUnavailable`] when this node composed no
    /// Analytical handle.
    #[cfg(feature = "test-support")]
    pub async fn query_sql_inactive_analytical(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
        attempt: analytical::AnalyticalAttemptContext,
    ) -> Result<OracleQueryStream, BifrostError> {
        let (roster, planned) = self.prepare_query_attempt(&context, &request).await?;
        self.run_sql_query(context, request, roster, Some(planned), Some(attempt))
            .await
    }

    /// Borrows this node's own root-derived Oracle resource capability.
    ///
    /// A physical baseline has to state the grant its query will actually run
    /// under before it runs, and the grant is derived from the live process
    /// resource plan rather than from a constant. Acquiring one envelope from
    /// this capability and reading its grant is the only way to observe the
    /// same arithmetic admission will apply.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub const fn role_resources(&self) -> &crate::resources::OracleResources {
        &self.memory.resources
    }

    /// Returns this node's process-owned Oracle spill directory.
    ///
    /// Terminal-cleanup and qualified-spill evidence needs to assert that an
    /// attempt's temporary files landed inside the node's own disposable child
    /// and nowhere else, which is only checkable against this root.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn analytical_spill_root(&self) -> &std::path::Path {
        self.spill_runtime.spill_path()
    }

    /// Leases one inactive Analytical attempt without executing anything.
    ///
    /// The cut, classification, and providers are prepared exactly as a real
    /// query would prepare them, and the returned session is the one a
    /// distributed plan would execute through. Nothing is planned or run, so a
    /// caller receives an attempt at rest — which is what makes the retry,
    /// fencing, and settlement orderings observable without racing an
    /// executing graph.
    ///
    /// # Errors
    ///
    /// Returns the same stable query, catalog, visibility, and admission errors
    /// as [`Self::query_sql`], plus [`BifrostError::OracleRoleUnavailable`]
    /// when this node composed no Analytical handle.
    #[cfg(feature = "test-support")]
    pub async fn lease_inactive_analytical_attempt(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
        attempt: &analytical::AnalyticalAttemptContext,
    ) -> Result<
        (
            datafusion::prelude::SessionContext,
            analytical::AnalyticalAttemptOwnership,
        ),
        BifrostError,
    > {
        let (roster, planned) = self.prepare_query_attempt(&context, &request).await?;
        let handle = self
            .analytical
            .as_ref()
            .ok_or(BifrostError::OracleRoleUnavailable)?;
        let work_units = Self::scannable_work_units(&planned.cuts);
        // The same single build production performs, so this attempt leases the
        // exact config its retained root was planned with.
        let retained = self
            .build_physical_root(
                &context,
                &request.sql,
                &planned.cuts,
                roster.oracles(),
                work_units,
            )
            .await?;
        let cut = roster
            .finalize(QueryClass::Analytical)
            .map_err(|_| BifrostError::OracleRoleUnavailable)?;
        // Admitted exactly as production admits: the leader envelope this graph
        // owns is the one this guard holds, and there is no second acquisition.
        let deadline = Instant::now()
            .checked_add(projected_request_deadline(
                request.deadline_ms,
                self.planner.config.default_deadline,
            ))
            .ok_or(BifrostError::QueryTimeout)?;
        let mut admitted = self
            .admit_sql_query(
                &context,
                QueryClass::Analytical,
                planned.local_ratio,
                deadline,
                cut.attempt_id(),
            )
            .await?;
        let (session, ownership) = handle.lease_session(analytical::AnalyticalLeaseInputs {
            attempt,
            cut: &cut,
            context: &context,
            admitted: &mut admitted,
            work_units,
            config: retained.config,
            deadline: tokio::time::Instant::from_std(deadline),
        })?;
        if ownership.retain_admission(admitted).is_err() {
            tracing::error!(
                public_query_id = %attempt.public_query_id,
                "Oracle analytical graph refused this query's admission owner"
            );
        }
        Ok((session, ownership))
    }

    /// Reports this node's graph-lease activations and the leases it still holds.
    ///
    /// Integration-only observable. A distributed plan must charge one envelope
    /// per follower no matter how many stage messages address it, and must
    /// return to zero live leases on every terminal path; neither is visible
    /// from outside the process any other way.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn graph_lease_counts(&self) -> (u64, usize) {
        (
            self.reservations.graph_leases_activated_total(),
            self.analytical
                .as_ref()
                .and_then(|handle| handle.worker().live().ok())
                .map_or(0, |live| live.graphs),
        )
    }

    /// Freezes the class-neutral leader roster and immutable cut for one attempt.
    ///
    /// Returns the pinned plan alongside the roster so a local leader executes
    /// the catalog snapshot it already paid for instead of pinning twice. No
    /// class exists yet: the roster is finalized into a signed participant cut
    /// only once the physical root has been built.
    ///
    /// # Errors
    ///
    /// Returns the same stable validation, readiness, catalog, membership, and
    /// deadline errors as [`Self::query_sql`].
    async fn prepare_query_attempt(
        &self,
        context: &AuthorizedQueryContext,
        request: &BifrostQueryRequest,
    ) -> Result<(participant_cut::OracleQueryAttemptRoster, PlannedSqlCut), BifrostError> {
        self.validate_query(request)?;
        if !self.is_ready() {
            return Err(BifrostError::OracleRoleUnavailable);
        }
        let duration =
            projected_request_deadline(request.deadline_ms, self.planner.config.default_deadline);
        let deadline = Instant::now()
            .checked_add(duration)
            .ok_or(BifrostError::QueryTimeout)?;
        let snapshot = self.cluster.snapshot();
        let tables = parse_select_tables(&request.sql)?;
        let planned = self
            .planner
            .pin_cut(context, &tables, deadline, &self.catalog)
            .await?;
        let now = chrono::Utc::now();
        let wall_deadline =
            now + chrono::Duration::from_std(duration).map_err(|_| BifrostError::QueryTimeout)?;
        let observed_age = now
            .signed_duration_since(snapshot.observed_at())
            .to_std()
            .unwrap_or_default();
        let attempt_id = QueryId::new(
            uuid::Uuid::parse_str(context.request_id.as_str())
                .map_err(|_| BifrostError::QueryExecutionFailed)?,
        );
        let roster = participant_cut::OracleQueryAttemptRoster::freeze(
            &snapshot,
            attempt_id,
            self.admission.local_role.key.node_id,
            wall_deadline,
            now,
            observed_age.saturating_add(Duration::from_secs(1)),
        )
        .map_err(|_| BifrostError::OracleRoleUnavailable)?;
        Ok((roster, planned))
    }

    /// Runs one SQL query's single terminal attempt.
    ///
    /// Every public raw-SQL entry point converges here: the participant cut and
    /// class are already fixed, and this owner runs the sole attempt the query
    /// is allowed. A stale source, a planning failure, or an execution failure
    /// is terminal; nothing repins, rebuilds, or falls back, and the caller may
    /// only submit a new logical query. Gate attaches its own request lifecycle
    /// to the returned stream afterwards rather than through this path.
    ///
    /// # Errors
    /// Returns the same stable query, catalog, admission, visibility, audit,
    /// timeout, or execution errors as [`Self::query_sql`].
    #[tracing::instrument(
        name = "bifrost.oracle.query",
        skip_all,
        fields(
            visibility = visibility_label(request.visibility),
            query_class = tracing::field::Empty,
            request_node_id = %self.admission.local_role.key.node_id.as_uuid(),
            leader_node_id = %self.admission.local_role.key.node_id.as_uuid(),
            search_role = "oracle"
        )
    )]
    async fn run_sql_query(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
        roster: participant_cut::OracleQueryAttemptRoster,
        prepared: Option<PlannedSqlCut>,
        analytical: Option<analytical::AnalyticalAttemptContext>,
    ) -> Result<OracleQueryStream, BifrostError> {
        self.validate_query(&request)?;
        if !self.is_ready() {
            return Err(BifrostError::OracleRoleUnavailable);
        }
        if roster.leader().node_id != self.admission.local_role.key.node_id
            || roster.leader().fencing_token != self.admission.local_role.fencing_token
            || roster.attempt_id().as_uuid().to_string() != context.request_id.as_str()
        {
            return Err(BifrostError::QueryPeerSecurity);
        }
        let remaining = roster
            .deadline()
            .signed_duration_since(chrono::Utc::now())
            .to_std()
            .map_err(|_| BifrostError::QueryTimeout)?;
        let deadline = Instant::now()
            .checked_add(remaining)
            .ok_or(BifrostError::QueryTimeout)?;
        let tables = parse_select_tables(&request.sql)?;
        // Phase timing at DEBUG: `oracle_query_duration_seconds` reports only a
        // total, which cannot separate a slow catalog pin from a slow fan-out.
        // This is the leader entry every public query passes through, so it is
        // where pre-fragment latency is attributable.
        let attempt_started = std::time::Instant::now();
        let mut query_telemetry = None;
        let stream = self
            .run_sql_attempt(
                SqlAttemptInput {
                    context: &context,
                    request: &request,
                    tables: &tables,
                    deadline,
                    roster,
                    prepared,
                    analytical: analytical.as_ref(),
                },
                &mut query_telemetry,
            )
            .await?;
        tracing::debug!(
            attempt_ms = attempt_started.elapsed().as_millis(),
            "Oracle leader opened one query stream"
        );
        Ok(stream)
    }

    /// Derives one candidate's immutable Analytical identity from the attempt.
    ///
    /// Every field is already fixed by the time a candidate exists: the public
    /// identity is this request's own attempt id, and the two digests are the
    /// same aggregate snapshot and permission digests the read decision was
    /// audited under, so a graph registered later cannot describe a different
    /// query than the one that was authorized. The private graph identity is
    /// allocated separately on purpose — a leaked public identity into the
    /// distributed graph, or the reverse, is what the stage authority's
    /// identity isolation exists to refuse.
    ///
    /// Holding a candidate selects nothing. It is discarded unchanged by every
    /// pre-selection refusal.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAuditUnavailable`] when a digest input is
    /// outside the bounded audit digest contract.
    fn candidate_attempt_context(
        context: &AuthorizedQueryContext,
        attempt_id: QueryId,
        planned: &PlannedSqlCut,
    ) -> Result<analytical::AnalyticalAttemptContext, BifrostError> {
        Ok(analytical::AnalyticalAttemptContext {
            public_query_id: analytical::PublicQueryId::from_uuid(attempt_id.as_uuid()),
            datafusion_query_id: analytical::DataFusionQueryId::from_uuid(uuid::Uuid::now_v7()),
            snapshot_digest: aggregate_audit_digest(
                planned.cuts.iter().map(|cut| cut.snapshot_digest.as_str()),
            )?
            .as_str()
            .to_owned(),
            permission_digest: audit_digest(&context.permission)?.as_str().to_owned(),
        })
    }

    /// Starts one query telemetry owner once across a possible stale retry.
    fn ensure_query_telemetry(
        &self,
        telemetry: &mut Option<QueryTelemetryGuard>,
        visibility: VisibilityMode,
        class: QueryClass,
    ) {
        telemetry.get_or_insert_with(|| self.telemetry.start_query(visibility, class));
    }

    /// Acquires every owner one built plan needs before it may execute.
    ///
    /// Admission, the retained physical projections, and the running-query
    /// registration are acquired together because a failure in any of them must
    /// release the ones already taken. The class is the root-derived one; no
    /// provisional class is ever admitted.
    ///
    /// # Errors
    ///
    /// Returns the stable admission, projection, or running-query conflict
    /// error, having already released any owner acquired earlier in the
    /// sequence.
    async fn admit_built_attempt(
        &self,
        context: &AuthorizedQueryContext,
        planned: &PlannedSqlCut,
        participant_cut: &OracleQueryAttemptCut,
        query_class: QueryClass,
        deadline: Instant,
        phases: &mut AttemptPhaseTimer,
    ) -> Result<(AdmittedQueryGuard, RunningQueryTerminalOwner), BifrostError> {
        let mut admitted = self
            .admit_sql_query(
                context,
                query_class,
                planned.local_ratio,
                deadline,
                participant_cut.attempt_id(),
            )
            .await?;
        phases.admitted();
        // Projected before any later transfer, never after: a selected
        // Analytical attempt moves this query's envelope out of the guard and
        // into the graph that owns it, and every physical projection is an
        // exact child split of that same envelope. Deriving them afterwards
        // would ask the guard for resources it no longer holds.
        if let Err(error) = admitted.retain_physical_projections(&planned.cuts) {
            return release_error(deadline, admitted, error, "projection rejection");
        }
        match self.register_running_query(context, query_class, &admitted, participant_cut) {
            Ok(running_query) => Ok((admitted, running_query)),
            Err(error) => release_error(deadline, admitted, error, "running-query rejection"),
        }
    }

    /// Builds the sole execution `TaskContext` for a graphless retained root.
    ///
    /// Admission supplies the runtime and memory pool only. The retained
    /// planning config is reused verbatim, so admitted capacity above the
    /// minimum-grant shape the root was planned for stays unused rather than
    /// reshaping a plan that is already final.
    ///
    /// # Errors
    ///
    /// Returns a stable execution error when `DataFusion` cannot construct the
    /// query-owned runtime environment.
    fn execution_session(
        &self,
        admitted: &AdmittedQueryGuard,
        config: datafusion::prelude::SessionConfig,
    ) -> Result<SessionContext, BifrostError> {
        let pool = admitted
            .memory_pool()
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        let runtime = self
            .spill_runtime
            .build_query_runtime(pool, admitted.spill_limit_bytes())?;
        let state = datafusion::execution::session_state::SessionStateBuilder::new()
            .with_default_features()
            .with_config(config)
            .with_runtime_env(runtime)
            .build();
        Ok(SessionContext::new_with_state(state))
    }

    /// Builds the typed-plan path's session from its own admitted grant.
    ///
    /// The typed entry lowers its plan after admission rather than before, so
    /// it shapes its session from the grant it already holds and installs its
    /// own empty bindings lock for the leaves that plan against it.
    ///
    /// # Errors
    ///
    /// Returns a stable execution error when `DataFusion` cannot construct the
    /// query-owned runtime environment.
    fn typed_execution_session(
        &self,
        admitted: &AdmittedQueryGuard,
        cuts: &[PinnedSealedTable],
    ) -> Result<SessionContext, BifrostError> {
        let config = crate::resources::OracleSessionShape::for_grant(
            admitted.granted_memory_bytes(),
            admitted.target_partitions(),
            Self::scannable_work_units(cuts),
        )
        .session_config()
        .with_extension(Arc::new(bindings::OracleExecutionLock::new()));
        self.execution_session(admitted, config)
    }

    /// Pins one cut, builds one physical root, and derives this attempt's class.
    ///
    /// This is the whole pre-admission half of an attempt, kept together because
    /// no step between them may observe a class that does not yet exist: the
    /// cut is frozen class-neutral, the root is built on the minimum-grant
    /// planning shape, the class is derived from that exact root, and only then
    /// is the participant cut signed with it.
    ///
    /// # Errors
    ///
    /// Returns the stable catalog, planning, or roster error. Every one of them
    /// is terminal; nothing here repins or rebuilds.
    async fn classify_one_build(
        &self,
        input: ClassifyInput<'_>,
    ) -> Result<ClassifiedAttempt, BifrostError> {
        let ClassifyInput {
            context,
            request,
            tables,
            deadline,
            roster,
            prepared,
            analytical,
            query_telemetry,
        } = input;
        let planned = match prepared {
            Some(planned) => planned,
            None => self.plan_sql_attempt(context, tables, deadline).await?,
        };
        let work_units = Self::scannable_work_units(&planned.cuts);
        let retained = self
            .build_physical_root(
                context,
                &request.sql,
                &planned.cuts,
                roster.oracles(),
                work_units,
            )
            .await?;
        let query_class = exec::query_class_for_root(retained.root.as_ref());
        tracing::Span::current().record("query_class", query_class_label(query_class));
        let participant_cut = roster
            .finalize(query_class)
            .map_err(|_| BifrostError::OracleRoleUnavailable)?;
        self.ensure_query_telemetry(query_telemetry, request.visibility, query_class);
        let attempt = match analytical {
            Some(attempt) => Some(attempt.clone()),
            None if query_class == QueryClass::Analytical && self.analytical.is_some() => Some(
                Self::candidate_attempt_context(context, participant_cut.attempt_id(), &planned)?,
            ),
            None => None,
        };
        Ok(ClassifiedAttempt {
            planned,
            retained,
            query_class,
            participant_cut,
            attempt,
            work_units,
        })
    }

    /// Audits and drains the pinned sources, then binds them to the retained plan.
    ///
    /// These are one step because a drain that is not bound owns reservations
    /// nobody will release: a binding failure must drop the drained tails here
    /// rather than leaving them for a caller to remember.
    ///
    /// # Errors
    ///
    /// Returns the stable audit, activation, drain, or binding-validation
    /// error. Every one of them settles the sole attempt.
    async fn drain_and_bind(
        &self,
        audit: CutAuditInput<'_>,
        config: &datafusion::prelude::SessionConfig,
        root: Option<&dyn ExecutionPlan>,
        cut_deadline: chrono::DateTime<chrono::Utc>,
        phases: &mut AttemptPhaseTimer,
    ) -> Result<Arc<bindings::OracleExecutionLock>, BifrostError> {
        let admitted = audit.admitted;
        let query_class = audit.query_class;
        let cuts = audit.cuts;
        let mut drained = self.audit_and_drain_cut(audit).await?;
        phases.drained();
        self.bind_execution_sources(
            config,
            admitted,
            cut_deadline,
            PlannedSources {
                query_class,
                cuts,
                root,
                drained: &mut drained,
            },
        )
    }

    /// Runs one complete SQL attempt from one build to a settled query stream.
    ///
    /// The order here is the whole contract: the immutable cut is pinned or
    /// reused, one physical root is built through the pinned planner, the class
    /// is read from that root, the frozen roster is finalized and signed with
    /// it, the envelope is admitted, the cut is audited and its live tails
    /// drained, the bindings are published once, and the retained root is
    /// executed exactly once. This is the sole attempt the query is allowed:
    /// any failure it reaches is terminal, and every owner it took is released
    /// before return.
    ///
    /// # Errors
    ///
    /// Returns the stable planning, membership, admission, audit, visibility,
    /// timeout, or execution error that terminated the attempt.
    async fn run_sql_attempt(
        &self,
        input: SqlAttemptInput<'_>,
        query_telemetry: &mut Option<QueryTelemetryGuard>,
    ) -> Result<OracleQueryStream, BifrostError> {
        let SqlAttemptInput {
            context,
            request,
            tables,
            deadline,
            roster,
            prepared,
            analytical,
        } = input;
        let ClassifiedAttempt {
            planned,
            retained,
            query_class,
            participant_cut,
            attempt,
            work_units,
        } = self
            .classify_one_build(ClassifyInput {
                context,
                request,
                tables,
                deadline,
                roster,
                prepared,
                analytical,
                query_telemetry,
            })
            .await?;
        let mut phases = AttemptPhaseTimer::started();
        let (mut admitted, running_query) = self
            .admit_built_attempt(
                context,
                &planned,
                &participant_cut,
                query_class,
                deadline,
                &mut phases,
            )
            .await?;
        let bound = match self
            .drain_and_bind(
                CutAuditInput {
                    context,
                    request,
                    cuts: &planned.cuts,
                    query_class,
                    deadline,
                    admitted: &admitted,
                },
                &retained.config,
                Some(retained.root.as_ref()),
                participant_cut.deadline(),
                &mut phases,
            )
            .await
        {
            Ok(bound) => bound,
            Err(error) => return release_error(deadline, admitted, error, "source rejection"),
        };
        // Retained here only until execution: an Analytical query moves this
        // owner onto its graph, and what is left is what the stream still owes.
        let mut running_query = Some(running_query);
        let execution = match self
            .execute_retained_root(RetainedExecutionInput {
                retained,
                logical_bytes_selected: Self::logical_selected_bytes(&planned.cuts),
                query_class,
                context,
                admitted: &mut admitted,
                participant_cut: &participant_cut,
                deadline,
                work_units,
                analytical: attempt.as_ref(),
                running_query: &mut running_query,
            })
            .await
        {
            Ok(execution) => {
                phases.emit();
                execution
            }
            Err(error) => {
                return release_error(deadline, admitted, error, "execution rejection");
            }
        };
        record_degraded_live_tail(
            &execution.degraded_sources,
            bound
                .get()
                .is_some_and(bindings::OracleExecutionBindings::degraded),
        );
        let settlement = AttemptSettlement {
            deadline,
            deadline_ms: participant_cut.deadline().timestamp_millis().max(0),
            visibility: request.visibility,
            freshness: request.freshness,
        };
        let output = AttemptOutput::new(execution, admitted, running_query);
        settle_attempt_output(output, settlement, query_telemetry).await
    }

    /// Executes the one retained root, graphless or on the Analytical graph.
    ///
    /// An Interactive root runs on a session built from the admitted runtime and
    /// the retained planning config. An Analytical root transfers the admitted
    /// envelope once into the graph supervisor, publishes participants
    /// immediately before dispatch, and executes through the existing stage
    /// lifecycle. Neither path rebuilds the root.
    ///
    /// # Errors
    ///
    /// Returns the stable admission, supervisor, reservation, runtime, or
    /// `DataFusion` execution error; every one of them is terminal.
    async fn execute_retained_root(
        &self,
        input: RetainedExecutionInput<'_>,
    ) -> Result<CutExecution, BifrostError> {
        let RetainedExecutionInput {
            retained,
            logical_bytes_selected,
            query_class,
            context,
            admitted,
            participant_cut,
            deadline,
            work_units,
            analytical,
            running_query,
        } = input;
        let RetainedPhysicalPlan { config, root } = retained;
        let (handle, attempt) = match (self.analytical.as_ref(), analytical) {
            (Some(handle), Some(attempt)) if query_class == QueryClass::Analytical => {
                (handle, attempt)
            }
            _ => {
                let session = self.execution_session(admitted, config)?;
                let scan_stats =
                    OracleQueryScanStats::from_plan(root.as_ref(), logical_bytes_selected);
                let schema = root.schema();
                let batches = execute_stream(root, session.task_ctx())
                    .map_err(|error| map_datafusion_error(&error))?;
                return Ok(CutExecution {
                    schema,
                    batches,
                    scan_stats,
                    degraded_sources: Arc::new(std::sync::Mutex::new(Vec::new())),
                    execution_path: QueryExecutionPath::Interactive,
                });
            }
        };
        let (leader, ownership) = handle.lease_session(analytical::AnalyticalLeaseInputs {
            attempt,
            cut: participant_cut,
            context,
            admitted,
            work_units,
            config,
            deadline: tokio::time::Instant::from_std(deadline),
        })?;
        let graph = ownership.key().graph();
        admitted.analytical = Some(ownership);
        // Execution moves the public entry onto the graph: from here the graph
        // is what "running" describes, and only its joined cleanup may retire
        // it. A refusal leaves the owner on the stream, which still fails it.
        if let Some(owner) = running_query.take()
            && let Err(returned) = handle.retain_running_query(graph, owner)
        {
            *running_query = Some(*returned);
        }
        // Published only after activation and immediately before dispatch, so
        // no follower is charged for a graph that never opened.
        if let Some(ownership) = admitted.analytical.as_ref() {
            ownership.publish_participants().await?;
        }
        let mut scan_stats = OracleQueryScanStats::from_plan(root.as_ref(), logical_bytes_selected);
        let schema = root.schema();
        let batches = execute_stream(Arc::clone(&root), leader.task_ctx())
            .map_err(|error| map_datafusion_error(&error))?;
        if let Some(ownership) = admitted.analytical.as_ref() {
            ownership.retain_metric_fold(analytical::AnalyticalGraphMetricFold::new(
                root,
                scan_stats.open_distributed_scan(),
            ))?;
        }
        Ok(CutExecution {
            schema,
            batches,
            scan_stats,
            degraded_sources: Arc::new(std::sync::Mutex::new(Vec::new())),
            execution_path: QueryExecutionPath::Analytical,
        })
    }

    /// Publishes the one binding set every planned leaf resolves through.
    ///
    /// Called after admission and the audited drain, and before any physical
    /// leaf can execute. It validates that every source the plan planned has
    /// exactly one binding, then sets the session's `OnceLock` once. A failed
    /// set drops the rejected bindings — releasing their drained reservations —
    /// and terminates the query rather than replacing live bindings.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when the session carries
    /// no bindings extension, when validation refuses the binding set, or when
    /// the lock was already bound.
    ///
    /// On success the bound lock is returned so the terminal path reads the
    /// degraded fact from the same post-admission source registry the leaves
    /// resolve against.
    fn bind_execution_sources(
        &self,
        config: &datafusion::prelude::SessionConfig,
        admitted: &AdmittedQueryGuard,
        deadline: chrono::DateTime<chrono::Utc>,
        sources: PlannedSources<'_>,
    ) -> Result<Arc<bindings::OracleExecutionLock>, BifrostError> {
        let PlannedSources {
            query_class,
            cuts,
            root,
            drained,
        } = sources;
        let lock = config
            .get_extension::<bindings::OracleExecutionLock>()
            .ok_or(BifrostError::QueryExecutionFailed)?;
        let mut local_batches = std::mem::take(&mut drained.batches);
        let mut planned = cuts
            .iter()
            .map(|cut| {
                let table = cut.binding.table_ref.fqn();
                local_batches.entry(table.clone()).or_default();
                bindings::OracleSourceKey::LocalDrained { table }
            })
            .collect::<Vec<_>>();
        let mut follower_assignments: std::collections::HashMap<
            bindings::OracleSourceKey,
            wyrd_spec::vala::api::FollowerScanAssignment,
        > = std::collections::HashMap::new();
        for placeholder in root.map(remote_placeholders).unwrap_or_default() {
            let Some(key) = placeholder.source_key() else {
                continue;
            };
            let source = key
                .follower()
                .ok_or(BifrostError::QueryExecutionFailed)?
                .clone();
            // The pinned cut is selected by the occurrence's own frozen table
            // binding, not by searching for a recomputed scan identity, so two
            // occurrences of one table can never resolve to each other's cut.
            let cut = cuts
                .iter()
                .find(|cut| {
                    cut.binding.tenant == source.tenant
                        && cut.binding.table_ref.fqn() == source.table
                })
                .ok_or(BifrostError::QueryExecutionFailed)?;
            let assignment = follower_scan_assignment(cut, source.tier, &placeholder)?;
            match follower_assignments.entry(key.clone()) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(assignment);
                    planned.push(key);
                }
                // A `DistributedLeafExec` contributes its original leaf plus one
                // variant per stage task. Every immutable fact of the key already
                // agreed to reach this arm, and a share is applied at encode
                // time, so the variants legitimately canonicalize onto the one
                // occurrence. Anything that would replace the first value is a
                // distinct read arriving under one identity.
                std::collections::hash_map::Entry::Occupied(slot) => {
                    if *slot.get() != assignment {
                        return Err(BifrostError::QueryExecutionFailed);
                    }
                }
            }
        }
        let value = bindings::OracleExecutionBindings::try_new(
            bindings::OracleExecutionBindingInputs {
                grant: bindings::OracleExecutionGrant {
                    query_class,
                    cancellation: admitted.cancellation.clone(),
                    deadline,
                    memory: self.memory.clone(),
                    telemetry: Arc::clone(&self.telemetry),
                },
                local_batches,
                follower_assignments,
                reservations: std::mem::take(&mut drained.reservations),
                degraded: drained.degraded,
            },
            &planned,
        )?;
        lock.set(value)
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
        Ok(lock)
    }

    /// Inserts one admitted query against the ingress-captured membership cut.
    ///
    /// # Errors
    /// Returns a stable role or conflict error when the immutable cut cannot be retained.
    fn register_running_query(
        &self,
        context: &AuthorizedQueryContext,
        query_class: QueryClass,
        admitted: &AdmittedQueryGuard,
        participant_cut: &OracleQueryAttemptCut,
    ) -> Result<RunningQueryTerminalOwner, BifrostError> {
        let now = chrono::Utc::now();
        if admitted.query_id != participant_cut.attempt_id()
            || admitted.leader.node_id != participant_cut.leader().node_id
            || admitted.leader.fencing_token != participant_cut.leader().fencing_token
            || participant_cut.deadline() <= now
        {
            return Err(BifrostError::QueryPeerSecurity);
        }
        let entry = RunningQueryEntry::with_cancellation(
            context.data_tenant_id,
            context.request_id.clone(),
            query_class,
            now,
            participant_cut.clone(),
            admitted.cancellation.clone(),
        );
        if !self.running_queries.insert(entry) {
            return Err(BifrostError::RunningQueryConflict);
        }
        Ok(RunningQueryTerminalOwner::new(
            Arc::clone(&self.running_queries),
            context.data_tenant_id,
            context.request_id.clone(),
        ))
    }

    /// Acquire the local admission owner for one SQL attempt.
    ///
    /// # Errors
    /// Returns the stable admission, timeout, cancellation, or SQL error from
    /// the retained Oracle admission owner.
    async fn admit_sql_query(
        &self,
        context: &AuthorizedQueryContext,
        query_class: QueryClass,
        local_ratio: f64,
        deadline: Instant,
        attempt_id: QueryId,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        let delegated = self.acquire_delegated_admission(context, query_class)?;
        let mut admitted = self
            .admission
            .admit_for_attempt(
                admission::PreparedAdmission {
                    tenant: context.data_tenant_id,
                    query_class,
                    local_ratio,
                    deadline,
                    cancellation: self.shutdown.child_token(),
                },
                attempt_id,
            )
            .await?;
        admitted.retain_delegated_grant(delegated);
        Ok(admitted)
    }

    /// Charges one dynamic-tenant delegated unit before local query admission.
    ///
    /// Both generic SQL and typed analytical plans use this exact background-
    /// backed gate, preserving admission-before-execution without giving either
    /// request path a `PostgreSQL` capability.
    ///
    /// This is synchronous and never waits. Durable capacity is refilled by a
    /// background worker; a query that arrives ahead of a refill is covered by
    /// bounded overdraft and proceeds under the local per-tenant ceilings. There
    /// is therefore no deadline to observe here — the operation cannot block, so
    /// it cannot consume the caller's remaining query budget.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAdmissionRejected`] when overdraft is
    /// exhausted, and [`BifrostError::OracleRoleUnavailable`] when delegated
    /// continuity is closed or local state cannot be read.
    fn acquire_delegated_admission(
        &self,
        context: &AuthorizedQueryContext,
        query_class: QueryClass,
    ) -> Result<DelegatedOracleAdmissionGrant, BifrostError> {
        self.delegated_admission
            .acquire(DelegatedAdmissionRequest {
                tenant_id: context.data_tenant_id,
                principal_id: context.principal.id,
                query_class,
            })
            .map_err(|error| match error {
                DelegatedOracleAdmissionError::QueueFull => BifrostError::QueryAdmissionRejected,
                DelegatedOracleAdmissionError::InvalidConfig
                | DelegatedOracleAdmissionError::ContinuityLost
                | DelegatedOracleAdmissionError::StateUnavailable => {
                    BifrostError::OracleRoleUnavailable
                }
            })
    }

    /// Delegates one SQL metadata attempt to the planner owner.
    async fn plan_sql_attempt(
        &self,
        context: &AuthorizedQueryContext,
        tables: &[TableRef],
        deadline: Instant,
    ) -> Result<PlannedSqlCut, BifrostError> {
        self.planner
            .pin_cut(context, tables, deadline, &self.catalog)
            .await
    }

    /// Acquires live fences, commits the read decision, and drains authorized tails.
    ///
    /// # Errors
    ///
    /// Returns timeout, visibility, audit, or parent-memory failures after
    /// releasing every fence acquired before the failure.
    async fn audit_and_drain_cut(
        &self,
        input: CutAuditInput<'_>,
    ) -> Result<DrainedTails, BifrostError> {
        let query_pool = input
            .admitted
            .memory_pool()
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        let drainer = TailFenceDrainer::new(
            &self.tails,
            &self.memory,
            TailFenceDrainerConfig {
                telemetry: Arc::clone(&self.telemetry),
                query_pool,
                query_class: input.query_class,
                deadline: input.deadline,
                cancellation: input.admitted.cancellation.clone(),
                freshness: input.request.freshness,
                query_id: input.admitted.query_id.into(),
                ticket_minter: self.tail_ticket_minter.clone(),
                cluster: Some(Arc::clone(&self.cluster)),
                discovery: self.tail_discovery_for_query(),
            },
        );
        let mut degraded = false;
        // Every Fused query drains its live tails on the leader. The single
        // planner owns distribution now, so there is no second architecture
        // that would instead ship a Scribe tail to a follower as an assignment.
        let acquired = if input.request.visibility == VisibilityMode::Fused {
            match drainer.acquire(input.cuts).await {
                Ok(fences) => fences,
                Err(error)
                    if input.request.freshness
                        == wyrd_spec::vala::api::FreshnessPolicy::AllowDegraded =>
                {
                    tracing::warn!(error = %error, "Oracle Fused cut omits unavailable live tail");
                    degraded = true;
                    Vec::new()
                }
                Err(error) => return Err(error),
            }
        } else {
            Vec::new()
        };
        let audit_span = tracing::info_span!(
            "bifrost.oracle.audit",
            audit_kind = "read_decision",
            query_class = query_class_label(input.query_class)
        );
        let audit_started = Instant::now();
        let result = async {
            let remaining = input
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(BifrostError::QueryTimeout)?;
            let decision = read_decision(
                input.context,
                &input.request.sql,
                input.cuts,
                input.request.visibility,
                input.query_class,
                input.deadline,
            )?;
            tokio::time::timeout(
                remaining,
                self.audit.append_read_decision(input.context, decision),
            )
            .await
            .map_err(|_| BifrostError::QueryTimeout)
            .and_then(|result| result)
        }
        .instrument(audit_span)
        .await;
        let _ = audit_started;
        if let Err(error) = result {
            tracing::error!(error = %error, "Oracle read-decision audit failed");
            let public_error = if error == BifrostError::QueryTimeout {
                error
            } else {
                BifrostError::QueryAuditUnavailable
            };
            drainer.release_acquired(acquired).await;
            return Err(public_error);
        }
        if acquired.is_empty() {
            Ok(DrainedTails {
                degraded,
                ..DrainedTails::default()
            })
        } else {
            let mut tail_data = drainer.drain(acquired).await?;
            tail_data.degraded |= degraded;
            Ok(tail_data)
        }
    }

    /// Accepts a typed lowered plan after validating its deadline.
    ///
    /// # Errors
    /// Returns a stable query error for elapsed deadlines, non-read-only plans,
    /// unavailable roles, admission failure, audit failure, or physical planning.
    #[tracing::instrument(
        name = "bifrost.oracle.query",
        skip_all,
        fields(
            visibility = visibility_label(options.visibility),
            query_class = "analytical",
            request_node_id = %self.admission.local_role.key.node_id.as_uuid(),
            leader_node_id = %self.admission.local_role.key.node_id.as_uuid(),
            search_role = "oracle"
        )
    )]
    pub async fn query_plan(
        &self,
        context: AuthorizedQueryContext,
        plan: datafusion::logical_expr::LogicalPlan,
        options: QueryOptions,
    ) -> Result<OracleQueryStream, BifrostError> {
        if options.deadline <= Instant::now() {
            return Err(BifrostError::QueryTimeout);
        }
        validate_read_only_plan(&plan)?;
        if !self.is_ready() {
            return Err(BifrostError::OracleRoleUnavailable);
        }
        let class = QueryClass::Analytical;
        let query_telemetry = self.telemetry.start_query(options.visibility, class);
        let delegated = self.acquire_delegated_admission(&context, class)?;
        let mut admitted = self
            .admission
            .admit(admission::PreparedAdmission {
                tenant: context.data_tenant_id,
                query_class: class,
                local_ratio: 0.0,
                deadline: options.deadline,
                cancellation: self.shutdown.child_token(),
            })
            .await?;
        admitted.retain_delegated_grant(delegated);
        self.execute_typed_plan(&context, plan, options, class, query_telemetry, admitted)
            .await
    }

    /// Fences the live tails a typed plan reads, audits the decision, then drains them.
    ///
    /// Ordering is load-bearing. Tails are fenced before the read decision is
    /// audited so the audited cut and the drained rows describe the same
    /// visibility, and a rejected audit releases every acquired fence before
    /// returning so a refused query leaves no tail pinned. Only `Fused`
    /// visibility acquires tails at all; every other mode reads the pinned
    /// sealed cut alone and drains nothing.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAdmissionRejected`] when the admitted owner
    /// carries no memory pool, and propagates fence-acquisition, audit, and
    /// drain failures unchanged. An audit failure releases acquired fences
    /// before it propagates; an acquisition or drain failure is settled by the
    /// drainer itself.
    async fn acquire_audit_and_drain_typed_tails(
        &self,
        context: &AuthorizedQueryContext,
        plan: &datafusion::logical_expr::LogicalPlan,
        options: QueryOptions,
        class: QueryClass,
        cuts: &[PinnedSealedTable],
        admitted: &AdmittedQueryGuard,
    ) -> Result<DrainedTails, BifrostError> {
        let fence_owner = TailFenceDrainer::new(
            &self.tails,
            &self.memory,
            TailFenceDrainerConfig {
                telemetry: Arc::clone(&self.telemetry),
                query_pool: admitted
                    .memory_pool()
                    .ok_or(BifrostError::QueryAdmissionRejected)?,
                query_class: class,
                deadline: options.deadline,
                cancellation: admitted.cancellation.clone(),
                freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
                query_id: admitted.query_id.into(),
                ticket_minter: self.tail_ticket_minter.clone(),
                cluster: Some(Arc::clone(&self.cluster)),
                discovery: self.tail_discovery_for_query(),
            },
        );
        let acquired = if options.visibility == VisibilityMode::Fused {
            fence_owner.acquire(cuts).await?
        } else {
            Vec::new()
        };
        if let Err(error) = self
            .audit_typed_decision(context, plan, options, class, cuts)
            .await
        {
            fence_owner.release_acquired(acquired).await;
            return Err(error);
        }
        if acquired.is_empty() {
            return Ok(DrainedTails::default());
        }
        fence_owner.drain(acquired).await
    }

    /// Installs immutable-cut providers and executes one typed plan.
    ///
    /// # Errors
    /// Returns typed capacity, timeout, audit, visibility, reconciliation, or
    /// execution failures and releases admission state through the returned
    /// stream owner. Before catalog or storage work, a one-byte reversible
    /// probe rejects a fully occupied or poisoned shared Oracle budget.
    async fn execute_typed_plan(
        &self,
        context: &AuthorizedQueryContext,
        plan: datafusion::logical_expr::LogicalPlan,
        options: QueryOptions,
        class: QueryClass,
        query_telemetry: QueryTelemetryGuard,
        admitted: AdmittedQueryGuard,
    ) -> Result<OracleQueryStream, BifrostError> {
        let cuts = self
            .planner
            .prepare_typed_cuts(
                &plan,
                context.data_tenant_id,
                options.deadline,
                &self.catalog,
            )
            .await?;
        let mut admitted = admitted;
        let session = match self.typed_execution_session(&admitted, &cuts) {
            Ok(session) => session,
            Err(error) => {
                return release_error(
                    options.deadline,
                    admitted,
                    error,
                    "typed execution lease rejection",
                );
            }
        };
        admitted.retain_physical_projections(&cuts)?;
        let logical_bytes_selected = Self::logical_selected_bytes(&cuts);
        let mut drained = self
            .acquire_audit_and_drain_typed_tails(context, &plan, options, class, &cuts, &admitted)
            .await?;
        let bound = match self.bind_execution_sources(
            &session.copied_config(),
            &admitted,
            absolute_deadline(options.deadline),
            PlannedSources {
                query_class: class,
                cuts: &cuts,
                root: None,
                drained: &mut drained,
            },
        ) {
            Ok(bound) => bound,
            Err(error) => {
                return release_error(options.deadline, admitted, error, "typed binding rejection");
            }
        };
        let degraded = bound
            .get()
            .is_some_and(bindings::OracleExecutionBindings::degraded);
        let providers = self
            .planner
            .build_typed_providers(planner::TypedProviderInputs {
                context,
                cuts,
                catalog: &self.catalog,
                audit: Arc::clone(&self.audit),
            })
            .await?;
        let rewritten = OraclePlanner::replace_typed_sources(plan, &providers)?;
        let (session, physical) = OraclePlanner::create_physical_plan(&rewritten, session).await?;
        let scan_stats = OracleQueryScanStats::from_plan(physical.as_ref(), logical_bytes_selected);
        let schema = physical.schema();
        let mut batches = execute_stream(physical, session.task_ctx())
            .map_err(|error| map_datafusion_error(&error))?;
        let first = await_first_batch(&mut batches, options.deadline).await?;
        if let Some(error) = map_first_batch_failure(first.as_ref()) {
            return release_error(
                options.deadline,
                admitted,
                error,
                "typed first-batch rejection",
            );
        }
        let (ipc, schema_frame) = QueryIpcEncoder::new(&schema)?;
        Ok(OracleQueryStream::new(QueryStreamInput {
            // The typed plan path has no Analytical candidate to select.
            execution_path: QueryExecutionPath::Interactive,
            schema_frame,
            ipc,
            batches,
            first,
            admitted,
            deadline: options.deadline,
            deadline_ms: absolute_deadline_ms(options.deadline),
            visibility: options.visibility,
            freshness_policy: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            degraded_sources: Arc::new(std::sync::Mutex::new(if degraded {
                vec![DegradedPartition {
                    ordinal: u32::MAX,
                    reason: "live_tail_unavailable",
                    sources: vec![QuerySource::LiveTail],
                }]
            } else {
                Vec::new()
            })),
            query_telemetry,
            scan_stats,
            gate_lifecycle: None,
            running_query: None,
        }))
    }

    /// Builds and durably appends one typed-plan success decision within its deadline.
    ///
    /// The operation runs only after the caller has acquired the complete optional
    /// Fused cut. It records the canonical audit latency and leaves acquired-fence
    /// cleanup to the typed execution workflow that owns those resources.
    ///
    /// # Errors
    ///
    /// Returns query timeout when no deadline remains or the append exceeds it,
    /// query audit unavailable when the durable writer refuses the event, or the
    /// typed decision-construction error for an invalid immutable cut.
    async fn audit_typed_decision(
        &self,
        context: &AuthorizedQueryContext,
        plan: &datafusion::logical_expr::LogicalPlan,
        options: QueryOptions,
        class: QueryClass,
        cuts: &[PinnedSealedTable],
    ) -> Result<(), BifrostError> {
        let audit_span = tracing::info_span!(
            "bifrost.oracle.audit",
            audit_kind = "read_decision",
            query_class = query_class_label(class)
        );
        let audit_started = Instant::now();
        let result = async {
            let remaining = options
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(BifrostError::QueryTimeout)?;
            tokio::time::timeout(
                remaining,
                self.audit.append_read_decision(
                    context,
                    plan_read_decision(context, plan, options, class, cuts)?,
                ),
            )
            .await
            .map_err(|_| BifrostError::QueryTimeout)?
            .map_err(|_| BifrostError::QueryAuditUnavailable)
        }
        .instrument(audit_span)
        .await;
        let _ = audit_started;
        result
    }

    /// Returns whether startup readiness completed and queries may enter admission.
    ///
    /// A composed Analytical half is a third input, not a parallel readiness
    /// state: a node retaining a graph it could not settle still holds that
    /// graph's supervisor guard, reservation residue, and charged envelope, so
    /// it must stop advertising itself even while startup is reconciled and
    /// admission has capacity. An Oracle composed without Analytical remains
    /// governed by the first two predicates alone.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.startup_reconciled()
            && self.admission.is_available()
            && self
                .analytical
                .as_ref()
                .is_none_or(|analytical| analytical.is_healthy())
    }

    /// Publishes a membership snapshot to future local admissions.
    pub fn refresh_membership(&self, snapshot: &ClusterSnapshot) {
        self.admission.refresh(snapshot);
    }

    /// Captures local admission and peer reservations without external IO.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn runtime_inspection(&self) -> OracleRuntimeInspection {
        self.admission.runtime_inspection()
    }

    /// Captures every local readiness input without performing network or SQL IO.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn readiness_snapshot(&self) -> OracleReadinessSnapshot {
        OracleReadinessSnapshot {
            startup_reconciled: self.startup_reconciled(),
            live_oracles: self.cluster.snapshot().live_oracles().len(),
            running_capacity: self.admission.slots.running_capacity(),
        }
    }

    /// Reports whether local startup readiness completed before membership activation.
    ///
    /// Server boot uses this dependency-local phase to avoid waiting on
    /// [`Self::is_ready`], which intentionally also requires the later durable
    /// membership activation and immutable snapshot publication.
    #[must_use]
    pub fn startup_reconciled(&self) -> bool {
        self.ready.load(Ordering::Acquire) && !self.shutdown.is_cancelled()
    }

    /// Awaits the exact startup result before role activation.
    ///
    /// # Errors
    /// Returns the recovery failure, a duplicate-wait invariant, or task loss.
    pub async fn await_startup(&self) -> Result<(), BifrostError> {
        let receiver = self
            .startup_result
            .lock()
            .map_err(|_| BifrostError::Internal {
                detail: "Oracle startup result lock is poisoned".to_owned(),
            })?
            .take()
            .ok_or_else(|| BifrostError::Internal {
                detail: "Oracle startup result was already consumed".to_owned(),
            })?;
        receiver.await.map_err(|_| BifrostError::Internal {
            detail: "Oracle startup task ended without a result".to_owned(),
        })?
    }

    /// Borrows the tenant SQL root retained by the Oracle composition boundary.
    #[must_use]
    pub fn tenant_sql(&self) -> &ValaPostgres {
        &self.vala
    }

    /// Returns the exact-role local delegated policy admission owner.
    #[must_use]
    pub fn delegated_admission(&self) -> &Arc<DelegatedOracleAdmission> {
        &self.delegated_admission
    }

    /// Transfers the typed continuity-loss receiver to the distributed query owner.
    ///
    /// # Errors
    ///
    /// Returns an internal error when the receiver was already transferred or
    /// its ownership lock is poisoned.
    pub fn take_delegated_continuity_loss(
        &self,
    ) -> Result<
        tokio::sync::mpsc::Receiver<wyrd_spec::vala::api::OracleAdmissionContinuityLost>,
        BifrostError,
    > {
        self.delegated_loss
            .lock()
            .map_err(|_| BifrostError::Internal {
                detail: "delegated continuity receiver lock is poisoned".to_owned(),
            })?
            .take()
            .ok_or_else(|| BifrostError::Internal {
                detail: "delegated continuity receiver was already transferred".to_owned(),
            })
    }

    /// Cancels lifecycle maintenance and drains owned cleanup until `deadline`.
    ///
    /// The lifecycle task is aborted at expiry. Dropping this future can leave
    /// local cleanup to guard drop, but
    /// [`Self::begin_shutdown`] has already synchronously rejected new work.
    pub async fn shutdown(&self, deadline: Instant) -> admission::OracleShutdownReport {
        self.begin_shutdown();
        // Before the capacity report, not after. A follower graph holds this
        // node's peer running permit through its lease, and a coordinator that
        // is itself shutting down will never send the message that would
        // release it — so a node that skipped this reports peer capacity still
        // charged for work that can no longer run.
        if let Some(analytical) = self.analytical.as_ref()
            && let Err(error) = analytical.worker().shutdown().await
        {
            tracing::warn!(
                %error,
                "Oracle analytical follower ownership did not release during shutdown"
            );
        }
        let report = self.admission.shutdown(deadline).await;
        if report.active_queries != 0
            || report.queued_queries != 0
            || report.peer_pending != 0
            || report.peer_running != 0
        {
            tracing::warn!(
                active_queries = report.active_queries,
                queued_queries = report.queued_queries,
                reserved_memory_bytes = report.reserved_memory_bytes,
                reserved_spill_bytes = report.reserved_spill_bytes,
                peer_pending = report.peer_pending,
                peer_running = report.peer_running,
                "Oracle shutdown reached deadline with residual local admission state"
            );
        }
        let maintenance = self
            .maintenance
            .lock()
            .ok()
            .and_then(|mut handle| handle.take());
        if let Some(mut maintenance) = maintenance {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or(Duration::ZERO);
            if tokio::time::timeout(remaining, &mut maintenance)
                .await
                .is_err()
            {
                maintenance.abort();
            }
        }
        let delegated = self
            .delegated_maintenance
            .lock()
            .ok()
            .and_then(|mut handle| handle.take());
        if let Some(mut delegated) = delegated {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or(Duration::ZERO);
            if tokio::time::timeout(remaining, &mut delegated)
                .await
                .is_err()
            {
                delegated.abort();
            }
        }
        let _ = deadline;
        report
    }

    /// Cancels Oracle lifecycle work without awaiting cleanup progress.
    ///
    /// This no-await operation is safe at an exhausted process deadline. It
    /// starts no external cleanup and leaves local guard cleanup authoritative.
    pub fn begin_shutdown(&self) {
        self.admission.close();
        self.shutdown.cancel();
    }

    /// Registers every pinned cut as a provider on one planning session.
    ///
    /// Every provider is planning-only: it captures the frozen source identity,
    /// schema, and tenant context, and nothing post-admission. The rows a Fused
    /// query returns arrive later through the execution bindings, so a provider
    /// built here is safe to plan against before the query is admitted.
    ///
    /// # Errors
    ///
    /// Returns a mapped `DataFusion` error when local hot sources cannot be
    /// resolved or a provider cannot be constructed, and the stable
    /// registration error when a binding cannot be installed.
    async fn register_cut_providers(
        &self,
        session: &SessionContext,
        context: &AuthorizedQueryContext,
        cuts: &[PinnedSealedTable],
        destinations: &[dispatcher::DispatchCandidate],
    ) -> Result<(), BifrostError> {
        for (index, cut) in cuts.iter().enumerate() {
            let table_name = cut.binding.table_ref.fqn();
            // A cut delegates its persisted sources only when the frozen roster
            // actually holds another Oracle and this cut has a published or hot
            // object for it to read. Every other cut stays leader-local, which
            // is what leaves a leader-executable query's root normal.
            let scannable = !cut.iceberg_files.is_empty() || !cut.hot_files.is_empty();
            let remote = destinations
                .get(index % destinations.len().max(1))
                .filter(|_| scannable)
                .map(|destination| exec::OracleRemoteSource {
                    table: table_name.clone(),
                    destination: destination.clone(),
                    iceberg: !cut.iceberg_files.is_empty(),
                });
            // The leader keeps its own hot sources even when a remote owner is
            // frozen. The placeholder that substitutes them still carries them
            // as its local plan, so dropping them here would leave the leader
            // with nothing to read whenever the planner declines to distribute
            // this cut. The follower reads the same files from its assignment.
            let hot_files = self.local_hot_sources(cut)?;
            tracing::debug!(
                target: "wyrd::oracle::planning",
                table = %cut.binding.table_ref.fqn(),
                destinations = destinations.len(),
                iceberg_files = cut.iceberg_files.len(),
                hot_files = cut.hot_files.len(),
                delegated = remote.is_some(),
                "oracle cut provider registration"
            );
            let provider = OracleTableProvider::try_new(OracleTableInputs {
                table: cut.iceberg_table.clone(),
                storage: Arc::clone(self.catalog.storage()),
                hot_files,
                iceberg_event_times: cut
                    .iceberg_files
                    .iter()
                    .map(|file| file.event_time)
                    .collect(),
                context: context.clone(),
                table_name,
                audit: Arc::clone(&self.audit),
                remote,
            })
            .await
            .map_err(|error| map_datafusion_error(&error))?;
            register_session_table(
                session,
                &cut.binding,
                Arc::new(provider) as Arc<dyn TableProvider>,
            )?;
        }
        Ok(())
    }

    /// Resolves leader-local hot file locations for one pinned cut.
    ///
    /// # Errors
    ///
    /// Returns metadata mismatch for process-size overflow or an invalid bound path.
    fn local_hot_sources(
        &self,
        cut: &PinnedSealedTable,
    ) -> Result<Vec<HotFileSource>, BifrostError> {
        cut.hot_files
            .iter()
            .map(|file| {
                let size_bytes = usize::try_from(file.file_size).map_err(|_| {
                    BifrostError::MetadataMismatch {
                        detail: "hot file size exceeds process bounds".to_owned(),
                    }
                })?;
                Ok(HotFileSource {
                    metadata_key: exec::hot_metadata_key(file, size_bytes)?,
                    location: self
                        .catalog
                        .object_location(&cut.binding, &file.file_path)
                        .map_err(BifrostCatalogError::into_public)?,
                    size_bytes,
                    event_time:
                        crate::catalog::event_time::EventTimeStatistics::from_catalog_timestamps(
                            file.min_event_time,
                            file.max_event_time,
                        ),
                })
            })
            .collect()
    }

    /// Sums immutable selected file sizes before execution starts.
    /// Counts the independently scannable files pinned across one cut set.
    ///
    /// This is the parallelism budget the cut actually offers. `DataFusion`
    /// cannot usefully spread a scan across more partitions than there are
    /// files to read, so this bounds the admitted partition ceiling before the
    /// session is built. Both sealed Iceberg files and hot Scribe files count,
    /// since each is an independently openable scan target.
    fn scannable_work_units(cuts: &[PinnedSealedTable]) -> usize {
        cuts.iter().fold(0_usize, |total, cut| {
            total
                .saturating_add(cut.iceberg_files.len())
                .saturating_add(cut.hot_files.len())
        })
    }

    fn logical_selected_bytes(cuts: &[PinnedSealedTable]) -> u64 {
        cuts.iter().fold(0_u64, |total, cut| {
            let iceberg = cut
                .iceberg_files
                .iter()
                .map(|file| file.file_size)
                .sum::<u64>();
            let hot = cut
                .hot_files
                .iter()
                .filter_map(|file| u64::try_from(file.file_size).ok())
                .sum::<u64>();
            total.saturating_add(iceberg).saturating_add(hot)
        })
    }

    /// Builds the one physical root this query will execute.
    ///
    /// Providers are registered on a planning-only session whose config comes
    /// from the minimum grant admission can succeed with, so the retained root
    /// is shaped by the cut rather than by whatever capacity the query is later
    /// granted. When this node composed the Analytical owners the build runs
    /// through the pinned distributed planner, which is what lets the root come
    /// back as a `DistributedExec`; the class is then read from that root alone.
    ///
    /// Nothing here admits, reserves, publishes, or reads a row. The planning
    /// session is dropped as soon as the root and its config are retained.
    ///
    /// # Errors
    ///
    /// Returns the mapped planning failure, which is terminal for this query:
    /// there is no second build and no fallback path.
    async fn build_physical_root(
        &self,
        context: &AuthorizedQueryContext,
        sql: &str,
        cuts: &[PinnedSealedTable],
        oracles: &[participant_cut::OracleQueryParticipant],
        work_units: usize,
    ) -> Result<RetainedPhysicalPlan, BifrostError> {
        let shape = crate::resources::OracleSessionShape::for_grant(
            crate::resources::ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            crate::resources::ORACLE_MIN_TARGET_PARTITIONS,
            work_units,
        );
        // Installed empty before any planning happens. The lock owns no runtime
        // and no memory; it is the one place a planned leaf's concrete source,
        // grant, and drained batches arrive once, after admission.
        let config = shape
            .session_config()
            .with_extension(Arc::new(bindings::OracleExecutionLock::new()));
        let state = datafusion::execution::session_state::SessionStateBuilder::new()
            .with_default_features()
            .with_config(config)
            .with_runtime_env(Arc::clone(&self.planning_runtime))
            .build();
        let planning = SessionContext::new_with_state(state);
        // Substitution is unconditional once a roster exists: the placeholder
        // carries the real leaf as its local plan, so a cut the planner leaves
        // on the leader is still read here. Gating on work units instead would
        // make remoteness a property of how many files a cut happens to hold,
        // which cannot express a query whose scan stays local while its
        // aggregate distributes.
        let destinations = match self.analytical.as_ref() {
            Some(handle) => handle.frozen_destinations(oracles)?,
            None => Vec::new(),
        };
        self.register_cut_providers(&planning, context, cuts, &destinations)
            .await?;
        let planning = match self.analytical.as_ref() {
            Some(handle) => handle.planning_session(&planning, oracles, work_units)?,
            None => planning,
        };
        if self.take_analytical_plan_failure() {
            return Err(BifrostError::QueryExecutionFailed);
        }
        let root = Self::plan_physical(&planning, sql)
            .await
            .map_err(|OracleExecutionError::Public(error)| error)?;
        Ok(RetainedPhysicalPlan {
            config: planning.copied_config(),
            root,
        })
    }

    /// Lowers one validated statement to its optimized physical plan.
    ///
    /// # Errors
    ///
    /// Returns a stable execution failure when the statement cannot be planned
    /// or optimized against the session's registered providers.
    async fn plan_physical(
        session: &SessionContext,
        sql: &str,
    ) -> Result<Arc<dyn ExecutionPlan>, OracleExecutionError> {
        let frame = session
            .sql(sql)
            .await
            .map_err(|error| map_datafusion_error(&error))?;
        frame
            .create_physical_plan()
            .await
            .map_err(|error| map_datafusion_error(&error))
            .map_err(OracleExecutionError::from)
    }
}

/// Projects omitted and zero request budgets to the configured immutable default.
fn projected_request_deadline(requested_ms: Option<u64>, default: Duration) -> Duration {
    requested_ms
        .filter(|deadline_ms| *deadline_ms != 0)
        .map_or(default, Duration::from_millis)
}

/// Returns exact deadline projection observations from the production helper.
#[cfg(feature = "test-support")]
#[must_use]
pub fn deadline_projection_for_test() -> (Duration, Duration, Duration, bool) {
    let default = Duration::from_secs(30);
    let omitted = projected_request_deadline(None, default);
    let zero = projected_request_deadline(Some(0), default);
    let explicit = projected_request_deadline(Some(125), default);
    let deadline = Instant::now() + explicit;
    let first = deadline.saturating_duration_since(Instant::now());
    std::thread::sleep(Duration::from_millis(1));
    let second = deadline.saturating_duration_since(Instant::now());
    (omitted, zero, explicit, second < first)
}

/// Awaits one stream lookahead within the query's absolute deadline.
///
/// # Errors
///
/// Returns [`BifrostError::QueryTimeout`] when the deadline has elapsed or the
/// lookahead does not complete in the remaining interval. Cancellation of the
/// caller drops the pending poll without consuming a later batch.
async fn await_first_batch(
    batches: &mut SendableRecordBatchStream,
    deadline: Instant,
) -> Result<Option<datafusion::error::Result<RecordBatch>>, BifrostError> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or(BifrostError::QueryTimeout)?;
    tokio::time::timeout(remaining, batches.next())
        .await
        .map_err(|_| BifrostError::QueryTimeout)
}

/// Projects one pinned Iceberg manifest entry into the descriptor a follower is
/// signed to read.
///
/// The pinned snapshot is the object's publication authority, so a cut with no
/// snapshot yields zero — a value descriptor validation refuses — rather than
/// inventing one. Unusable event-time statistics reach the wire as the absent
/// pair, which is the descriptor's only representation of "retain this file".
fn iceberg_file_descriptor(
    file: &crate::catalog::PinnedIcebergFile,
    snapshot_id: Option<i64>,
) -> PersistedFileDescriptor {
    let (min_event_time_micros, max_event_time_micros) = file.event_time.descriptor_pair();
    PersistedFileDescriptor::Iceberg(wyrd_spec::vala::api::IcebergFileDescriptor {
        path: file.file_path.clone(),
        size_bytes: file.file_size,
        row_count: file.row_count,
        snapshot_id: snapshot_id.unwrap_or_default(),
        min_event_time_micros,
        max_event_time_micros,
    })
}

/// Projects one unresolved `vala.file_list` row into the descriptor a follower
/// is signed to read.
///
/// The row's identity and decoded checksum are what let a follower resolve the
/// object without querying the catalog by path, and are what make the hot
/// metadata cache key safe: two distinct objects that shared a key would return
/// one object's footer for the other's rows.
///
/// # Errors
/// Returns [`BifrostError::QueryExecutionFailed`] when the row's checksum is
/// absent, not valid hex, not exactly 32 decoded bytes, or all zero, and when
/// its durable size or row count cannot be represented. Defaulting any of these
/// would mint a valid-looking identity — every unchecksummed object in a tenant
/// would share the all-zero key — so the query fails here, before the
/// descriptor is signed and before any provider, cache, or object I/O sees it.
fn hot_file_descriptor(
    row: &vala_sql::row_types::file_list::HotFileRow,
) -> Result<PersistedFileDescriptor, BifrostError> {
    let event_time = crate::catalog::event_time::EventTimeStatistics::from_catalog_timestamps(
        row.min_event_time,
        row.max_event_time,
    );
    let (min_event_time_micros, max_event_time_micros) = event_time.descriptor_pair();
    let sha256 = row
        .file_checksum
        .as_deref()
        .and_then(|hex| hex::decode(hex).ok())
        .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
        .filter(|sha256| sha256 != &[0_u8; 32])
        .ok_or(BifrostError::QueryExecutionFailed)?;
    Ok(PersistedFileDescriptor::Hot(
        wyrd_spec::vala::api::HotFileDescriptor {
            path: row.file_path.clone(),
            size_bytes: u64::try_from(row.file_size)
                .map_err(|_| BifrostError::QueryExecutionFailed)?,
            row_count: u64::try_from(row.row_count)
                .map_err(|_| BifrostError::QueryExecutionFailed)?,
            file_list_id: row.id,
            sha256,
            min_event_time_micros,
            max_event_time_micros,
        },
    ))
}

/// Everything one attempt's binding step needs to publish its planned sources.
///
/// Grouping these keeps the class, the pinned cuts, the retained root, and the
/// drained tails travelling together: each is only meaningful for the same
/// single attempt, and the retained root is what decides which of the cuts'
/// sources were actually planned as remote.
struct PlannedSources<'a> {
    /// Class derived from the retained physical root and admitted under.
    query_class: QueryClass,
    /// The attempt's immutable pinned table cuts, in plan order.
    cuts: &'a [PinnedSealedTable],
    /// Retained physical root, absent on the typed path, which plans its own
    /// providers and never produces a remote leaf.
    root: Option<&'a dyn ExecutionPlan>,
    /// Audited drain output, moved into the published bindings.
    drained: &'a mut DrainedTails,
}

/// Collects every remote source placeholder reachable from one retained root.
///
/// The retained root is the only authority on which sources the single planner
/// actually kept: a cut may pin a remote owner and still be planned away by
/// projection or pruning, and binding an assignment for a leaf that no longer
/// exists would make the binding set disagree with the plan it governs.
pub(super) fn remote_placeholders(
    root: &dyn ExecutionPlan,
) -> Vec<codec::RemoteSourcePlaceholderExec> {
    let mut found = Vec::new();
    let mut pending: Vec<&dyn ExecutionPlan> = vec![root];
    while let Some(node) = pending.pop() {
        if let Some(placeholder) = node.downcast_ref::<codec::RemoteSourcePlaceholderExec>() {
            found.push(placeholder.clone());
        }
        // A leaf that was scaled up is wrapped in `DistributedLeafExec`, whose
        // per-task variants are deliberately not its `children`. Both callers —
        // the binder that must bind every planned remote source and the router
        // that must place its stage on that source's frozen peer — would
        // otherwise read a split leaf as if the plan had no remote source at
        // all, so the descent belongs in this one walk.
        if let Some(split) = node.downcast_ref::<datafusion_distributed::DistributedLeafExec>() {
            pending.push(split.original().as_ref());
            pending.extend(split.variants().iter().map(Arc::as_ref));
        }
        for child in node.children() {
            pending.push(child.as_ref());
        }
    }
    found
}

/// Builds the one assignment a cut's frozen remote owner is signed to read.
///
/// Every object the cut pinned in `tier` becomes a descriptor here, in cut
/// order, because the leader registered no local reader for them. One
/// assignment names one tier: the follower resolves its whole descriptor list
/// through a single reader and refuses a mixed list. The closure and predicates
/// come from the planned placeholder rather than being recomputed, so what the
/// follower is authorized to read is exactly what the retained plan projected.
///
/// # Errors
///
/// Returns [`BifrostError::QueryExecutionFailed`] when a pinned hot row carries
/// no usable durable identity, which would otherwise mint a valid-looking but
/// colliding object key.
fn follower_scan_assignment(
    cut: &PinnedSealedTable,
    tier: RemotePersistedTier,
    placeholder: &codec::RemoteSourcePlaceholderExec,
) -> Result<wyrd_spec::vala::api::FollowerScanAssignment, BifrostError> {
    let files = match tier {
        RemotePersistedTier::Iceberg => cut
            .iceberg_files
            .iter()
            .map(|file| iceberg_file_descriptor(file, cut.snapshot_id))
            .collect::<Vec<_>>(),
        RemotePersistedTier::Hot => cut
            .hot_files
            .iter()
            .map(hot_file_descriptor)
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(wyrd_spec::vala::api::FollowerScanAssignment {
        scan_id: placeholder.scan_id().to_owned(),
        binding: tail_fence::TailFenceDrainer::wire_binding(cut)?,
        persisted: wyrd_spec::vala::api::PersistedFileAssignment { files },
        scribe_provider_cut: None,
        // The planned placeholder already carries the full-schema fingerprint
        // the follower validates against; recomputing it here from the
        // placeholder's own projected schema would name the closure instead.
        schema_fingerprint: placeholder.schema_fingerprint().to_owned(),
        required_columns: placeholder.required_columns().to_vec(),
        predicates: placeholder.predicates().to_vec(),
    })
}

/// The persisted tier one remote source names.
///
/// A follower assignment resolves its whole descriptor list through a single
/// reader — the catalog provider's scan for compacted output, `HotParquetExec`
/// for staged output — so a cut holding both tiers delegates them as two remote
/// sources rather than one list no reader can serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemotePersistedTier {
    /// Data files in the cut's pinned Iceberg snapshot.
    Iceberg,
    /// Staged hot Parquet the pinned snapshot's manifest does not name.
    Hot,
}

impl RemotePersistedTier {
    /// Returns the stable suffix this tier contributes to a scan identity.
    const fn tag(self) -> &'static str {
        match self {
            Self::Iceberg => "iceberg",
            Self::Hot => "hot",
        }
    }
}

/// Derives the scan identity one physical remote-scan occurrence binds by.
///
/// `occurrence` is the provider's request-local scan ordinal, so a same-table
/// self-join or a repeated CTE mints one identity per physical occurrence
/// instead of two leaves sharing — and overwriting — a single assignment. It is
/// minted at this one site and copied onto the assignment, never recomputed.
fn persisted_follower_scan_id(table: &str, tier: RemotePersistedTier, occurrence: u64) -> String {
    format!("oracle:{table}:{}:{occurrence}", tier.tag())
}

/// Computes the fingerprint a follower scan assignment must carry for a schema,
/// after canonicalizing equivalent UTC timezone spellings.
///
/// Iceberg projects UTC as `+00:00`, while Arrow's Parquet reader projects the
/// same logical timezone as `UTC`. This boundary removes that adapter spelling
/// drift without weakening any column, order, or non-UTC type check.
///
/// Every component that builds or validates a `FollowerScanAssignment` must use
/// this function; the follower compares its own result against the assignment's
/// value after resolving the provider, so an independently derived fingerprint
/// is rejected on any spelling difference.
pub fn assignment_schema_fingerprint(schema: &Schema) -> String {
    let fields = schema
        .fields()
        .iter()
        .map(|field| {
            let data_type = match field.data_type() {
                DataType::Timestamp(unit, Some(timezone)) if timezone.as_ref() == "+00:00" => {
                    DataType::Timestamp(*unit, Some("UTC".into()))
                }
                data_type => data_type.clone(),
            };
            Field::new(field.name(), data_type, field.is_nullable())
        })
        .collect::<Vec<_>>();
    hex::encode(SchemaFingerprint::from_arrow_schema(&Schema::new(fields)).as_ref())
}

/// Registers one table beneath its explicit Wyrd catalog/schema hierarchy.
///
/// `DataFusion` does not synthesize catalog providers when a three-part table
/// reference is registered. Oracle therefore creates the tenant-free logical
/// hierarchy (`vala.<domain>.<table>`) explicitly while the provider itself
/// remains bound to the authenticated tenant's physical cut.
///
/// # Errors
///
/// Returns a stable planning failure when the logical namespace is malformed
/// or `DataFusion` rejects a duplicate/incompatible schema or table.
fn register_session_table(
    session: &SessionContext,
    binding: &crate::catalog::TenantTableBinding,
    provider: Arc<dyn TableProvider>,
) -> Result<(), BifrostError> {
    let schema_name = binding
        .logical_namespace
        .strip_prefix("vala.")
        .filter(|name| !name.is_empty())
        .ok_or(BifrostError::QueryExecutionFailed)?;
    let catalog = session.catalog("vala").unwrap_or_else(|| {
        let catalog: Arc<dyn CatalogProvider> = Arc::new(MemoryCatalogProvider::new());
        session.register_catalog("vala", Arc::clone(&catalog));
        catalog
    });
    let schema = if let Some(schema) = catalog.schema(schema_name) {
        schema
    } else {
        let schema = Arc::new(MemorySchemaProvider::new());
        catalog
            .register_schema(schema_name, schema.clone())
            .map_err(|error| map_datafusion_error(&error))?;
        schema
    };
    let alias = Arc::clone(&provider);
    schema
        .register_table(binding.table_name.clone(), provider)
        .map_err(|error| map_datafusion_error(&error))?;
    // DataFusion treats a quoted dotted identifier (`"vala.traces.spans"`)
    // as one table in its default `datafusion.public` catalog. Keep this
    // private alias alongside the canonical `vala.traces.spans` hierarchy so
    // both public SQL spellings resolve to the same authenticated provider.
    session
        .register_table(TableReference::bare(binding.table_ref.fqn()), alias)
        .map_err(|error| map_datafusion_error(&error))?;
    Ok(())
}

/// Stream of terminal-aware logical query frames.
pub type OracleFrameStream = dyn Stream<Item = Result<QueryStreamFrame, BifrostError>> + Send;

/// Returns a terminal failed frame for a late execution error.
///
/// This entry is for failures that never reached Analytical selection, so the
/// terminal names [`QueryExecutionPath::Interactive`]: REQ-002 makes selection
/// irreversible, and a caller must never read a path the server did not run.
#[must_use]
pub fn failed_terminal(code: QueryTerminalErrorCode, row_count: u64) -> QueryTerminalFrame {
    failed_terminal_for_visibility(
        code,
        row_count,
        VisibilityMode::PublishedOnly,
        QueryExecutionPath::Interactive,
    )
}

/// Returns a contract-valid failed terminal for the query's visibility cut.
///
/// `execution_path` is the path the stream had already selected, so a failure
/// after Analytical selection reports `Analytical` rather than silently
/// presenting itself as an Interactive failure.
fn failed_terminal_for_visibility(
    code: QueryTerminalErrorCode,
    row_count: u64,
    visibility: VisibilityMode,
    execution_path: QueryExecutionPath,
) -> QueryTerminalFrame {
    let mut source_completion = vec![
        SourceCompletion {
            source: QuerySource::Iceberg,
            outcome: SourceCompletionOutcome::Complete,
        },
        SourceCompletion {
            source: QuerySource::HotSealed,
            outcome: SourceCompletionOutcome::Complete,
        },
    ];
    if visibility == VisibilityMode::Fused {
        source_completion.push(SourceCompletion {
            source: QuerySource::LiveTail,
            outcome: SourceCompletionOutcome::Complete,
        });
    }
    QueryTerminalFrame {
        outcome: QueryTerminalOutcome::Failed,
        freshness: wyrd_spec::vala::api::QueryFreshness::Complete,
        execution_path,
        row_count,
        warnings: Vec::new(),
        source_completion,
        error: Some(wyrd_spec::vala::api::QueryTerminalError { code, detail: None }),
        // A failed stream never finished its Arrow IPC stream, so there is no
        // end-of-stream delta to report.
        arrow_ipc_eos: Vec::new(),
    }
}

/// Validates every row's hidden tenant column without filtering mismatches.
///
/// # Errors
/// Returns [`BifrostError::QueryTenantInvariant`] when the managed column is
/// absent, null, or differs from the authenticated tenant.
pub fn validate_tenant_batch(
    batch: &RecordBatch,
    tenant: DataTenantId,
) -> Result<(), BifrostError> {
    let index = batch
        .schema()
        .index_of("data_tenant_id")
        .map_err(|_| BifrostError::QueryTenantInvariant)?;
    let values = batch
        .column(index)
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .ok_or(BifrostError::QueryTenantInvariant)?;
    let expected = tenant.to_string();
    if (0..values.len()).any(|row| values.is_null(row) || values.value(row) != expected) {
        return Err(BifrostError::QueryTenantInvariant);
    }
    Ok(())
}

/// Parses one exact SQL query and returns its distinct canonical table references.
///
/// # Errors
///
/// Returns invalid SQL for parse failures, non-query statements, unknown
/// namespaces, unsafe names, or a statement that references no Bifrost table.
fn parse_select_tables(sql: &str) -> Result<Vec<TableRef>, BifrostError> {
    let statements = DFParser::parse_sql(sql).map_err(|error| BifrostError::QueryInvalidSql {
        detail: format!("parse error: {error}"),
    })?;
    if statements.len() != 1 {
        return Err(BifrostError::QueryInvalidSql {
            detail: format!(
                "exactly one SELECT statement is required; found {}",
                statements.len()
            ),
        });
    }
    let statement = statements
        .into_iter()
        .next()
        .ok_or_else(|| BifrostError::QueryInvalidSql {
            detail: "exactly one SELECT statement is required".to_owned(),
        })?;
    if !matches!(&statement, DfStatement::Statement(inner) if matches!(**inner, SqlStatement::Query(_)))
    {
        return Err(BifrostError::QueryInvalidSql {
            detail: "only a single SELECT statement is supported".to_owned(),
        });
    }
    let (relations, _) = resolve_table_references(&statement, true).map_err(|error| {
        BifrostError::QueryInvalidSql {
            detail: format!("table reference error: {error}"),
        }
    })?;
    if relations.is_empty() {
        return Err(BifrostError::QueryInvalidSql {
            detail: "query must reference at least one Bifrost table".to_owned(),
        });
    }
    relations
        .into_iter()
        .map(|reference| {
            let name = reference.to_string();
            TableRef::parse_fqn(&name).ok_or_else(|| BifrostError::QueryInvalidSql {
                detail: "query contains an invalid Bifrost table reference".to_owned(),
            })
        })
        .collect()
}

/// Computes a stable scrubbed digest from whitespace-normalized SQL or plan text.
fn query_digest(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(normalized.as_bytes()))
    )
}

/// Converts one normalized value into the bounded T1 audit digest type.
///
/// # Errors
///
/// Returns audit unavailable when the locked audit scalar rejects the digest.
fn audit_digest(value: &str) -> Result<QueryAuditDigest, BifrostError> {
    QueryAuditDigest::new(query_digest(value)).map_err(|_| BifrostError::QueryAuditUnavailable)
}

/// Produces one digest from an ordered list without exposing its source values.
///
/// # Errors
///
/// Returns audit unavailable when the locked audit scalar rejects the digest.
fn aggregate_audit_digest<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Result<QueryAuditDigest, BifrostError> {
    let joined = values.into_iter().collect::<Vec<_>>().join("\n");
    audit_digest(&joined)
}

/// Builds the exact locked read-decision detail for one SQL visibility cut.
///
/// # Errors
///
/// Returns audit unavailable when a digest or T1 bounded invariant is invalid,
/// and timeout when no positive settled deadline remains.
fn read_decision(
    context: &AuthorizedQueryContext,
    sql: &str,
    cuts: &[PinnedSealedTable],
    visibility: VisibilityMode,
    query_class: QueryClass,
    deadline: Instant,
) -> Result<BifrostQueryReadDecision, BifrostError> {
    let mut binding_digests = cuts
        .iter()
        .map(|cut| audit_digest(&cut.binding.table_ref.fqn()))
        .collect::<Result<Vec<_>, _>>()?;
    binding_digests.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    binding_digests.dedup_by(|left, right| left.as_str() == right.as_str());
    let deadline_ms = u64::try_from(
        deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?
            .as_millis()
            .max(1),
    )
    .map_err(|_| BifrostError::QueryAuditUnavailable)?;
    let slot_units = admission_limits(u32::MAX, query_class).1;
    BifrostQueryReadDecision::try_new(AuditDetail::BifrostQueryReadDecision {
        query_digest: audit_digest(sql)?,
        query_class,
        visibility,
        binding_digests,
        snapshot_digest: aggregate_audit_digest(
            cuts.iter().map(|cut| cut.snapshot_digest.as_str()),
        )?,
        manifest_digest: aggregate_audit_digest(
            cuts.iter().map(|cut| cut.hot_manifest_digest.as_str()),
        )?,
        projection_digest: audit_digest(sql)?,
        permission_digest: audit_digest(&context.permission)?,
        execution: QueryExecutionMode::Local,
        selected_node_count: 1,
        worker_count: 0,
        slot_units,
        // One attempt per logical query: the audit contract still carries the
        // ordinal, and Oracle now only ever writes its first value.
        retry_ordinal: 0,
        deadline_ms,
    })
}

/// Builds the exact locked read-decision detail for one closed typed plan.
///
/// # Errors
///
/// Returns invalid SQL when the plan has no bound table scan, audit unavailable
/// when a digest is invalid, and timeout when its deadline has elapsed.
fn plan_read_decision(
    context: &AuthorizedQueryContext,
    plan: &datafusion::logical_expr::LogicalPlan,
    options: QueryOptions,
    query_class: QueryClass,
    cuts: &[PinnedSealedTable],
) -> Result<BifrostQueryReadDecision, BifrostError> {
    let plan_text = plan.display_indent().to_string();
    let mut binding_digests = Vec::new();
    collect_plan_binding_digests(plan, &mut binding_digests)?;
    if binding_digests.is_empty() {
        return Err(BifrostError::QueryInvalidSql {
            detail: "typed query plans must contain at least one bound table scan".to_owned(),
        });
    }
    binding_digests.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    binding_digests.dedup_by(|left, right| left.as_str() == right.as_str());
    let deadline_ms = u64::try_from(
        options
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?
            .as_millis()
            .max(1),
    )
    .map_err(|_| BifrostError::QueryAuditUnavailable)?;
    let snapshot_digest =
        aggregate_audit_digest(cuts.iter().map(|cut| cut.snapshot_digest.as_str()))?;
    let manifest_digest =
        aggregate_audit_digest(cuts.iter().map(|cut| cut.hot_manifest_digest.as_str()))?;
    BifrostQueryReadDecision::try_new(AuditDetail::BifrostQueryReadDecision {
        query_digest: audit_digest(&plan_text)?,
        query_class,
        visibility: options.visibility,
        binding_digests,
        snapshot_digest,
        manifest_digest,
        projection_digest: audit_digest(&plan_text)?,
        permission_digest: audit_digest(&context.permission)?,
        execution: QueryExecutionMode::Local,
        selected_node_count: 1,
        worker_count: 0,
        slot_units: admission_limits(u32::MAX, query_class).1,
        retry_ordinal: 0,
        deadline_ms,
    })
}

/// Collects scrubbed table-binding digests from a validated typed plan.
///
/// # Errors
///
/// Returns audit unavailable when one table name cannot enter the T1 digest.
fn collect_plan_binding_digests(
    plan: &datafusion::logical_expr::LogicalPlan,
    output: &mut Vec<QueryAuditDigest>,
) -> Result<(), BifrostError> {
    if let datafusion::logical_expr::LogicalPlan::TableScan(scan) = plan {
        output.push(audit_digest(&scan.table_name.to_string())?);
    }
    for input in plan.inputs() {
        collect_plan_binding_digests(input, output)?;
    }
    Ok(())
}

/// Returns the closed production metric label for one admission class.
fn query_class_label(class: QueryClass) -> &'static str {
    OracleQueryClassLabel::from(class).as_str()
}

/// Returns the closed production metric label for one visibility mode.
const fn visibility_label(visibility: VisibilityMode) -> &'static str {
    match visibility {
        VisibilityMode::PublishedOnly => "published_only",
        VisibilityMode::Fused => "fused",
    }
}

/// Returns the closed metric label for one stable late terminal code.
const fn terminal_error_label(code: QueryTerminalErrorCode) -> &'static str {
    match code {
        QueryTerminalErrorCode::QueryTimeout => "query_timeout",
        QueryTerminalErrorCode::QueryVisibilityUnavailable => "query_visibility_unavailable",
        QueryTerminalErrorCode::QueryTenantInvariant => "query_tenant_invariant",
        QueryTerminalErrorCode::QueryReconciliationInvariant => "query_reconciliation_invariant",
        QueryTerminalErrorCode::QueryPeerSecurity => "query_peer_security",
        QueryTerminalErrorCode::QueryAuditUnavailable => "query_audit_unavailable",
        QueryTerminalErrorCode::CatalogUnreachable => "catalog_unreachable",
        QueryTerminalErrorCode::StorageUnreachable => "storage_unreachable",
        QueryTerminalErrorCode::QueryExecutionFailed => "query_execution_failed",
    }
}

/// Everything one executed attempt produced, before its first batch is settled.
///
/// These six values are produced together by a successful attempt and consumed
/// together by settlement, which must be able to release the admitted owner and
/// the running-query terminal on any failure path.
struct AttemptOutput {
    /// Output schema of the leader plan.
    schema: SchemaRef,
    /// Undrained leader output stream.
    batches: SendableRecordBatchStream,
    /// Scan telemetry derived from the planned leader.
    scan_stats: OracleQueryScanStats,
    /// Shared accumulator recording ordered degradation reasons.
    degraded_sources: DegradedSourceAccumulator,
    /// Admitted query owner released on every failure path.
    admitted: AdmittedQueryGuard,
    /// Running-query terminal owner transferred into the returned stream.
    running_query: Option<RunningQueryTerminalOwner>,
    /// Execution path this attempt irreversibly selected before it opened.
    execution_path: QueryExecutionPath,
}

impl AttemptOutput {
    /// Joins one executed cut with the two owners settlement must be able to release.
    ///
    /// The cut supplies the stream and the path it was executed on; the guard
    /// and the terminal owner are what any failure below must hand back. They
    /// are only ever produced together, so they are joined here rather than
    /// threaded through the caller field by field.
    fn new(
        execution: CutExecution,
        admitted: AdmittedQueryGuard,
        running_query: Option<RunningQueryTerminalOwner>,
    ) -> Self {
        let CutExecution {
            schema,
            batches,
            scan_stats,
            degraded_sources,
            execution_path,
        } = execution;
        Self {
            schema,
            batches,
            scan_stats,
            degraded_sources,
            admitted,
            running_query,
            execution_path,
        }
    }
}

/// Request-scoped facts settlement needs that do not come from execution.
struct AttemptSettlement {
    /// Absolute whole-query deadline.
    deadline: Instant,
    /// The pinned cut's deadline as a nonnegative Unix epoch millisecond.
    deadline_ms: i64,
    /// Caller-selected visibility retained on the returned stream.
    visibility: VisibilityMode,
    /// Caller-selected source-loss policy retained on the returned stream.
    freshness: wyrd_spec::vala::api::FreshnessPolicy,
}

/// Awaits the first batch and converts one executed attempt into a query stream.
///
/// The first batch is the decision point for the whole attempt. A typed stale
/// object error means a data file the pinned cut referenced was already deleted
/// when the scan reached it; there is no second attempt to move to, so the
/// attempt is cancelled, its distributed children are joined, and the query
/// fails. Any other first-batch failure, a missing telemetry guard, or a
/// schema-frame failure settles the distributed children and releases the
/// admitted owner before returning, so no child outlives its parent on a
/// failure path.
///
/// # Errors
///
/// Returns [`BifrostError::QueryTimeout`] when the first batch does not arrive
/// before the deadline, [`BifrostError::QueryExecutionFailed`] for a stale first
/// batch or a missing telemetry guard, and the mapped first-batch failure
/// otherwise.
async fn settle_attempt_output(
    output: AttemptOutput,
    settle: AttemptSettlement,
    query_telemetry: &mut Option<QueryTelemetryGuard>,
) -> Result<OracleQueryStream, BifrostError> {
    let AttemptOutput {
        schema,
        mut batches,
        scan_stats,
        degraded_sources,
        admitted,
        running_query,
        execution_path,
    } = output;
    let AttemptSettlement {
        deadline,
        deadline_ms,
        visibility,
        freshness,
    } = settle;
    let Ok(first) =
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), batches.next()).await
    else {
        return settle_distributed_failure(
            deadline,
            batches,
            admitted,
            BifrostError::QueryTimeout,
            "first-batch timeout",
        )
        .await;
    };
    if first
        .as_ref()
        .is_some_and(|result| result.as_ref().is_err_and(is_stale_iceberg_object_error))
    {
        admitted.cancellation.cancel();
        drop(batches);
        admitted.distributed_settlement.join().await;
        return release_error(
            deadline,
            admitted,
            BifrostError::QueryExecutionFailed,
            "stale first batch",
        );
    }
    if let Some(error) = map_first_batch_failure(first.as_ref()) {
        return settle_distributed_failure(
            deadline,
            batches,
            admitted,
            error,
            "first-batch rejection",
        )
        .await;
    }
    let Some(query_telemetry) = query_telemetry.take() else {
        let error = BifrostError::QueryExecutionFailed;
        return settle_distributed_failure(deadline, batches, admitted, error, "missing telemetry")
            .await;
    };
    let (ipc, schema_frame) = match QueryIpcEncoder::new(&schema) {
        Ok(opened) => opened,
        Err(error) => {
            return settle_distributed_failure(
                deadline,
                batches,
                admitted,
                error,
                "schema preparation",
            )
            .await;
        }
    };
    Ok(OracleQueryStream::new(QueryStreamInput {
        execution_path,
        schema_frame,
        ipc,
        batches,
        first,
        admitted,
        deadline,
        deadline_ms,
        visibility,
        freshness_policy: freshness,
        degraded_sources,
        query_telemetry,
        scan_stats,
        // Gate attaches its own lifecycle to the returned stream through
        // `with_gate_lifecycle`; nothing on the attempt path owns one.
        gate_lifecycle: None,
        running_query,
    }))
}

/// Projects a monotonic execution deadline as a nonnegative Unix epoch millisecond.
///
/// The typed-plan path carries no participant cut, so it has no pinned wall-clock
/// deadline to read. Converting the monotonic instant names the same point in
/// time the server already enforces rather than inventing a second budget. A
/// deadline that has already passed yields the current time, never a negative
/// millisecond, because the wire contract is nonnegative.
fn absolute_deadline_ms(deadline: Instant) -> i64 {
    absolute_deadline(deadline).timestamp_millis().max(0)
}

/// Projects one monotonic deadline onto the absolute wall clock.
///
/// Execution governance is observed by leaves that only see a wall clock, so a
/// monotonic budget has to be projected once at the boundary rather than
/// re-derived per leaf.
fn absolute_deadline(deadline: Instant) -> chrono::DateTime<chrono::Utc> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let remaining =
        chrono::Duration::from_std(remaining).unwrap_or_else(|_| chrono::Duration::zero());
    chrono::Utc::now() + remaining
}

/// Records an unavailable live tail as a degraded partition on the shared list.
///
/// A drained tail that could not be read is a source loss the caller must see
/// alongside the execution-time degradations, so it is appended to the same
/// accumulator. The sentinel `u32::MAX` ordinal keeps it outside the
/// participant ordinal space, where every real partition's reason lives. A
/// poisoned accumulator is ignored rather than escalated: losing one
/// degradation note must not fail a query that otherwise succeeded.
fn record_degraded_live_tail(degraded_sources: &DegradedSourceAccumulator, degraded_tails: bool) {
    if degraded_tails && let Ok(mut degraded) = degraded_sources.lock() {
        degraded.push(DegradedPartition {
            ordinal: u32::MAX,
            reason: "live_tail_unavailable",
            sources: vec![QuerySource::LiveTail],
        });
    }
}

/// Returns `(ceiling, demand)` for `class` on a node advertising `usable_slots`.
/// The ceiling derivations are the locked D71 policy and are unchanged. The
/// Analytical demand is clamped to `min(2, usable_slots.max(1))` so it upholds
/// the schedulability invariant `demand <= running_capacity.max(1)` at the
/// admission boundary: a node whose usable capacity is 1 must never advertise a
/// per-node demand of 2, which would be structurally unschedulable against its
/// own running semaphore. Interactive demand stays 1. Because every production
/// producer calls this with `usable_slots = u32::MAX` (see the reserve-request
/// builders in the dispatcher path), the transmitted Analytical wire demand is
/// still 2; the clamp only takes effect for internal capacity-bounded callers,
/// and the load-bearing peer-side clamp lives in
/// [`ReservationRegistry::take_for_execute`].
fn admission_limits(usable_slots: u32, class: QueryClass) -> (u32, u32) {
    match class {
        QueryClass::Interactive => ((usable_slots.saturating_mul(80) / 100).max(1), 1),
        QueryClass::Analytical => (
            (usable_slots.saturating_mul(40) / 100).max(2),
            2.min(usable_slots.max(1)),
        ),
    }
}

/// Maps a pre-stream `DataFusion` failure into the stable public catalog.
fn map_datafusion_error(error: &datafusion::error::DataFusionError) -> BifrostError {
    tracing::error!(error = %error, "Oracle DataFusion operation failed");
    if datafusion_resources_exhausted(error) {
        return BifrostError::QueryAdmissionRejected;
    }
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("tenant invariant") {
        BifrostError::QueryTenantInvariant
    } else if message.contains("reconciliation invariant") {
        BifrostError::QueryReconciliationInvariant
    } else if message.contains("audit unavailable") {
        BifrostError::QueryAuditUnavailable
    } else {
        BifrostError::QueryExecutionFailed
    }
}

/// Reports whether any typed `DataFusion` source in an execution error chain is
/// a resource-capacity refusal, including contextual wrappers added by plans.
fn datafusion_resources_exhausted(error: &datafusion::error::DataFusionError) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = source {
        if current
            .downcast_ref::<datafusion::error::DataFusionError>()
            .is_some_and(|error| {
                matches!(
                    error,
                    datafusion::error::DataFusionError::ResourcesExhausted(_)
                )
            })
        {
            return true;
        }
        source = current.source();
    }
    false
}

/// Projects a failed first lookahead before any schema frame can be emitted.
fn map_first_batch_failure(
    first: Option<&Result<RecordBatch, datafusion::error::DataFusionError>>,
) -> Option<BifrostError> {
    first
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(|error| {
            // The stable public error deliberately discards engine detail, which
            // leaves an execution failure with no attributable cause anywhere in
            // the logs. Record the underlying engine error once, here, before the
            // mapping erases it.
            tracing::warn!(%error, "Oracle query failed on its first batch");
            map_datafusion_error(error)
        })
}

/// Recursively rejects logical-plan variants that can write or bypass bound sources.
///
/// # Errors
///
/// Returns invalid SQL for DML, DDL, COPY, statements, extensions, analysis,
/// describe, or any descendant containing one of those variants.
fn validate_read_only_plan(
    plan: &datafusion::logical_expr::LogicalPlan,
) -> Result<(), BifrostError> {
    use datafusion::logical_expr::LogicalPlan;

    if matches!(
        plan,
        LogicalPlan::Dml(_)
            | LogicalPlan::Ddl(_)
            | LogicalPlan::Copy(_)
            | LogicalPlan::Statement(_)
            | LogicalPlan::Extension(_)
            | LogicalPlan::Analyze(_)
            | LogicalPlan::DescribeTable(_)
    ) {
        return Err(BifrostError::QueryInvalidSql {
            detail: "typed query plans must be closed read-only plans".to_owned(),
        });
    }
    for input in plan.inputs() {
        validate_read_only_plan(input)?;
    }
    Ok(())
}

/// Partitions every persisted scan completely and disjointly across fixed Oracle workers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum OraclePartitionStrategy {
    /// Contiguous quotient-plus-remainder assignment from the pinned default.
    #[default]
    FileCount,
    /// Contiguous assignment that advances at the pinned average-byte boundary.
    FileSize,
    /// Stable assignment constrained to the immutable selected Oracle cut.
    StableHash,
}

/// Assigns ordered values by the selected partition strategy.
fn partition_values<T, F, K>(
    values: &[T],
    selected_workers: &[Vec<u8>],
    strategy: OraclePartitionStrategy,
    size: F,
    stable_key: K,
) -> Vec<Vec<T>>
where
    T: Clone,
    F: Fn(&T) -> u64,
    K: Fn(&T) -> Vec<u8>,
{
    let worker_count = selected_workers.len();
    if worker_count == 0 {
        return Vec::new();
    }
    let mut partitions = vec![Vec::new(); worker_count];
    match strategy {
        OraclePartitionStrategy::FileCount => {
            let quotient = values.len() / worker_count;
            let remainder = values.len() % worker_count;
            let mut start = 0;
            for (worker, partition) in partitions.iter_mut().enumerate() {
                let length = quotient + usize::from(worker < remainder);
                let end = start + length;
                partition.extend_from_slice(&values[start..end]);
                start = end;
            }
        }
        OraclePartitionStrategy::FileSize => {
            let average = values.iter().map(&size).sum::<u64>() / worker_count as u64;
            let mut worker = 0;
            let mut worker_size = 0_u64;
            for value in values {
                worker_size = worker_size.saturating_add(size(value));
                if worker_size >= average
                    && worker != worker_count - 1
                    && !partitions[worker].is_empty()
                {
                    worker_size = size(value);
                    worker += 1;
                    partitions[worker].push(value.clone());
                    continue;
                }
                partitions[worker].push(value.clone());
            }
        }
        OraclePartitionStrategy::StableHash => {
            let mut ring = selected_workers
                .iter()
                .enumerate()
                .map(|(ordinal, identity)| (*blake3::hash(identity).as_bytes(), ordinal))
                .collect::<Vec<_>>();
            ring.sort_by_key(|(point, _)| *point);
            for value in values {
                let point = *blake3::hash(&stable_key(value)).as_bytes();
                let ordinal = ring
                    .iter()
                    .find(|(worker_point, _)| *worker_point >= point)
                    .or_else(|| ring.first())
                    .map_or(0, |(_, ordinal)| *ordinal);
                partitions[ordinal].push(value.clone());
            }
        }
    }
    partitions
}

/// Derives the identity-bound common-plan placeholder for one pinned Scribe stream.
fn scribe_follower_scan_id(table: &str, node_id: NodeId, writer_epoch: u64) -> String {
    format!(
        "oracle:{table}:scribe:{}:{writer_epoch}:live",
        node_id.as_uuid()
    )
}

/// Closed partition examples returned by the Task 3B production algorithms.
#[cfg(feature = "test-support")]
#[derive(Debug, PartialEq, Eq)]
pub struct PartitionAssignmentExamples {
    /// Contiguous quotient-plus-remainder result.
    pub file_count: Vec<Vec<u64>>,
    /// Pinned average-byte-boundary result.
    pub file_size: Vec<Vec<u64>>,
    /// Stable-hash result constrained to three selected workers.
    pub stable_hash: Vec<Vec<u64>>,
}

/// Runs the three production partition algorithms over immutable examples.
#[cfg(feature = "test-support")]
#[must_use]
pub fn partition_assignment_examples_for_test() -> PartitionAssignmentExamples {
    let values = [1_u64, 2, 3, 4, 5];
    PartitionAssignmentExamples {
        file_count: partition_values(
            &values,
            &[vec![1], vec![2], vec![3]],
            OraclePartitionStrategy::FileCount,
            |_| 1,
            |value| value.to_le_bytes().to_vec(),
        ),
        file_size: partition_values(
            &values,
            &[vec![1], vec![2], vec![3]],
            OraclePartitionStrategy::FileSize,
            |value| *value,
            |value| value.to_le_bytes().to_vec(),
        ),
        stable_hash: partition_values(
            &values,
            &[vec![1], vec![2], vec![3]],
            OraclePartitionStrategy::StableHash,
            |_| 1,
            |value| value.to_le_bytes().to_vec(),
        ),
    }
}

/// Detects the sole typed Iceberg object-loss race eligible for a pre-byte replan.
pub fn is_stale_iceberg_object_error(error: &datafusion::error::DataFusionError) -> bool {
    exec::is_stale_iceberg_object_error(error)
}

/// Reports whether an execution error carries the tenant tripwire's refusal.
///
/// Re-exported for the private peer service, which classifies follower stream
/// errors outside this crate and must preserve the tenant-invariant outcome
/// instead of reporting a generic worker failure.
#[must_use]
pub fn is_tenant_invariant_error(error: &datafusion::error::DataFusionError) -> bool {
    exec::is_tenant_invariant_error(error)
}

/// Release an admitted query after an attempt-local terminal error.
///
/// # Errors
///
/// Always returns the caller-supplied original error after the cleanup attempt.
fn release_error<T>(
    _deadline: Instant,
    admitted: AdmittedQueryGuard,
    original: BifrostError,
    phase: &'static str,
) -> Result<T, BifrostError> {
    tracing::error!(phase, error = ?original, "Oracle query released after failure");
    admitted.release();
    Err(original)
}

/// Cancels and joins every distributed child before releasing query admission.
async fn settle_distributed_failure<T>(
    deadline: Instant,
    batches: SendableRecordBatchStream,
    admitted: AdmittedQueryGuard,
    original: BifrostError,
    phase: &'static str,
) -> Result<T, BifrostError> {
    admitted.cancellation.cancel();
    admitted.request_cancellation.cancel();
    drop(batches);
    admitted.distributed_settlement.join().await;
    release_error(deadline, admitted, original, phase)
}

/// Awaits the actual first physical batch while retaining admission ownership.
///
/// A ready batch returns with the unchanged owner for stream transfer. If the
/// absolute query deadline wins, the one-shot cleanup consumes the owner.
/// Production delegates that cleanup to [`release_error`] so it remains bounded
/// by the same absolute deadline and preserves the query-timeout error.
///
/// # Errors
///
/// Returns the cleanup operation's preserved error after the deadline wins.
#[cfg(test)]
async fn await_first_batch_or_release<F, T, O, C, R>(
    deadline: Instant,
    first_batch: F,
    owner: O,
    cleanup: C,
) -> Result<(T, O), BifrostError>
where
    F: std::future::Future<Output = T>,
    C: FnOnce(O) -> R,
    R: std::future::Future<Output = Result<(T, O), BifrostError>>,
{
    match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), first_batch).await {
        Ok(first) => Ok((first, owner)),
        Err(_) => cleanup(owner).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{StringArray, UInt64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::atomic::AtomicUsize;

    /// Only eligible live-tail loss under explicit policy becomes degraded.
    #[test]
    fn eligible_source_loss_obeys_parent_terminal_matrix() {
        use wyrd_spec::vala::api::FreshnessPolicy;

        let eligible = dispatcher::DispatchError::EligibleSourceLoss {
            cause: dispatcher::EligibleSourceLossCause::ProviderResolution,
        };
        assert!(follower_source_loss_degrades(
            &eligible,
            FreshnessPolicy::AllowDegraded,
            &[QuerySource::LiveTail],
        ));
        assert!(!follower_source_loss_degrades(
            &eligible,
            FreshnessPolicy::Strict,
            &[QuerySource::LiveTail],
        ));
        assert!(!follower_source_loss_degrades(
            &eligible,
            FreshnessPolicy::AllowDegraded,
            &[QuerySource::Iceberg],
        ));
        assert!(!follower_source_loss_degrades(
            &eligible,
            FreshnessPolicy::AllowDegraded,
            &[QuerySource::HotSealed],
        ));
        for error in [
            dispatcher::DispatchError::Unavailable,
            dispatcher::DispatchError::Capacity,
            dispatcher::DispatchError::Terminal,
            dispatcher::DispatchError::StaleObject,
            dispatcher::DispatchError::FileNotFound,
        ] {
            assert!(!follower_source_loss_degrades(
                &error,
                FreshnessPolicy::AllowDegraded,
                &[QuerySource::LiveTail],
            ));
        }
    }

    /// Generic `DataFusion` resource exhaustion is classified structurally as capacity.
    #[test]
    fn datafusion_resource_exhaustion_maps_to_query_admission_rejected() {
        let exhausted = datafusion::error::DataFusionError::ResourcesExhausted(
            "message intentionally contains no capacity keyword".to_owned(),
        );
        let error = datafusion::error::DataFusionError::Context(
            "physical plan wrapper".to_owned(),
            Box::new(exhausted),
        );
        assert_eq!(
            map_datafusion_error(&error),
            BifrostError::QueryAdmissionRejected
        );
    }

    /// Stale replanning accepts only an exact typed storage not-found cause.
    #[test]
    fn stale_iceberg_replan_rejects_string_only_not_found_messages() {
        let typed_non_iceberg = datafusion::error::DataFusionError::External(Box::new(
            std::io::Error::from(std::io::ErrorKind::NotFound),
        ));
        assert!(!is_stale_iceberg_object_error(&typed_non_iceberg));

        let typed_iceberg =
            exec::iceberg_datafusion_error(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(is_stale_iceberg_object_error(&typed_iceberg));

        let text_only = datafusion::error::DataFusionError::Execution(
            "404 parquet object not found".to_owned(),
        );
        assert!(!is_stale_iceberg_object_error(&text_only));
    }

    /// Synthetic first-batch owner exposing cleanup and final-drop observations.
    struct FirstBatchOwner {
        /// Stable identity proving the ready path returns the same owner.
        id: usize,
        /// Number of one-shot cleanup operations that consumed this owner.
        cleanups: Arc<AtomicUsize>,
        /// Number of owners whose final destructor ran.
        drops: Arc<AtomicUsize>,
    }

    impl Drop for FirstBatchOwner {
        /// Records final owner destruction after cleanup or caller release.
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Consumes a synthetic owner and returns the preserved timeout error.
    ///
    /// # Errors
    ///
    /// Always returns [`BifrostError::QueryTimeout`] after consuming the owner.
    async fn release_first_batch_owner(
        owner: FirstBatchOwner,
    ) -> Result<(u8, FirstBatchOwner), BifrostError> {
        owner.cleanups.fetch_add(1, Ordering::SeqCst);
        drop(owner);
        Err(BifrostError::QueryTimeout)
    }

    /// A pending first batch consumes its owner through cleanup and preserves timeout.
    #[tokio::test]
    async fn first_batch_timeout_releases_owner_and_preserves_query_timeout() {
        let cleanups = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let owner = FirstBatchOwner {
            id: 7,
            cleanups: Arc::clone(&cleanups),
            drops: Arc::clone(&drops),
        };
        let deadline = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .expect("test deadline remains representable");
        let result = await_first_batch_or_release(
            deadline,
            std::future::pending::<u8>(),
            owner,
            release_first_batch_owner,
        )
        .await;
        assert!(matches!(result, Err(BifrostError::QueryTimeout)));
        assert_eq!(cleanups.load(Ordering::SeqCst), 1);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    /// A ready first batch transfers the same owner without invoking cleanup.
    #[tokio::test]
    async fn first_batch_ready_transfers_owner_without_cleanup() {
        let cleanups = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let owner = FirstBatchOwner {
            id: 11,
            cleanups: Arc::clone(&cleanups),
            drops: Arc::clone(&drops),
        };
        let (value, owner) = await_first_batch_or_release(
            Instant::now() + Duration::from_secs(1),
            std::future::ready(23_u8),
            owner,
            release_first_batch_owner,
        )
        .await
        .expect("ready batch preserves ownership");
        assert_eq!(value, 23);
        assert_eq!(owner.id, 11);
        assert_eq!(cleanups.load(Ordering::SeqCst), 0);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        drop(owner);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    /// Keeps stateful Oracle owners and bounded dispatch orchestration out of the façade.
    #[test]
    fn oracle_owner_workflows_remain_in_focused_modules() {
        let facade = include_str!("mod.rs");
        for forbidden in [
            ["struct ", "OraclePlanner"].concat(),
            ["struct ", "OracleAdmission"].concat(),
            ["struct ", "TailFenceDrainer"].concat(),
            ["struct ", "OracleQueryStream"].concat(),
            ["async fn ", "dispatch_fragment_attempts"].concat(),
        ] {
            assert!(
                !facade.contains(&forbidden),
                "stateful owner workflow returned to oracle/mod.rs: {forbidden}"
            );
        }
        for (owner, owner_name) in [
            (include_str!("planner.rs"), "OraclePlanner"),
            (include_str!("admission.rs"), "OracleAdmission"),
            (include_str!("tail_fence.rs"), "TailFenceDrainer"),
            (include_str!("query_stream.rs"), "OracleQueryStream"),
        ] {
            let declaration = ["struct ", owner_name].concat();
            assert!(owner.contains(&declaration), "missing owner: {declaration}");
        }
    }

    /// The synchronous floor rejects empty, multi-statement, and non-SELECT SQL.
    #[test]
    fn query_floor_rejects_non_selects() {
        let planner = OraclePlanner::new(OracleConfig::default());
        for sql in ["", "UPDATE x SET y = 1", "SELECT 1; SELECT 2"] {
            let request = BifrostQueryRequest {
                sql: sql.to_owned(),
                visibility: VisibilityMode::PublishedOnly,
                freshness: wyrd_spec::vala::api::FreshnessPolicy::default(),
                deadline_ms: None,
            };
            assert!(planner.validate_query(&request).is_err());
        }
    }

    /// The SQL parser accepts a complete query and extracts its canonical tables.
    #[test]
    fn query_floor_extracts_joined_and_cte_tables() {
        let tables = parse_select_tables(
            "WITH recent AS (SELECT * FROM vala.bifrost.spans) \
             SELECT * FROM recent JOIN vala.bifrost.observations o ON true",
        )
        .expect("complete SELECT is accepted");
        assert_eq!(
            tables.iter().map(TableRef::fqn).collect::<Vec<_>>(),
            vec![
                "vala.bifrost.observations".to_owned(),
                "vala.bifrost.spans".to_owned(),
            ]
        );
    }

    /// Class ceilings remain independent and analytical work is never downgraded.
    #[test]
    fn admission_class_ceiling_and_demand_are_locked() {
        assert_eq!(admission_limits(10, QueryClass::Interactive), (8, 1));
        assert_eq!(admission_limits(10, QueryClass::Analytical), (4, 2));
        assert_eq!(admission_limits(1, QueryClass::Analytical), (2, 1));
    }

    /// `admission_limits` clamps Analytical demand to the node's own capacity.
    ///
    /// Proves the schedulability invariant `demand <= running_capacity.max(1)`
    /// holds at the derivation boundary for every small topology: a capacity-1
    /// node derives demand 1 (never the structurally unschedulable 2), and no
    /// node derives a demand exceeding its usable capacity. Interactive demand
    /// stays fixed at 1. The test name carries the `admission_limits` substring
    /// so the focused verification filter selects it.
    #[test]
    fn admission_limits_clamp_analytical_demand_to_capacity() {
        assert_eq!(admission_limits(1, QueryClass::Analytical).1, 1);
        for usable_slots in 1..=8u32 {
            let (_, analytical_demand) = admission_limits(usable_slots, QueryClass::Analytical);
            assert!(
                analytical_demand <= usable_slots.max(1),
                "analytical demand {analytical_demand} exceeds capacity {usable_slots}"
            );
            assert!(
                analytical_demand >= 1,
                "analytical demand must never be zero (would bypass the semaphore)"
            );
            assert_eq!(
                admission_limits(usable_slots, QueryClass::Interactive).1,
                1,
                "interactive demand is unchanged"
            );
        }
        // Producers pass u32::MAX, so the transmitted wire demand stays 2.
        assert_eq!(admission_limits(u32::MAX, QueryClass::Analytical).1, 2);
    }

    /// Tenant tripwire rejects a foreign row instead of filtering it away.
    #[test]
    fn tenant_tripwire_rejects_foreign_rows() {
        let tenant = DataTenantId::new_v7();
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::UInt64, false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(UInt64Array::from(vec![1])) as Arc<dyn Array>,
                Arc::new(StringArray::from(vec![tenant.to_string()])) as Arc<dyn Array>,
            ],
        )
        .expect("test batch has matching schema");
        assert!(validate_tenant_batch(&batch, tenant).is_ok());
        let foreign = DataTenantId::new_v7();
        assert!(validate_tenant_batch(&batch, foreign).is_err());
    }

    /// Late execution failure produces one closed failed terminal shape.
    #[test]
    fn late_failure_terminal_is_closed_and_non_success() {
        let terminal = failed_terminal(QueryTerminalErrorCode::QueryExecutionFailed, 17);
        assert_eq!(terminal.outcome, QueryTerminalOutcome::Failed);
        assert_eq!(terminal.row_count, 17);
        assert!(terminal.validate(VisibilityMode::PublishedOnly).is_ok());
        assert_eq!(
            terminal
                .error
                .as_ref()
                .expect("failed terminal has an error")
                .code,
            QueryTerminalErrorCode::QueryExecutionFailed
        );
    }

    /// The standard recorder observes the canonical failed stream labels.
    #[test]
    fn oracle_terminal_metric_records_closed_label_delta() {
        let recorder = wyrd_bench::BenchmarkRecorder::new();
        metrics::with_local_recorder(&recorder, || {
            let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 2))));
            let mut query =
                telemetry.start_query(VisibilityMode::PublishedOnly, QueryClass::Analytical);
            query.start_stream();
            query.finish("failed", "complete");
        });
        let snapshot = recorder.snapshot();
        let observed = snapshot.histograms.iter().any(|(series, _)| {
            series.starts_with("oracle_query_duration_seconds{")
                && series.contains("outcome=\"failed\"")
        });
        assert!(
            observed,
            "canonical stream metric was not recorded: {snapshot:?}"
        );
    }

    /// Oracle construction publishes every closed idle query, slot, and
    /// Analytical in-flight gauge series.
    ///
    /// The Analytical attempt and exchange gauges are registered at zero by the
    /// same owner, so a scrape taken before any distributed work exists still
    /// carries the series a terminal-cleanup alert compares against.
    #[test]
    fn oracle_telemetry_registers_closed_idle_gauges() {
        let recorder = wyrd_bench::BenchmarkRecorder::new();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 2))));
        let expected = [
            "oracle_queries_active{class=\"interactive\"}",
            "oracle_queries_active{class=\"analytical\"}",
            "oracle_queries_queued{class=\"interactive\"}",
            "oracle_queries_queued{class=\"analytical\"}",
            "oracle_tenant_budget_pressure{class=\"interactive\"}",
            "oracle_tenant_budget_pressure{class=\"analytical\"}",
            "bifrost_oracle_analytical_attempts_active",
            "bifrost_oracle_analytical_exchanges_active",
        ];
        let initial = recorder.snapshot();
        assert_eq!(initial.gauges.len(), expected.len());
        for series in expected {
            assert_eq!(initial.gauges.get(series), Some(&0.0), "{series}");
        }

        {
            for query_class in [QueryClass::Interactive, QueryClass::Analytical] {
                for visibility in [VisibilityMode::PublishedOnly, VisibilityMode::Fused] {
                    let query = telemetry.start_query(visibility, query_class);
                    drop(query);
                }
                let _ = query_class;
            }
        }

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .gauges
                .keys()
                .filter(|series| {
                    series.starts_with("oracle_queries_active{")
                        || series.starts_with("oracle_queries_queued{")
                })
                .count(),
            4
        );
        for series in expected {
            assert_eq!(snapshot.gauges.get(series), Some(&0.0), "{series}");
        }
    }

    /// Stream payload counters retain exact rows and Arrow IPC bytes for every
    /// closed lifecycle outcome, including cancellation and client drop.
    #[test]
    fn oracle_stream_payload_metrics_close_each_outcome() {
        let recorder = wyrd_bench::BenchmarkRecorder::new();
        metrics::with_local_recorder(&recorder, || {
            for outcome in ["success", "failed"] {
                let telemetry =
                    Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 2))));
                let mut query =
                    telemetry.start_query(VisibilityMode::PublishedOnly, QueryClass::Analytical);
                query.start_stream();
                query.record_payload(0, 7);
                query.record_payload(3, 11);
                query.finish(outcome, "complete");
            }

            let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 2))));
            let mut cancelled =
                telemetry.start_query(VisibilityMode::PublishedOnly, QueryClass::Analytical);
            cancelled.start_stream();
            cancelled.record_payload(3, 18);
            cancelled
                .cancellation_marker()
                .store(true, Ordering::Release);
            drop(cancelled);

            let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 2))));
            let mut dropped =
                telemetry.start_query(VisibilityMode::PublishedOnly, QueryClass::Analytical);
            dropped.start_stream();
            dropped.record_payload(3, 18);
            drop(dropped);
        });

        let snapshot = recorder.snapshot();
        let rows = snapshot
            .counters
            .iter()
            .find(|(series, _)| series.as_str() == "oracle_query_rows_total{class=\"analytical\"}")
            .map(|(_, value)| *value)
            .unwrap_or_default();
        let bytes = snapshot
            .counters
            .iter()
            .find(|(series, _)| {
                series.as_str() == "oracle_query_bytes_returned_total{class=\"analytical\"}"
            })
            .map(|(_, value)| *value)
            .unwrap_or_default();
        assert_eq!(rows, 12);
        assert_eq!(bytes, 72);
    }

    /// Equivalent Arrow UTC spellings produce one sealed-fragment schema identity.
    #[test]
    fn oracle_sealed_fragment_fingerprint_canonicalizes_utc_aliases() {
        let iceberg = Schema::new(vec![Field::new(
            "event_time",
            DataType::Timestamp(
                arrow::datatypes::TimeUnit::Microsecond,
                Some("+00:00".into()),
            ),
            false,
        )]);
        let parquet = Schema::new(vec![Field::new(
            "event_time",
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        )]);
        assert_eq!(
            assignment_schema_fingerprint(&iceberg),
            assignment_schema_fingerprint(&parquet),
        );
    }
}
