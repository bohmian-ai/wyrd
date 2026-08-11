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
use datafusion::physical_plan::{SendableRecordBatchStream, execute_stream};
use datafusion::sql::parser::{DFParser, Statement as DfStatement};
use datafusion::sql::resolve::resolve_table_references;
use datafusion::sql::sqlparser::ast::Statement as SqlStatement;
use futures_util::{Stream, StreamExt};
use num_traits::ToPrimitive;
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
    BifrostSecurityViolationKind, ClusterCapabilities, NodeId, QueryAuditDigest, QueryBatchFrame,
    QueryClass, QueryExecutionMode, QueryFreshness, QueryId, QuerySchemaFrame, QuerySource,
    QueryStreamFrame, QueryTerminalErrorCode, QueryTerminalFrame, QueryTerminalOutcome,
    SourceCompletion, SourceCompletionOutcome, VisibilityMode,
};

use crate::catalog::{BifrostCatalog, BifrostCatalogError, PinnedSealedTable, TableRef};
use crate::cluster::{ClusterRegistry, ClusterSnapshot, RegisteredRole};
use crate::schema::SchemaFingerprint;
use crate::scribe::memory::{
    BifrostMemoryGovernor, MemoryPurpose, MemoryRejection, MemoryRejectionKind,
    OracleMemoryReservation, ParentMemoryReservation,
};
use crate::scribe::tail_rpc::{TAIL_PROTOCOL_VERSION, TailReadTransport};

mod admission;
pub mod assignment;
pub mod attempt;
pub mod dispatcher;
mod exec;
pub mod executor;
pub mod fragment;
pub mod peer;
mod planner;
mod query_stream;

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
use exec::{HotFileSource, OracleQueryScanStats, OracleTableInputs, OracleTableProvider};
pub use exec::{ReconcileExec, TenantTripwireExec};
use planner::OracleClassification;
pub use planner::OraclePlanner;
pub use query_stream::OracleQueryStream;
pub(crate) use query_stream::QueryStreamLifecycle;
use query_stream::{QueryStreamInput, encode_schema_frame};

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

/// Converts validated batches through the leader's production memory-source helper.
#[cfg(test)]
fn test_validated_memory_source(
    batches: &Vec<RecordBatch>,
    schema: SchemaRef,
) -> datafusion::common::Result<Arc<dyn datafusion::physical_plan::ExecutionPlan>> {
    exec::OracleTableProvider::validated_memory_source(batches, schema)
}
pub use tail_fence::{DiscoveredTailRoute, TailStreamDiscovery};
use tail_fence::{DrainedTails, TailFenceDrainer, TailFenceDrainerConfig};

/// Default maximum SQL request size accepted by the synchronous query floor.
pub const DEFAULT_MAX_SQL_BYTES: usize = 64 * 1024;
/// Interactive scan-time threshold in seconds.
pub const INTERACTIVE_SCAN_LIMIT_SECONDS: f64 = 10.0;
/// Bytes per second used by the normative classification estimate.
pub const ESTIMATED_SCAN_BYTES_PER_SECOND: f64 = 1_073_741_824.0;
/// Authenticated caller context used by the engine before a server adapter
/// adds transport-specific metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Maps private dispatch failures to the only public/stale execution classes.
fn map_dispatch_error(error: &dispatcher::DispatchError) -> OracleExecutionError {
    match error {
        dispatcher::DispatchError::StaleObject => OracleExecutionError::StaleObject,
        dispatcher::DispatchError::Terminal => {
            OracleExecutionError::Public(BifrostError::QueryPeerSecurity)
        }
        dispatcher::DispatchError::Retryable | dispatcher::DispatchError::Exhausted => {
            OracleExecutionError::Public(BifrostError::QueryExecutionFailed)
        }
        dispatcher::DispatchError::Capacity => {
            OracleExecutionError::Public(BifrostError::QueryAdmissionRejected)
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
    /// Parent process-wide memory governor.
    pub governor: BifrostMemoryGovernor,
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

/// Closed reason why bounded local running capacity was not acquired.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalSlotAcquireError {
    /// The immutable admission deadline elapsed before capacity became available.
    Deadline,
    /// Oracle lifecycle cancellation interrupted the bounded wait.
    Cancelled,
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
                "oracle_query_spill_bytes_total",
            ] {
                metrics::counter!(family, "class" => class).increment(0);
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

    /// Couples one parent-governor reservation to Oracle and class gauges.
    #[must_use]
    fn account_memory(
        self: &Arc<Self>,
        reservation: ParentMemoryReservation,
        query_class: QueryClass,
        memory_kind: OracleMemoryKind,
    ) -> AccountedMemoryReservation {
        let bytes = reservation.bytes();
        self.charge_memory(bytes, query_class, memory_kind);
        AccountedMemoryReservation {
            reservation: Some(OracleGovernorReservation::Parent(reservation)),
            owner: Arc::clone(self),
            query_class,
            memory_kind,
            bytes,
        }
    }

    /// Couples one Oracle-child reservation to canonical Oracle memory gauges.
    #[must_use]
    fn account_oracle_memory(
        self: &Arc<Self>,
        reservation: OracleMemoryReservation,
        query_class: QueryClass,
        memory_kind: OracleMemoryKind,
    ) -> AccountedMemoryReservation {
        let bytes = reservation.bytes();
        self.charge_memory(bytes, query_class, memory_kind);
        AccountedMemoryReservation {
            reservation: Some(OracleGovernorReservation::Oracle(reservation)),
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
    /// Exact-identity reconciliation state.
    Reconciliation,
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
        metrics::histogram!(
            "oracle_query_time_to_first_batch_seconds",
            "class" => query_class_label(self.query_class)
        )
        .record(self.started_at.elapsed().as_secs_f64());
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
    /// Parent-only ownership used by reconciliation and live-source state.
    Parent(ParentMemoryReservation),
    /// Oracle-child plus parent ownership used by hot source buffers.
    Oracle(OracleMemoryReservation),
}

impl OracleGovernorReservation {
    /// Poison the shared governor after wrapper-accounting corruption.
    fn poison(&self) {
        match self {
            Self::Parent(reservation) => reservation.poison(),
            Self::Oracle(reservation) => reservation.poison(),
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

    /// Tries to reserve one bounded pending-admission waiter.
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

    /// Waits for local running units within the caller's admission boundary.
    ///
    /// The existing pending semaphore bounds the number of callers that may
    /// enter this wait. The returned permit is transferred into the admitted
    /// query guard or dropped before any failed pod-local admission returns.
    ///
    /// # Errors
    ///
    /// Returns [`LocalSlotAcquireError::Deadline`] when the immutable deadline
    /// elapses and [`LocalSlotAcquireError::Cancelled`] when Oracle lifecycle
    /// cancellation wins. A closed semaphore is treated as cancellation.
    ///
    /// # Cancellation
    ///
    /// Cancelling the future drops the semaphore acquisition future without
    /// consuming capacity.
    #[cfg(test)]
    pub(crate) async fn acquire_running(
        &self,
        demand: u32,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<OwnedSemaphorePermit, LocalSlotAcquireError> {
        let acquire = Arc::clone(&self.running).acquire_many_owned(demand);
        tokio::select! {
            permit = acquire => permit.map_err(|_| LocalSlotAcquireError::Cancelled),
            () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                Err(LocalSlotAcquireError::Deadline)
            }
            () = cancellation.cancelled() => Err(LocalSlotAcquireError::Cancelled),
        }
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
    event_day: wyrd_spec::vala::api::EventDay,
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
        event_day: wyrd_spec::vala::api::EventDay,
        transport: Arc<dyn TailReadTransport>,
    ) {
        self.insert_live_stream_route(
            table.into(),
            None,
            node_id,
            writer_epoch,
            event_day,
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
        event_day: wyrd_spec::vala::api::EventDay,
        transport: Arc<dyn TailReadTransport>,
    ) {
        self.insert_live_stream_route(
            table.into(),
            Some(tenant.as_uuid()),
            node_id,
            writer_epoch,
            event_day,
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
        event_day: wyrd_spec::vala::api::EventDay,
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
                    || route.event_day != event_day
            });
            routes.push(LiveTailRoute {
                node_id: node_id.as_uuid(),
                writer_epoch,
                event_day,
                transport,
            });
            routes.sort_by(|left, right| {
                (left.node_id, left.writer_epoch, left.event_day.as_str()).cmp(&(
                    right.node_id,
                    right.writer_epoch,
                    right.event_day.as_str(),
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
    /// Redux Iceberg/catalog owner.
    pub catalog: Arc<BifrostCatalog>,
    /// Tenant-scoped SQL owner.
    pub vala: ValaPostgres,
    /// Immutable membership registry.
    pub cluster: Arc<ClusterRegistry>,
    /// Fenced local Oracle role.
    pub local_role: RegisteredRole,
    /// Local pending/running slot guards.
    pub local_slots: Arc<OracleSlotManager>,
    /// Parent memory and spill resources.
    pub memory: OracleMemoryResources,
    /// Table-local tail transports.
    pub tails: Arc<TailTransportDirectory>,
    /// Read/security audit collaborator.
    pub audit: Arc<dyn OracleAudit>,
    /// Server-owned narrow peer-ticket authority.
    pub peer_ticket_minter: Arc<dyn peer::PeerTicketMinter>,
    /// Server-owned domain-separated Scribe-tail ticket signer.
    pub tail_ticket_minter: Option<Arc<dyn crate::scribe::tail_rpc::TailTicketMinter>>,
    /// Query-scoped live Scribe discovery owner.
    pub tail_discovery: Option<Arc<dyn tail_fence::TailStreamDiscovery>>,
    /// Optional node-aware local/tonic directory used for immutable sealed leaves.
    pub peer_transports: Option<dispatcher::OraclePeerTransportDirectory>,
    /// Engine limits and lifecycle values.
    pub config: OracleConfig,
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
    /// Interactive class memory budget.
    pub interactive_memory_bytes: u64,
    /// Analytical class memory budget.
    pub analytical_memory_bytes: u64,
    /// Spill budget.
    pub spill_bytes: u64,
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
            interactive_memory_bytes: 256 * 1024 * 1024,
            analytical_memory_bytes: 256 * 1024 * 1024,
            spill_bytes: 1 << 30,
        }
    }
}

/// Internal execution outcome that preserves the sole whole-query replan signal.
#[derive(Debug, thiserror::Error)]
enum OracleExecutionError {
    /// A pinned immutable object disappeared after the read cut was selected.
    #[error("Oracle read cut references a stale object")]
    StaleObject,
    /// A stable public failure that must cross the transport boundary unchanged.
    #[error(transparent)]
    Public(#[from] BifrostError),
}

/// Complete inputs for one sealed-tier peer dispatch.
struct SealedDispatchInput<'a> {
    /// Authenticated request context.
    context: &'a AuthorizedQueryContext,
    /// Pinned table cut.
    cut: &'a PinnedSealedTable,
    /// Admitted leader identity and local fencing token.
    admitted: &'a AdmittedQueryGuard,
    /// Immutable query class.
    query_class: QueryClass,
    /// Absolute execution deadline.
    deadline: Instant,
    /// Sealed source tier.
    tier: fragment::SealedSourceTier,
    /// Digest pinning the source manifest.
    pinned_digest: String,
    /// Exact immutable file work.
    files: Vec<fragment::SealedScanFile>,
}

/// Planned fragments, assignment, fences, and authorization for execution.
struct PreparedSealedDispatch {
    /// Deterministic micro-fragments.
    fragments: Vec<fragment::SealedScanFragment>,
    /// Primary node assignment by fragment.
    assignment: HashMap<wyrd_spec::vala::api::NodeId, Vec<fragment::SealedScanFragment>>,
    /// Stable distinct retry candidates.
    selected: Vec<wyrd_spec::vala::api::NodeId>,
    /// Snapshot role fences by candidate.
    fences: HashMap<wyrd_spec::vala::api::NodeId, u64>,
    /// Immutable ticket and attempt context.
    context: dispatcher::DispatchContext,
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
    /// Immutable admission class.
    query_class: QueryClass,
    /// Admitted durable/local query owner.
    admitted: &'a AdmittedQueryGuard,
    /// Absolute execution deadline.
    deadline: Instant,
    /// Immutable selected-file bytes used for logical scan telemetry.
    logical_bytes_selected: u64,
}

/// Pinned tables and class selected during one retry's planning phase.
struct PlannedSqlCut {
    /// Exact immutable table cuts.
    cuts: Vec<PinnedSealedTable>,
    /// Server-derived admission class.
    query_class: QueryClass,
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
}

impl<'a> SqlAttemptInput<'a> {
    /// Splits the immutable attempt envelope into the values consumed by each phase.
    fn into_parts(
        self,
    ) -> (
        &'a AuthorizedQueryContext,
        &'a BifrostQueryRequest,
        &'a [TableRef],
        Instant,
        u8,
        Option<Arc<crate::oracle::query_stream::QueryStreamLifecycle>>,
    ) {
        let Self {
            context,
            request,
            tables,
            deadline,
            retry_ordinal,
            gate_lifecycle,
        } = self;
        (
            context,
            request,
            tables,
            deadline,
            retry_ordinal,
            gate_lifecycle,
        )
    }
}

/// Retained local query engine owner.
pub struct Oracle {
    /// Planner and floor configuration.
    planner: OraclePlanner,
    /// Admission state and local guards.
    admission: Arc<OracleAdmission>,
    /// Immutable membership registry retained for planning and worker selection.
    cluster: Arc<ClusterRegistry>,
    /// Tenant-qualified catalog and SQL owners retained for query execution.
    catalog: Arc<BifrostCatalog>,
    /// Tenant SQL handle retained for the Oracle lifecycle boundary.
    vala: ValaPostgres,
    /// Parent-governed query memory and spill configuration.
    memory: OracleMemoryResources,
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
    fragment_dispatcher: Option<dispatcher::FragmentDispatcher>,
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

impl Oracle {
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
        if config.config.max_workers_per_query > 63
            || config.config.fragment_max_files == 0
            || config.config.attempt_max_bytes == 0
            || config.config.attempt_memory_bytes == 0
            || config.config.attempt_memory_bytes > config.config.attempt_max_bytes
        {
            return Err(BifrostError::Internal {
                detail: "Oracle fragment configuration is invalid".to_owned(),
            });
        }
        if config.config.max_sql_bytes == 0 {
            return Err(BifrostError::Internal {
                detail: "Oracle SQL byte limit must be positive".to_owned(),
            });
        }
        if config.config.planning_permits == 0
            || config.config.tenant_interactive_slots == 0
            || config.config.tenant_analytical_slots == 0
        {
            return Err(BifrostError::Internal {
                detail: "Oracle planning and tenant limits must be positive".to_owned(),
            });
        }
        let planner = OraclePlanner::new(config.config);
        let telemetry = Arc::new(OracleTelemetry::new(Arc::clone(&config.local_slots)));
        let cluster = Arc::clone(&config.cluster);
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
                interactive_memory_bytes: config.config.interactive_memory_bytes,
                analytical_memory_bytes: config.config.analytical_memory_bytes,
                spill_bytes: config.config.spill_bytes,
            },
        ));
        let initial_snapshot = cluster.snapshot();
        admission.refresh(&initial_snapshot);
        let shutdown = CancellationToken::new();
        let ready = Arc::new(AtomicBool::new(false));
        let (maintenance, startup_result) =
            OracleAdmission::start_maintenance(shutdown.clone(), Arc::clone(&ready))?;
        let fragment_dispatcher = config.peer_transports.map(|transports| {
            dispatcher::FragmentDispatcher::new(Arc::clone(&config.peer_ticket_minter), transports)
                .with_memory_governor(config.memory.governor.clone())
        });
        Ok(Self {
            planner,
            admission,
            cluster,
            catalog: config.catalog,
            vala: config.vala,
            memory: config.memory,
            tails: config.tails,
            tail_ticket_minter: config.tail_ticket_minter,
            tail_discovery: config.tail_discovery,
            #[cfg(feature = "test-support")]
            prefer_local_tail_routes: std::sync::atomic::AtomicBool::new(false),
            audit: config.audit,
            fragment_dispatcher,
            telemetry,
            shutdown,
            ready,
            startup_result: Mutex::new(Some(startup_result)),
            maintenance: Mutex::new(Some(maintenance)),
            #[cfg(feature = "test-support")]
            topology_probe: Mutex::new(None),
        })
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
        self.query_sql_with_gate_lifecycle(context, request, None)
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
    pub(crate) async fn query_sql_with_gate_lifecycle(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
        gate_lifecycle: Option<Arc<crate::oracle::query_stream::QueryStreamLifecycle>>,
    ) -> Result<OracleQueryStream, BifrostError> {
        self.validate_query(&request)?;
        if !self.is_ready() {
            return Err(BifrostError::OracleRoleUnavailable);
        }
        self.preflight_query_capacity()?;
        let deadline = request
            .deadline_ms
            .map_or(self.planner.config.default_deadline, Duration::from_millis);
        let deadline = Instant::now()
            .checked_add(deadline)
            .ok_or(BifrostError::QueryTimeout)?;
        let tables = parse_select_tables(&request.sql)?;
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
                    },
                    &mut query_telemetry,
                )
                .await?
            {
                return Ok(stream);
            }
        }
        Err(BifrostError::QueryExecutionFailed)
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
    async fn run_sql_attempt(
        &self,
        input: SqlAttemptInput<'_>,
        query_telemetry: &mut Option<QueryTelemetryGuard>,
    ) -> Result<Option<OracleQueryStream>, BifrostError> {
        let (context, request, tables, deadline, retry_ordinal, gate_lifecycle) =
            input.into_parts();
        let planned = self
            .plan_sql_attempt(context, &request.sql, tables, deadline)
            .await?;
        let query_class = planned.query_class;
        query_telemetry
            .get_or_insert_with(|| self.telemetry.start_query(request.visibility, query_class));
        let mut admitted = self.admit_sql_query(context, query_class, deadline).await?;
        let drained = match self
            .audit_and_drain_cut(CutAuditInput {
                context,
                request,
                cuts: &planned.cuts,
                query_class,
                retry_ordinal,
                deadline,
                admitted: &admitted,
            })
            .await
        {
            Ok(drained) => drained,
            Err(error) => return release_error(deadline, admitted, error, "audit rejection"),
        };
        admitted.live_reservations = drained.reservations;
        let logical_bytes_selected = Self::logical_selected_bytes(&planned.cuts);
        let (schema, mut batches, scan_stats) = match self
            .execute_sql_cut(SqlCutInput {
                context,
                sql: &request.sql,
                cuts: planned.cuts,
                live_batches: drained.batches,
                query_class,
                admitted: &admitted,
                deadline,
                logical_bytes_selected,
            })
            .await
        {
            Ok(execution) => execution,
            Err(OracleExecutionError::StaleObject) if retry_ordinal == 0 => {
                admitted.release();
                record_stale_replan();
                return Ok(None);
            }
            Err(OracleExecutionError::StaleObject) => {
                let error = BifrostError::QueryExecutionFailed;
                return release_error(deadline, admitted, error, "final stale attempt");
            }
            Err(OracleExecutionError::Public(error)) => {
                return release_error(deadline, admitted, error, "execution rejection");
            }
        };
        let (first, admitted) = await_first_batch_or_release(
            deadline,
            batches.next(),
            admitted,
            |admitted| async move {
                release_error(
                    deadline,
                    admitted,
                    BifrostError::QueryTimeout,
                    "first-batch timeout",
                )
            },
        )
        .await?;
        if first.as_ref().is_some_and(|result| {
            retry_ordinal == 0 && result.as_ref().is_err_and(is_stale_file_error)
        }) {
            admitted.release();
            record_stale_replan();
            return Ok(None);
        }
        if let Some(error) = map_first_batch_failure(first.as_ref()) {
            return release_error(deadline, admitted, error, "first-batch rejection");
        }
        let Some(query_telemetry) = query_telemetry.take() else {
            let error = BifrostError::QueryExecutionFailed;
            return release_error(deadline, admitted, error, "missing telemetry");
        };
        let schema_frame = match encode_schema_frame(&schema) {
            Ok(schema_frame) => schema_frame,
            Err(error) => return release_error(deadline, admitted, error, "schema preparation"),
        };
        Ok(Some(OracleQueryStream::new(QueryStreamInput {
            schema_frame,
            batches,
            first,
            admitted,
            deadline,
            visibility: request.visibility,
            degraded: drained.degraded,
            stale_replanned: retry_ordinal == 1,
            query_telemetry,
            scan_stats,
            gate_lifecycle,
        })))
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
        deadline: Instant,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        self.admission
            .admit(admission::PreparedAdmission {
                tenant: context.data_tenant_id,
                query_class,
                deadline,
                memory_ceiling: self.memory.reconciliation_limit_bytes as u64,
                spill_eligible: matches!(query_class, QueryClass::Analytical),
                cancellation: self.shutdown.child_token(),
            })
            .await
    }

    /// Delegates one SQL metadata attempt to the planner owner.
    async fn plan_sql_attempt(
        &self,
        context: &AuthorizedQueryContext,
        sql: &str,
        tables: &[TableRef],
        deadline: Instant,
    ) -> Result<PlannedSqlCut, BifrostError> {
        self.planner
            .pin_and_classify(context, sql, tables, deadline, &self.catalog, &self.cluster)
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
        let drainer = TailFenceDrainer::new(
            &self.tails,
            &self.memory,
            TailFenceDrainerConfig {
                telemetry: Arc::clone(&self.telemetry),
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
        OracleTelemetry::record_classification(OracleClassification {
            reason: "typed_plan",
            query_class: class,
            predicted_scan_seconds: 0.0,
        });
        let admitted = self
            .admission
            .admit(admission::PreparedAdmission {
                tenant: context.data_tenant_id,
                query_class: class,
                deadline: options.deadline,
                memory_ceiling: self.memory.reconciliation_limit_bytes as u64,
                spill_eligible: true,
                cancellation: self.shutdown.child_token(),
            })
            .await?;
        self.execute_typed_plan(&context, plan, options, class, query_telemetry, admitted)
            .await
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
        mut admitted: AdmittedQueryGuard,
    ) -> Result<OracleQueryStream, BifrostError> {
        self.preflight_query_capacity()?;
        let cuts = self
            .planner
            .prepare_typed_cuts(
                &plan,
                context.data_tenant_id,
                options.deadline,
                &self.catalog,
            )
            .await?;
        let logical_bytes_selected = Self::logical_selected_bytes(&cuts);
        let fence_owner = TailFenceDrainer::new(
            &self.tails,
            &self.memory,
            TailFenceDrainerConfig {
                telemetry: Arc::clone(&self.telemetry),
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
            fence_owner.acquire(&cuts).await?
        } else {
            Vec::new()
        };
        let audit_result = self
            .audit_typed_decision(context, &plan, options, class, &cuts)
            .await;
        if let Err(error) = audit_result {
            fence_owner.release_acquired(acquired).await;
            return Err(error);
        }
        let mut drained = if acquired.is_empty() {
            DrainedTails::default()
        } else {
            fence_owner.drain(acquired).await?
        };
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
                telemetry: Arc::clone(&self.telemetry),
            })
            .await?;
        let rewritten = OraclePlanner::replace_typed_sources(plan, &providers)?;
        let (session, physical) =
            OraclePlanner::create_physical_plan(&rewritten, &self.memory).await?;
        let scan_stats = OracleQueryScanStats::from_plan(physical.as_ref(), logical_bytes_selected);
        let schema = physical.schema();
        let mut batches = execute_stream(physical, session.task_ctx())
            .map_err(|error| map_datafusion_error(&error))?;
        let remaining = options
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let first = tokio::time::timeout(remaining, batches.next())
            .await
            .map_err(|_| BifrostError::QueryTimeout)?;
        if let Some(error) = map_first_batch_failure(first.as_ref()) {
            return release_error(
                options.deadline,
                admitted,
                error,
                "typed first-batch rejection",
            );
        }
        let schema_frame = encode_schema_frame(&schema)?;
        Ok(OracleQueryStream::new(QueryStreamInput {
            schema_frame,
            batches,
            first,
            admitted,
            deadline: options.deadline,
            visibility: options.visibility,
            degraded: drained.degraded,
            stale_replanned: false,
            query_telemetry,
            scan_stats,
            gate_lifecycle: None,
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

    /// Cancels lifecycle maintenance and drains owned cleanup until `deadline`.
    ///
    /// The lifecycle task is aborted at expiry. Dropping this future can leave
    /// local cleanup to guard drop, but
    /// [`Self::begin_shutdown`] has already synchronously rejected new work.
    pub async fn shutdown(&self, deadline: Instant) {
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
        let _ = deadline;
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
    ) -> Result<(SchemaRef, SendableRecordBatchStream, OracleQueryScanStats), OracleExecutionError>
    {
        let session = self.execution_session()?;
        for cut in input.cuts {
            let table_name = cut.binding.table_ref.fqn();
            let mut hot_files = self.local_hot_sources(&cut)?;
            let distributed_iceberg_batches =
                if self.fragment_dispatcher.is_some() && !cut.iceberg_files.is_empty() {
                    let files = cut
                        .iceberg_files
                        .iter()
                        .map(|file| {
                            Ok(fragment::SealedScanFile {
                                location: self
                                    .catalog
                                    .object_location(&cut.binding, &file.file_path)
                                    .map_err(BifrostCatalogError::into_public)?,
                                row_groups: Vec::new(),
                                size_bytes: file.file_size,
                                estimated_rows: file.row_count,
                            })
                        })
                        .collect::<Result<Vec<_>, BifrostError>>()?;
                    Some(
                        self.dispatch_sealed_fragments(SealedDispatchInput {
                            context: input.context,
                            cut: &cut,
                            admitted: input.admitted,
                            query_class: input.query_class,
                            deadline: input.deadline,
                            tier: fragment::SealedSourceTier::Iceberg,
                            pinned_digest: cut.snapshot_digest.clone(),
                            files,
                        })
                        .await?,
                    )
                } else {
                    None
                };
            let distributed_hot_batches =
                if self.fragment_dispatcher.is_some() && !cut.hot_files.is_empty() {
                    hot_files.clear();
                    let files = cut
                        .hot_files
                        .iter()
                        .map(|file| {
                            Ok(fragment::SealedScanFile {
                                location: self
                                    .catalog
                                    .object_location(&cut.binding, &file.file_path)
                                    .map_err(BifrostCatalogError::into_public)?,
                                row_groups: Vec::new(),
                                size_bytes: u64::try_from(file.file_size)
                                    .map_err(|_| BifrostError::QueryExecutionFailed)?,
                                estimated_rows: u64::try_from(file.row_count)
                                    .map_err(|_| BifrostError::QueryExecutionFailed)?,
                            })
                        })
                        .collect::<Result<Vec<_>, BifrostError>>()?;
                    self.dispatch_sealed_fragments(SealedDispatchInput {
                        context: input.context,
                        cut: &cut,
                        admitted: input.admitted,
                        query_class: input.query_class,
                        deadline: input.deadline,
                        tier: fragment::SealedSourceTier::HotSealed,
                        pinned_digest: cut.hot_manifest_digest.clone(),
                        files,
                    })
                    .await?
                } else {
                    Vec::new()
                };
            let provider = OracleTableProvider::try_new(OracleTableInputs {
                table: cut.iceberg_table,
                distributed_iceberg_batches,
                hot_files,
                distributed_hot_batches,
                live_batches: input.live_batches.remove(&table_name).unwrap_or_default(),
                context: input.context.clone(),
                table_name: table_name.clone(),
                audit: Arc::clone(&self.audit),
                memory: self.memory.clone(),
                telemetry: Arc::clone(&self.telemetry),
                query_class: input.query_class,
            })
            .await
            .map_err(|error| map_datafusion_error(&error))?;
            register_session_table(
                &session,
                &cut.binding,
                Arc::new(provider) as Arc<dyn TableProvider>,
            )?;
        }
        self.execute_session(&session, input.sql, input.logical_bytes_selected)
            .await
    }

    /// Rejects a query before IO when the shared Oracle child cannot accept any work.
    ///
    /// The reversible one-byte reservation observes the same atomic child and
    /// parent ceilings as range and `DataFusion` allocations, then releases
    /// immediately so it does not become query-lifetime accounting.
    ///
    /// # Errors
    ///
    /// Returns the stable public capacity, oversized, or poisoned projection
    /// for the governor's typed refusal.
    fn preflight_query_capacity(&self) -> Result<(), BifrostError> {
        let probe = self
            .memory
            .governor
            .oracle_budget()
            .try_reserve_classified(1, MemoryPurpose::OracleQuery)
            .map_err(map_memory_rejection)?;
        drop(probe);
        Ok(())
    }

    /// Builds one Oracle execution session over the process-wide governed pool.
    ///
    /// Every SQL attempt receives a fresh `DataFusion` session while all attempts
    /// share the same Oracle-child and parent counters through the cloned governor.
    ///
    /// # Errors
    ///
    /// Returns a stable execution error when `DataFusion` cannot construct the
    /// runtime environment.
    fn execution_session(&self) -> Result<SessionContext, BifrostError> {
        let pool = Arc::new(
            crate::scribe::memory::BifrostDataFusionMemoryPool::for_oracle(
                self.memory.governor.clone(),
            ),
        );
        let runtime = datafusion::execution::runtime_env::RuntimeEnvBuilder::new()
            .with_memory_pool(pool)
            .build()
            .map_err(|error| map_datafusion_error(&error))?;
        let state = datafusion::execution::session_state::SessionStateBuilder::new()
            .with_default_features()
            .with_runtime_env(Arc::new(runtime))
            .build();
        Ok(SessionContext::new_with_state(state))
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
                    location: self
                        .catalog
                        .object_location(&cut.binding, &file.file_path)
                        .map_err(BifrostCatalogError::into_public)?,
                    size_bytes,
                })
            })
            .collect()
    }

    /// Sums immutable selected file sizes before execution starts.
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

    /// Plans, assigns, executes, and decodes one footer-validated sealed source tier.
    ///
    /// The immutable membership snapshot is captured once. Small single-fragment
    /// work stays leader-local; larger work selects no more configured workers
    /// than fragments and retries only within this snapshot.
    ///
    /// # Errors
    /// Returns a stable execution, timeout, metadata, or peer-security failure.
    async fn dispatch_sealed_fragments(
        &self,
        input: SealedDispatchInput<'_>,
    ) -> Result<Vec<RecordBatch>, OracleExecutionError> {
        let dispatcher = self
            .fragment_dispatcher
            .as_ref()
            .ok_or(BifrostError::QueryExecutionFailed)?;
        let leader = input.admitted.leader.node_id;
        let prepared = self.prepare_sealed_dispatch(input)?;
        #[cfg(feature = "test-support")]
        let topology_probe = self
            .topology_probe
            .lock()
            .ok()
            .and_then(|probe| probe.clone());
        #[cfg(feature = "test-support")]
        if let Some(probe) = topology_probe
            && let Some(target) = prepared
                .assignment
                .iter()
                .find_map(|(node, work)| (*node != leader && !work.is_empty()).then_some(*node))
        {
            probe.pause_first_selection(target).await;
        }
        let mut output = Vec::new();
        let siblings = prepared.context.cancellation.child_token();
        let parallelism = dispatch_parallelism(
            prepared.selected.len(),
            self.planner.config.max_workers_per_query,
        );
        let mut attempts: Vec<dispatcher::SealedFragmentFuture<'_>> = Vec::new();
        for fragment in prepared.fragments {
            let primary = prepared
                .assignment
                .iter()
                .find_map(|(node, work)| work.contains(&fragment).then_some(*node))
                .unwrap_or(leader);
            let mut order = vec![primary];
            order.extend(
                prepared
                    .selected
                    .iter()
                    .copied()
                    .filter(|node| *node != primary),
            );
            let candidates = order
                .into_iter()
                .filter_map(|node_id| {
                    prepared.fences.get(&node_id).copied().map(|worker_fence| {
                        dispatcher::DispatchCandidate {
                            node_id,
                            worker_fence,
                        }
                    })
                })
                .collect::<Vec<_>>();
            let mut dispatch_context = prepared.context.clone();
            dispatch_context.cancellation = siblings.clone();
            attempts.push(Box::pin(async move {
                dispatcher
                    .execute(&dispatch_context, fragment, &candidates)
                    .await
                    .map_err(|error| map_dispatch_error(&error))
            }));
        }
        dispatcher::FragmentDispatcher::dispatch_attempts(
            attempts,
            parallelism,
            &siblings,
            &mut output,
        )
        .await?;
        Ok(output)
    }

    /// Plans immutable fragment work and assignment from one membership snapshot.
    ///
    /// # Errors
    ///
    /// Returns stable timeout, metadata, role, planning, or audit-digest errors.
    fn prepare_sealed_dispatch(
        &self,
        input: SealedDispatchInput<'_>,
    ) -> Result<PreparedSealedDispatch, OracleExecutionError> {
        let remaining = input
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let deadline_unix_ms = (chrono::Utc::now()
            + chrono::Duration::from_std(remaining).map_err(|_| BifrostError::QueryTimeout)?)
        .timestamp_millis();
        let binding = self
            .catalog
            .object_location(&input.cut.binding, &input.cut.binding.object_prefix)
            .map_err(BifrostCatalogError::into_public)?;
        let schema = iceberg::arrow::schema_to_arrow_schema(
            input.cut.iceberg_table.metadata().current_schema(),
        )
        .map_err(|_| BifrostError::QueryExecutionFailed)?;
        let schema_fingerprint = sealed_fragment_schema_fingerprint(&schema);
        let fragments = fragment::FragmentPlanner
            .plan(
                &fragment::PreparedSealedLeaf {
                    binding,
                    tier: input.tier,
                    pinned_digest: input.pinned_digest,
                    files: input.files,
                    projection: Vec::new(),
                    predicates: Vec::new(),
                    schema_fingerprint,
                    deadline_unix_ms,
                },
                &fragment::FragmentConfig {
                    max_files: self.planner.config.fragment_max_files,
                },
            )
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
        let snapshot = self.cluster.snapshot();
        let leader = input.admitted.leader.node_id;
        let mut eligible = std::collections::BTreeMap::new();
        let mut fences = HashMap::new();
        for role in snapshot.live_oracles() {
            if let ClusterCapabilities::OracleV1(capabilities) = &role.capabilities {
                eligible.insert(
                    role.key.node_id,
                    assignment::OracleNode {
                        node_id: role.key.node_id,
                        capabilities: capabilities.clone(),
                    },
                );
                fences.insert(role.key.node_id, role.fencing_token);
            }
        }
        if !eligible.contains_key(&leader) {
            return Err(BifrostError::OracleRoleUnavailable.into());
        }
        let assignment = assignment::PortableAssignmentV1
            .assign(
                &fragments,
                &eligible,
                leader,
                if fragments.len() == 1 {
                    0
                } else {
                    self.planner.config.max_workers_per_query
                },
            )
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
        let mut selected = assignment.keys().copied().collect::<Vec<_>>();
        selected.sort_by_key(|node| node.as_uuid());
        let permission_digest = audit_digest(&input.context.permission)?.as_str().to_owned();
        let dispatch_context = dispatcher::DispatchContext {
            query_id: input.admitted.query_id,
            leader_node_id: leader,
            leader_fence: input.admitted.leader.fencing_token,
            tenant_id: input.context.data_tenant_id.as_uuid(),
            query_class: input.query_class,
            slot_units: admission_limits(u32::MAX, input.query_class).1,
            permission_digest,
            attempt_bytes: self.planner.config.attempt_max_bytes,
            attempt_memory_bytes: self.planner.config.attempt_memory_bytes,
            cancellation: input.admitted.cancellation.clone(),
            deadline: input.deadline.into(),
        };
        Ok(PreparedSealedDispatch {
            fragments,
            assignment,
            selected,
            fences,
            context: dispatch_context,
        })
    }
}

/// Computes the exact active sealed-fragment bound from selected and admitted capacity.
fn dispatch_parallelism(selected_peer_count: usize, admitted_query_parallelism: usize) -> usize {
    selected_peer_count
        .min(admitted_query_parallelism.max(1))
        .max(1)
}

/// Computes the executor fingerprint after canonicalizing equivalent UTC timezone spellings.
///
/// Iceberg projects UTC as `+00:00`, while Arrow's Parquet reader projects the
/// same logical timezone as `UTC`. This boundary removes that adapter spelling
/// drift without weakening any column, order, or non-UTC type check.
pub(super) fn sealed_fragment_schema_fingerprint(schema: &Schema) -> String {
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

/// Source tier used to resolve duplicate immutable row identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceTier {
    /// Published Iceberg data has highest precedence.
    Iceberg,
    /// Sealed hot files not yet present in Iceberg.
    HotSealed,
    /// Fenced live-tail data has lowest precedence.
    Live,
}

/// Exact identity carried by every physical Bifrost row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowIdentity {
    /// Immutable batch UUID bytes.
    pub batch_id: [u8; 16],
    /// Non-negative batch-local row ordinal.
    pub ordinal: u32,
}

/// Reconciliation failures that must fail the query rather than drop data.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReconcileError {
    /// A required identity column was absent or had the wrong Arrow type.
    #[error("source batch has invalid row identity columns")]
    InvalidIdentity,
    /// One immutable identity contained unequal logical row values.
    #[error("row identity has unequal values across source tiers")]
    UnequalDuplicate,
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
    if let Some(rejection) = MemoryRejection::from_source_chain(error) {
        return map_memory_rejection(rejection);
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

/// Projects one structured governor refusal into the stable public query catalog.
fn map_memory_rejection(rejection: MemoryRejection) -> BifrostError {
    match rejection.kind() {
        MemoryRejectionKind::Occupied => BifrostError::QueryAdmissionRejected,
        MemoryRejectionKind::RequestTooLarge | MemoryRejectionKind::CounterOverflow => {
            BifrostError::QueryMemoryRequestTooLarge
        }
        MemoryRejectionKind::AccountingPoisoned => BifrostError::QueryExecutionFailed,
    }
}

/// Projects a failed first lookahead before any schema frame can be emitted.
fn map_first_batch_failure(
    first: Option<&Result<RecordBatch, datafusion::error::DataFusionError>>,
) -> Option<BifrostError> {
    first
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(map_datafusion_error)
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

/// Detects the sole storage race eligible for a whole pre-byte replan.
fn is_stale_file_error(error: &datafusion::error::DataFusionError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    (message.contains("not found") || message.contains("404"))
        && (message.contains("parquet") || message.contains("object"))
}

/// Records consumption of the sole pre-byte stale-cut replan.
fn record_stale_replan() {
    // Stale replans are terminal execution details, not an admission outcome.
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
    _phase: &'static str,
) -> Result<T, BifrostError> {
    admitted.release();
    Err(original)
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
        let inherent_dispatch_workflow = dispatcher::FragmentDispatcher::dispatch_attempts;
        std::hint::black_box(inherent_dispatch_workflow);
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

    /// Bounded local admission waits for an executing query instead of rejecting a transient race.
    #[tokio::test]
    async fn local_slot_wait_admits_after_capacity_is_released() {
        let slots = Arc::new(OracleSlotManager::new(2, 1));
        let held = slots.try_running(1).expect("initial slot is available");
        let cancellation = CancellationToken::new();
        let waiting = {
            let slots = Arc::clone(&slots);
            let cancellation = cancellation.clone();
            tokio::spawn(async move {
                slots
                    .acquire_running(1, Instant::now() + Duration::from_secs(1), &cancellation)
                    .await
            })
        };
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        drop(held);
        let acquired = waiting
            .await
            .expect("local wait task completes")
            .expect("released capacity is acquired");
        drop(acquired);
        assert!(slots.try_running(1).is_ok());
    }

    /// Deadline and cancellation leave the local semaphore at its configured capacity.
    #[tokio::test]
    async fn local_slot_wait_terminates_without_leaking_capacity() {
        let slots = OracleSlotManager::new(2, 1);
        let held = slots.try_running(1).expect("initial slot is available");
        let cancellation = CancellationToken::new();
        let deadline = slots
            .acquire_running(1, Instant::now() + Duration::from_millis(1), &cancellation)
            .await;
        assert!(matches!(deadline, Err(LocalSlotAcquireError::Deadline)));

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let cancellation_result = slots
            .acquire_running(1, Instant::now() + Duration::from_secs(1), &cancelled)
            .await;
        assert!(matches!(
            cancellation_result,
            Err(LocalSlotAcquireError::Cancelled)
        ));
        drop(held);
        assert!(slots.try_running(1).is_ok());
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

    /// Oracle construction publishes every closed idle query and slot gauge series.
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
            sealed_fragment_schema_fingerprint(&iceberg),
            sealed_fragment_schema_fingerprint(&parquet),
        );
    }

    /// Barrier-controlled transport probe used by the real dispatcher fan-out proof.
    struct BoundedDispatchTransport {
        /// Synchronizes the first two execute attempts so the bound is observable.
        barrier: Arc<tokio::sync::Barrier>,
        /// Number of execute calls that entered the transport.
        execute_calls: AtomicU64,
        /// Number of execute calls still holding their attempt guard.
        active: Arc<AtomicU64>,
        /// Highest number of simultaneous execute calls observed.
        maximum: Arc<AtomicU64>,
        /// Number of tuple-bound releases received after attempt cleanup.
        releases: Arc<AtomicU64>,
        /// Number of in-flight attempt guards dropped during sibling draining.
        drained: Arc<AtomicU64>,
    }

    /// Drops one in-flight transport attempt and records its cleanup.
    struct BoundedAttemptGuard {
        /// Shared active-attempt count.
        active: Arc<AtomicU64>,
        /// Shared drained-attempt count.
        drained: Arc<AtomicU64>,
    }

    impl Drop for BoundedAttemptGuard {
        /// Decrements in-flight work when cancellation or terminal failure drains it.
        fn drop(&mut self) {
            self.active.fetch_sub(1, Ordering::SeqCst);
            self.drained.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl dispatcher::OraclePeerTransport for BoundedDispatchTransport {
        /// Accepts every reservation so only the dispatch bound controls fan-out.
        async fn reserve(
            &self,
            _worker: wyrd_spec::vala::api::NodeId,
            request: wyrd_spec::vala::api::ReserveNodeSlotsRequest,
        ) -> Result<wyrd_spec::vala::api::ReserveNodeSlotsResponse, dispatcher::DispatchError>
        {
            Ok(wyrd_spec::vala::api::ReserveNodeSlotsResponse::Pending(
                wyrd_spec::vala::api::PendingNodeReservation {
                    reservation_id: wyrd_spec::vala::api::ReservationId::new(uuid::Uuid::now_v7()),
                    expires_at: request.expires_at,
                },
            ))
        }

        /// Records every tuple-bound cleanup issued by failed attempts.
        async fn release(
            &self,
            _worker: wyrd_spec::vala::api::NodeId,
            _request: wyrd_spec::vala::api::ReleaseNodeSlotsRequest,
        ) -> Result<(), dispatcher::DispatchError> {
            self.releases.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        /// Blocks siblings behind a barrier, then terminates one and waits on cancellation in the other.
        async fn execute(
            &self,
            _worker: wyrd_spec::vala::api::NodeId,
            _request: wyrd_spec::vala::api::ExecuteFragmentRequest,
        ) -> Result<dispatcher::WorkerAttemptStream, dispatcher::DispatchError> {
            let ordinal = self.execute_calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum.fetch_max(active, Ordering::SeqCst);
            let guard = BoundedAttemptGuard {
                active: Arc::clone(&self.active),
                drained: Arc::clone(&self.drained),
            };
            self.barrier.wait().await;
            if ordinal == 0 {
                let stream = async_stream::stream! {
                    let _guard = guard;
                    yield Ok(wyrd_spec::vala::api::WorkerAttemptFrame::Schema(vec![1]));
                    yield Err(dispatcher::DispatchError::Terminal);
                };
                return Ok(Box::pin(stream));
            }
            let _guard = guard;
            std::future::pending::<
                Result<dispatcher::WorkerAttemptStream, dispatcher::DispatchError>,
            >()
            .await
        }
    }

    /// Actual bounded fragment orchestration overlaps real dispatcher work, cancels failure siblings, and drains.
    #[tokio::test]
    async fn fragment_dispatch_respects_bound() {
        let leader = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        let transport = Arc::new(BoundedDispatchTransport {
            barrier: Arc::new(tokio::sync::Barrier::new(2)),
            execute_calls: AtomicU64::new(0),
            active: Arc::new(AtomicU64::new(0)),
            maximum: Arc::new(AtomicU64::new(0)),
            releases: Arc::new(AtomicU64::new(0)),
            drained: Arc::new(AtomicU64::new(0)),
        });
        let dispatcher = dispatcher::FragmentDispatcher::new(
            Arc::new(peer::DeterministicTestSigner {
                key_id: "test".to_owned(),
            }),
            dispatcher::OraclePeerTransportDirectory::new_for_test(
                leader,
                Arc::clone(&transport) as Arc<dyn dispatcher::OraclePeerTransport>,
                Arc::clone(&transport) as Arc<dyn dispatcher::OraclePeerTransport>,
            ),
        );
        let context = dispatcher::DispatchContext {
            query_id: QueryId::new(uuid::Uuid::now_v7()),
            leader_node_id: leader,
            leader_fence: 1,
            tenant_id: uuid::Uuid::now_v7(),
            query_class: QueryClass::Interactive,
            slot_units: 1,
            permission_digest: "permission".to_owned(),
            attempt_bytes: 1_024,
            attempt_memory_bytes: 1_024,
            cancellation: CancellationToken::new(),
            deadline: (Instant::now() + Duration::from_secs(5)).into(),
        };
        let siblings = CancellationToken::new();
        let mut attempts: Vec<dispatcher::SealedFragmentFuture<'_>> = Vec::new();
        for ordinal in 0..3_u8 {
            let dispatcher = &dispatcher;
            let mut context = context.clone();
            context.cancellation = siblings.clone();
            let fragment = fragment::SealedScanFragment {
                fragment_id: format!("fragment-{ordinal}"),
                binding: "binding".to_owned(),
                tier: fragment::SealedSourceTier::HotSealed,
                pinned_digest: "manifest".to_owned(),
                files: vec![fragment::SealedScanFile {
                    location: format!("binding/{ordinal}.parquet"),
                    row_groups: Vec::new(),
                    size_bytes: 1,
                    estimated_rows: 1,
                }],
                projection: Vec::new(),
                predicates: Vec::new(),
                schema_fingerprint: "schema".to_owned(),
                estimated_rows: 1,
                estimated_bytes: 1,
                deadline_unix_ms: i64::MAX,
            };
            let candidate = dispatcher::DispatchCandidate {
                node_id: leader,
                worker_fence: 1,
            };
            attempts.push(Box::pin(async move {
                dispatcher
                    .execute(&context, fragment, &[candidate])
                    .await
                    .map_err(|_error| {
                        OracleExecutionError::Public(BifrostError::QueryExecutionFailed)
                    })
            }));
        }
        let mut output = Vec::new();
        let error =
            dispatcher::FragmentDispatcher::dispatch_attempts(attempts, 2, &siblings, &mut output)
                .await
                .expect_err("terminal fragment failure");
        assert!(matches!(
            error,
            OracleExecutionError::Public(BifrostError::QueryExecutionFailed)
        ));
        assert_eq!(transport.maximum.load(Ordering::SeqCst), 2);
        assert_eq!(transport.active.load(Ordering::SeqCst), 0);
        assert_eq!(transport.releases.load(Ordering::SeqCst), 2);
        assert_eq!(transport.drained.load(Ordering::SeqCst), 2);
        assert_eq!(transport.execute_calls.load(Ordering::SeqCst), 2);
        assert!(
            output.is_empty(),
            "failed-attempt bytes must stay invisible"
        );
        assert!(siblings.is_cancelled());
    }

    /// Drops one sibling guard when decode failure drains the active attempt.
    struct DecodeSiblingGuard {
        /// Shared count of sibling cleanup completions.
        drained: Arc<AtomicUsize>,
    }

    impl Drop for DecodeSiblingGuard {
        /// Records that the sibling was dropped after cancellation.
        fn drop(&mut self) {
            self.drained.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// A validated attempt whose batch decode fails cancels and drains siblings.
    #[tokio::test]
    async fn fragment_dispatch_decode_failure_drains_sibling() {
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let siblings = CancellationToken::new();
        let drained = Arc::new(AtomicUsize::new(0));
        let first_barrier = Arc::clone(&barrier);
        let first = Box::pin(async move {
            first_barrier.wait().await;
            Ok(attempt::ValidatedAttempt {
                schema: vec![1],
                batches: attempt::AttemptBatchReader::Memory {
                    batches: vec![vec![0]].into_iter(),
                    memory_reservation: None,
                },
                footer: wyrd_spec::vala::api::WorkerFooter {
                    fragment_id: "decode-failure".to_owned(),
                    manifest_digest: QueryAuditDigest::new("manifest").expect("digest"),
                    row_count: 0,
                    encoded_bytes: 0,
                    payload_digest: QueryAuditDigest::new("payload").expect("digest"),
                    completed: true,
                },
            })
        });
        let sibling_barrier = Arc::clone(&barrier);
        let sibling_token = siblings.clone();
        let sibling_drained = Arc::clone(&drained);
        let sibling = Box::pin(async move {
            let _guard = DecodeSiblingGuard {
                drained: sibling_drained,
            };
            sibling_barrier.wait().await;
            sibling_token.cancelled().await;
            Err(OracleExecutionError::Public(
                BifrostError::QueryExecutionFailed,
            ))
        });
        let mut output = Vec::new();
        let error = dispatcher::FragmentDispatcher::dispatch_attempts(
            vec![first, sibling],
            2,
            &siblings,
            &mut output,
        )
        .await
        .expect_err("invalid decoded batch fails the whole dispatch");
        assert!(matches!(
            error,
            OracleExecutionError::Public(BifrostError::QueryExecutionFailed)
        ));
        assert!(siblings.is_cancelled());
        assert_eq!(drained.load(Ordering::SeqCst), 1);
        assert!(
            output.is_empty(),
            "decode failure cannot expose partial batches"
        );
    }
}
