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
use chrono::{DateTime, Utc};
use datafusion::catalog::{CatalogProvider, MemoryCatalogProvider, MemorySchemaProvider};
use datafusion::common::TableReference;
use datafusion::dataframe::DataFrame;
use datafusion::datasource::MemTable;
use datafusion::datasource::TableProvider;
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::{SendableRecordBatchStream, execute_stream};
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
    BifrostSecurityViolationKind, ClusterCapabilities, FollowerScanAssignment, NodeId,
    PersistedFileAssignment, PersistedFileDescriptor, QueryAuditDigest, QueryBatchFrame,
    QueryClass, QueryExecutionMode, QueryFreshness, QueryId, QuerySchemaFrame, QuerySource,
    QueryStreamFrame, QueryTerminalErrorCode, QueryTerminalFrame, QueryTerminalOutcome,
    ScribeProviderCut, SourceCompletion, SourceCompletionOutcome, TenantTableBinding,
    VisibilityMode,
};
use wyrd_spec::vala::managed_columns::DATA_TENANT_ID;

use crate::catalog::event_time::{EventTimeBoundsDefect, EventTimeStatistics};
use crate::catalog::{BifrostCatalog, BifrostCatalogError, PinnedSealedTable, TableRef};
use crate::cluster::{ClusterRegistry, ClusterSnapshot, RegisteredRole};
use crate::oracle::pruning::{EventTimeQueryInterval, FilePruningSource};
use crate::schema::SchemaFingerprint;
use crate::scribe::tail_rpc::{TAIL_PROTOCOL_VERSION, TailReadTransport};

mod admission;
pub mod analytical;
pub mod analytical_supervisor;
pub mod analytical_transport;
pub mod attempt;
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
mod splitter;

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
use exec::{
    HotFileSource, OracleQueryScanStats, OracleTableInputs, OracleTableProvider,
    RemotePersistedSources,
};
pub use ownership::{
    DEFAULT_DELEGATED_ALLOCATION_UNITS, DelegatedAdmissionBlock, DelegatedAdmissionRequest,
    DelegatedOracleAdmission, DelegatedOracleAdmissionConfig, DelegatedOracleAdmissionError,
    DelegatedOracleAdmissionGrant, DelegatedOracleAdmissionWorker,
};
pub use participant_cut::{
    OracleQueryAttemptCut, OracleQueryAttemptCutError, OracleQueryParticipant,
};
use planner::OracleClassification;
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
use tail_fence::{DrainedTails, ScribeFollowerSource, TailFenceDrainer, TailFenceDrainerConfig};

/// Default maximum SQL request size accepted by the synchronous query floor.
pub const DEFAULT_MAX_SQL_BYTES: usize = 64 * 1024;
/// Interactive scan-time threshold in seconds.
pub const INTERACTIVE_SCAN_LIMIT_SECONDS: f64 = 10.0;
/// Bytes per second used by the normative classification estimate.
pub const ESTIMATED_SCAN_BYTES_PER_SECOND: f64 = 1_073_741_824.0;
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

/// Derives the exact public source tiers represented by one role-local assignment set.
fn follower_assignment_sources(
    role: wyrd_spec::vala::api::ClusterRole,
    assignments: &[FollowerScanAssignment],
) -> Vec<QuerySource> {
    if role == wyrd_spec::vala::api::ClusterRole::Scribe {
        return assignments
            .iter()
            .any(|assignment| assignment.scribe_provider_cut.is_some())
            .then_some(QuerySource::LiveTail)
            .into_iter()
            .collect();
    }
    let mut sources = Vec::new();
    for assignment in assignments {
        // Classified from the descriptors themselves. A mixed list never
        // survives follower preflight, and reporting no tier for one is
        // correct here: this function names the tiers a degraded result
        // actually lost, and an assignment that cannot be resolved lost none.
        match AssignedPersistedSource::classify(&assignment.persisted.files) {
            Ok(AssignedPersistedSource::Iceberg { .. }) => sources.push(QuerySource::Iceberg),
            Ok(AssignedPersistedSource::Hot) => sources.push(QuerySource::HotSealed),
            Ok(AssignedPersistedSource::Empty) | Err(_) => {}
        }
    }
    sources.sort();
    sources.dedup();
    sources
}

/// One attempt's fixed stale-object replacement boundary.
#[derive(Clone, Copy)]
struct StaleReplacementGate {
    /// Zero-based full-attempt ordinal.
    retry_ordinal: u8,
    /// Whether any public schema or record frame has been exposed.
    output_started: bool,
}

impl StaleReplacementGate {
    /// Creates the pre-output gate used while building one attempt stream.
    const fn before_output(retry_ordinal: u8) -> Self {
        Self {
            retry_ordinal,
            output_started: false,
        }
    }

    /// Settles the real admitted owner before authorizing the sole typed replacement.
    ///
    /// A retry is returned only for the first attempt, before public output, and
    /// after synchronous release of the complete admitted owner. Every other
    /// combination returns the unchanged owner for terminal settlement.
    ///
    /// # Errors
    ///
    /// Returns the unchanged admitted owner when replacement is ineligible.
    ///
    /// The owner is boxed on the error path: it is far larger than the unit
    /// success value, and returning it inline would make every caller's
    /// `Result` pay that size on the common eligible path.
    fn settle_for_typed_stale(
        self,
        admitted: AdmittedQueryGuard,
        typed_stale: bool,
    ) -> Result<(), Box<AdmittedQueryGuard>> {
        if self.retry_ordinal == 0 && !self.output_started && typed_stale {
            admitted.release();
            record_stale_replan();
            Ok(())
        } else {
            Err(Box::new(admitted))
        }
    }
}

/// Decodes footer-validated attempt payloads into Arrow batches.
///
/// # Errors
///
/// Returns execution failure when an attempt payload or Arrow IPC stream is invalid.
fn decode_attempt_batches(
    attempt: attempt::ValidatedAttempt,
    output: &mut Vec<RecordBatch>,
) -> Result<(), BifrostError> {
    for bytes in attempt.batches {
        let bytes = bytes.map_err(|_| BifrostError::QueryExecutionFailed)?;
        let reader = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
        for batch in reader {
            output.push(batch.map_err(|_| BifrostError::QueryExecutionFailed)?);
        }
    }
    Ok(())
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

    /// Records one classification decision and its predicted scan duration.
    fn record_classification(classification: OracleClassification) {
        let _ = (
            classification.query_class,
            classification.reason,
            classification.predicted_scan_seconds,
        );
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
#[cfg(feature = "test-support")]
#[derive(Debug, Default, Clone, Copy)]
pub struct AcceptingOracleAudit;

#[cfg(feature = "test-support")]
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
    /// Server-owned east-west stage authority for the inactive Analytical path.
    ///
    /// Absent on a deployment whose Oracle role cannot serve stage operations.
    /// The inactive Analytical owners are only composed when it is present, so
    /// a node without it has no follower ingress to mount and no leader handle
    /// to execute through.
    pub stage_authority: Option<Arc<dyn peer::OracleStageAuthority>>,
    /// Server-owned domain-separated Scribe-tail ticket signer.
    pub tail_ticket_minter: Option<Arc<dyn crate::scribe::tail_rpc::TailTicketMinter>>,
    /// Query-scoped live Scribe discovery owner.
    pub tail_discovery: Option<Arc<dyn tail_fence::TailStreamDiscovery>>,
    /// Optional node-aware local/tonic directory used for immutable sealed leaves.
    pub peer_transports: Option<dispatcher::OraclePeerTransportDirectory>,
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
    /// Bytes one Analytical attempt may retain in live exchange buffers.
    pub analytical_exchange_buffer_bytes: usize,
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
            analytical_exchange_buffer_bytes: 16 * 1024 * 1024,
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

/// Follower scan assignments accumulated while registering each pinned cut.
///
/// One statement may pin several tables; each contributes scan ids to the same
/// three maps. Accumulating them in one value keeps the per-cut registration
/// from taking three separate mutable borrows.
#[derive(Default)]
struct CutAssignments {
    /// Oracle-side assignments keyed by scan id.
    oracle_assignments: HashMap<String, FollowerScanAssignment>,
    /// Canonical table name for each scan id.
    source_groups: HashMap<String, String>,
    /// Scribe-side assignments keyed by owning node then scan id.
    scribe_assignments: HashMap<NodeId, HashMap<String, FollowerScanAssignment>>,
}

impl CutAssignments {
    /// Records one Oracle-side scan id and the table it reads from.
    ///
    /// Every distributed source class a cut carries — Iceberg files, hot files,
    /// and each live Scribe tail — is registered through this one path so the
    /// assignment and its `source_groups` entry can never disagree about which
    /// table a scan id belongs to.
    ///
    /// The recorded closure is a safe pre-planning default: the full physical
    /// schema with no predicates. `execute_distributed_session` overwrites it
    /// with the real closed predicate and projection closure recovered from
    /// this scan id's `RemoteScanExec` once the physical plan exists; a scan id
    /// with no recovered closure (e.g. pruned out of the final plan) keeps this
    /// unpruned default rather than being narrowed to an empty projection.
    fn record_scan(
        &mut self,
        scan_id: &str,
        table_name: &str,
        binding: &TenantTableBinding,
        schema_fingerprint: &str,
        physical_schema: &Schema,
        files: Vec<PersistedFileDescriptor>,
    ) {
        self.oracle_assignments.insert(
            scan_id.to_owned(),
            FollowerScanAssignment {
                scan_id: scan_id.to_owned(),
                binding: binding.clone(),
                persisted: PersistedFileAssignment { files },
                scribe_provider_cut: None,
                schema_fingerprint: schema_fingerprint.to_owned(),
                required_columns: physical_schema
                    .fields()
                    .iter()
                    .map(|field| field.name().clone())
                    .collect(),
                predicates: Vec::new(),
            },
        );
        self.source_groups
            .insert(scan_id.to_owned(), table_name.to_owned());
    }
}

/// Immutable shared inputs for building every follower's remote scan.
///
/// One distributed session builds several remote scans from the same
/// dispatcher, signed dispatch facts, participant cut, and assignment maps.
/// Borrowing them as one context keeps each build call honest about what it may
/// read without copying the maps per follower.
#[derive(Clone, Copy)]
struct RemoteScanBuildContext<'a> {
    /// Shared dispatcher that opens authenticated peer streams.
    dispatcher: &'a Arc<dispatcher::FragmentDispatcher>,
    /// Signed dispatch facts replayed identically to every participant.
    dispatch_context: &'a dispatcher::DispatchContext,
    /// Admitted query owner supplying leader identity and settlement.
    admitted: &'a AdmittedQueryGuard,
    /// One ingress-captured, role-fenced participant cut.
    participant_cut: &'a OracleQueryAttemptCut,
    /// Oracle-side assignments keyed by scan id.
    oracle_assignments: &'a HashMap<String, FollowerScanAssignment>,
    /// Scribe-side assignments keyed by owning node then scan id.
    scribe_assignments: &'a HashMap<NodeId, HashMap<String, FollowerScanAssignment>>,
    /// Caller-selected source-loss policy retained through dispatch.
    freshness: wyrd_spec::vala::api::FreshnessPolicy,
    /// One absolute deadline captured at ingress, in Unix milliseconds.
    deadline_unix_ms: i64,
    /// Shared accumulator recording ordered degradation reasons.
    degraded_sources: &'a DegradedSourceAccumulator,
}

/// Disjoint follower scan assignments derived from one pinned visibility cut.
///
/// These three maps are built together from the same cut and consumed together
/// by distributed execution, so they travel as one value rather than as three
/// positional parameters.
struct DistributedScanAssignments {
    /// Oracle-side assignments keyed by scan id.
    oracle_assignments: HashMap<String, FollowerScanAssignment>,
    /// Canonical table name for each scan id.
    source_groups: HashMap<String, String>,
    /// Scribe-side assignments keyed by owning node then scan id.
    scribe_assignments: HashMap<NodeId, HashMap<String, FollowerScanAssignment>>,
}

/// Complete inputs for lowering one pinned SQL visibility cut.
struct SqlCutInput<'a> {
    /// Authenticated request context.
    context: &'a AuthorizedQueryContext,
    /// Validated SQL statement.
    sql: &'a str,
    /// Exact pinned table cuts.
    cuts: Vec<PinnedSealedTable>,
    /// Drained live batches keyed by canonical table name.
    live_batches: HashMap<String, Vec<RecordBatch>>,
    /// Remote Scribe sources selected by the audited visibility cut.
    scribe_sources: Vec<ScribeFollowerSource>,
    /// Immutable admission class.
    query_class: QueryClass,
    /// Caller-selected source-loss policy retained through follower dispatch.
    freshness: wyrd_spec::vala::api::FreshnessPolicy,
    /// Admitted durable/local query owner used by distributed dispatch.
    admitted: &'a AdmittedQueryGuard,
    /// Query-owned `DataFusion` runtime acquired before provider and data IO.
    session: SessionContext,
    /// Absolute execution deadline.
    deadline: Instant,
    /// Immutable selected-file bytes used for logical scan telemetry.
    logical_bytes_selected: u64,
    /// One ingress-captured, role-fenced participant cut used by every dispatch.
    participant_cut: &'a OracleQueryAttemptCut,
}

/// Table-local facts needed to derive every pinned Scribe follower assignment.
struct ScribeAssignmentTable<'a> {
    /// Canonical logical table identity encoded in the common scan placeholder.
    table: String,
    /// Authenticated tenant/table binding carried by every role-local assignment.
    binding: TenantTableBinding,
    /// Exact physical schema pinned by the immutable table cut.
    physical_schema: SchemaRef,
    /// Complete sealed manifest used to project this Scribe's persisted cursor.
    sealed_manifest: &'a [vala_sql::row_types::file_list::HotFileRow],
}

/// Pinned tables and class selected during one retry's planning phase.
///
/// Produced by classification and, when the leader is this same process,
/// carried into execution so the catalog is pinned once per query rather than
/// once to classify and again to execute.
pub struct PlannedSqlCut {
    /// Exact immutable table cuts.
    pub(crate) cuts: Vec<PinnedSealedTable>,
    /// Server-derived admission class.
    pub(crate) query_class: QueryClass,
    /// Fraction of pinned sealed bytes in the local hot tier.
    pub(crate) local_ratio: f64,
}

impl PlannedSqlCut {
    /// Returns the server-derived admission class chosen for this plan.
    #[must_use]
    pub fn query_class(&self) -> QueryClass {
        self.query_class
    }
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
    /// Whole-query retry ordinal.
    retry_ordinal: u8,
    /// Absolute query deadline.
    deadline: Instant,
    /// Admitted query owner supplying cancellation and durable query identity.
    admitted: &'a AdmittedQueryGuard,
    /// One immutable membership cut supplying every role-fenced Scribe assignment.
    participant_cut: &'a OracleQueryAttemptCut,
}

/// Immutable inputs shared by one complete SQL execution attempt.
struct SqlAttemptInput<'a> {
    /// Authenticated request context.
    context: &'a AuthorizedQueryContext,
    /// Original validated query request.
    request: &'a BifrostQueryRequest,
    /// Tables parsed from the validated SQL statement.
    tables: &'a [TableRef],
    /// Absolute whole-query deadline shared across retry attempts.
    deadline: Instant,
    /// Zero-based stale-replan attempt ordinal.
    retry_ordinal: u8,
    /// Optional Gate request lifecycle transferred into a returned stream.
    gate_lifecycle: Option<Arc<crate::oracle::query_stream::QueryStreamLifecycle>>,
    /// One signed ingress-captured participant cut reused by every retry phase.
    participant_cut: &'a OracleQueryAttemptCut,
    /// Server-derived class signed into the forwarding envelope.
    query_class: QueryClass,
    /// Catalog snapshot already pinned in this process during classification.
    ///
    /// Present only on a first attempt whose leader is this node. Classification
    /// must pin the catalog to size the scan, and pinning again to execute costs a
    /// second catalog round trip for a snapshot taken microseconds later. Reusing
    /// it means the query reads sealed files as of request arrival rather than
    /// execution start. A forwarded query never carries one: the participant cut
    /// transports cluster membership, not the file list, so a remote leader pins
    /// for itself.
    prepared: Option<PlannedSqlCut>,
    /// Inactive Analytical lease, present only on the harness entry point.
    ///
    /// `None` on every production path, which is what keeps routing Interactive:
    /// the attempt leases Oracle's own session and never installs a distributed
    /// planner, worker set, or signing channel resolver.
    analytical: Option<&'a analytical::AnalyticalAttemptContext>,
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
    /// Table-local Scribe tail transport directory.
    tails: Arc<TailTransportDirectory>,
    /// Query-scoped Scribe-tail ticket signer, when the server has a Scribe role.
    tail_ticket_minter: Option<Arc<dyn crate::scribe::tail_rpc::TailTicketMinter>>,
    /// Query-scoped live Scribe discovery owner.
    tail_discovery: Option<Arc<dyn tail_fence::TailStreamDiscovery>>,
    /// Test-tier switch that routes fused reads through explicitly registered local transports.
    #[cfg(feature = "test-support")]
    prefer_local_tail_routes: std::sync::atomic::AtomicBool,
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

    /// Pauses only the first attempt and lets the replan proceed immediately.
    async fn pause_first_selection(&self, target: NodeId) {
        if self
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        if let Ok(mut selected) = self.target.lock() {
            *selected = Some(target);
        }
        self.selected.notify_waiters();
        while !self.resumed.load(Ordering::Acquire) {
            self.resume.notified().await;
        }
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
        let (demand_tx, demand_rx) =
            tokio::sync::mpsc::channel(config.config.queue_capacity.max(1) as usize);
        let (loss_tx, loss_rx) = tokio::sync::mpsc::channel(1);
        let delegated_admission = Arc::new(
            DelegatedOracleAdmission::new(
                admission.local_role.key.node_id,
                admission.local_role.fencing_token,
                config.delegated_admission_config,
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
                    config.operator_pool,
                    Arc::clone(&delegated_admission),
                    demand_rx,
                    shutdown.clone(),
                )
                .run(),
            );
        let ready = Arc::new(AtomicBool::new(false));
        let (maintenance, startup_result) =
            OracleAdmission::start_maintenance(shutdown.clone(), Arc::clone(&ready))?;
        let fragment_dispatcher = config.peer_transports.map(|transports| {
            Arc::new(dispatcher::FragmentDispatcher::new(
                Arc::clone(&config.peer_ticket_minter),
                transports,
            ))
        });
        let analytical_node_id = admission.local_role.key.node_id;
        let analytical_fence = admission.local_role.fencing_token;
        let analytical = config.stage_authority.map(|authority| {
            let supervisor = Arc::new(analytical::AnalyticalSupervisor::new());
            let worker = Arc::new(analytical::AnalyticalStageIngress::new(
                analytical::AnalyticalStageIngressConfig {
                    node_id: analytical_node_id,
                    oracle_fence: analytical_fence,
                    authority: Arc::clone(&authority),
                    supervisor: Arc::clone(&supervisor),
                    oracle_resources: config.memory.resources.clone(),
                    spill: Arc::clone(&config.spill_runtime),
                    exchange_buffer_bytes: config.config.analytical_exchange_buffer_bytes,
                },
            ));
            Arc::new(analytical::AnalyticalExecutionHandle::new(
                worker,
                authority,
                supervisor,
                Arc::clone(&config.spill_runtime),
                config.memory.resources.clone(),
                analytical::AnalyticalExecutionConfig {
                    node_id: analytical_node_id,
                    oracle_fence: analytical_fence,
                    ticket_ttl: chrono::Duration::seconds(30),
                    exchange_buffer_bytes: config.config.analytical_exchange_buffer_bytes,
                    scratch_bytes: config.config.analytical_scratch_bytes,
                },
            ))
        });
        Ok(Self {
            planner,
            admission,
            delegated_admission,
            delegated_loss: Mutex::new(Some(loss_rx)),
            cluster,
            running_queries,
            catalog: config.catalog,
            vala: config.vala,
            memory: config.memory,
            spill_runtime: config.spill_runtime,
            tails: config.tails,
            tail_ticket_minter: config.tail_ticket_minter,
            tail_discovery: config.tail_discovery,
            #[cfg(feature = "test-support")]
            prefer_local_tail_routes: std::sync::atomic::AtomicBool::new(false),
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
        let (cut, planned) = self.prepare_query_attempt(&context, &request).await?;
        let query_class = planned.query_class;
        self.query_sql_with_cut_and_gate_lifecycle(
            context,
            request,
            cut,
            query_class,
            None,
            Some(planned),
            None,
        )
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
        let (cut, planned) = self.prepare_query_attempt(&context, &request).await?;
        let query_class = planned.query_class;
        self.query_sql_with_cut_and_gate_lifecycle(
            context,
            request,
            cut,
            query_class,
            None,
            Some(planned),
            Some(attempt),
        )
        .await
    }

    /// Prepares the immutable local-leader participant cut before an attempt begins.
    ///
    /// Returns the pinned plan alongside the cut so a local leader can execute the
    /// catalog snapshot classification already paid for instead of pinning twice.
    async fn prepare_query_attempt(
        &self,
        context: &AuthorizedQueryContext,
        request: &BifrostQueryRequest,
    ) -> Result<(OracleQueryAttemptCut, PlannedSqlCut), BifrostError> {
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
        let planned = self
            .planner
            .classify_for_forwarding(context, request, deadline, &self.catalog, &snapshot)
            .await?;
        let query_class = planned.query_class;
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
        let cut = OracleQueryAttemptCut::try_from_snapshot(
            &snapshot,
            attempt_id,
            self.admission.local_role.key.node_id,
            query_class,
            wall_deadline,
            now,
            observed_age.saturating_add(Duration::from_secs(1)),
        )
        .map_err(|_| BifrostError::OracleRoleUnavailable)?;
        Ok((cut, planned))
    }

    /// Executes one authenticated query as the exact leader named by a signed participant cut.
    ///
    /// Both local ingress and private forwarding use this operation. The immutable cut and its
    /// absolute deadline survive the single allowed stale-Iceberg replan unchanged.
    ///
    /// # Errors
    /// Returns a closed query, policy, role-fence, deadline, admission, audit, or execution error.
    pub async fn query_sql_with_participant_cut(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
        participant_cut: OracleQueryAttemptCut,
        query_class: QueryClass,
        prepared: Option<PlannedSqlCut>,
    ) -> Result<OracleQueryStream, BifrostError> {
        self.query_sql_with_cut_and_gate_lifecycle(
            context,
            request,
            participant_cut,
            query_class,
            None,
            prepared,
            None,
        )
        .await
    }

    /// Starts a SQL query while retaining an optional Gate lifecycle owner.
    ///
    /// The lifecycle owner is attached before the first frame is emitted, so
    /// Gate duration and active-stream accounting remain truthful through
    /// terminal consumption or client cancellation.
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
    async fn query_sql_with_cut_and_gate_lifecycle(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
        participant_cut: OracleQueryAttemptCut,
        query_class: QueryClass,
        gate_lifecycle: Option<Arc<crate::oracle::query_stream::QueryStreamLifecycle>>,
        mut prepared: Option<PlannedSqlCut>,
        analytical: Option<analytical::AnalyticalAttemptContext>,
    ) -> Result<OracleQueryStream, BifrostError> {
        self.validate_query(&request)?;
        if !self.is_ready() {
            return Err(BifrostError::OracleRoleUnavailable);
        }
        if participant_cut.leader().node_id != self.admission.local_role.key.node_id
            || participant_cut.leader().fencing_token != self.admission.local_role.fencing_token
            || participant_cut.attempt_id().as_uuid().to_string() != context.request_id.as_str()
        {
            return Err(BifrostError::QueryPeerSecurity);
        }
        let remaining = participant_cut
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
        for retry_ordinal in 0_u8..=1 {
            if let Some(stream) = self
                .run_sql_attempt(
                    SqlAttemptInput {
                        context: &context,
                        request: &request,
                        tables: &tables,
                        deadline,
                        retry_ordinal,
                        gate_lifecycle: gate_lifecycle.as_ref().map(Arc::clone),
                        participant_cut: &participant_cut,
                        query_class,
                        // Only the first attempt may reuse the classification
                        // snapshot. A stale-Iceberg retry exists precisely to
                        // observe a newer catalog, so it must pin again.
                        prepared: prepared.take(),
                        analytical: analytical.as_ref(),
                    },
                    &mut query_telemetry,
                )
                .await?
            {
                tracing::debug!(
                    attempt_ms = attempt_started.elapsed().as_millis(),
                    retry_ordinal,
                    "Oracle leader opened one query stream"
                );
                return Ok(stream);
            }
        }
        Err(BifrostError::QueryExecutionFailed)
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

    /// Execute one bounded plan/admit/audit/scan attempt for a SQL query.
    ///
    /// A stale first attempt returns `Ok(None)` only after releasing its full
    /// admission owner. A completed attempt transfers that owner into the
    /// returned stream so terminal consumption or cancellation releases it.
    ///
    /// # Errors
    /// Returns stable planning, admission, audit, timeout, execution, or
    /// cleanup errors. Failed attempts release admitted state before return.
    /// Acquires every owner one planned attempt needs before it may execute.
    ///
    /// Admission, the execution session, retained physical projections, and the
    /// running-query registration are acquired together because a failure in any
    /// of them must release the ones already taken. The session's parallelism is
    /// narrowed here to the work the pinned cut actually offers.
    ///
    /// # Errors
    ///
    /// Returns the stable admission, lease, projection, or running-query
    /// conflict error, having already released any owner acquired earlier in the
    /// sequence.
    async fn admit_and_lease_attempt(
        &self,
        context: &AuthorizedQueryContext,
        planned: &PlannedSqlCut,
        participant_cut: &OracleQueryAttemptCut,
        deadline: Instant,
        phases: &mut AttemptPhaseTimer,
        analytical: Option<&analytical::AnalyticalAttemptContext>,
    ) -> Result<
        (
            SessionContext,
            AdmittedQueryGuard,
            RunningQueryTerminalOwner,
        ),
        BifrostError,
    > {
        let admitted = self
            .admit_sql_query(
                context,
                planned.query_class,
                planned.local_ratio,
                deadline,
                participant_cut.attempt_id(),
            )
            .await?;
        phases.admitted();
        let work_units = Self::scannable_work_units(&planned.cuts);
        let (session, mut admitted) = match analytical {
            Some(attempt) => self.lease_analytical_session(
                deadline,
                admitted,
                attempt,
                participant_cut,
                context,
                work_units,
            )?,
            None => self.lease_session(deadline, admitted, work_units, "lease rejection")?,
        };
        admitted.retain_physical_projections(&planned.cuts)?;
        let running_query =
            self.register_running_query(context, planned.query_class, &admitted, participant_cut)?;
        Ok((session, admitted, running_query))
    }

    async fn run_sql_attempt(
        &self,
        input: SqlAttemptInput<'_>,
        query_telemetry: &mut Option<QueryTelemetryGuard>,
    ) -> Result<Option<OracleQueryStream>, BifrostError> {
        let SqlAttemptInput {
            context,
            request,
            tables,
            deadline,
            retry_ordinal,
            gate_lifecycle,
            participant_cut,
            query_class: expected_query_class,
            prepared,
            analytical,
        } = input;
        let stale_replacement = StaleReplacementGate::before_output(retry_ordinal);
        let planned = match prepared {
            Some(planned) => planned,
            None => {
                self.plan_sql_attempt(
                    context,
                    &request.sql,
                    tables,
                    deadline,
                    oracle_cut_cpu_cores(participant_cut),
                )
                .await?
            }
        };
        if planned.query_class != expected_query_class {
            return Err(BifrostError::QueryPeerSecurity);
        }
        self.ensure_query_telemetry(query_telemetry, request.visibility, planned.query_class);
        let mut phases = AttemptPhaseTimer::started();
        let (session, mut admitted, running_query) = self
            .admit_and_lease_attempt(
                context,
                &planned,
                participant_cut,
                deadline,
                &mut phases,
                analytical,
            )
            .await?;
        let mut drained = match self
            .audit_and_drain_cut(CutAuditInput {
                context,
                request,
                cuts: &planned.cuts,
                query_class: planned.query_class,
                retry_ordinal,
                deadline,
                admitted: &admitted,
                participant_cut,
            })
            .await
        {
            Ok(drained) => drained,
            Err(error) => return release_error(deadline, admitted, error, "audit rejection"),
        };
        phases.drained();
        admitted.live_reservations = std::mem::take(&mut drained.reservations);
        let degraded_tails = drained.degraded;
        let (schema, batches, scan_stats, degraded_sources) = match self
            .execute_sql_cut(SqlCutInput {
                context,
                sql: &request.sql,
                logical_bytes_selected: Self::logical_selected_bytes(&planned.cuts),
                cuts: planned.cuts,
                live_batches: drained.batches,
                scribe_sources: drained.follower_sources,
                query_class: planned.query_class,
                freshness: request.freshness,
                admitted: &admitted,
                session,
                deadline,
                participant_cut,
            })
            .await
        {
            Ok(execution) => {
                phases.emit();
                execution
            }
            Err(OracleExecutionError::Public(error)) => {
                return release_error(deadline, admitted, error, "execution rejection");
            }
        };
        record_degraded_live_tail(&degraded_sources, degraded_tails);
        settle_attempt_output(
            AttemptOutput {
                schema,
                batches,
                scan_stats,
                degraded_sources,
                admitted,
                running_query,
            },
            AttemptSettlement {
                deadline,
                retry_ordinal,
                stale_replacement,
                visibility: request.visibility,
                freshness: request.freshness,
                gate_lifecycle,
            },
            query_telemetry,
        )
        .await
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
        sql: &str,
        tables: &[TableRef],
        deadline: Instant,
        live_oracle_cpu: f64,
    ) -> Result<PlannedSqlCut, BifrostError> {
        self.planner
            .pin_and_classify(
                context,
                sql,
                tables,
                deadline,
                &self.catalog,
                live_oracle_cpu,
            )
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
        let distributed_followers = self.fragment_dispatcher.is_some();
        let acquired = if input.request.visibility == VisibilityMode::Fused
            && !distributed_followers
        {
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
                input.retry_ordinal,
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
        if distributed_followers && input.request.visibility == VisibilityMode::Fused {
            Ok(DrainedTails {
                follower_sources: self
                    .scribe_follower_sources(input.cuts, input.participant_cut)?,
                degraded,
                ..DrainedTails::default()
            })
        } else if acquired.is_empty() {
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

    /// Builds one explicit-empty persisted assignment for every pinned Scribe and table.
    ///
    /// # Errors
    /// Returns visibility unavailable when a participant epoch or its persisted
    /// WAL projection cannot be represented by the authenticated follower cut.
    fn scribe_follower_sources(
        &self,
        cuts: &[PinnedSealedTable],
        participant_cut: &OracleQueryAttemptCut,
    ) -> Result<Vec<ScribeFollowerSource>, BifrostError> {
        let tables = cuts
            .iter()
            .map(|cut| {
                let physical_schema = iceberg::arrow::schema_to_arrow_schema(
                    cut.iceberg_table.metadata().current_schema(),
                )
                .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
                Ok(ScribeAssignmentTable {
                    table: cut.binding.table_ref.fqn(),
                    binding: TenantTableBinding {
                        tenant_id: cut.binding.tenant,
                        namespace: cut.binding.logical_namespace.clone(),
                        table: cut.binding.table_name.clone(),
                    },
                    physical_schema: Arc::new(physical_schema),
                    sealed_manifest: &cut.sealed_manifest,
                })
            })
            .collect::<Result<Vec<_>, BifrostError>>()?;
        build_scribe_follower_sources(
            &tables,
            participant_cut.scribes(),
            self.planner.config.attempt_max_bytes,
        )
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
        OracleTelemetry::record_classification(OracleClassification {
            reason: "typed_plan",
            query_class: class,
            predicted_scan_seconds: 0.0,
        });
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
        let (session, mut admitted) = self.lease_session(
            options.deadline,
            admitted,
            Self::scannable_work_units(&cuts),
            "typed execution lease rejection",
        )?;
        admitted.retain_physical_projections(&cuts)?;
        let logical_bytes_selected = Self::logical_selected_bytes(&cuts);
        let mut drained = self
            .acquire_audit_and_drain_typed_tails(context, &plan, options, class, &cuts, &admitted)
            .await?;
        admitted.live_reservations = std::mem::take(&mut drained.reservations);
        let providers = self
            .planner
            .build_typed_providers(planner::TypedProviderInputs {
                context,
                class,
                cuts,
                drained: &mut drained,
                catalog: &self.catalog,
                audit: Arc::clone(&self.audit),
                memory: self.memory.clone(),
                query_pool: Arc::clone(&session.runtime_env().memory_pool),
                telemetry: Arc::clone(&self.telemetry),
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
            schema_frame,
            ipc,
            batches,
            first,
            admitted,
            deadline: options.deadline,
            visibility: options.visibility,
            freshness_policy: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            degraded_sources: Arc::new(std::sync::Mutex::new(if drained.degraded {
                vec![DegradedPartition {
                    ordinal: u32::MAX,
                    reason: "live_tail_unavailable",
                    sources: vec![QuerySource::LiveTail],
                }]
            } else {
                Vec::new()
            })),
            stale_replanned: false,
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
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.startup_reconciled() && self.admission.is_available()
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

    /// Lowers one complete SQL statement against exact pinned table providers.
    ///
    /// This operation happens only after the read-decision audit commits. Plan
    /// execution remains lazy until the caller performs the pre-byte lookahead.
    ///
    /// # Errors
    ///
    /// Returns a stable catalog or execution failure when a pinned source
    /// provider, logical plan, optimization, or physical stream cannot be built.
    async fn execute_sql_cut(
        &self,
        mut input: SqlCutInput<'_>,
    ) -> Result<
        (
            SchemaRef,
            SendableRecordBatchStream,
            OracleQueryScanStats,
            DegradedSourceAccumulator,
        ),
        OracleExecutionError,
    > {
        // `SessionContext` is an `Arc`-backed handle, so this clones the
        // handle rather than the session; the envelope stays whole for the
        // distributed call below.
        let session = input.session.clone();
        let mut assignments = CutAssignments::default();
        let mut scribe_sources = HashMap::<String, Vec<ScribeFollowerSource>>::new();
        for source in std::mem::take(&mut input.scribe_sources) {
            scribe_sources
                .entry(source.table.clone())
                .or_default()
                .push(source);
        }
        let mut live_batches = std::mem::take(&mut input.live_batches);
        for cut in std::mem::take(&mut input.cuts) {
            self.register_cut_provider(
                cut,
                &session,
                &input,
                &mut live_batches,
                &mut scribe_sources,
                &mut assignments,
            )
            .await?;
        }
        let CutAssignments {
            oracle_assignments,
            source_groups,
            scribe_assignments,
        } = assignments;
        if oracle_assignments.is_empty() && scribe_assignments.is_empty() {
            let (schema, stream, stats) = self
                .execute_session(&session, input.sql, input.logical_bytes_selected)
                .await?;
            Ok((
                schema,
                stream,
                stats,
                Arc::new(std::sync::Mutex::new(Vec::new())),
            ))
        } else {
            self.execute_distributed_session(
                &session,
                &input,
                DistributedScanAssignments {
                    oracle_assignments,
                    source_groups,
                    scribe_assignments,
                },
            )
            .await
        }
    }

    /// Registers one pinned table cut as a session provider and records its assignments.
    ///
    /// In distributed mode every source class the cut carries — Iceberg files,
    /// hot files, and each live Scribe tail — becomes its own scan id so the
    /// splitter can fan them across participants independently, and the same
    /// scan ids are mirrored into every Scribe that can serve them. In local
    /// mode no scan ids are produced and the hot files stay with the leader's
    /// own provider. Either way the resulting provider is registered under the
    /// cut's binding before the statement is planned.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when the cut's Iceberg
    /// schema cannot be converted to Arrow, and a mapped `DataFusion` error when
    /// the provider cannot be built or registered.
    /// Builds one pinned table's provider and registers it on the session.
    ///
    /// Owns the tail of cut registration: choosing local versus distributed
    /// source material, assembling [`OracleTableInputs`], and installing the
    /// provider under the cut's binding. A distributed cut reads nothing
    /// locally — its persisted files travel to followers as assignments — so
    /// its local hot-file list is deliberately empty. `distributed` and the
    /// canonical table name are re-derived here rather than threaded in, so
    /// this tail cannot disagree with the caller about either.
    ///
    /// # Errors
    ///
    /// Returns a mapped `DataFusion` error when local hot sources cannot be
    /// resolved or the provider cannot be constructed, and the stable
    /// registration error when the binding cannot be installed.
    async fn register_table_provider(
        &self,
        cut: PinnedSealedTable,
        session: &SessionContext,
        input: &SqlCutInput<'_>,
        live_batches: &mut HashMap<String, Vec<RecordBatch>>,
        remote_sources: RemotePersistedSources,
    ) -> Result<(), OracleExecutionError> {
        let distributed = self.fragment_dispatcher.is_some();
        let table_name = cut.binding.table_ref.fqn();
        let local_hot_files = if distributed {
            Vec::new()
        } else {
            self.local_hot_sources(&cut)?
        };
        let provider_inputs = OracleTableInputs {
            table: cut.iceberg_table,
            storage: Arc::clone(self.catalog.storage()),
            distributed_iceberg_batches: None,
            hot_files: local_hot_files,
            distributed_hot_batches: Vec::new(),
            live_batches: live_batches.remove(&table_name).unwrap_or_default(),
            context: input.context.clone(),
            table_name: table_name.clone(),
            audit: Arc::clone(&self.audit),
            memory: self.memory.clone(),
            query_pool: Arc::clone(&session.runtime_env().memory_pool),
            telemetry: Arc::clone(&self.telemetry),
            query_class: input.query_class,
        };
        let provider = if distributed {
            OracleTableProvider::try_new_distributed(provider_inputs, remote_sources).await
        } else {
            OracleTableProvider::try_new(provider_inputs).await
        }
        .map_err(|error| map_datafusion_error(&error))?;
        register_session_table(
            session,
            &cut.binding,
            Arc::new(provider) as Arc<dyn TableProvider>,
        )?;
        Ok(())
    }

    async fn register_cut_provider(
        &self,
        cut: PinnedSealedTable,
        session: &SessionContext,
        input: &SqlCutInput<'_>,
        live_batches: &mut HashMap<String, Vec<RecordBatch>>,
        scribe_sources: &mut HashMap<String, Vec<ScribeFollowerSource>>,
        assignments: &mut CutAssignments,
    ) -> Result<(), OracleExecutionError> {
        let distributed = self.fragment_dispatcher.is_some();
        let table_name = cut.binding.table_ref.fqn();
        let physical_schema = Arc::new(
            iceberg::arrow::schema_to_arrow_schema(cut.iceberg_table.metadata().current_schema())
                .map_err(|_| BifrostError::QueryExecutionFailed)?,
        );
        let schema_fingerprint = assignment_schema_fingerprint(physical_schema.as_ref());
        let binding = TenantTableBinding {
            tenant_id: input.context.data_tenant_id,
            namespace: cut.binding.logical_namespace.clone(),
            table: cut.binding.table_ref.name.clone(),
        };
        let mut remote_sources = RemotePersistedSources::default();
        let mut common_scan_ids = Vec::new();
        if distributed {
            let scan_id = format!("oracle:{table_name}:iceberg");
            let mut files = cut
                .iceberg_files
                .iter()
                .map(|file| iceberg_file_descriptor(file, cut.snapshot_id))
                .collect::<Vec<_>>();
            files.sort_by(|left, right| left.path().cmp(right.path()));
            assignments.record_scan(
                &scan_id,
                &table_name,
                &binding,
                &schema_fingerprint,
                physical_schema.as_ref(),
                files,
            );
            common_scan_ids.push(scan_id.clone());
            remote_sources.iceberg_scan_id = Some(scan_id);
        }
        if distributed && !cut.hot_files.is_empty() {
            let scan_id = format!("oracle:{table_name}:hot");
            let mut files = cut
                .hot_files
                .iter()
                .map(hot_file_descriptor)
                .collect::<Result<Vec<_>, _>>()?;
            files.sort_by(|left, right| left.path().cmp(right.path()));
            assignments.record_scan(
                &scan_id,
                &table_name,
                &binding,
                &schema_fingerprint,
                physical_schema.as_ref(),
                files,
            );
            common_scan_ids.push(scan_id.clone());
            remote_sources.hot_scan_id = Some(scan_id);
        }
        let table_scribes = scribe_sources.remove(&table_name).unwrap_or_default();
        for template in &table_scribes {
            let live_scan_id = template.assignment.scan_id.clone();
            assignments.record_scan(
                &live_scan_id,
                &table_name,
                &binding,
                &schema_fingerprint,
                physical_schema.as_ref(),
                Vec::new(),
            );
            common_scan_ids.push(live_scan_id.clone());
            remote_sources.scribe_scan_ids.push(live_scan_id);
        }
        for source in table_scribes {
            let node_assignments = assignments
                .scribe_assignments
                .entry(source.node_id)
                .or_default();
            for scan_id in &common_scan_ids {
                let mut assignment = source.assignment.clone();
                assignment.scan_id.clone_from(scan_id);
                node_assignments.insert(scan_id.clone(), assignment);
            }
        }
        self.register_table_provider(cut, session, input, live_batches, remote_sources)
            .await?;
        Ok(())
    }

    /// Rejects dispatch when any distributed assignment's projection closure
    /// would drop the hidden tenant column.
    ///
    /// This is the last construction-time gate before a
    /// [`FollowerScanAssignment`] is signed into the assignment-authority
    /// digest and dispatched. A defaulted or mis-merged `required_columns`
    /// that omitted [`DATA_TENANT_ID`] would still pass a signature check —
    /// the signature proves the digest matches what was signed, not that
    /// the signed projection was safe — so this invariant must be enforced
    /// here, before signing, rather than trusted implicitly.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryTenantInvariant`] when `required_columns`
    /// is empty or does not contain [`DATA_TENANT_ID`].
    fn ensure_required_columns_closure(
        assignment: &FollowerScanAssignment,
    ) -> Result<(), BifrostError> {
        if assignment.required_columns.is_empty()
            || !assignment
                .required_columns
                .iter()
                .any(|column| column == DATA_TENANT_ID)
        {
            return Err(BifrostError::QueryTenantInvariant);
        }
        Ok(())
    }

    /// Splits one complete native plan, dispatches its disjoint follower children, and runs finals.
    async fn execute_distributed_session(
        &self,
        session: &SessionContext,
        input: &SqlCutInput<'_>,
        assignments: DistributedScanAssignments,
    ) -> Result<
        (
            SchemaRef,
            SendableRecordBatchStream,
            OracleQueryScanStats,
            DegradedSourceAccumulator,
        ),
        OracleExecutionError,
    > {
        let context = input.context;
        let sql = input.sql;
        let query_class = input.query_class;
        let freshness = input.freshness;
        let admitted = input.admitted;
        let deadline = input.deadline;
        let logical_bytes_selected = input.logical_bytes_selected;
        let participant_cut = input.participant_cut;
        let DistributedScanAssignments {
            mut oracle_assignments,
            source_groups,
            mut scribe_assignments,
        } = assignments;
        let dispatcher = self
            .fragment_dispatcher
            .as_ref()
            .ok_or(BifrostError::QueryExecutionFailed)?;
        let (distributed_session, split) =
            plan_distributed_split(session, sql, source_groups, participant_cut).await?;
        // The optimized physical plan is the first point at which the real
        // predicate/projection closure for each distributed leaf is known
        // (`oracle_assignments` is built before SQL planning as a safe,
        // unpruned full-schema placeholder). Overwrite each persisted-file
        // assignment's closure with what the provider actually attached to its
        // `RemoteScanExec` placeholder during `scan()`; a scan id with no
        // recovered closure keeps its existing safe default rather than being
        // narrowed to an empty (tenant-dropping) projection.
        //
        // Scribe assignments share the live scan's identity, so they recover
        // the same closure from the same placeholder. A Scribe follower's
        // projection is not a file-pruning hint — there are no object reads —
        // but the signed predicates are enforced against the assembled tail
        // before it is returned, so narrowing here is what keeps a selective
        // query from shipping the whole live tail into attempt encoding.
        let remote_scan_closures = splitter::collect_remote_scan_closures(&split);
        for (scan_id, (required_columns, predicates)) in &remote_scan_closures {
            if let Some(assignment) = oracle_assignments.get_mut(scan_id) {
                assignment.required_columns.clone_from(required_columns);
                assignment.predicates.clone_from(predicates);
            }
            for assignments in scribe_assignments.values_mut() {
                if let Some(assignment) = assignments.get_mut(scan_id) {
                    assignment.required_columns.clone_from(required_columns);
                    assignment.predicates.clone_from(predicates);
                }
            }
        }
        // The recovered closure is the first authoritative statement of what
        // each fragment will actually filter on, so it is the earliest point a
        // file can be excluded — and it is still before partitioning, digest,
        // and signing, which is what keeps an excluded file off the wire.
        prune_assignments_by_event_time(&mut oracle_assignments);
        for assignment in oracle_assignments.values() {
            Self::ensure_required_columns_closure(assignment)?;
        }
        for assignments in scribe_assignments.values() {
            for assignment in assignments.values() {
                Self::ensure_required_columns_closure(assignment)?;
            }
        }
        let permission_digest = audit_digest(&context.permission)?.as_str().to_owned();
        let dispatch_context = dispatcher::DispatchContext {
            query_id: admitted.query_id,
            leader_node_id: admitted.leader.node_id,
            leader_fence: admitted.leader.fencing_token,
            tenant_id: context.data_tenant_id.as_uuid(),
            query_class,
            slot_units: admission_limits(u32::MAX, query_class).1,
            permission_digest,
            attempt_bytes: self.planner.config.attempt_max_bytes,
            attempt_memory_bytes: self.planner.config.attempt_memory_bytes,
            query_memory_pool: admitted
                .memory_pool()
                .ok_or(BifrostError::QueryAdmissionRejected)?,
            granted_memory_bytes: admitted.granted_memory_bytes(),
            admitted_target_partitions: admitted.target_partitions(),
            cancellation: admitted.cancellation.clone(),
            deadline: deadline.into(),
        };
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let deadline_unix_ms = (chrono::Utc::now()
            + chrono::Duration::from_std(remaining).map_err(|_| BifrostError::QueryTimeout)?)
        .timestamp_millis();
        let mut remote_plans = HashMap::new();
        let degraded_sources = Arc::new(std::sync::Mutex::new(Vec::new()));
        let build = RemoteScanBuildContext {
            dispatcher,
            dispatch_context: &dispatch_context,
            admitted,
            participant_cut,
            oracle_assignments: &oracle_assignments,
            scribe_assignments: &scribe_assignments,
            freshness,
            deadline_unix_ms,
            degraded_sources: &degraded_sources,
        };
        for follower in split.followers {
            let result_scan_id = follower.result_scan_id.clone();
            remote_plans.insert(result_scan_id, self.build_remote_scan(follower, &build)?);
        }
        let leader = splitter::substitute_remote_plans(split.leader, remote_plans)
            .map_err(|error| map_datafusion_error(&error))?;
        let scan_stats = OracleQueryScanStats::from_plan(leader.as_ref(), logical_bytes_selected);
        let schema = leader.schema();
        let stream = execute_stream(leader, distributed_session.task_ctx())
            .map_err(|error| map_datafusion_error(&error))?;
        Ok((schema, stream, scan_stats, degraded_sources))
    }

    /// Builds one participant-fanned remote scan for a single follower subtree.
    ///
    /// Resolves the follower's assigned Oracle sources, proves every assignment
    /// shares one tenant/table binding, serializes the fragment plan exactly
    /// once so every participant receives byte-identical bytes, then fans the
    /// assignments across the signed participant cut. Oracle participants are
    /// ordered remote-first so the leader takes the last partition, and Scribe
    /// participants are appended only where they hold an assigned scan.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when an assigned scan is
    /// missing from the cut, when assignments disagree about their binding, or
    /// when no participant remains after fanning; [`BifrostError::OracleRoleUnavailable`]
    /// when the cut holds no Oracle candidate; and a mapped `DataFusion` error
    /// when the fragment plan cannot be serialized.
    fn build_remote_scan(
        &self,
        follower: splitter::FollowerSubtree,
        build: &RemoteScanBuildContext<'_>,
    ) -> Result<Arc<dyn datafusion::physical_plan::ExecutionPlan>, OracleExecutionError> {
        let RemoteScanBuildContext {
            dispatcher,
            dispatch_context,
            admitted,
            oracle_assignments,
            scribe_assignments,
            freshness,
            deadline_unix_ms,
            degraded_sources,
            participant_cut: _,
        } = *build;
        let oracle_templates = follower
            .source_scan_ids
            .iter()
            .map(|scan_id| {
                oracle_assignments
                    .get(scan_id)
                    .cloned()
                    .ok_or(BifrostError::QueryExecutionFailed)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let binding = oracle_templates
            .first()
            .map(|assignment| assignment.binding.clone())
            .or_else(|| {
                scribe_assignments.values().find_map(|assignments| {
                    follower
                        .source_scan_ids
                        .first()
                        .and_then(|scan_id| assignments.get(scan_id))
                        .map(|assignment| assignment.binding.clone())
                })
            })
            .ok_or(BifrostError::QueryExecutionFailed)?;
        if oracle_templates
            .iter()
            .any(|assignment| assignment.binding != binding)
        {
            return Err(BifrostError::QueryExecutionFailed.into());
        }
        let remote_schema = follower.plan.schema();
        let (physical_plan_bytes, plan_fingerprint) = codec::encode_follower_subtree(follower.plan)
            .map_err(|error| map_datafusion_error(&error))?;
        let partitions =
            fan_participant_partitions(&follower.source_scan_ids, &oracle_templates, build)?;
        let remote = exec::RemoteScanExec::new(exec::RemoteScanConfig {
            schema: remote_schema,
            dispatcher: Arc::clone(dispatcher),
            context: dispatch_context.clone(),
            settlement: Arc::clone(&admitted.distributed_settlement),
            physical_plan_bytes,
            binding,
            plan_fingerprint,
            deadline_unix_ms,
            partitions,
            freshness,
            degraded_sources: Arc::clone(degraded_sources),
        });
        #[cfg(feature = "test-support")]
        let remote = remote.with_topology_probe(
            self.topology_probe
                .lock()
                .ok()
                .and_then(|probe| probe.clone()),
        );
        Ok(Arc::new(remote) as Arc<dyn datafusion::physical_plan::ExecutionPlan>)
    }

    /// Builds one Oracle execution session over governed memory and query spill.
    ///
    /// Every attempt receives a fresh `DataFusion` session. All attempts share
    /// Oracle-child and parent memory counters, while the admitted guard's
    /// immutable spill share becomes this session's exact disk ceiling.
    /// `DataFusion`'s merge reservation remains enabled so external sorts retain
    /// bounded progress memory before consuming their remaining fair share.
    ///
    /// Target partitions, batch size, and the join preference all come from one
    /// [`OracleSessionShape`](crate::resources::OracleSessionShape) derived from
    /// the admitted grant. This is the only site that builds them, so a query
    /// cannot end up with a partition count sized for one ceiling and a batch
    /// size sized for another.
    ///
    /// # Errors
    ///
    /// Returns a stable execution error when `DataFusion` cannot construct the
    /// runtime environment.
    fn execution_session(
        &self,
        admitted: &AdmittedQueryGuard,
        work_units: usize,
    ) -> Result<SessionContext, BifrostError> {
        let pool = admitted
            .memory_pool()
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        let runtime = self
            .spill_runtime
            .build_query_runtime(pool, admitted.spill_limit_bytes())?;
        let shape = crate::resources::OracleSessionShape::for_grant(
            admitted.granted_memory_bytes(),
            admitted.target_partitions(),
            work_units,
        );
        let config = shape.session_config();
        let state = datafusion::execution::session_state::SessionStateBuilder::new()
            .with_default_features()
            .with_config(config)
            .with_runtime_env(runtime)
            .build();
        Ok(SessionContext::new_with_state(state))
    }

    /// Acquires the query execution session or releases its admission atomically.
    ///
    /// This boundary prevents either SQL entry point from retaining class,
    /// tenant, memory, or spill admission after the execution lease refuses.
    ///
    /// # Errors
    ///
    /// Returns the stable typed execution-session error after synchronously
    /// releasing the supplied admission owner.
    fn lease_session(
        &self,
        deadline: Instant,
        admitted: AdmittedQueryGuard,
        work_units: usize,
        failure_phase: &'static str,
    ) -> Result<(SessionContext, AdmittedQueryGuard), BifrostError> {
        match self.execution_session(&admitted, work_units) {
            Ok(session) => Ok((session, admitted)),
            Err(error) => release_error(deadline, admitted, error, failure_phase),
        }
    }

    /// Leases the inactive Analytical session for one attempt, or releases it.
    ///
    /// The returned session plans and executes distributed. Its graph and
    /// attempt ownership is attached to the admitted guard rather than returned
    /// separately, so it settles when the query stream drains and cannot be
    /// dropped early by a caller that only holds the session.
    ///
    /// # Errors
    ///
    /// Returns the stable admission, supervisor, or runtime error after
    /// synchronously releasing the supplied admission owner.
    fn lease_analytical_session(
        &self,
        deadline: Instant,
        admitted: AdmittedQueryGuard,
        attempt: &analytical::AnalyticalAttemptContext,
        participant_cut: &OracleQueryAttemptCut,
        context: &AuthorizedQueryContext,
        work_units: usize,
    ) -> Result<(SessionContext, AdmittedQueryGuard), BifrostError> {
        let Some(handle) = self.analytical.as_ref() else {
            return release_error(
                deadline,
                admitted,
                BifrostError::OracleRoleUnavailable,
                "analytical lease without a composed handle",
            );
        };
        match handle.lease_session(attempt, participant_cut, context, work_units) {
            Ok((session, ownership)) => {
                let mut admitted = admitted;
                admitted.analytical = Some(ownership);
                Ok((session, admitted))
            }
            Err(error) => release_error(deadline, admitted, error, "analytical lease rejection"),
        }
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

    /// Builds and starts one physical plan after all pinned providers are registered.
    ///
    /// # Errors
    ///
    /// Returns a stable execution failure when `DataFusion` cannot lower or start the plan.
    async fn execute_session(
        &self,
        session: &SessionContext,
        sql: &str,
        logical_bytes_selected: u64,
    ) -> Result<(SchemaRef, SendableRecordBatchStream, OracleQueryScanStats), OracleExecutionError>
    {
        let frame = session
            .sql(sql)
            .await
            .map_err(|error| map_datafusion_error(&error))?;
        let physical = frame
            .create_physical_plan()
            .await
            .map_err(|error| map_datafusion_error(&error))?;
        let scan_stats = OracleQueryScanStats::from_plan(physical.as_ref(), logical_bytes_selected);
        let schema = physical.schema();
        let stream = execute_stream(physical, session.task_ctx())
            .map_err(|error| map_datafusion_error(&error))?;
        Ok((schema, stream, scan_stats))
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
#[must_use]
pub fn failed_terminal(code: QueryTerminalErrorCode, row_count: u64) -> QueryTerminalFrame {
    failed_terminal_for_visibility(code, row_count, VisibilityMode::PublishedOnly)
}

/// Returns a contract-valid failed terminal for the query's visibility cut.
fn failed_terminal_for_visibility(
    code: QueryTerminalErrorCode,
    row_count: u64,
    visibility: VisibilityMode,
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
    retry_ordinal: u8,
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
        retry_ordinal,
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

/// Classifies an optimized logical plan by its real global/heavy operators.
///
/// Parsing text is intentionally insufficient here: aliases, nested CTEs, and
/// optimizer rewrites must receive the same admission class as their final
/// logical operator graph.
fn optimized_plan_is_complex(plan: &datafusion::logical_expr::LogicalPlan) -> bool {
    use datafusion::logical_expr::LogicalPlan;

    if matches!(
        plan,
        LogicalPlan::Window(_)
            | LogicalPlan::Aggregate(_)
            | LogicalPlan::Join(_)
            | LogicalPlan::Repartition(_)
            | LogicalPlan::Union(_)
            | LogicalPlan::Distinct(_)
            | LogicalPlan::RecursiveQuery(_)
    ) {
        return true;
    }
    if let LogicalPlan::Sort(sort) = plan
        && sort.fetch.is_none()
    {
        return true;
    }
    plan.inputs().into_iter().any(optimized_plan_is_complex)
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
    running_query: RunningQueryTerminalOwner,
}

/// Request-scoped facts settlement needs that do not come from execution.
struct AttemptSettlement {
    /// Absolute whole-query deadline shared across retry attempts.
    deadline: Instant,
    /// Zero-based stale-replan attempt ordinal.
    retry_ordinal: u8,
    /// Gate deciding whether a typed stale first batch may be replanned.
    stale_replacement: StaleReplacementGate,
    /// Caller-selected visibility retained on the returned stream.
    visibility: VisibilityMode,
    /// Caller-selected source-loss policy retained on the returned stream.
    freshness: wyrd_spec::vala::api::FreshnessPolicy,
    /// Optional Gate request lifecycle transferred into the returned stream.
    gate_lifecycle: Option<Arc<crate::oracle::query_stream::QueryStreamLifecycle>>,
}

/// Awaits the first batch and converts one executed attempt into a query stream.
///
/// The first batch is the decision point for the whole attempt. A typed stale
/// object error observed before any output means the pinned cut moved under the
/// query, so the attempt is cancelled, its distributed children are joined, and
/// the caller is told to replan — but only while replacement is still eligible.
/// Any other first-batch failure, a missing telemetry guard, or a schema-frame
/// failure settles the distributed children and releases the admitted owner
/// before returning, so no child outlives its parent on a failure path.
///
/// # Errors
///
/// Returns [`BifrostError::QueryTimeout`] when the first batch does not arrive
/// before the deadline, [`BifrostError::QueryExecutionFailed`] for a stale first
/// batch that can no longer be replanned or a missing telemetry guard, and the
/// mapped first-batch failure otherwise. `Ok(None)` means the caller must
/// replan rather than that the query returned no rows.
async fn settle_attempt_output(
    output: AttemptOutput,
    settle: AttemptSettlement,
    query_telemetry: &mut Option<QueryTelemetryGuard>,
) -> Result<Option<OracleQueryStream>, BifrostError> {
    let AttemptOutput {
        schema,
        mut batches,
        scan_stats,
        degraded_sources,
        admitted,
        running_query,
    } = output;
    let AttemptSettlement {
        deadline,
        retry_ordinal,
        stale_replacement,
        visibility,
        freshness,
        gate_lifecycle,
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
        return match stale_replacement.settle_for_typed_stale(admitted, true) {
            Ok(()) => Ok(None),
            Err(admitted) => release_error(
                deadline,
                *admitted,
                BifrostError::QueryExecutionFailed,
                "final stale first batch",
            ),
        };
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
    Ok(Some(OracleQueryStream::new(QueryStreamInput {
        schema_frame,
        ipc,
        batches,
        first,
        admitted,
        deadline,
        visibility,
        freshness_policy: freshness,
        degraded_sources,
        stale_replanned: retry_ordinal == 1,
        query_telemetry,
        scan_stats,
        gate_lifecycle,
        running_query: Some(running_query),
    })))
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

/// Sums the advertised CPU cores of every Oracle participant in the signed cut.
///
/// Planning sizes the distributed statement against the cut's total Oracle
/// compute, so Scribe participants contribute nothing here even when they will
/// serve live-tail scans: they execute assigned fragments but do not widen the
/// leader's partition target.
fn oracle_cut_cpu_cores(participant_cut: &OracleQueryAttemptCut) -> f64 {
    participant_cut
        .oracles()
        .iter()
        .filter_map(|participant| match &participant.capabilities {
            ClusterCapabilities::OracleV1(capabilities) => Some(capabilities.cpu_cores),
            ClusterCapabilities::ScribeV1(_) => None,
        })
        .sum()
}

/// Fans one follower's assignments across every participant in the signed cut.
///
/// Oracle participants are ordered remote-first so the leader takes the final
/// partition and cannot starve its peers of work when the assignment split is
/// uneven. Scribe participants are appended only where the cut actually holds
/// an assignment for this follower's scans; a Scribe with no assigned scan
/// contributes no partition rather than an empty one.
///
/// # Errors
///
/// Returns [`BifrostError::OracleRoleUnavailable`] when the cut holds no Oracle
/// candidate, and [`BifrostError::QueryExecutionFailed`] when a Scribe holds a
/// partial assignment set or when no participant remains after fanning.
fn fan_participant_partitions(
    source_scan_ids: &[String],
    oracle_templates: &[FollowerScanAssignment],
    build: &RemoteScanBuildContext<'_>,
) -> Result<Vec<exec::RemotePartitionDescriptor>, OracleExecutionError> {
    let RemoteScanBuildContext {
        admitted,
        participant_cut,
        scribe_assignments,
        ..
    } = *build;
    let oracle_candidates = participant_cut
        .oracles()
        .iter()
        .filter(|participant| participant.node_id != admitted.leader.node_id)
        .chain(
            participant_cut
                .oracles()
                .iter()
                .filter(|participant| participant.node_id == admitted.leader.node_id),
        )
        .map(|participant| dispatcher::DispatchCandidate {
            node_id: participant.node_id,
            role: participant.role,
            worker_fence: participant.fencing_token,
            endpoint: Some(participant.endpoint.clone()),
        })
        .collect::<Vec<_>>();
    if oracle_candidates.is_empty() {
        return Err(BifrostError::OracleRoleUnavailable.into());
    }
    let mut partitions = oracle_candidates
        .iter()
        .cloned()
        .zip(partition_oracle_assignments(
            oracle_templates,
            &oracle_candidates,
        ))
        .map(|(candidate, assignments)| exec::RemotePartitionDescriptor {
            sources: follower_assignment_sources(candidate.role, &assignments),
            candidate,
            assignments,
        })
        .collect::<Vec<_>>();
    for participant in participant_cut.scribes() {
        let Some(assignments) = scribe_assignments.get(&participant.node_id) else {
            continue;
        };
        let assignments = source_scan_ids
            .iter()
            .map(|scan_id| {
                assignments
                    .get(scan_id)
                    .cloned()
                    .ok_or(BifrostError::QueryExecutionFailed)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if assignments.is_empty() {
            continue;
        }
        let candidate = dispatcher::DispatchCandidate {
            node_id: participant.node_id,
            role: participant.role,
            worker_fence: participant.fencing_token,
            endpoint: Some(participant.endpoint.clone()),
        };
        let assignment_sources = follower_assignment_sources(candidate.role, &assignments);
        partitions.push(exec::RemotePartitionDescriptor {
            candidate,
            assignments,
            sources: assignment_sources,
        });
    }
    if partitions.is_empty() {
        return Err(BifrostError::QueryExecutionFailed.into());
    }
    Ok(partitions)
}

/// Plans one distributed physical split from a validated SQL statement.
///
/// Installs the remote-scan rule, its post-remote companion, and limit pushdown
/// on top of the caller's optimizer stack, then plans the statement exactly once
/// in a session carrying those rules. The rule records every follower subtree it
/// captured while planning, so the followers are taken from the rule after the
/// physical plan exists rather than by re-walking the tree.
///
/// # Errors
///
/// Returns a mapped `DataFusion` error when the statement cannot be planned,
/// when the physical plan cannot be created, or when the rule's captured
/// followers cannot be taken.
async fn plan_distributed_split(
    session: &SessionContext,
    sql: &str,
    source_groups: HashMap<String, String>,
    participant_cut: &OracleQueryAttemptCut,
) -> Result<(SessionContext, splitter::SplitPhysicalPlan), OracleExecutionError> {
    let remote_rule = Arc::new(splitter::RemoteScanRule::new(
        source_groups,
        participant_cut
            .oracles()
            .len()
            .saturating_add(participant_cut.scribes().len()),
    ));
    let mut physical_rules = session.state().physical_optimizers().to_vec();
    physical_rules.push(Arc::clone(&remote_rule)
        as Arc<
            dyn datafusion::physical_optimizer::PhysicalOptimizerRule + Send + Sync,
        >);
    physical_rules.push(Arc::new(splitter::PostRemoteOptimizerRule));
    physical_rules.push(Arc::new(
        datafusion::physical_optimizer::limit_pushdown::LimitPushdown::new(),
    ));
    let distributed_session = SessionContext::new_with_state(
        datafusion::execution::session_state::SessionStateBuilder::new_from_existing(
            session.state(),
        )
        .with_physical_optimizer_rules(physical_rules)
        .build(),
    );
    let frame = distributed_session
        .sql(sql)
        .await
        .map_err(|error| map_datafusion_error(&error))?;
    let physical = frame
        .create_physical_plan()
        .await
        .map_err(|error| map_datafusion_error(&error))?;
    let followers = remote_rule
        .take_followers()
        .map_err(|error| map_datafusion_error(&error))?;
    Ok((
        distributed_session,
        splitter::SplitPhysicalPlan {
            leader: physical,
            followers,
        },
    ))
}

/// Applies the locked independent class ceilings and per-node slot demand.
///
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

/// Returns the partition bounds one Scribe memory cut may claim.
///
/// The bounds are the complete bucketable hourly span, and that is the exact
/// answer rather than a missing optimization. A Scribe holds a partition in
/// memory precisely while nothing has published it, so the only evidence Oracle
/// has — the sealed manifest — describes the partitions that are *not* the ones
/// this cut needs to reach. Narrowing the window to the published partitions
/// would drop every acknowledged row still waiting in memory for a partition
/// that has published nothing yet, and no wider inference from published rows
/// recovers it either. Scribe answers what it still holds from the generation
/// authority it owns; the cut bounds the reader by writer incarnation,
/// projection, batch count, and retained bytes, which are the bounds Oracle can
/// state truthfully.
///
/// The endpoints are one hour inside the nanosecond instant domain because that
/// is the domain partition bucketing itself is defined on; an event time
/// outside it cannot become a partition at all, so no admitted row can fall
/// beyond these bounds.
///
/// # Errors
///
/// Returns [`BifrostError::QueryVisibilityUnavailable`] when either endpoint
/// does not bucket into a canonical hourly partition, which the fixed margin
/// makes unreachable.
fn scribe_memory_partition_bounds() -> Result<
    (
        wyrd_spec::vala::api::TimePartitionWire,
        wyrd_spec::vala::api::TimePartitionWire,
    ),
    BifrostError,
> {
    /// Nanoseconds in one hour, the margin that keeps truncation representable.
    const MARGIN_NANOS: i64 = 3_600 * 1_000_000_000;

    let hour = crate::catalog::TimeGranularity::Hour;
    let bucket = |instant: DateTime<Utc>| {
        hour.bucket(instant)
            .map(crate::catalog::layout::TimePartition::to_wire)
            .map_err(|_| BifrostError::QueryVisibilityUnavailable)
    };
    let first = bucket(DateTime::from_timestamp_nanos(i64::MIN + MARGIN_NANOS))?;
    let last = bucket(DateTime::from_timestamp_nanos(i64::MAX - MARGIN_NANOS))?;
    Ok((first, last))
}

/// Cross-products pinned tables and Scribes into explicit-empty persisted assignments.
///
/// The cut bounds a follower's partition range, writer incarnation, and
/// retention. It deliberately carries no published-WAL interval: a node numbers
/// WAL records from one global counter while sealing per `(table, partition)`,
/// so a published envelope routinely encloses records belonging to a still-live
/// member of another bucket. Scribe answers what remains readable from its own
/// generation authority, which is exact.
///
/// # Errors
/// Returns visibility unavailable when a partition bound or the configured
/// retained-byte ceiling cannot be represented by the private cut.
fn build_scribe_follower_sources(
    tables: &[ScribeAssignmentTable<'_>],
    scribes: &[OracleQueryParticipant],
    attempt_max_bytes: usize,
) -> Result<Vec<ScribeFollowerSource>, BifrostError> {
    tables
        .iter()
        .flat_map(|table| scribes.iter().map(move |participant| (table, participant)))
        .map(|(table, participant)| {
            let writer_epoch = participant.fencing_token;
            let node_id = participant.node_id;
            let stream_rows = table
                .sealed_manifest
                .iter()
                .filter(|row| {
                    row.node_id == node_id.as_uuid()
                        && u64::try_from(row.writer_epoch).ok() == Some(writer_epoch)
                })
                .cloned()
                .collect::<Vec<_>>();
            tracing::debug!(
                operation = "oracle_scribe_cut",
                node = %node_id.as_uuid(),
                writer_epoch,
                sealed_rows = stream_rows.len(),
                bounds = ?stream_rows
                    .iter()
                    .map(|row| (row.wal_lsn_min, row.wal_lsn_max, row.file_ordinal))
                    .collect::<Vec<_>>(),
                "Oracle pinned one Scribe memory cut for a Scribe stream"
            );
            let (start_partition, end_partition) = scribe_memory_partition_bounds()?;
            Ok(ScribeFollowerSource {
                table: table.table.clone(),
                node_id,
                assignment: FollowerScanAssignment {
                    scan_id: scribe_follower_scan_id(
                        &format!("{}.{}", table.binding.namespace, table.binding.table),
                        node_id,
                        writer_epoch,
                    ),
                    binding: table.binding.clone(),
                    persisted: PersistedFileAssignment { files: Vec::new() },
                    scribe_provider_cut: Some(ScribeProviderCut {
                        writer_epoch,
                        start_partition,
                        end_partition,
                        maximum_batch_count: 1_024,
                        maximum_retained_bytes: u64::try_from(attempt_max_bytes)
                            .map_err(|_| BifrostError::QueryVisibilityUnavailable)?,
                    }),
                    schema_fingerprint: assignment_schema_fingerprint(&table.physical_schema),
                    required_columns: table
                        .physical_schema
                        .fields()
                        .iter()
                        .map(|field| field.name().clone())
                        .collect(),
                    predicates: Vec::new(),
                },
            })
        })
        .collect()
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

/// Drops every assigned file whose immutable event-time bounds cannot intersect
/// the assignment's own signed predicates.
///
/// Runs after each assignment has recovered its real closure from the optimized
/// physical plan and before partitioning, digest, and signing, so an excluded
/// file is never named on the wire, never reserved against, never resolved by a
/// follower, and never opened. The decision reads only the descriptor's own
/// declared pair, which the leader minted from the catalog row or manifest
/// entry the cut pinned.
///
/// This is a strict narrowing of an already-authorized set: a follower's
/// `assigned ⊆ planned` drift check and its residual filter are unaffected,
/// because removing a file can only remove rows the predicate would have
/// discarded anyway. A descriptor whose pair is absent or unordered is retained
/// with its exact defect, so an unusable statistic loses rows from nothing.
///
/// Each considered file emits exactly one `bifrost_oracle_file_pruning_total`
/// observation under its own source label, which is what lets the emitted
/// counts reconcile against the file count of the cut.
fn prune_assignments_by_event_time(assignments: &mut HashMap<String, FollowerScanAssignment>) {
    for assignment in assignments.values_mut() {
        let interval = EventTimeQueryInterval::from_predicates(&assignment.predicates);
        if interval.is_unbounded() {
            continue;
        }
        assignment.persisted.files.retain(|descriptor| {
            let source = match descriptor {
                PersistedFileDescriptor::Hot(_) => FilePruningSource::Hot,
                PersistedFileDescriptor::Iceberg(_) => FilePruningSource::Iceberg,
            };
            let statistics = match descriptor.event_time_micros() {
                Some((min_micros, max_micros)) => EventTimeStatistics::Bounded {
                    min_micros,
                    max_micros,
                },
                None => EventTimeStatistics::Unusable(EventTimeBoundsDefect::Missing),
            };
            interval.retains(source, statistics)
        });
    }
}

fn partition_oracle_assignments(
    assignments: &[FollowerScanAssignment],
    workers: &[dispatcher::DispatchCandidate],
) -> Vec<Vec<FollowerScanAssignment>> {
    let worker_count = workers.len();
    if worker_count == 0 {
        return Vec::new();
    }
    let mut partitions = vec![Vec::with_capacity(assignments.len()); worker_count];
    for assignment in assignments {
        let specialized_files = partition_values(
            &assignment.persisted.files,
            &workers
                .iter()
                .map(|worker| worker.node_id.as_uuid().as_bytes().to_vec())
                .collect::<Vec<_>>(),
            OraclePartitionStrategy::default(),
            wyrd_spec::vala::api::PersistedFileDescriptor::size_bytes,
            |descriptor| descriptor.path().as_bytes().to_vec(),
        );
        for (partition, files) in partitions.iter_mut().zip(specialized_files) {
            let mut specialized = assignment.clone();
            specialized.persisted.files = files;
            partition.push(specialized);
        }
    }
    partitions
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

/// Derives the identity-bound common-plan placeholder for one pinned Scribe stream.
fn scribe_follower_scan_id(table: &str, node_id: NodeId, writer_epoch: u64) -> String {
    format!(
        "oracle:{table}:scribe:{}:{writer_epoch}:live",
        node_id.as_uuid()
    )
}

/// Specializes only role-local provider assignments around one common encoded plan.
fn common_physical_fragment(
    physical_plan_bytes: &[u8],
    assignments: Vec<FollowerScanAssignment>,
    binding: &TenantTableBinding,
    target_role: wyrd_spec::vala::api::ClusterRole,
    plan_fingerprint: &str,
    deadline_unix_ms: i64,
) -> dispatcher::PhysicalDispatchFragment {
    dispatcher::PhysicalDispatchFragment {
        physical_plan_bytes: physical_plan_bytes.to_vec(),
        assignments,
        binding: binding.clone(),
        target_role,
        plan_fingerprint: plan_fingerprint.to_owned(),
        deadline_unix_ms,
    }
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

/// Records consumption of the sole pre-byte stale-cut replan.
///
/// A stale replan means a data file this query's pinned snapshot referenced was
/// already deleted when the scan reached it — a live reader raced a Forge
/// compaction that reclaimed its input. The query recovers by replanning, but it
/// pays a second pin, a second admission, and a second audit, so a sustained
/// rate here is a retention problem rather than normal operation. It is WARN,
/// not DEBUG: correctness holds, but the cost is real and its cause is upstream.
fn record_stale_replan() {
    metrics::counter!("oracle_query_stale_replans_total").increment(1);
    tracing::warn!("Oracle replanned one query after its pinned data file was already deleted");
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
    use std::collections::HashSet;
    use std::sync::atomic::AtomicUsize;

    /// Creates one stable tenant/table binding for follower-assignment tests.
    fn test_follower_binding(table: &str) -> TenantTableBinding {
        TenantTableBinding {
            tenant_id: DataTenantId::new_v7(),
            namespace: "bifrost".to_owned(),
            table: table.to_owned(),
        }
    }

    /// Creates one persisted-only assignment with an explicit ordered file list.
    fn test_persisted_assignment(scan_id: &str, files: &[&str]) -> FollowerScanAssignment {
        FollowerScanAssignment {
            scan_id: scan_id.to_owned(),
            binding: test_follower_binding(scan_id),
            persisted: PersistedFileAssignment {
                files: files
                    .iter()
                    .map(|file| test_persisted_descriptor(file))
                    .collect(),
            },
            scribe_provider_cut: None,
            schema_fingerprint: format!("schema-{scan_id}"),
            required_columns: vec!["data_tenant_id".to_owned()],
            predicates: Vec::new(),
        }
    }

    /// A signed assignment loses every file whose declared event-time bounds
    /// cannot intersect its own predicates, and keeps every file whose evidence
    /// cannot support the exclusion.
    ///
    /// This runs before partitioning, digest, and signing, so an excluded file
    /// is never named on the wire: proving the file list itself shrank is
    /// proving no follower can resolve, reserve for, or open that object. Both
    /// source variants are asserted in one owner because they carry their
    /// bounds in different descriptor fields, and a version that read only one
    /// of them would still pass a single-source test.
    ///
    /// # Panics
    /// Panics when the retained file list or the emitted decisions violate the
    /// pruning contract.
    #[test]
    fn distributed_assignments_drop_files_disjoint_from_their_signed_predicates() {
        use wyrd_spec::vala::WYRD_EVENT_TIME;
        use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

        let lower_micros = 1_787_493_600_000_000_i64;
        let upper_micros = 1_787_497_200_000_000_i64;
        let iceberg = |path: &str, bounds: Option<(i64, i64)>| {
            PersistedFileDescriptor::Iceberg(wyrd_spec::vala::api::IcebergFileDescriptor {
                path: path.to_owned(),
                size_bytes: 4_096,
                row_count: 128,
                snapshot_id: 1,
                min_event_time_micros: bounds.map(|(min, _)| min),
                max_event_time_micros: bounds.map(|(_, max)| max),
            })
        };
        let hot = |path: &str, bounds: Option<(i64, i64)>| {
            PersistedFileDescriptor::Hot(wyrd_spec::vala::api::HotFileDescriptor {
                path: path.to_owned(),
                size_bytes: 4_096,
                row_count: 128,
                file_list_id: uuid::Uuid::now_v7(),
                sha256: [7_u8; 32],
                min_event_time_micros: bounds.map(|(min, _)| min),
                max_event_time_micros: bounds.map(|(_, max)| max),
            })
        };

        let mut assignment = test_persisted_assignment("scan-1", &[]);
        assignment.persisted.files = vec![
            iceberg("overlapping.parquet", Some((lower_micros, upper_micros))),
            iceberg("disjoint.parquet", Some((0, lower_micros - 1))),
            // Bounds the leader could not decode reach the wire absent.
            iceberg("unusable.parquet", None),
            hot(
                "hot-endpoint.parquet",
                Some((upper_micros, upper_micros + 9)),
            ),
            hot(
                "hot-disjoint.parquet",
                Some((upper_micros + 1, upper_micros + 2)),
            ),
        ];
        assignment.predicates = vec![
            ScanPredicate::GtEq(
                WYRD_EVENT_TIME.to_owned(),
                ScanLiteral::TimestampMicros(lower_micros),
            ),
            ScanPredicate::LtEq(
                WYRD_EVENT_TIME.to_owned(),
                ScanLiteral::TimestampMicros(upper_micros),
            ),
        ];
        let unconstrained = assignment.clone();
        let mut assignments = HashMap::from([("scan-1".to_owned(), assignment)]);

        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let guard = metrics::set_default_local_recorder(&recorder);
        prune_assignments_by_event_time(&mut assignments);
        drop(guard);

        assert_eq!(
            assignments["scan-1"]
                .persisted
                .files
                .iter()
                .map(PersistedFileDescriptor::path)
                .collect::<Vec<_>>(),
            vec![
                "overlapping.parquet",
                "unusable.parquet",
                "hot-endpoint.parquet",
            ],
            "only the provably disjoint files leave the assignment"
        );
        let snapshot = recorder.snapshot();
        let count = |source: &str, outcome: &str| {
            snapshot
                .counters
                .get(&format!(
                    "bifrost_oracle_file_pruning_total{{outcome=\"{outcome}\",source=\"{source}\"}}"
                ))
                .copied()
                .unwrap_or_default()
        };
        assert_eq!(count("iceberg", "included"), 1, "{snapshot:?}");
        assert_eq!(count("iceberg", "excluded"), 1, "{snapshot:?}");
        assert_eq!(
            count("iceberg", "fail_open_missing_bounds"),
            1,
            "{snapshot:?}"
        );
        assert_eq!(count("hot", "included"), 1, "{snapshot:?}");
        assert_eq!(count("hot", "excluded"), 1, "{snapshot:?}");

        // A closure that constrains no event-time endpoint leaves every signed
        // file in place rather than deciding it against an empty interval.
        let mut unconstrained = HashMap::from([("scan-1".to_owned(), unconstrained)]);
        unconstrained
            .get_mut("scan-1")
            .expect("fixture assignment")
            .predicates = vec![ScanPredicate::IsNotNull(WYRD_EVENT_TIME.to_owned())];
        prune_assignments_by_event_time(&mut unconstrained);
        assert_eq!(
            unconstrained["scan-1"].persisted.files.len(),
            5,
            "an unconstrained closure excludes no signed file"
        );
    }

    /// A distributed assignment whose `required_columns` omits the hidden
    /// tenant column, or is empty, is rejected before dispatch. A signed
    /// digest over a wrong projection would still verify cleanly, so this
    /// invariant must be enforced by construction rather than trusted.
    #[test]
    fn required_columns_closure_rejects_missing_or_empty_tenant_column() {
        let mut assignment = test_persisted_assignment("scan-1", &["s3://bucket/a.parquet"]);
        assert!(
            Oracle::ensure_required_columns_closure(&assignment).is_ok(),
            "fixture default includes data_tenant_id"
        );

        assignment.required_columns = vec!["service_name".to_owned()];
        assert!(matches!(
            Oracle::ensure_required_columns_closure(&assignment),
            Err(BifrostError::QueryTenantInvariant)
        ));

        assignment.required_columns = Vec::new();
        assert!(matches!(
            Oracle::ensure_required_columns_closure(&assignment),
            Err(BifrostError::QueryTenantInvariant)
        ));
    }

    /// Every pinned Scribe receives exactly one empty-persisted hot cut per pinned table.
    #[test]
    fn pinned_scribes_cross_product_all_tables_with_explicit_empty_hot_assignments() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "event_id",
            DataType::Utf8,
            false,
        )]));
        let empty_manifest = Vec::new();
        let tables = ["spans", "observations"]
            .into_iter()
            .map(|table| ScribeAssignmentTable {
                table: format!("vala.bifrost.{table}"),
                binding: test_follower_binding(table),
                physical_schema: Arc::clone(&schema),
                sealed_manifest: &empty_manifest,
            })
            .collect::<Vec<_>>();
        let scribes = [(11_u128, 101_u64), (12_u128, 102_u64), (13_u128, 103_u64)]
            .into_iter()
            .map(|(node, fence)| OracleQueryParticipant {
                node_id: NodeId::new(uuid::Uuid::from_u128(node)),
                endpoint: format!("https://scribe-{node}.internal"),
                role: wyrd_spec::vala::api::ClusterRole::Scribe,
                fencing_token: fence,
                capabilities: ClusterCapabilities::ScribeV1(
                    wyrd_spec::vala::api::ScribeCapabilitiesV1 {
                        tail_protocol_version: 1,
                    },
                ),
            })
            .collect::<Vec<_>>();
        let assignments = build_scribe_follower_sources(&tables, &scribes, 64 * 1024)
            .expect("valid pinned Scribes produce bounded assignments");

        assert_eq!(assignments.len(), tables.len() * scribes.len());
        let mut pairs = HashSet::new();
        let mut scan_ids = HashSet::new();
        for source in assignments {
            assert!(pairs.insert((source.table.clone(), source.node_id)));
            assert!(scan_ids.insert(source.assignment.scan_id.clone()));
            assert!(source.assignment.persisted.files.is_empty());
            let participant = scribes
                .iter()
                .find(|participant| participant.node_id == source.node_id)
                .expect("assignment node is an exact pinned Scribe");
            assert_eq!(
                source.assignment.scan_id,
                scribe_follower_scan_id(
                    &format!(
                        "{}.{}",
                        source.assignment.binding.namespace, source.assignment.binding.table
                    ),
                    source.node_id,
                    participant.fencing_token,
                )
            );
            let provider = source
                .assignment
                .scribe_provider_cut
                .expect("every pinned Scribe assignment carries the mandatory hot cut");
            assert_eq!(provider.writer_epoch, participant.fencing_token);
            assert_eq!(
                source.assignment.required_columns,
                vec!["event_id".to_owned()]
            );
        }
        assert_eq!(scan_ids.len(), tables.len() * scribes.len());
        for table in &tables {
            for scribe in &scribes {
                assert!(pairs.contains(&(table.table.clone(), scribe.node_id)));
            }
        }
    }

    /// Persisted work partitions are deterministic, complete, disjoint, and explicitly empty.
    #[test]
    fn oracle_partitions_preserve_exact_disjoint_persisted_union() {
        let originals = vec![
            test_persisted_assignment("spans", &["a", "b", "c"]),
            test_persisted_assignment("observations", &["d"]),
        ];
        let workers = (1..=4)
            .map(|node| dispatcher::DispatchCandidate {
                node_id: NodeId::new(uuid::Uuid::from_u128(node)),
                role: wyrd_spec::vala::api::ClusterRole::Oracle,
                worker_fence: 1,
                endpoint: None,
            })
            .collect::<Vec<_>>();
        let partitions = partition_oracle_assignments(&originals, &workers);
        assert_eq!(
            partitions,
            partition_oracle_assignments(&originals, &workers)
        );
        assert_eq!(partitions.len(), 4);
        for (scan_ordinal, original) in originals.iter().enumerate() {
            let worker_files = partitions
                .iter()
                .map(|partition| partition[scan_ordinal].persisted.files.clone())
                .collect::<Vec<_>>();
            let mut union = worker_files.iter().flatten().cloned().collect::<Vec<_>>();
            union.sort_by(|left, right| left.path().cmp(right.path()));
            assert_eq!(union, original.persisted.files);
            let unique = union
                .iter()
                .map(PersistedFileDescriptor::path)
                .collect::<HashSet<_>>();
            assert_eq!(unique.len(), union.len(), "no file is duplicated");
            for left in 0..worker_files.len() {
                for right in (left + 1)..worker_files.len() {
                    assert!(
                        worker_files[left]
                            .iter()
                            .all(|file| !worker_files[right].contains(file))
                    );
                }
            }
        }
        assert!(partitions[3][0].persisted.files.is_empty());
        assert!(partitions[1][1].persisted.files.is_empty());
    }

    /// Oracle and Scribe dispatches retain byte-identical plans and fingerprints.
    #[test]
    fn common_follower_plan_specializes_only_role_local_assignments() {
        let binding = test_follower_binding("spans");
        let oracle_assignment = test_persisted_assignment("spans", &["a"]);
        let mut scribe_assignment = test_persisted_assignment("spans", &[]);
        scribe_assignment.scribe_provider_cut = Some(ScribeProviderCut {
            writer_epoch: 7,
            start_partition: crate::test_support::day_partition(2026, 8, 20).to_wire(),
            end_partition: crate::test_support::day_partition(2026, 8, 20).to_wire(),
            maximum_batch_count: 1,
            maximum_retained_bytes: 1024,
        });
        let encoded = [9_u8, 8, 7, 6];
        let oracle = common_physical_fragment(
            &encoded,
            vec![oracle_assignment],
            &binding,
            wyrd_spec::vala::api::ClusterRole::Oracle,
            "common-fingerprint",
            42,
        );
        let scribe = common_physical_fragment(
            &encoded,
            vec![scribe_assignment],
            &binding,
            wyrd_spec::vala::api::ClusterRole::Scribe,
            "common-fingerprint",
            42,
        );
        assert_eq!(oracle.physical_plan_bytes, scribe.physical_plan_bytes);
        assert_eq!(oracle.plan_fingerprint, scribe.plan_fingerprint);
        assert_ne!(oracle.target_role, scribe.target_role);
        assert_ne!(oracle.assignments, scribe.assignments);
    }

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

    /// A hot file-list row with no usable checksum yields no descriptor at all.
    ///
    /// Defaulting the digest would mint a valid-looking identity that every
    /// unchecksummed object in the tenant shares, and that identity is the hot
    /// metadata cache key: two distinct objects under one key return one
    /// object's footer for the other's rows. The refusal happens while the
    /// assignment is being built, so nothing is signed, resolved, cached, or
    /// opened on the strength of a guessed identity.
    ///
    /// # Panics
    /// Panics when an unusable checksum, size, or row count produces a
    /// descriptor.
    #[test]
    fn a_hot_row_without_a_usable_checksum_produces_no_signed_descriptor() {
        let row = |checksum: Option<&str>| vala_sql::row_types::file_list::HotFileRow {
            id: uuid::Uuid::now_v7(),
            data_tenant_id: uuid::Uuid::now_v7(),
            namespace: "vala.traces".to_owned(),
            table_name: "spans".to_owned(),
            file_path: "tenant/spans/a.parquet".to_owned(),
            file_ordinal: 0,
            file_checksum: checksum.map(ToOwned::to_owned),
            file_size: 4_096,
            row_count: 1,
            min_event_time: None,
            max_event_time: None,
            partition_granularity: "hour".to_owned(),
            partition_start: chrono::DateTime::from_timestamp_micros(1_787_493_600_000_000)
                .expect("fixture partition start is representable"),
            compacted: false,
            committed_snapshot_id: None,
            forge_publication_operation_id: None,
            node_id: uuid::Uuid::now_v7(),
            writer_epoch: 1,
            wal_lsn_min: 1,
            wal_lsn_max: 1,
            created_at: chrono::DateTime::from_timestamp_micros(1_787_493_600_000_000)
                .expect("fixture creation time is representable"),
        };
        let valid = "a".repeat(64);
        let descriptor = hot_file_descriptor(&row(Some(&valid))).expect("a checksummed row signs");
        assert!(
            descriptor.is_valid(),
            "a decoded 32-byte digest is a usable identity"
        );

        for unusable in [
            None,
            Some(""),
            // Not hex.
            Some("zz"),
            // Sixteen bytes, not thirty-two.
            Some("00112233445566778899aabbccddeeff"),
            // Decodes cleanly to the value a default would have produced.
            Some("0".repeat(64).as_str()),
        ] {
            assert!(
                hot_file_descriptor(&row(unusable)).is_err(),
                "{unusable:?} is not an object identity"
            );
        }

        // A descriptor that reached the wire carrying the defaulted digest is
        // refused by validation as well, so neither side depends on the other.
        let mut defaulted = descriptor;
        if let PersistedFileDescriptor::Hot(hot) = &mut defaulted {
            hot.sha256 = [0_u8; 32];
        }
        assert!(!defaulted.is_valid());
    }

    /// The persisted source of an assignment is its descriptors' variant, and a
    /// list mixing both variants is refused rather than classified.
    ///
    /// A scan id is a leader-chosen label the wire carries; the descriptor
    /// variant is what preflight validates and what the assignment-authority
    /// digest covers. The label is deliberately made to disagree with the
    /// descriptors here so that a classifier reading the label cannot pass.
    ///
    /// # Panics
    /// Panics when classification disagrees with the descriptor variants.
    #[test]
    fn persisted_source_is_classified_from_descriptors_not_the_scan_id() {
        let hot = PersistedFileDescriptor::Hot(wyrd_spec::vala::api::HotFileDescriptor {
            path: "a.parquet".to_owned(),
            size_bytes: 4_096,
            row_count: 1,
            file_list_id: uuid::Uuid::now_v7(),
            sha256: [3_u8; 32],
            min_event_time_micros: None,
            max_event_time_micros: None,
        });
        let iceberg = test_persisted_descriptor("b.parquet");

        assert_eq!(
            AssignedPersistedSource::classify(&[]),
            Ok(AssignedPersistedSource::Empty)
        );
        assert_eq!(
            AssignedPersistedSource::classify(std::slice::from_ref(&hot)),
            Ok(AssignedPersistedSource::Hot)
        );
        assert_eq!(
            AssignedPersistedSource::classify(std::slice::from_ref(&iceberg)),
            Ok(AssignedPersistedSource::Iceberg { snapshot_id: 1 })
        );
        assert_eq!(
            AssignedPersistedSource::classify(&[hot.clone(), iceberg.clone()]),
            Err(AssignedSourceRejection::MixedSources),
            "a list resolved by two different readers has no single correct leaf"
        );

        // One assignment is one pinned cut, so two snapshots in one list have
        // no single snapshot binding that can serve them.
        let mut newer = test_persisted_descriptor("c.parquet");
        if let PersistedFileDescriptor::Iceberg(file) = &mut newer {
            file.snapshot_id = 2;
        }
        assert_eq!(
            AssignedPersistedSource::classify(&[iceberg, newer]),
            Err(AssignedSourceRejection::MixedSnapshots)
        );

        // The scan id says Iceberg; the descriptors say hot. The descriptors win.
        let mut mislabeled = test_persisted_assignment("oracle:spans:iceberg", &[]);
        mislabeled.persisted.files = vec![hot];
        assert_eq!(
            follower_assignment_sources(
                wyrd_spec::vala::api::ClusterRole::Oracle,
                std::slice::from_ref(&mislabeled)
            ),
            vec![QuerySource::HotSealed]
        );
    }

    /// A role-local assignment set emits the exact degraded public source tiers
    /// its assignments actually carry.
    #[test]
    fn follower_assignment_sources_preserve_oracle_and_scribe_tiers() {
        let mut iceberg = test_persisted_assignment("oracle:spans:iceberg", &["a"]);
        let mut hot = test_persisted_assignment("oracle:spans:hot", &[]);
        hot.persisted.files = vec![PersistedFileDescriptor::Hot(
            wyrd_spec::vala::api::HotFileDescriptor {
                path: "b.parquet".to_owned(),
                size_bytes: 4_096,
                row_count: 1,
                file_list_id: uuid::Uuid::now_v7(),
                sha256: [5_u8; 32],
                min_event_time_micros: None,
                max_event_time_micros: None,
            },
        )];
        assert_eq!(
            follower_assignment_sources(
                wyrd_spec::vala::api::ClusterRole::Oracle,
                &[iceberg.clone(), hot]
            ),
            vec![QuerySource::Iceberg, QuerySource::HotSealed]
        );
        iceberg.persisted.files.clear();
        iceberg.scribe_provider_cut = Some(ScribeProviderCut {
            writer_epoch: 7,
            start_partition: crate::test_support::day_partition(2026, 8, 20).to_wire(),
            end_partition: crate::test_support::day_partition(2026, 8, 20).to_wire(),
            maximum_batch_count: 1,
            maximum_retained_bytes: 1024,
        });
        assert_eq!(
            follower_assignment_sources(wyrd_spec::vala::api::ClusterRole::Scribe, &[iceberg]),
            vec![QuerySource::LiveTail]
        );
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

    /// The sole typed pre-output replacement settles its real admission owner first.
    #[test]
    fn stale_replacement_gate_settles_once_and_rejects_second_or_post_output_attempts() {
        let (admitted, shared, _request_cancellation) = admission::admitted_guard_for_test();
        let cancellation = admitted.cancellation.clone();
        assert_eq!(admission::active_queries_for_test(&shared), 1);
        assert!(
            StaleReplacementGate::before_output(0)
                .settle_for_typed_stale(admitted, true)
                .is_ok()
        );
        assert!(cancellation.is_cancelled());
        assert_eq!(admission::active_queries_for_test(&shared), 0);

        let (second, second_shared, _request_cancellation) = admission::admitted_guard_for_test();
        let second =
            match StaleReplacementGate::before_output(1).settle_for_typed_stale(second, true) {
                Ok(()) => panic!("second stale attempt must not replace"),
                Err(admitted) => admitted,
            };
        assert_eq!(admission::active_queries_for_test(&second_shared), 1);
        second.release();
        assert_eq!(admission::active_queries_for_test(&second_shared), 0);

        let (post_output, post_output_shared, _request_cancellation) =
            admission::admitted_guard_for_test();
        let post_output = match (StaleReplacementGate {
            retry_ordinal: 0,
            output_started: true,
        })
        .settle_for_typed_stale(post_output, true)
        {
            Ok(()) => panic!("post-output stale failure must not replace"),
            Err(admitted) => admitted,
        };
        post_output.release();
        assert_eq!(admission::active_queries_for_test(&post_output_shared), 0);

        let (string_only, string_shared, _request_cancellation) =
            admission::admitted_guard_for_test();
        let string_only = match StaleReplacementGate::before_output(0)
            .settle_for_typed_stale(string_only, false)
        {
            Ok(()) => panic!("string-only not-found must not replace"),
            Err(admitted) => admitted,
        };
        string_only.release();
        assert_eq!(admission::active_queries_for_test(&string_shared), 0);
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

    /// Pauses exactly one selected attempt and leaves the replan unblocked.
    #[cfg(feature = "test-support")]
    #[tokio::test]
    async fn topology_probe_pauses_only_the_first_selection() {
        let probe = Arc::new(OracleTopologyProbe::default());
        let paused = Arc::clone(&probe);
        let first = tokio::spawn(async move {
            paused
                .pause_first_selection(NodeId::new(uuid::Uuid::from_u128(2)))
                .await;
        });
        probe.wait_selected().await;
        probe.resume();
        first.await.expect("first selection resumes");
        assert_eq!(
            probe.selected_worker(),
            Some(NodeId::new(uuid::Uuid::from_u128(2)))
        );
        probe
            .pause_first_selection(NodeId::new(uuid::Uuid::from_u128(3)))
            .await;
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

    /// Classification uses the total live Oracle CPU and never returns a zero-CPU class.
    #[test]
    fn classification_uses_live_cpu_formula() {
        assert_eq!(
            OraclePlanner::classify(1, 0.0, false),
            QueryClass::Interactive
        );
        assert_eq!(
            OraclePlanner::classify(11 * 1_073_741_824, 1.0, false),
            QueryClass::Analytical
        );
        assert_eq!(
            OraclePlanner::classify(1, 8.0, true),
            QueryClass::Analytical
        );
    }

    /// Bounded Top-K sorting defers to scan cost while unbounded or nested-heavy work stays analytical.
    #[tokio::test]
    async fn bounded_top_k_uses_scan_cost_without_downgrading_heavy_plans() {
        let session = SessionContext::new();
        let provider = MemTable::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "row_id",
                DataType::Int64,
                false,
            )])),
            vec![Vec::new()],
        )
        .expect("schema-only provider");
        session
            .register_table("events", Arc::new(provider))
            .expect("register bounded-sort table");

        let bounded = session
            .sql("SELECT row_id FROM events ORDER BY row_id LIMIT 64")
            .await
            .expect("bounded Top-K query plans")
            .into_optimized_plan()
            .expect("bounded Top-K query optimizes");
        assert!(!optimized_plan_is_complex(&bounded));
        let small = OraclePlanner::classification(1, 8.0, optimized_plan_is_complex(&bounded));
        assert_eq!(small.query_class, QueryClass::Interactive);
        assert_eq!(small.reason, "estimated_scan");
        let large = OraclePlanner::classification(
            11 * 1_073_741_824,
            1.0,
            optimized_plan_is_complex(&bounded),
        );
        assert_eq!(large.query_class, QueryClass::Analytical);
        assert_eq!(large.reason, "predicted_scan");

        let unbounded = session
            .sql("SELECT row_id FROM events ORDER BY row_id")
            .await
            .expect("unbounded sort query plans")
            .into_optimized_plan()
            .expect("unbounded sort query optimizes");
        assert!(optimized_plan_is_complex(&unbounded));

        let nested_heavy = session
            .sql("SELECT count(*) AS total FROM events ORDER BY total LIMIT 64")
            .await
            .expect("bounded sort over aggregate plans")
            .into_optimized_plan()
            .expect("bounded sort over aggregate optimizes");
        assert!(optimized_plan_is_complex(&nested_heavy));
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
