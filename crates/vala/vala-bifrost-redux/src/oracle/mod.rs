//! Local Oracle query owner and deterministic planning primitives.
//!
//! The module keeps query-floor validation, admission classification, and
//! source reconciliation in the Redux crate so later serving adapters cannot
//! bypass the engine's invariants.  IO-backed execution is intentionally
//! composed around these small owners.

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
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
use futures_util::stream::FuturesUnordered;
use futures_util::{Stream, StreamExt};
use num_traits::ToPrimitive;
use rand::Rng;
use sha2::{Digest as _, Sha256};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use vala_sql::ValaPostgres;
use vala_sql::row_types::oracle_admission::{
    AdmissionAcquire, AdmissionReconcileScope, AdmissionRequest, LeaseMutation, RoleFence,
};
use wyrd_runtime::Principal;
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    AdmissionScope, AuditDetail, AuthMethod, BifrostQueryRequest, BifrostSecurityPhase,
    BifrostSecurityViolationKind, ClusterCapabilities, OracleAdmissionLease, QueryAuditDigest,
    QueryBatchFrame, QueryClass, QueryExecutionMode, QueryFreshness, QueryId, QuerySchemaFrame,
    QuerySource, QueryStreamFrame, QueryTerminalErrorCode, QueryTerminalFrame,
    QueryTerminalOutcome, SourceCompletion, SourceCompletionOutcome, VisibilityMode,
};
#[cfg(feature = "test-support")]
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult};

use crate::catalog::{BifrostCatalog, BifrostCatalogError, PinnedSealedTable, TableRef};
use crate::cluster::{ClusterRegistry, RegisteredRole};
use crate::schema::SchemaFingerprint;
use crate::scribe::memory::{BifrostMemoryGovernor, ParentMemoryReservation};
use crate::scribe::tail_rpc::{TAIL_PROTOCOL_VERSION, TailReadTransport};

pub mod assignment;
pub mod attempt;
pub mod dispatcher;
mod exec;
pub mod executor;
pub mod fragment;
pub mod peer;
pub mod telemetry;

use exec::{HotFileSource, OracleTableInputs, OracleTableProvider};
pub use exec::{ReconcileExec, TenantTripwireExec};

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

/// Bounded local admission slots for one Oracle process.
#[derive(Debug)]
pub struct OracleSlotManager {
    /// Semaphore bounding requests waiting to enter durable admission.
    pending: Arc<Semaphore>,
    /// Semaphore representing local running slot units.
    running: Arc<Semaphore>,
    /// Immutable configured pending capacity used for readiness diagnostics.
    pending_limit: usize,
    /// Immutable configured running capacity used for placement calculations.
    running_limit: usize,
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
    /// Creates telemetry around the same slot owner used by admission.
    #[must_use]
    fn new(slots: Arc<OracleSlotManager>) -> Self {
        Self {
            slots,
            memory_bytes: AtomicU64::new(0),
        }
    }

    /// Starts production accounting for one classified logical query.
    #[must_use]
    fn start_query(
        self: &Arc<Self>,
        visibility: VisibilityMode,
        query_class: QueryClass,
    ) -> QueryTelemetryGuard {
        Self::register_sparse_series(query_class);
        metrics::gauge!("bifrost_oracle_slots_total", "role" => "leader")
            .set(self.slots.running_capacity().to_f64().unwrap_or(f64::MAX));
        metrics::gauge!(
            "bifrost_oracle_in_flight",
            "visibility" => visibility_label(visibility),
            "query_class" => query_class_label(query_class)
        )
        .increment(1.0);
        QueryTelemetryGuard {
            visibility,
            query_class,
            started_at: Instant::now(),
            first_batch_recorded: false,
            stream_started: false,
            finished: false,
        }
    }

    /// Registers sparse event families with zero-valued closed-label series.
    ///
    /// A healthy query may not reject admission, spill, deduplicate, replan, or
    /// detect a security violation. Registering those families at real query
    /// entry keeps the production scrape contract discoverable without
    /// fabricating an event or changing subsequent counter values.
    fn register_sparse_series(query_class: QueryClass) {
        metrics::counter!(
            "bifrost_oracle_admission_rejections_total",
            "scope" => "cluster",
            "query_class" => query_class_label(query_class)
        )
        .increment(0);
        metrics::counter!(
            "bifrost_oracle_spill_bytes_total",
            "role" => "leader",
            "operator" => "reconcile"
        )
        .increment(0);
        metrics::counter!(
            "bifrost_oracle_spill_operations_total",
            "role" => "leader",
            "operator" => "reconcile",
            "outcome" => "spilled"
        )
        .increment(0);
        metrics::counter!(
            "bifrost_oracle_rows_deduplicated_total",
            "losing_source" => "live_tail"
        )
        .increment(0);
        metrics::counter!("bifrost_oracle_stale_replans_total", "outcome" => "retried")
            .increment(0);
        metrics::counter!(
            "bifrost_oracle_security_events_total",
            "event_class" => "tenant_row"
        )
        .increment(0);
    }

    /// Records one classification decision and its predicted scan duration.
    fn record_classification(classification: OracleClassification) {
        metrics::counter!(
            "bifrost_oracle_classification_total",
            "query_class" => query_class_label(classification.query_class),
            "reason" => classification.reason
        )
        .increment(1);
        metrics::histogram!(
            "bifrost_oracle_predicted_scan_seconds",
            "query_class" => query_class_label(classification.query_class)
        )
        .record(classification.predicted_scan_seconds);
    }

    /// Starts a bounded admission-waiter gauge and duration observation.
    #[must_use]
    fn start_admission_waiter(query_class: QueryClass) -> AdmissionWaitTelemetryGuard {
        metrics::gauge!(
            "bifrost_oracle_admission_waiters",
            "query_class" => query_class_label(query_class)
        )
        .increment(1.0);
        AdmissionWaitTelemetryGuard {
            query_class,
            started_at: Instant::now(),
            finished: false,
        }
    }

    /// Records one canonical admission rejection.
    fn record_admission_rejection(scope: &'static str, query_class: QueryClass) {
        metrics::counter!(
            "bifrost_oracle_admission_rejections_total",
            "scope" => scope,
            "query_class" => query_class_label(query_class)
        )
        .increment(1);
    }

    /// Records one local slot reservation result.
    fn record_slot_reservation(query_class: QueryClass, outcome: &'static str) {
        metrics::counter!(
            "bifrost_oracle_slot_reservations_total",
            "role" => "leader",
            "query_class" => query_class_label(query_class),
            "outcome" => outcome
        )
        .increment(1);
    }

    /// Begins gauge accounting for one acquired local slot reservation.
    #[must_use]
    fn start_slot_use(query_class: QueryClass, demand: u32) -> SlotTelemetryGuard {
        metrics::gauge!(
            "bifrost_oracle_slots_in_use",
            "role" => "leader",
            "query_class" => query_class_label(query_class)
        )
        .increment(f64::from(demand));
        SlotTelemetryGuard {
            query_class,
            demand,
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
        let total = self
            .memory_bytes
            .fetch_add(bytes as u64, Ordering::AcqRel)
            .saturating_add(bytes as u64);
        metrics::gauge!("bifrost_oracle_memory_bytes", "role" => "leader")
            .set(total.to_f64().unwrap_or(f64::MAX));
        metrics::gauge!(
            "bifrost_oracle_class_memory_bytes",
            "query_class" => query_class_label(query_class),
            "memory_kind" => memory_kind.label()
        )
        .increment(bytes.to_f64().unwrap_or(f64::MAX));
        AccountedMemoryReservation {
            reservation: Some(reservation),
            owner: Arc::clone(self),
            query_class,
            memory_kind,
            bytes,
        }
    }

    /// Releases gauge accounting after the underlying governor reservation.
    fn release_memory(&self, query_class: QueryClass, memory_kind: OracleMemoryKind, bytes: usize) {
        let total = self
            .memory_bytes
            .fetch_sub(bytes as u64, Ordering::AcqRel)
            .saturating_sub(bytes as u64);
        metrics::gauge!("bifrost_oracle_memory_bytes", "role" => "leader")
            .set(total.to_f64().unwrap_or(f64::MAX));
        metrics::gauge!(
            "bifrost_oracle_class_memory_bytes",
            "query_class" => query_class_label(query_class),
            "memory_kind" => memory_kind.label()
        )
        .decrement(bytes.to_f64().unwrap_or(f64::MAX));
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

impl OracleMemoryKind {
    /// Returns the closed low-cardinality metric value.
    const fn label(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Reconciliation => "reconciliation",
            Self::Tail => "tail",
        }
    }
}

/// Query-lifetime accounting that emits one terminal outcome on every drop.
struct QueryTelemetryGuard {
    /// Immutable visibility label.
    visibility: VisibilityMode,
    /// Immutable admission class label.
    query_class: QueryClass,
    /// Query start used by duration and first-batch histograms.
    started_at: Instant,
    /// Whether the first yielded batch was already observed.
    first_batch_recorded: bool,
    /// Whether a public stream was successfully constructed.
    stream_started: bool,
    /// Whether a terminal outcome was already emitted.
    finished: bool,
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
            "bifrost_oracle_time_to_first_batch_seconds",
            "visibility" => visibility_label(self.visibility),
            "query_class" => query_class_label(self.query_class)
        )
        .record(self.started_at.elapsed().as_secs_f64());
    }

    /// Emits the final query and stream outcome exactly once.
    fn finish(&mut self, outcome: &'static str, freshness: &'static str) {
        if self.finished {
            return;
        }
        self.finished = true;
        metrics::counter!(
            "bifrost_oracle_queries_total",
            "visibility" => visibility_label(self.visibility),
            "query_class" => query_class_label(self.query_class),
            "outcome" => outcome
        )
        .increment(1);
        metrics::histogram!(
            "bifrost_oracle_query_duration_seconds",
            "visibility" => visibility_label(self.visibility),
            "query_class" => query_class_label(self.query_class),
            "outcome" => outcome
        )
        .record(self.started_at.elapsed().as_secs_f64());
        if self.stream_started {
            metrics::counter!(
                "bifrost_oracle_streams_total",
                "outcome" => outcome,
                "freshness" => freshness
            )
            .increment(1);
        }
    }
}

impl Drop for QueryTelemetryGuard {
    /// Records cancellation or a pre-stream failure and closes in-flight state.
    fn drop(&mut self) {
        if !self.finished {
            let outcome = if self.stream_started {
                "cancelled"
            } else {
                "failed"
            };
            self.finish(outcome, "complete");
        }
        metrics::gauge!(
            "bifrost_oracle_in_flight",
            "visibility" => visibility_label(self.visibility),
            "query_class" => query_class_label(self.query_class)
        )
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
    fn finish(&mut self, scope: &'static str, outcome: &'static str) {
        if self.finished {
            return;
        }
        self.finished = true;
        metrics::histogram!(
            "bifrost_oracle_admission_wait_seconds",
            "scope" => scope,
            "outcome" => outcome
        )
        .record(self.started_at.elapsed().as_secs_f64());
    }
}

impl Drop for AdmissionWaitTelemetryGuard {
    /// Closes the waiter gauge and records unexpected exits as failures.
    fn drop(&mut self) {
        if !self.finished {
            self.finish("cluster", "failed");
        }
        metrics::gauge!(
            "bifrost_oracle_admission_waiters",
            "query_class" => query_class_label(self.query_class)
        )
        .decrement(1.0);
    }
}

/// Local slot gauge guard tied to the running semaphore permit.
struct SlotTelemetryGuard {
    /// Immutable query class.
    query_class: QueryClass,
    /// Reserved local slot units.
    demand: u32,
}

impl Drop for SlotTelemetryGuard {
    /// Removes the exact slot demand from the in-use gauge.
    fn drop(&mut self) {
        metrics::gauge!(
            "bifrost_oracle_slots_in_use",
            "role" => "leader",
            "query_class" => query_class_label(self.query_class)
        )
        .decrement(f64::from(self.demand));
    }
}

/// Parent reservation coupled to canonical Oracle memory gauges.
struct AccountedMemoryReservation {
    /// Governor reservation released before the gauges are decremented.
    reservation: Option<ParentMemoryReservation>,
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
    /// Releases parent capacity and then updates current-memory gauges.
    fn drop(&mut self) {
        self.reservation.take();
        self.owner
            .release_memory(self.query_class, self.memory_kind, self.bytes);
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

    /// Returns the currently configured running capacity.
    #[must_use]
    pub fn running_capacity(&self) -> usize {
        self.running_limit
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
    /// Durable admission lease owner.
    pub admission_leases: vala_sql::queries::oracle_admission::OracleAdmissionLeases,
    /// Platform-admin pool used for cross-tenant startup recovery.
    pub operator_pool: vala_sql::OperatorPool,
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
    /// Durable lease lifetime renewed while a query stream remains owned.
    pub lease_ttl: Duration,
    /// Cadence for renewing a live durable query lease.
    pub lease_renew_interval: Duration,
    /// Cadence for expiry and authoritative tenant-counter maintenance.
    pub maintenance_interval: Duration,
    /// Maximum remote workers selected per query; leader is additional.
    pub max_workers_per_query: usize,
    /// Maximum sealed files represented by one micro-fragment.
    pub fragment_max_files: usize,
    /// Hard byte ceiling for one complete worker attempt.
    pub attempt_max_bytes: usize,
    /// In-memory attempt threshold before permission-restricted spill.
    pub attempt_memory_bytes: usize,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            max_sql_bytes: DEFAULT_MAX_SQL_BYTES,
            default_deadline: Duration::from_secs(30),
            planning_permits: 16,
            tenant_interactive_slots: 8,
            tenant_analytical_slots: 4,
            lease_ttl: Duration::from_mins(1),
            lease_renew_interval: Duration::from_secs(20),
            maintenance_interval: Duration::from_secs(5),
            max_workers_per_query: 2,
            fragment_max_files: 16,
            attempt_max_bytes: 64 * 1024 * 1024,
            attempt_memory_bytes: 8 * 1024 * 1024,
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
    /// Admitted leader identity and lease.
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
    /// Admission lifecycle cancellation shared with lease renewal and streaming.
    cancellation: &'a CancellationToken,
}

/// Retained local query engine owner.
pub struct Oracle {
    /// Planner and floor configuration.
    planner: OraclePlanner,
    /// Admission state and local guards.
    admission: Arc<OracleAdmission>,
    /// Tenant-qualified catalog and SQL owners retained for query execution.
    catalog: Arc<BifrostCatalog>,
    /// Tenant SQL handle retained for the Oracle lifecycle boundary.
    vala: ValaPostgres,
    /// Parent-governed query memory and spill configuration.
    memory: OracleMemoryResources,
    /// Table-local Scribe tail transport directory.
    tails: Arc<TailTransportDirectory>,
    /// Mandatory immutable read/security audit collaborator.
    audit: Arc<dyn OracleAudit>,
    /// Optional distributed fragment owner assembled from server capabilities.
    fragment_dispatcher: Option<dispatcher::FragmentDispatcher>,
    /// Production metrics owner shared by query execution and admission.
    telemetry: Arc<OracleTelemetry>,
    /// Lifecycle cancellation token.
    shutdown: CancellationToken,
    /// Whether startup reconciliation completed.
    ready: Arc<AtomicBool>,
    /// One-shot startup recovery result consumed by the server activation boundary.
    startup_result: Mutex<Option<StartupResultReceiver>>,
    /// Cancellation-bound admission maintenance task.
    maintenance: Mutex<Option<JoinHandle<()>>>,
}

/// One-shot startup recovery result consumed exactly once by activation.
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
            || config.config.lease_ttl.is_zero()
            || config.config.lease_renew_interval.is_zero()
            || config.config.maintenance_interval.is_zero()
        {
            return Err(BifrostError::Internal {
                detail: "Oracle planning, tenant, lease, and maintenance limits must be positive"
                    .to_owned(),
            });
        }
        let planner = OraclePlanner::new(config.config);
        let telemetry = Arc::new(OracleTelemetry::new(Arc::clone(&config.local_slots)));
        let admission = Arc::new(OracleAdmission::new(
            config.cluster,
            config.local_slots,
            Arc::new(config.admission_leases),
            config.local_role,
            config.config,
            config.operator_pool,
        ));
        let shutdown = CancellationToken::new();
        let ready = Arc::new(AtomicBool::new(false));
        let (maintenance, startup_result) =
            admission.start_maintenance(shutdown.clone(), Arc::clone(&ready))?;
        let fragment_dispatcher = config.peer_transports.map(|transports| {
            dispatcher::FragmentDispatcher::new(Arc::clone(&config.peer_ticket_minter), transports)
                .with_memory_governor(config.memory.governor.clone())
        });
        Ok(Self {
            planner,
            admission,
            catalog: config.catalog,
            vala: config.vala,
            memory: config.memory,
            tails: config.tails,
            audit: config.audit,
            fragment_dispatcher,
            telemetry,
            shutdown,
            ready,
            startup_result: Mutex::new(Some(startup_result)),
            maintenance: Mutex::new(Some(maintenance)),
        })
    }

    /// Validates the query floor before any asynchronous metadata operation.
    ///
    /// # Errors
    /// Returns a stable public query error when the SQL is empty, oversized, or
    /// contains more than one statement or a non-`SELECT` leading keyword.
    pub fn validate_query(&self, request: &BifrostQueryRequest) -> Result<(), BifrostError> {
        self.planner.validate_query(request)
    }

    /// Resolves one tenant-qualified published table into a typed-plan `DataFrame`.
    ///
    /// This keeps catalog provider and `DataFusion` session construction inside
    /// retained Oracle. Server adapters may add bound expressions to the
    /// returned frame, but cannot bypass Oracle for source resolution.
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
        let provider: Arc<dyn TableProvider> = match self
            .catalog
            .provider_with_hot_batches(&table, tenant, Vec::new())
            .await
        {
            Ok(provider) => Arc::new(provider),
            Err(BifrostCatalogError::TableNotFound(_)) => {
                let definition = crate::tables::builtin_table(namespace.as_str(), name)
                    .ok_or(BifrostError::QueryExecutionFailed)?;
                Arc::new(
                    MemTable::try_new((definition.schema)(), vec![vec![]])
                        .map_err(|error| map_datafusion_error(&error))?,
                )
            }
            Err(error) => return Err(error.into_public()),
        };
        session
            .register_table(TableReference::bare(fqn), provider)
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
    pub async fn query_sql(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
    ) -> Result<OracleQueryStream, BifrostError> {
        self.validate_query(&request)?;
        if !self.is_ready() {
            return Err(BifrostError::OracleRoleUnavailable);
        }
        let deadline = request
            .deadline_ms
            .map_or(self.planner.config.default_deadline, Duration::from_millis);
        let deadline = Instant::now()
            .checked_add(deadline)
            .ok_or(BifrostError::QueryTimeout)?;
        let tables = parse_select_tables(&request.sql)?;
        let mut query_telemetry = None;
        for retry_ordinal in 0_u8..=1 {
            let planned = self
                .pin_and_classify(&context, &request.sql, &tables, deadline)
                .await?;
            let query_class = planned.query_class;
            if query_telemetry.is_none() {
                query_telemetry = Some(self.telemetry.start_query(request.visibility, query_class));
            }
            let mut admitted = self
                .admission
                .admit(
                    context.data_tenant_id,
                    query_class,
                    deadline,
                    self.shutdown.child_token(),
                )
                .await?;
            let drained = self
                .audit_and_drain_cut(CutAuditInput {
                    context: &context,
                    request: &request,
                    cuts: &planned.cuts,
                    query_class,
                    retry_ordinal,
                    deadline,
                    cancellation: &admitted.cancellation,
                })
                .await?;
            let degraded = drained.degraded;
            admitted.live_reservations = drained.reservations;
            let execution = self
                .execute_sql_cut(SqlCutInput {
                    context: &context,
                    sql: &request.sql,
                    cuts: planned.cuts,
                    live_batches: drained.batches,
                    query_class,
                    admitted: &admitted,
                    deadline,
                })
                .await;
            let (schema, mut batches) = match execution {
                Ok(execution) => execution,
                Err(OracleExecutionError::StaleObject) if retry_ordinal == 0 => {
                    record_stale_replan();
                    drop(admitted);
                    continue;
                }
                Err(OracleExecutionError::StaleObject) => {
                    return Err(BifrostError::QueryExecutionFailed);
                }
                Err(OracleExecutionError::Public(error)) => return Err(error),
            };
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(BifrostError::QueryTimeout)?;
            match tokio::time::timeout(remaining, batches.next())
                .await
                .map_err(|_| BifrostError::QueryTimeout)?
            {
                Some(Err(error)) if retry_ordinal == 0 && is_stale_file_error(&error) => {
                    record_stale_replan();
                    drop(admitted);
                }
                first => {
                    let query_telemetry = query_telemetry
                        .take()
                        .ok_or(BifrostError::QueryExecutionFailed)?;
                    return build_query_stream(QueryStreamInput {
                        schema,
                        batches,
                        first,
                        admitted,
                        deadline,
                        visibility: request.visibility,
                        query_class,
                        degraded,
                        stale_replanned: retry_ordinal == 1,
                        query_telemetry,
                    });
                }
            }
        }
        Err(BifrostError::QueryExecutionFailed)
    }

    /// Pins table metadata and derives the immutable class for one retry attempt.
    ///
    /// # Errors
    ///
    /// Returns timeout, catalog, planning, or byte-accounting failures.
    async fn pin_and_classify(
        &self,
        context: &AuthorizedQueryContext,
        sql: &str,
        tables: &[TableRef],
        deadline: Instant,
    ) -> Result<PlannedSqlCut, BifrostError> {
        let planning = self.planner.try_planning()?;
        let mut cuts = Vec::with_capacity(tables.len());
        let mut estimated_bytes = 0_u64;
        for table in tables {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(BifrostError::QueryTimeout)?;
            let cut = tokio::time::timeout(
                remaining,
                self.catalog.pin_sealed_table(table, context.data_tenant_id),
            )
            .await
            .map_err(|_| BifrostError::QueryTimeout)?
            .map_err(BifrostCatalogError::into_public)?;
            estimated_bytes = estimated_bytes
                .checked_add(cut.estimated_bytes)
                .ok_or(BifrostError::QueryAdmissionRejected)?;
            cuts.push(cut);
        }
        let optimized_plan = self.prepare_optimized_sql_plan(sql, &cuts).await?;
        let live_cpu = self
            .admission
            .cluster
            .snapshot()
            .live_oracles()
            .iter()
            .filter_map(|role| match &role.capabilities {
                ClusterCapabilities::OracleV1(capabilities) => Some(capabilities.cpu_cores),
                ClusterCapabilities::ScribeV1(_) => None,
            })
            .sum::<f64>();
        let classification = OraclePlanner::classification(
            estimated_bytes,
            live_cpu,
            optimized_plan_is_complex(&optimized_plan),
        );
        tracing::Span::current()
            .record("query_class", query_class_label(classification.query_class));
        OracleTelemetry::record_classification(classification);
        drop(planning);
        Ok(PlannedSqlCut {
            cuts,
            query_class: classification.query_class,
        })
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
            Arc::clone(&self.telemetry),
            input.query_class,
            input.deadline,
            input.cancellation.clone(),
            input.request.freshness,
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
        let remaining = input
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let audit_started = Instant::now();
        let decision = read_decision(
            input.context,
            &input.request.sql,
            input.cuts,
            input.request.visibility,
            input.query_class,
            input.retry_ordinal,
            input.deadline,
        )?;
        let result = tokio::time::timeout(
            remaining,
            self.audit.append_read_decision(input.context, decision),
        )
        .await
        .map_err(|_| BifrostError::QueryTimeout)
        .and_then(|result| result);
        let outcome = if result.is_ok() { "success" } else { "failed" };
        metrics::histogram!(
            "bifrost_oracle_audit_seconds",
            "audit_kind" => "read_decision",
            "outcome" => outcome
        )
        .record(audit_started.elapsed().as_secs_f64());
        if let Err(error) = result {
            tracing::error!(error = %error, "Oracle read-decision audit failed");
            return Err(if error == BifrostError::QueryTimeout {
                error
            } else {
                BifrostError::QueryAuditUnavailable
            });
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
            query_class: class,
            reason: "typed_plan",
            predicted_scan_seconds: 0.0,
        });
        let admitted = self
            .admission
            .admit(
                context.data_tenant_id,
                class,
                options.deadline,
                self.shutdown.child_token(),
            )
            .await?;
        let remaining = options
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let _audit_span = tracing::info_span!(
            "bifrost.oracle.audit",
            audit_kind = "read_decision",
            query_class = query_class_label(class)
        );
        let audit_started = Instant::now();
        let audit_result = tokio::time::timeout(
            remaining,
            self.audit.append_read_decision(
                &context,
                plan_read_decision(&context, &plan, options, class)?,
            ),
        )
        .await
        .map_err(|_| BifrostError::QueryTimeout)?
        .map_err(|_| BifrostError::QueryAuditUnavailable);
        metrics::histogram!(
            "bifrost_oracle_audit_seconds",
            "audit_kind" => "read_decision",
            "outcome" => if audit_result.is_ok() { "success" } else { "failed" }
        )
        .record(audit_started.elapsed().as_secs_f64());
        audit_result?;
        let _plan_span = tracing::info_span!("bifrost.oracle.plan", plan_kind = "typed");
        let session = SessionContext::new();
        let physical = session
            .state()
            .create_physical_plan(&plan)
            .await
            .map_err(|error| map_datafusion_error(&error))?;
        let schema = physical.schema();
        let _source_span = tracing::info_span!(
            "bifrost.oracle.source",
            query_class = query_class_label(class)
        );
        let mut batches = execute_stream(physical, session.task_ctx())
            .map_err(|error| map_datafusion_error(&error))?;
        let remaining = options
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let first = tokio::time::timeout(remaining, batches.next())
            .await
            .map_err(|_| BifrostError::QueryTimeout)?;
        build_query_stream(QueryStreamInput {
            schema,
            batches,
            first,
            admitted,
            deadline: options.deadline,
            visibility: options.visibility,
            query_class: class,
            degraded: false,
            stale_replanned: false,
            query_telemetry,
        })
    }

    /// Returns whether startup reconciliation completed and queries may enter admission.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.startup_reconciled() && self.admission.is_available()
    }

    /// Reports whether local startup reconciliation completed before membership activation.
    ///
    /// Server boot uses this dependency-local phase to avoid waiting on
    /// [`Self::is_ready`], which intentionally also requires the later durable
    /// membership activation and immutable snapshot publication.
    #[must_use]
    pub fn startup_reconciled(&self) -> bool {
        self.ready.load(Ordering::Acquire) && !self.shutdown.is_cancelled()
    }

    /// Awaits the exact startup recovery result before role activation.
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
            detail: "Oracle startup recovery task ended without a result".to_owned(),
        })?
    }

    /// Borrows the tenant SQL root retained by the Oracle composition boundary.
    #[must_use]
    pub fn tenant_sql(&self) -> &ValaPostgres {
        &self.vala
    }

    /// Cancels lifecycle maintenance and waits for no further work.
    pub async fn shutdown(&self, deadline: Instant) {
        self.shutdown.cancel();
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
    ) -> Result<(SchemaRef, SendableRecordBatchStream), OracleExecutionError> {
        let _source_span = tracing::info_span!(
            "bifrost.oracle.source",
            query_class = query_class_label(input.query_class)
        );
        let session = SessionContext::new();
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
        self.execute_session(&session, input.sql).await
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

    /// Builds and starts one physical plan after all pinned providers are registered.
    ///
    /// # Errors
    ///
    /// Returns a stable execution failure when `DataFusion` cannot lower or start the plan.
    async fn execute_session(
        &self,
        session: &SessionContext,
        sql: &str,
    ) -> Result<(SchemaRef, SendableRecordBatchStream), OracleExecutionError> {
        let frame = session
            .sql(sql)
            .await
            .map_err(|error| map_datafusion_error(&error))?;
        let physical = frame
            .create_physical_plan()
            .await
            .map_err(|error| map_datafusion_error(&error))?;
        let schema = physical.schema();
        let stream = execute_stream(physical, session.task_ctx())
            .map_err(|error| map_datafusion_error(&error))?;
        Ok((schema, stream))
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
        let mut output = Vec::new();
        let siblings = prepared.context.cancellation.child_token();
        let parallelism = dispatch_parallelism(
            prepared.selected.len(),
            self.planner.config.max_workers_per_query,
        );
        let mut attempts: Vec<SealedFragmentFuture<'_>> = Vec::new();
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
        dispatch_fragment_attempts(attempts, parallelism, &siblings, &mut output).await?;
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
        let snapshot = self.admission.cluster.snapshot();
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

    /// Lowers and optimizes SQL against schema-only pinned table providers.
    ///
    /// This classification plan cannot read source rows: every registered
    /// provider is an empty in-memory table carrying only the public schema
    /// from the already pinned Iceberg metadata. The actual provider union is
    /// built only after admission, fence finalization, and audit commit.
    ///
    /// # Errors
    ///
    /// Returns a stable planning failure when schema projection, registration,
    /// SQL lowering, or logical optimization fails.
    #[tracing::instrument(
        name = "bifrost.oracle.plan",
        skip_all,
        fields(table_count = cuts.len())
    )]
    async fn prepare_optimized_sql_plan(
        &self,
        sql: &str,
        cuts: &[PinnedSealedTable],
    ) -> Result<datafusion::logical_expr::LogicalPlan, BifrostError> {
        let session = SessionContext::new();
        for cut in cuts {
            let physical = iceberg::arrow::schema_to_arrow_schema(
                cut.iceberg_table.metadata().current_schema(),
            )
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
            let fields = physical
                .fields()
                .iter()
                .filter(|field| field.name() != "data_tenant_id")
                .cloned()
                .collect::<Vec<_>>();
            let public = Arc::new(Schema::new(fields));
            let provider = MemTable::try_new(public, vec![Vec::new()])
                .map_err(|error| map_datafusion_error(&error))?;
            register_session_table(
                &session,
                &cut.binding,
                Arc::new(provider) as Arc<dyn TableProvider>,
            )?;
        }
        session
            .sql(sql)
            .await
            .map_err(|error| map_datafusion_error(&error))?
            .into_optimized_plan()
            .map_err(|error| map_datafusion_error(&error))
    }
}

/// Computes the exact active sealed-fragment bound from selected and admitted capacity.
fn dispatch_parallelism(selected_peer_count: usize, admitted_query_parallelism: usize) -> usize {
    selected_peer_count
        .min(admitted_query_parallelism.max(1))
        .max(1)
}

/// One lazily polled production fragment attempt owned by bounded query dispatch.
type SealedFragmentFuture<'a> = Pin<
    Box<dyn Future<Output = Result<attempt::ValidatedAttempt, OracleExecutionError>> + Send + 'a>,
>;

/// Polls actual fragment attempts under the admitted bound and drains siblings on failure.
///
/// Bytes are decoded only after a complete attempt validates. A terminal or
/// decode failure cancels the shared token and awaits every active sibling so
/// reservation guards release before this operation returns.
///
/// # Errors
///
/// Returns the first transport, terminal, or decode failure after sibling cleanup.
async fn dispatch_fragment_attempts(
    attempts: Vec<SealedFragmentFuture<'_>>,
    parallelism: usize,
    siblings: &CancellationToken,
    output: &mut Vec<RecordBatch>,
) -> Result<(), OracleExecutionError> {
    let mut attempts = attempts.into_iter();
    let mut pending = FuturesUnordered::new();
    loop {
        while pending.len() < parallelism {
            let Some(attempt) = attempts.next() else {
                break;
            };
            pending.push(attempt);
        }
        let Some(attempt) = pending.next().await else {
            return Ok(());
        };
        match attempt {
            Ok(attempt) => {
                if let Err(error) = decode_attempt_batches(attempt, output) {
                    siblings.cancel();
                    while pending.next().await.is_some() {}
                    return Err(error.into());
                }
            }
            Err(error) => {
                siblings.cancel();
                while pending.next().await.is_some() {}
                return Err(error);
            }
        }
    }
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
    schema
        .register_table(binding.table_name.clone(), provider)
        .map_err(|error| map_datafusion_error(&error))?;
    Ok(())
}

/// Query floor and logical-plan preparation owner.
#[derive(Debug, Clone)]
pub struct OraclePlanner {
    /// Synchronous floor, deadline, and capacity configuration.
    config: OracleConfig,
    /// Bounded permits covering sealed metadata planning.
    planning: Arc<Semaphore>,
}

/// One closed classification result with its production accounting inputs.
#[derive(Debug, Clone, Copy)]
struct OracleClassification {
    /// Locked query class selected for admission.
    query_class: QueryClass,
    /// Closed reason explaining the class selection.
    reason: &'static str,
    /// Predicted sealed scan duration from the normative formula.
    predicted_scan_seconds: f64,
}

impl OraclePlanner {
    /// Creates a planner with bounded synchronous validation settings.
    #[must_use]
    pub fn new(config: OracleConfig) -> Self {
        Self {
            planning: Arc::new(Semaphore::new(config.planning_permits)),
            config,
        }
    }

    /// Validates one non-empty, single-statement `SELECT` request.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryInvalidSql`] for floor violations.
    pub fn validate_query(&self, request: &BifrostQueryRequest) -> Result<(), BifrostError> {
        request
            .validate()
            .map_err(|error| BifrostError::QueryInvalidSql {
                detail: error.to_string(),
            })?;
        if request.sql.len() > self.config.max_sql_bytes {
            return Err(BifrostError::QueryInvalidSql {
                detail: "query exceeds configured SQL byte limit".to_owned(),
            });
        }
        parse_select_tables(&request.sql)?;
        Ok(())
    }

    /// Tries to reserve one bounded planning slot without queuing unbounded work.
    ///
    /// # Errors
    ///
    /// Returns admission rejection while the planning bound is saturated.
    fn try_planning(&self) -> Result<OwnedSemaphorePermit, BifrostError> {
        Arc::clone(&self.planning)
            .try_acquire_owned()
            .map_err(|_| BifrostError::QueryAdmissionRejected)
    }

    /// Applies the normative estimated-byte classification formula.
    #[must_use]
    pub fn classify(estimated_bytes: u64, live_oracle_cpu: f64, complex: bool) -> QueryClass {
        Self::classification(estimated_bytes, live_oracle_cpu, complex).query_class
    }

    /// Produces the class, closed reason, and predicted duration from one cut.
    #[must_use]
    fn classification(
        estimated_bytes: u64,
        live_oracle_cpu: f64,
        complex: bool,
    ) -> OracleClassification {
        let cpu = (live_oracle_cpu * 0.8).floor().max(1.0);
        let seconds =
            estimated_bytes.to_f64().unwrap_or(f64::MAX) / ESTIMATED_SCAN_BYTES_PER_SECOND / cpu;
        if complex {
            OracleClassification {
                query_class: QueryClass::Analytical,
                reason: "global_operator",
                predicted_scan_seconds: seconds,
            }
        } else if seconds > INTERACTIVE_SCAN_LIMIT_SECONDS {
            OracleClassification {
                query_class: QueryClass::Analytical,
                reason: "predicted_scan",
                predicted_scan_seconds: seconds,
            }
        } else {
            OracleClassification {
                query_class: QueryClass::Interactive,
                reason: "estimated_scan",
                predicted_scan_seconds: seconds,
            }
        }
    }
}

/// Admission owner for bounded local capacity and immutable membership snapshots.
pub struct OracleAdmission {
    /// Immutable membership snapshots used for one admission calculation.
    cluster: Arc<ClusterRegistry>,
    /// Local pending and running capacity guards.
    slots: Arc<OracleSlotManager>,
    /// Durable cluster/class/tenant lease transaction owner.
    leases: Arc<vala_sql::queries::oracle_admission::OracleAdmissionLeases>,
    /// Platform-admin pool used only during startup recovery.
    operator_pool: vala_sql::OperatorPool,
    /// Local role identity and fence authorizing lease mutation.
    local_role: RegisteredRole,
    /// Validated tenant ceilings and lifecycle configuration.
    config: OracleConfig,
    /// Bounded tenant scopes queued for periodic authoritative repair.
    tenant_reconcile_queue: Arc<Mutex<TenantReconcileQueue>>,
}

/// Bounded deduplicated tenant scopes repaired by admission maintenance.
#[derive(Default)]
struct TenantReconcileQueue {
    /// FIFO order for bounded maintenance work.
    order: VecDeque<(DataTenantId, QueryClass)>,
    /// Exact set preventing duplicate hot-path insertions.
    members: HashSet<(DataTenantId, QueryClass)>,
}

/// Closed insertion outcome for the bounded tenant reconciliation queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TenantReconcileInsert {
    /// A new distinct scope entered the FIFO.
    Queued,
    /// The scope was already pending and required no second entry.
    Coalesced,
    /// The fixed 64-scope queue had no capacity for a new scope.
    Saturated,
}

/// Immutable durable capacity request computed from one membership snapshot.
struct AdmissionPlan {
    /// Candidate lease bound to the local leader fence.
    lease: OracleAdmissionLease,
    /// Cluster-wide usable slot ceiling.
    cluster_limit: u32,
    /// Independent class ceiling.
    class_limit: u32,
    /// Tenant/class ceiling.
    tenant_limit: u32,
    /// Bounded durable acquisition deadline.
    deadline: Instant,
}

/// Shared terminal reason and cancellation-bound renewal task.
type LeaseRenewal = (Arc<Mutex<Option<QueryTerminalErrorCode>>>, JoinHandle<()>);

impl TenantReconcileQueue {
    /// Enqueues one tenant/class scope without performing durable IO.
    fn enqueue(
        &mut self,
        data_tenant_id: DataTenantId,
        query_class: QueryClass,
    ) -> TenantReconcileInsert {
        let key = (data_tenant_id, query_class);
        if self.members.contains(&key) {
            TenantReconcileInsert::Coalesced
        } else if self.order.len() < 64 {
            self.order.push_back(key);
            self.members.insert(key);
            TenantReconcileInsert::Queued
        } else {
            TenantReconcileInsert::Saturated
        }
    }

    /// Removes at most `limit` scopes for one maintenance transaction.
    fn take(&mut self, limit: usize) -> Vec<AdmissionReconcileScope> {
        let mut scopes = Vec::with_capacity(self.order.len().min(limit));
        while scopes.len() < limit {
            let Some((data_tenant_id, query_class)) = self.order.pop_front() else {
                break;
            };
            scopes.push(AdmissionReconcileScope::Tenant {
                data_tenant_id,
                query_class,
            });
        }
        scopes
    }

    /// Completes successful scopes so a future rejection may enqueue them.
    fn complete(&mut self, scopes: &[AdmissionReconcileScope]) {
        for scope in scopes {
            if let AdmissionReconcileScope::Tenant {
                data_tenant_id,
                query_class,
            } = scope
            {
                self.members.remove(&(*data_tenant_id, *query_class));
            }
        }
    }

    /// Restores a failed maintenance batch at the FIFO head in original order.
    fn requeue(&mut self, scopes: &[AdmissionReconcileScope]) {
        for scope in scopes.iter().rev() {
            if let AdmissionReconcileScope::Tenant {
                data_tenant_id,
                query_class,
            } = scope
            {
                self.order.push_front((*data_tenant_id, *query_class));
            }
        }
    }
}

impl std::fmt::Debug for OracleAdmission {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OracleAdmission")
            .finish_non_exhaustive()
    }
}

impl OracleAdmission {
    /// Computes one fenced durable admission plan from the current immutable snapshot.
    ///
    /// # Errors
    ///
    /// Returns role-unavailable, timeout, internal-duration, or capacity errors
    /// when the snapshot cannot produce a valid local lease.
    fn plan_admission(
        &self,
        tenant: DataTenantId,
        query_class: QueryClass,
        deadline: Instant,
    ) -> Result<(AdmissionPlan, chrono::Duration), BifrostError> {
        let snapshot = self.cluster.snapshot();
        let live = snapshot.live_oracles();
        let local_node = self.local_role.key.node_id;
        if !live.iter().any(|role| role.key.node_id == local_node) {
            return Err(BifrostError::OracleRoleUnavailable);
        }
        let usable_slots = live
            .iter()
            .filter_map(|role| match &role.capabilities {
                ClusterCapabilities::OracleV1(capabilities) => Some(capabilities.usable_slots),
                ClusterCapabilities::ScribeV1(_) => None,
            })
            .try_fold(0_u32, u32::checked_add)
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        let (class_limit, demand) = admission_limits(usable_slots, query_class);
        let tenant_limit = match query_class {
            QueryClass::Interactive => self.config.tenant_interactive_slots,
            QueryClass::Analytical => self.config.tenant_analytical_slots,
        };
        let lease_ttl = chrono::Duration::from_std(self.config.lease_ttl).map_err(|_| {
            BifrostError::Internal {
                detail: "Oracle lease TTL exceeds chrono bounds".to_owned(),
            }
        })?;
        if deadline <= Instant::now() {
            return Err(BifrostError::QueryTimeout);
        }
        let now = chrono::Utc::now();
        Ok((
            AdmissionPlan {
                lease: OracleAdmissionLease {
                    query_id: QueryId::new(uuid::Uuid::now_v7()),
                    data_tenant_id: tenant,
                    query_class,
                    slot_units: demand,
                    selected_node_ids: vec![local_node],
                    leader_node_id: local_node,
                    leader_fencing_token: self.local_role.fencing_token,
                    acquired_at: now,
                    expires_at: now + lease_ttl,
                },
                cluster_limit: usable_slots,
                class_limit,
                tenant_limit,
                deadline: deadline.min(Instant::now() + Duration::from_millis(250)),
            },
            lease_ttl,
        ))
    }

    /// Performs the bounded one-retry durable lease transaction.
    ///
    /// # Errors
    ///
    /// Returns admission rejection on timeout and execution failure on SQL errors.
    async fn acquire_plan(&self, plan: &AdmissionPlan) -> Result<AdmissionAcquire, BifrostError> {
        for attempt in 0_u8..=1 {
            let request = AdmissionRequest {
                lease: plan.lease.clone(),
                cluster_limit: plan.cluster_limit,
                class_limit: plan.class_limit,
                tenant_limit: plan.tenant_limit,
            };
            let remaining = plan
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(BifrostError::QueryAdmissionRejected)?;
            let operation = async {
                let mut conn = self
                    .leases
                    .postgres()
                    .tenant_conn(request.lease.data_tenant_id)
                    .await?;
                let outcome = self.leases.acquire(&mut conn, &request).await?;
                conn.commit().await?;
                Ok::<AdmissionAcquire, vala_sql::SqlError>(outcome)
            };
            let outcome = tokio::time::timeout(remaining, operation)
                .await
                .map_err(|_| BifrostError::QueryAdmissionRejected)?
                .map_err(|error| {
                    tracing::error!(error = %error, "Oracle durable admission failed");
                    BifrostError::QueryExecutionFailed
                })?;
            if matches!(outcome, AdmissionAcquire::Acquired(_)) || attempt == 1 {
                return Ok(outcome);
            }
            let jitter_ms = rand::thread_rng().gen_range(20_u64..=100);
            let remaining = plan
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(BifrostError::QueryAdmissionRejected)?;
            tokio::time::sleep(remaining.min(Duration::from_millis(jitter_ms))).await;
        }
        Err(BifrostError::QueryAdmissionRejected)
    }

    /// Renews one tenant-bound durable lease and commits the outer transaction.
    ///
    /// # Errors
    ///
    /// Returns [`vala_sql::SqlError`] when the tenant transaction, fenced
    /// renewal, or commit fails.
    async fn renew_lease(
        &self,
        data_tenant_id: DataTenantId,
        query_id: QueryId,
        leader: &RoleFence,
        expiry: chrono::DateTime<chrono::Utc>,
    ) -> Result<LeaseMutation, vala_sql::SqlError> {
        let mut conn = self.leases.postgres().tenant_conn(data_tenant_id).await?;
        let mutation = self
            .leases
            .renew(&mut conn, query_id, leader, expiry)
            .await?;
        conn.commit().await?;
        Ok(mutation)
    }

    /// Releases one tenant-bound durable lease and commits the outer transaction.
    ///
    /// # Errors
    ///
    /// Returns [`vala_sql::SqlError`] when the tenant transaction, fenced
    /// release, or commit fails.
    async fn release_lease(
        &self,
        data_tenant_id: DataTenantId,
        query_id: QueryId,
        leader: &RoleFence,
    ) -> Result<LeaseMutation, vala_sql::SqlError> {
        let mut conn = self.leases.postgres().tenant_conn(data_tenant_id).await?;
        let mutation = self.leases.release(&mut conn, query_id, leader).await?;
        conn.commit().await?;
        Ok(mutation)
    }

    /// Expires and reconciles one tenant/class maintenance scope atomically.
    ///
    /// # Errors
    ///
    /// Returns [`vala_sql::SqlError`] when the tenant transaction, expiry,
    /// reconciliation, or commit fails.
    async fn maintain_scope(
        &self,
        scope: &AdmissionReconcileScope,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), vala_sql::SqlError> {
        let AdmissionReconcileScope::Tenant {
            data_tenant_id,
            query_class,
        } = scope
        else {
            return Err(vala_sql::SqlError::InvariantViolation {
                detail: "maintenance requires a tenant scope".to_owned(),
            });
        };
        let mut conn = self.leases.postgres().tenant_conn(*data_tenant_id).await?;
        self.leases.expire_batch(&mut conn, now, 128).await?;
        self.leases
            .reconcile_scopes(
                &mut conn,
                &[
                    AdmissionReconcileScope::Cluster,
                    AdmissionReconcileScope::Class {
                        query_class: *query_class,
                    },
                    scope.clone(),
                ],
                now,
            )
            .await?;
        conn.commit().await
    }

    /// Reclaims expired shared admission state before this role advertises readiness.
    async fn recover_startup(&self) -> Result<(), vala_sql::SqlError> {
        self.leases
            .recover_shared_scopes(&self.operator_pool, chrono::Utc::now())
            .await
    }

    /// Starts cancellation-bound renewal for one acquired durable lease.
    fn start_lease_renewal(
        self: &Arc<Self>,
        lease: &OracleAdmissionLease,
        cancellation: &CancellationToken,
        lease_ttl: chrono::Duration,
    ) -> LeaseRenewal {
        let renewal_cancel = cancellation.clone();
        let renewal_terminal = Arc::new(Mutex::new(None));
        let terminal = Arc::clone(&renewal_terminal);
        let admission = Arc::clone(self);
        let data_tenant_id = lease.data_tenant_id;
        let query_id = lease.query_id;
        let interval_duration = self.config.lease_renew_interval;
        let leader = RoleFence {
            node_id: self.local_role.key.node_id,
            fencing_token: self.local_role.fencing_token,
        };
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval_duration);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                tokio::select! {
                    () = renewal_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        let expiry = chrono::Utc::now() + lease_ttl;
                        let result = admission
                            .renew_lease(data_tenant_id, query_id, &leader, expiry)
                            .await;
                        if let Some((code, outcome)) = renewal_failure(&result) {
                            if let Ok(mut reason) = terminal.lock() {
                                *reason = Some(code);
                            }
                            metrics::counter!(
                                "bifrost_oracle_lease_renewals_total",
                                "outcome" => outcome
                            ).increment(1);
                            renewal_cancel.cancel();
                            break;
                        }
                        metrics::counter!(
                            "bifrost_oracle_lease_renewals_total",
                            "outcome" => "renewed"
                        ).increment(1);
                    }
                }
            }
        });
        (renewal_terminal, task)
    }

    /// Creates a local admission owner.
    #[must_use]
    fn new(
        cluster: Arc<ClusterRegistry>,
        slots: Arc<OracleSlotManager>,
        leases: Arc<vala_sql::queries::oracle_admission::OracleAdmissionLeases>,
        local_role: RegisteredRole,
        config: OracleConfig,
        operator_pool: vala_sql::OperatorPool,
    ) -> Self {
        Self {
            cluster,
            slots,
            leases,
            operator_pool,
            local_role,
            config,
            tenant_reconcile_queue: Arc::new(Mutex::new(TenantReconcileQueue::default())),
        }
    }

    /// Starts startup reconciliation followed by cancellation-bound maintenance.
    ///
    /// # Errors
    ///
    /// Returns an internal error when construction occurs outside a Tokio runtime.
    fn start_maintenance(
        self: &Arc<Self>,
        shutdown: CancellationToken,
        ready: Arc<AtomicBool>,
    ) -> Result<(JoinHandle<()>, StartupResultReceiver), BifrostError> {
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| BifrostError::Internal {
                detail: "Oracle construction requires an active Tokio runtime".to_owned(),
            })?;
        let admission = Arc::clone(self);
        let queue = Arc::clone(&self.tenant_reconcile_queue);
        let maintenance_interval = self.config.maintenance_interval;
        let (startup_tx, startup_rx) = tokio::sync::oneshot::channel();
        let task = runtime.spawn(async move {
            if let Err(error) = admission.recover_startup().await {
                tracing::error!(error = %error, "Oracle startup admission recovery failed");
                let _ = startup_tx.send(Err(BifrostError::Internal {
                    detail: format!("Oracle startup admission recovery failed: {error}"),
                }));
                return;
            }
            ready.store(true, Ordering::Release);
            let _ = startup_tx.send(Ok(()));
            let mut interval = tokio::time::interval(maintenance_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    _ = interval.tick() => {
                        let scopes = queue
                            .lock()
                            .map(|mut queue| queue.take(64))
                            .unwrap_or_default();
                        for scope in scopes {
                            match admission.maintain_scope(&scope, chrono::Utc::now()).await {
                                Ok(()) => {
                                    if let Ok(mut queue) = queue.lock() {
                                        queue.complete(std::slice::from_ref(&scope));
                                    }
                                }
                                Err(error) => {
                                    if let Ok(mut queue) = queue.lock() {
                                        queue.requeue(std::slice::from_ref(&scope));
                                    }
                                    metrics::counter!(
                                        "bifrost_oracle_admission_reconcile_total",
                                        "outcome" => "requeued"
                                    )
                                    .increment(1);
                                    tracing::error!(error = %error, "Oracle tenant admission reconciliation failed; scopes requeued");
                                }
                            }
                        }
                    }
                }
            }
            ready.store(false, Ordering::Release);
        });
        Ok((task, startup_rx))
    }

    /// Reports whether at least one Oracle role and local slot are available.
    #[must_use]
    pub fn is_available(&self) -> bool {
        !self.cluster.snapshot().live_oracles().is_empty() && self.slots.running_capacity() > 0
    }

    /// Acquires cluster/class/tenant capacity and the matching local running guard.
    ///
    /// The calculation uses one immutable membership snapshot. This single-node
    /// task selects the local leader and never downgrades analytical work. The
    /// supplied cancellation is a child of Oracle's lifecycle token and remains
    /// owned by the returned stream guard so role shutdown cancels accepted work.
    ///
    /// # Errors
    ///
    /// Returns stable admission rejection for bounded waiter, durable capacity,
    /// local-capacity, or placement failure; SQL failures fail the query closed.
    #[tracing::instrument(
        name = "bifrost.oracle.admission",
        skip_all,
        fields(query_class = query_class_label(query_class))
    )]
    async fn admit(
        self: &Arc<Self>,
        tenant: DataTenantId,
        query_class: QueryClass,
        deadline: Instant,
        cancellation: CancellationToken,
    ) -> Result<AdmittedQueryGuard, BifrostError> {
        let mut waiter = OracleTelemetry::start_admission_waiter(query_class);
        let pending = match self.slots.try_pending() {
            Ok(pending) => pending,
            Err(error) => {
                OracleTelemetry::record_admission_rejection("cluster", query_class);
                waiter.finish("cluster", "rejected");
                return Err(error);
            }
        };
        let (plan, lease_ttl) = self.plan_admission(tenant, query_class, deadline)?;
        let demand = plan.lease.slot_units;
        let local_node = plan.lease.leader_node_id;
        let acquired = self.acquire_plan(&plan).await?;
        match acquired {
            AdmissionAcquire::Rejected {
                scope,
                retry_after_ms,
            } => {
                debug_assert_eq!(retry_after_ms, 1_000);
                if scope == AdmissionScope::Tenant {
                    let insert = self
                        .tenant_reconcile_queue
                        .lock()
                        .map_or(TenantReconcileInsert::Saturated, |mut queue| {
                            queue.enqueue(tenant, query_class)
                        });
                    if insert == TenantReconcileInsert::Saturated {
                        metrics::counter!(
                            "bifrost_oracle_admission_reconcile_queue_total",
                            "outcome" => "saturated"
                        )
                        .increment(1);
                    }
                }
                let scope = admission_scope_label(scope);
                OracleTelemetry::record_admission_rejection(scope, query_class);
                waiter.finish(scope, "rejected");
                Err(BifrostError::QueryAdmissionRejected)
            }
            AdmissionAcquire::Acquired(lease) => {
                waiter.finish("cluster", "acquired");
                drop(pending);
                let _slot_span = tracing::info_span!(
                    "bifrost.oracle.slot_reservation",
                    role = "leader",
                    query_class = query_class_label(query_class),
                    slot_units = demand
                );
                let running = match self.slots.try_running(demand) {
                    Ok(running) => {
                        OracleTelemetry::record_slot_reservation(query_class, "acquired");
                        running
                    }
                    Err(error) => {
                        OracleTelemetry::record_slot_reservation(query_class, "rejected");
                        OracleTelemetry::record_admission_rejection("cluster", query_class);
                        let leader = RoleFence {
                            node_id: local_node,
                            fencing_token: self.local_role.fencing_token,
                        };
                        let _ = self
                            .release_lease(lease.data_tenant_id, lease.query_id, &leader)
                            .await;
                        return Err(error);
                    }
                };
                let slot_telemetry = OracleTelemetry::start_slot_use(query_class, demand);
                let (renewal_terminal, renewal) =
                    self.start_lease_renewal(&lease, &cancellation, lease_ttl);
                Ok(AdmittedQueryGuard {
                    data_tenant_id: lease.data_tenant_id,
                    query_id: lease.query_id,
                    leader: RoleFence {
                        node_id: local_node,
                        fencing_token: self.local_role.fencing_token,
                    },
                    admission: Arc::clone(self),
                    running: Some(running),
                    slot_telemetry: Some(slot_telemetry),
                    live_reservations: Vec::new(),
                    cancellation,
                    renewal_terminal,
                    renewal: Some(renewal),
                })
            }
        }
    }
}

/// Maps a non-renewed lease result to its terminal code and closed metric label.
fn renewal_failure(
    result: &Result<LeaseMutation, vala_sql::SqlError>,
) -> Option<(QueryTerminalErrorCode, &'static str)> {
    match result {
        Ok(LeaseMutation::Renewed(_)) => None,
        Ok(LeaseMutation::StaleLeaderFence) => Some((
            QueryTerminalErrorCode::QueryPeerSecurity,
            "stale_leader_fence",
        )),
        Ok(LeaseMutation::Missing) => {
            Some((QueryTerminalErrorCode::QueryExecutionFailed, "missing"))
        }
        Ok(LeaseMutation::Released | LeaseMutation::AlreadyReleased) => {
            Some((QueryTerminalErrorCode::QueryExecutionFailed, "released"))
        }
        Err(error) => {
            tracing::error!(error = %error, "Oracle admission renewal failed");
            Some((QueryTerminalErrorCode::QueryExecutionFailed, "sql_error"))
        }
    }
}

/// Stream-owned durable/local capacity cleanup.
struct AdmittedQueryGuard {
    /// Tenant whose transaction owns the durable lease and cleanup.
    data_tenant_id: DataTenantId,
    /// Durable lease identity released on every completion path.
    query_id: QueryId,
    /// Fenced leader authorized to release the durable lease.
    leader: RoleFence,
    /// Durable admission workflow owner retained through asynchronous cleanup.
    admission: Arc<OracleAdmission>,
    /// Local running capacity retained until stream completion/drop.
    running: Option<OwnedSemaphorePermit>,
    /// Canonical local slot-use gauge retained with the running permit.
    slot_telemetry: Option<SlotTelemetryGuard>,
    /// Parent reservations retaining drained live batches through stream cleanup.
    live_reservations: Vec<AccountedMemoryReservation>,
    /// Cancellation shared with the 20-second lease renewer and stream.
    cancellation: CancellationToken,
    /// Typed late terminal selected by a failed durable lease renewal.
    renewal_terminal: Arc<Mutex<Option<QueryTerminalErrorCode>>>,
    /// Renewal task aborted when the stream completes or drops.
    renewal: Option<JoinHandle<()>>,
}

impl Drop for AdmittedQueryGuard {
    /// Releases local capacity immediately and durable capacity asynchronously.
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Some(renewal) = self.renewal.take() {
            renewal.abort();
        }
        self.running.take();
        self.slot_telemetry.take();
        let data_tenant_id = self.data_tenant_id;
        let query_id = self.query_id;
        let leader = RoleFence {
            node_id: self.leader.node_id,
            fencing_token: self.leader.fencing_token,
        };
        let admission = Arc::clone(&self.admission);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Err(error) = admission
                    .release_lease(data_tenant_id, query_id, &leader)
                    .await
                {
                    tracing::error!(error = %error, "Oracle admission cleanup failed");
                }
            });
        }
    }
}

/// Owns the dependencies and invariants for one query's bounded live-tail cut.
///
/// The owner keeps fence discovery, parent-memory accounting, telemetry,
/// deadline, freshness, and cancellation aligned across acquisition and drain.
/// It does not outlive the query attempt that constructed it.
struct TailFenceDrainer<'a> {
    /// Directory used to resolve every discovered Scribe stream transport.
    tails: &'a TailTransportDirectory,
    /// Parent-governed memory resources charged for decoded live batches.
    memory: &'a OracleMemoryResources,
    /// Query telemetry retaining live-tail memory accounting.
    telemetry: Arc<OracleTelemetry>,
    /// Immutable admission class applied to live-tail memory metrics.
    query_class: QueryClass,
    /// Absolute deadline shared by fence acquisition and every page read.
    deadline: Instant,
    /// Admission lifecycle cancellation shared with renewal and streaming.
    cancellation: CancellationToken,
    /// Caller-selected strict or degraded live-source failure policy.
    freshness: wyrd_spec::vala::api::FreshnessPolicy,
}

/// One metadata-only acquired fence retained until its post-audit drain.
struct AcquiredTailFence {
    /// Canonical table receiving drained batches.
    table: String,
    /// Transport that owns the retained interval.
    transport: Arc<dyn TailReadTransport>,
    /// Immutable fence metadata returned by Scribe.
    fence: wyrd_spec::vala::api::TailReadFence,
    /// Successful acquisition time used by the hold-duration histogram.
    acquired_at: Instant,
    /// Whether every requested page completed before automatic release.
    drain_succeeded: bool,
}

/// One planned metadata-only fence acquisition for a discovered Scribe stream.
struct TailFenceAcquisition {
    /// Canonical table that will receive the retained live interval.
    table: String,
    /// Transport selected for the exact Scribe stream identity.
    transport: Arc<dyn TailReadTransport>,
    /// Validated private request pinning cursor, schema, and deadline metadata.
    request: wyrd_spec::vala::api::AcquireTailFenceRequest,
}

/// Bounded live rows and their parent-governor reservations.
#[derive(Default)]
struct DrainedTails {
    /// Per-table shallow Arrow batches.
    batches: HashMap<String, Vec<RecordBatch>>,
    /// Reservations retained until the final query stream drops.
    reservations: Vec<AccountedMemoryReservation>,
    /// Whether one requested live source was unavailable.
    degraded: bool,
}

/// One live-tail interval drained and charged under the parent governor.
struct DrainedTailFence {
    /// Canonical table receiving the interval.
    table: String,
    /// Shallow live batches retained by this interval.
    batches: Vec<RecordBatch>,
    /// Parent reservations retaining every decoded batch.
    reservations: Vec<AccountedMemoryReservation>,
}

impl TailFenceDrainer<'_> {
    /// Creates one query-attempt owner for live-tail acquisition and draining.
    #[must_use]
    fn new<'a>(
        tails: &'a TailTransportDirectory,
        memory: &'a OracleMemoryResources,
        telemetry: Arc<OracleTelemetry>,
        query_class: QueryClass,
        deadline: Instant,
        cancellation: CancellationToken,
        freshness: wyrd_spec::vala::api::FreshnessPolicy,
    ) -> TailFenceDrainer<'a> {
        TailFenceDrainer {
            tails,
            memory,
            telemetry,
            query_class,
            deadline,
            cancellation,
            freshness,
        }
    }

    /// Acquires the exact last-sealed-to-live interval for each observed stream.
    ///
    /// Acquisition reads metadata only. Any partial failure releases every
    /// previously acquired fence before returning.
    ///
    /// # Errors
    ///
    /// Returns visibility unavailable for missing transports, invalid
    /// cursor/schema metadata, admission cancellation, expired deadlines, or
    /// Scribe rejection.
    ///
    /// # Cancellation
    ///
    /// Admission cancellation stops outstanding acquisition work and returns
    /// query execution failure after releasing every completed fence.
    #[tracing::instrument(
        name = "bifrost.oracle.tail_fence",
        skip_all,
        fields(table_count = cuts.len())
    )]
    async fn acquire(
        &self,
        cuts: &[PinnedSealedTable],
    ) -> Result<Vec<AcquiredTailFence>, BifrostError> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let wire_deadline = chrono::Utc::now()
            + chrono::Duration::from_std(remaining).map_err(|_| BifrostError::QueryTimeout)?;
        let work = self.plan_acquisitions(cuts, wire_deadline)?;
        let mut results = Vec::with_capacity(work.len());
        let mut pending = FuturesUnordered::new();
        for acquisition in work {
            pending.push(self.acquire_fence(acquisition));
            if pending.len() == 8 {
                while let Some(result) = pending.next().await {
                    results.push(result);
                }
            }
        }
        while let Some(result) = pending.next().await {
            results.push(result);
        }
        let mut acquired = Vec::new();
        let mut failure = None;
        for result in results {
            match result {
                Ok(fence) => acquired.push(fence),
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        if let Some(error) = failure {
            self.release_acquired(acquired).await;
            Err(error)
        } else {
            Ok(acquired)
        }
    }

    /// Releases every completed acquisition before returning from a partial failure.
    async fn release_acquired(&self, acquired: Vec<AcquiredTailFence>) {
        for mut fence in acquired {
            if !self.release_one(&mut fence).await {
                tracing::warn!("Oracle live-tail fence release was not confirmed");
            }
        }
    }

    /// Drains every acquired interval under page and parent-memory bounds.
    ///
    /// Every fence is released before this method returns, including all error
    /// paths. Strict mode fails on the first unavailable source; degraded mode
    /// records the unavailable live tier and retains complete drained sources.
    ///
    /// # Errors
    ///
    /// Returns visibility unavailable for strict drain failure, malformed
    /// cursor progress, deadline expiry, admission cancellation, or
    /// parent-memory exhaustion.
    ///
    /// # Cancellation
    ///
    /// Admission cancellation stops outstanding page reads. Each active drain
    /// releases its fence through the drop guard before returning failure.
    #[tracing::instrument(
        name = "bifrost.oracle.tail",
        skip_all,
        fields(fence_count = fences.len(), freshness = ?self.freshness)
    )]
    async fn drain(&self, fences: Vec<AcquiredTailFence>) -> Result<DrainedTails, BifrostError> {
        let mut drained = DrainedTails::default();
        let results = futures_util::stream::iter(fences)
            .map(|acquired| self.drain_fence(acquired))
            .buffer_unordered(8)
            .collect::<Vec<_>>()
            .await;
        let mut failed_tables = HashSet::new();
        let mut strict_failure = None;
        for result in results {
            match result {
                Ok(interval) => {
                    drained
                        .batches
                        .entry(interval.table)
                        .or_default()
                        .extend(interval.batches);
                    drained.reservations.extend(interval.reservations);
                }
                Err((table, error)) => {
                    failed_tables.insert(table);
                    if strict_failure.is_none() {
                        strict_failure = Some(error);
                    }
                }
            }
        }
        if let Some(error) = strict_failure {
            if error == BifrostError::QueryTimeout
                || self.freshness == wyrd_spec::vala::api::FreshnessPolicy::Strict
            {
                return Err(error);
            }
            drained.degraded = true;
            for table in failed_tables {
                drained.batches.remove(&table);
            }
        }
        Ok(drained)
    }

    /// Plans one exact fence request for every sealed or independently live stream.
    ///
    /// The synchronous stage merges duplicate hot-file cursors, adds streams
    /// discovered without sealed files, and validates all private request
    /// metadata before any transport IO begins.
    ///
    /// # Errors
    ///
    /// Returns visibility unavailable when stream routing, cursor conversion,
    /// event-day parsing, or schema request construction fails.
    fn plan_acquisitions(
        &self,
        cuts: &[PinnedSealedTable],
        wire_deadline: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<TailFenceAcquisition>, BifrostError> {
        let mut work = Vec::new();
        for cut in cuts {
            let table = cut.binding.table_ref.fqn();
            let mut streams = std::collections::BTreeMap::<
                (uuid::Uuid, String, u64),
                (
                    wyrd_spec::vala::api::EventDay,
                    wyrd_spec::vala::api::TailCursor,
                    Arc<dyn TailReadTransport>,
                ),
            >::new();
            for file in &cut.hot_files {
                let event_day = wyrd_spec::vala::api::EventDay::new(
                    file.partition_day.format("%Y-%m-%d").to_string(),
                )
                .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
                let writer_epoch = u64::try_from(file.writer_epoch)
                    .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
                let wal_lsn = u64::try_from(file.wal_lsn_max)
                    .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
                let Some(transport) =
                    self.tails
                        .get_for_stream(&table, file.node_id, cut.binding.tenant)
                else {
                    return Err(BifrostError::QueryVisibilityUnavailable);
                };
                let key = (file.node_id, event_day.as_str().to_owned(), writer_epoch);
                let cursor = wyrd_spec::vala::api::TailCursor {
                    writer_epoch,
                    wal_lsn,
                    batch_id: uuid::Uuid::from_u128(u128::MAX),
                    row_ordinal: u32::MAX,
                };
                streams
                    .entry(key)
                    .and_modify(|(_, current, _)| {
                        if cursor.wal_lsn > current.wal_lsn {
                            *current = cursor.clone();
                        }
                    })
                    .or_insert((event_day, cursor, transport));
            }
            for route in self.tails.live_streams(&table, cut.binding.tenant) {
                let key = (
                    route.node_id,
                    route.event_day.as_str().to_owned(),
                    route.writer_epoch,
                );
                streams.entry(key).or_insert_with(|| {
                    (
                        route.event_day,
                        wyrd_spec::vala::api::TailCursor {
                            writer_epoch: route.writer_epoch,
                            wal_lsn: 0,
                            batch_id: uuid::Uuid::nil(),
                            row_ordinal: 0,
                        },
                        route.transport,
                    )
                });
            }
            if streams.is_empty() {
                return Err(BifrostError::QueryVisibilityUnavailable);
            }
            for (_, (event_day, exclusive_sealed, transport)) in streams {
                let request = tail_fence_request(cut, event_day, exclusive_sealed, wire_deadline)?;
                work.push(TailFenceAcquisition {
                    table: table.clone(),
                    transport,
                    request,
                });
            }
        }
        Ok(work)
    }

    /// Acquires one metadata-only Scribe fence from an owned transport request.
    ///
    /// Keeping the transport owned by this future makes the Oracle query future
    /// transport-safe without exposing a trait-object borrow through Gate's
    /// HTTP/gRPC handler futures.
    ///
    /// # Errors
    ///
    /// Returns visibility unavailable when Scribe rejects the interval, query
    /// timeout when the shared deadline expires, or execution failure when the
    /// admitted query is cancelled.
    ///
    /// # Cancellation
    ///
    /// Admission cancellation wins without waiting for the transport timeout.
    async fn acquire_fence(
        &self,
        acquisition: TailFenceAcquisition,
    ) -> Result<AcquiredTailFence, BifrostError> {
        let TailFenceAcquisition {
            table,
            transport,
            request,
        } = acquisition;
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let acquisition = tokio::select! {
            () = self.cancellation.cancelled() => {
                return Err(BifrostError::QueryExecutionFailed);
            }
            result = tokio::time::timeout(remaining, transport.acquire_fence(request)) => result,
        };
        match acquisition {
            Ok(Ok(fence)) => {
                metrics::counter!(
                    "bifrost_oracle_tail_fences_total",
                    "locality" => "local",
                    "outcome" => "success"
                )
                .increment(1);
                Ok(AcquiredTailFence {
                    table,
                    transport,
                    fence,
                    acquired_at: Instant::now(),
                    drain_succeeded: false,
                })
            }
            Ok(Err(error)) => {
                metrics::counter!(
                    "bifrost_oracle_tail_fences_total",
                    "locality" => "local",
                    "outcome" => "failed"
                )
                .increment(1);
                tracing::warn!(error = %error, "Oracle live-tail fence acquisition failed");
                Err(BifrostError::QueryVisibilityUnavailable)
            }
            Err(_) => {
                metrics::counter!(
                    "bifrost_oracle_tail_fences_total",
                    "locality" => "local",
                    "outcome" => "failed"
                )
                .increment(1);
                Err(BifrostError::QueryTimeout)
            }
        }
    }

    /// Drains one retained fence and releases it on every completion path.
    ///
    /// # Errors
    ///
    /// Returns the canonical table plus timeout, cancellation, or visibility
    /// failure when a page, cursor, or parent-memory bound cannot be satisfied.
    ///
    /// # Cancellation
    ///
    /// Admission cancellation interrupts the active page read. The release
    /// guard then releases the retained Scribe interval before returning.
    async fn drain_fence(
        &self,
        mut acquired: AcquiredTailFence,
    ) -> Result<DrainedTailFence, (String, BifrostError)> {
        let table = acquired.table.clone();
        let mut after = None;
        let mut batches = Vec::new();
        let mut reservations = Vec::new();
        loop {
            let Some(remaining) = self.deadline.checked_duration_since(Instant::now()) else {
                self.release_one(&mut acquired).await;
                return Err((table, BifrostError::QueryTimeout));
            };
            let page_started = Instant::now();
            let page_result = tokio::select! {
                () = self.cancellation.cancelled() => {
                    self.release_one(&mut acquired).await;
                    return Err((table, BifrostError::QueryExecutionFailed));
                }
                result = tokio::time::timeout(
                    remaining,
                    acquired.transport.read_page(wyrd_spec::vala::api::TailPageRequest {
                        fence_id: acquired.fence.fence_id,
                        after: after.clone(),
                        max_rows: 4_096,
                        max_encoded_bytes: 16 * 1024 * 1024,
                    }),
                ) => result,
            };
            let page_outcome = if matches!(&page_result, Ok(Ok(_))) {
                "success"
            } else {
                "failed"
            };
            metrics::counter!(
                "bifrost_oracle_tail_pages_total",
                "locality" => "local",
                "outcome" => page_outcome
            )
            .increment(1);
            metrics::histogram!(
                "bifrost_oracle_tail_page_seconds",
                "locality" => "local",
                "outcome" => page_outcome
            )
            .record(page_started.elapsed().as_secs_f64());
            let page = match page_result {
                Ok(Ok(page)) => page,
                Ok(Err(error)) => {
                    tracing::warn!(error = %error, "Oracle live-tail drain failed");
                    self.release_one(&mut acquired).await;
                    return Err((table, BifrostError::QueryVisibilityUnavailable));
                }
                Err(_) => {
                    self.release_one(&mut acquired).await;
                    return Err((table, BifrostError::QueryTimeout));
                }
            };
            for batch in page.batches {
                let Ok(reservation) = self
                    .memory
                    .governor
                    .try_reserve_parent(batch.get_array_memory_size())
                else {
                    self.release_one(&mut acquired).await;
                    return Err((table, BifrostError::QueryVisibilityUnavailable));
                };
                reservations.push(self.telemetry.account_memory(
                    reservation,
                    self.query_class,
                    OracleMemoryKind::Tail,
                ));
                batches.push(batch.as_ref().clone());
            }
            after = page.next;
            if page.complete {
                break;
            }
            if after.is_none() {
                self.release_one(&mut acquired).await;
                return Err((table, BifrostError::QueryVisibilityUnavailable));
            }
        }
        let released = self.release_one(&mut acquired).await;
        if !released {
            tracing::warn!("Oracle live-tail fence release was not confirmed");
            return Err((table, BifrostError::QueryVisibilityUnavailable));
        }
        acquired.drain_succeeded = true;
        Ok(DrainedTailFence {
            table,
            batches,
            reservations,
        })
    }

    /// Awaits one fence release and updates hold telemetry truthfully.
    async fn release_one(&self, acquired: &mut AcquiredTailFence) -> bool {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_default();
        let released = tokio::time::timeout(
            remaining,
            acquired
                .transport
                .release_fence_async(acquired.fence.fence_id),
        )
        .await
        .is_ok_and(|result| result.is_ok_and(|response| response.released));
        acquired.drain_succeeded = released;
        released
    }
}

impl Drop for AcquiredTailFence {
    /// Records hold duration; abandoned intervals rely on Scribe TTL recovery.
    fn drop(&mut self) {
        let outcome = if self.drain_succeeded {
            "success"
        } else {
            "failed"
        };
        metrics::histogram!(
            "bifrost_oracle_tail_fence_hold_seconds",
            "locality" => "local",
            "outcome" => outcome
        )
        .record(self.acquired_at.elapsed().as_secs_f64());
    }
}

/// Builds one validated metadata-only tail-fence request from a pinned sealed cut.
///
/// # Errors
///
/// Returns visibility unavailable for missing sealed cursor metadata, schema
/// conversion failure, or invalid private wire values.
fn tail_fence_request(
    cut: &PinnedSealedTable,
    event_day: wyrd_spec::vala::api::EventDay,
    exclusive_sealed: wyrd_spec::vala::api::TailCursor,
    deadline: chrono::DateTime<chrono::Utc>,
) -> Result<wyrd_spec::vala::api::AcquireTailFenceRequest, BifrostError> {
    let arrow_schema =
        iceberg::arrow::schema_to_arrow_schema(cut.iceberg_table.metadata().current_schema())
            .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
    let fingerprint = crate::contracts::projected_source_schema_fingerprint(&arrow_schema);
    let fingerprint = wyrd_spec::vala::api::SchemaFingerprint::new(hex::encode(fingerprint.0))
        .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
    Ok(wyrd_spec::vala::api::AcquireTailFenceRequest {
        binding: wyrd_spec::vala::api::TenantTableBinding {
            tenant_id: cut.binding.tenant,
            namespace: cut
                .binding
                .logical_namespace
                .strip_prefix("vala.")
                .ok_or(BifrostError::QueryVisibilityUnavailable)?
                .to_owned(),
            table: cut.binding.table_name.clone(),
        },
        event_day,
        exclusive_sealed,
        deadline,
        schema_fingerprint: fingerprint,
        tail_protocol_version: TAIL_PROTOCOL_VERSION,
    })
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

/// Owned query stream handle.  Frame production remains lazy and cancellation-aware.
pub struct OracleQueryStream {
    /// Stable fingerprint known before the first transport byte is emitted.
    pub schema_fingerprint: String,
    /// Underlying frame stream.
    pub frames: std::pin::Pin<Box<OracleFrameStream>>,
}

impl std::fmt::Debug for OracleQueryStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OracleQueryStream")
            .finish_non_exhaustive()
    }
}

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
    let empty = audit_digest("typed-plan:no-sealed-source-summary")?;
    BifrostQueryReadDecision::try_new(AuditDetail::BifrostQueryReadDecision {
        query_digest: audit_digest(&plan_text)?,
        query_class,
        visibility: options.visibility,
        binding_digests,
        snapshot_digest: empty.clone(),
        manifest_digest: empty,
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
            | LogicalPlan::Sort(_)
            | LogicalPlan::Join(_)
            | LogicalPlan::Repartition(_)
            | LogicalPlan::Union(_)
            | LogicalPlan::Distinct(_)
            | LogicalPlan::RecursiveQuery(_)
    ) {
        return true;
    }
    plan.inputs().into_iter().any(optimized_plan_is_complex)
}

/// Returns the closed production metric label for one admission class.
const fn query_class_label(class: QueryClass) -> &'static str {
    match class {
        QueryClass::Interactive => "interactive",
        QueryClass::Analytical => "analytical",
    }
}

/// Returns the closed production metric label for one visibility mode.
const fn visibility_label(visibility: VisibilityMode) -> &'static str {
    match visibility {
        VisibilityMode::PublishedOnly => "published_only",
        VisibilityMode::Fused => "fused",
    }
}

/// Returns the closed production metric label for one durable admission scope.
const fn admission_scope_label(scope: AdmissionScope) -> &'static str {
    match scope {
        AdmissionScope::Cluster => "cluster",
        AdmissionScope::Class => "class",
        AdmissionScope::Tenant => "tenant",
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
fn admission_limits(usable_slots: u32, class: QueryClass) -> (u32, u32) {
    match class {
        QueryClass::Interactive => ((usable_slots.saturating_mul(80) / 100).max(1), 1),
        QueryClass::Analytical => ((usable_slots.saturating_mul(40) / 100).max(2), 2),
    }
}

/// Maps a pre-stream `DataFusion` failure into the stable public catalog.
fn map_datafusion_error(error: &datafusion::error::DataFusionError) -> BifrostError {
    tracing::error!(error = %error, "Oracle DataFusion operation failed");
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
    metrics::counter!(
        "bifrost_oracle_stale_replans_total",
        "outcome" => "retried"
    )
    .increment(1);
}

/// Complete owned inputs for one terminal-aware query stream.
struct QueryStreamInput {
    /// Public output schema.
    schema: SchemaRef,
    /// Lazy physical batch stream.
    batches: SendableRecordBatchStream,
    /// Pre-byte lookahead result.
    first: Option<Result<RecordBatch, datafusion::error::DataFusionError>>,
    /// Stream-owned durable and local admission capacity.
    admitted: AdmittedQueryGuard,
    /// Absolute execution deadline.
    deadline: Instant,
    /// Requested visibility contract.
    visibility: VisibilityMode,
    /// Immutable admission class.
    query_class: QueryClass,
    /// Whether live visibility degraded.
    degraded: bool,
    /// Whether the one stale-cut replan was consumed.
    stale_replanned: bool,
    /// Production query telemetry retained through terminal emission.
    query_telemetry: QueryTelemetryGuard,
}

/// Builds a terminal-aware stream around one already audited physical stream.
///
/// # Errors
///
/// Returns query execution failure when the output schema cannot be encoded.
fn build_query_stream(input: QueryStreamInput) -> Result<OracleQueryStream, BifrostError> {
    let QueryStreamInput {
        schema,
        batches,
        first,
        admitted,
        deadline,
        visibility,
        query_class,
        degraded,
        stale_replanned,
        mut query_telemetry,
    } = input;
    let schema_frame = encode_schema_frame(&schema)?;
    let schema_fingerprint = schema_frame.schema_fingerprint.clone();
    let lease_cancellation = admitted.cancellation.clone();
    let renewal_terminal = Arc::clone(&admitted.renewal_terminal);
    query_telemetry.start_stream();
    let _stream_span = tracing::info_span!(
        "bifrost.oracle.stream",
        visibility = visibility_label(visibility),
        query_class = query_class_label(query_class)
    );
    let frames = async_stream::stream! {
        let _admitted = admitted;
        let mut batches = batches;
        let mut next = first;
        let mut row_count = 0_u64;
        yield Ok(QueryStreamFrame::Schema(schema_frame));
        loop {
            let event = if let Some(value) = next.take() {
                QueryStreamEvent::Batch(Some(value))
            } else {
                next_query_stream_event(
                    &mut batches,
                    &lease_cancellation,
                    &renewal_terminal,
                    deadline,
                )
                .await
            };
            match event {
                QueryStreamEvent::Batch(Some(Ok(batch))) => {
                    query_telemetry.first_batch();
                    row_count = row_count.saturating_add(batch.num_rows() as u64);
                    match encode_batch_frame(&batch) {
                        Ok(frame) => yield Ok(QueryStreamFrame::Batch(frame)),
                        Err(_) => {
                            query_telemetry.finish("failed", "complete");
                            yield Ok(QueryStreamFrame::Terminal(failed_terminal_for_visibility(
                                QueryTerminalErrorCode::QueryExecutionFailed,
                                row_count,
                                visibility,
                            )));
                            return;
                        }
                    }
                }
                QueryStreamEvent::Batch(Some(Err(error))) => {
                    let code = terminal_error_code(&error);
                    tracing::error!(
                        error = %error,
                        error_code = terminal_error_label(code),
                        "Oracle query stream execution failed"
                    );
                    query_telemetry.finish("failed", "complete");
                    yield Ok(QueryStreamFrame::Terminal(failed_terminal_for_visibility(
                        code,
                        row_count,
                        visibility,
                    )));
                    return;
                }
                QueryStreamEvent::Batch(None) => break,
                QueryStreamEvent::Failed(code) => {
                    query_telemetry.finish("failed", "complete");
                    yield Ok(QueryStreamFrame::Terminal(failed_terminal_for_visibility(
                        code, row_count, visibility,
                    )));
                    return;
                }
            }
        }
        let terminal = successful_terminal(visibility, degraded, stale_replanned, row_count);
        debug_assert!(terminal.validate(visibility).is_ok());
        query_telemetry.finish(
            if degraded { "degraded" } else { "success" },
            if degraded { "degraded" } else { "complete" },
        );
        yield Ok(QueryStreamFrame::Terminal(terminal));
    };
    Ok(OracleQueryStream {
        schema_fingerprint,
        frames: Box::pin(frames),
    })
}

/// One cancellation-, timeout-, or batch-aware stream step.
enum QueryStreamEvent {
    /// Next physical batch result, or `None` when execution completed.
    Batch(Option<Result<RecordBatch, datafusion::error::DataFusionError>>),
    /// Stable terminal failure selected before another batch is exposed.
    Failed(QueryTerminalErrorCode),
}

/// Waits for the next physical batch while enforcing lease cancellation and deadline.
async fn next_query_stream_event(
    batches: &mut SendableRecordBatchStream,
    cancellation: &CancellationToken,
    renewal_terminal: &Mutex<Option<QueryTerminalErrorCode>>,
    deadline: Instant,
) -> QueryStreamEvent {
    if cancellation.is_cancelled() {
        return QueryStreamEvent::Failed(renewal_terminal_code(renewal_terminal));
    }
    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
        return QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryTimeout);
    };
    tokio::select! {
        () = cancellation.cancelled() => {
            QueryStreamEvent::Failed(renewal_terminal_code(renewal_terminal))
        }
        () = tokio::time::sleep(remaining) => {
            QueryStreamEvent::Failed(QueryTerminalErrorCode::QueryTimeout)
        }
        value = batches.next() => QueryStreamEvent::Batch(value),
    }
}

/// Reads the renewal-selected terminal code without exposing lock poisoning.
fn renewal_terminal_code(
    renewal_terminal: &Mutex<Option<QueryTerminalErrorCode>>,
) -> QueryTerminalErrorCode {
    renewal_terminal
        .lock()
        .ok()
        .and_then(|reason| *reason)
        .unwrap_or(QueryTerminalErrorCode::QueryExecutionFailed)
}

/// Constructs the validated success/degraded terminal for one completed stream.
fn successful_terminal(
    visibility: VisibilityMode,
    degraded: bool,
    stale_replanned: bool,
    row_count: u64,
) -> QueryTerminalFrame {
    let freshness = if degraded {
        QueryFreshness::Degraded
    } else {
        QueryFreshness::Complete
    };
    let outcome = if degraded {
        QueryTerminalOutcome::Degraded
    } else {
        QueryTerminalOutcome::Success
    };
    let mut warnings = Vec::new();
    if degraded {
        warnings.push(wyrd_spec::vala::api::QueryWarning::LiveTailUnavailable);
    }
    if stale_replanned {
        warnings.push(wyrd_spec::vala::api::QueryWarning::StaleCutReplanned);
    }
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
            outcome: if degraded {
                SourceCompletionOutcome::Unavailable
            } else {
                SourceCompletionOutcome::Complete
            },
        });
    }
    QueryTerminalFrame {
        outcome,
        freshness,
        row_count,
        warnings,
        source_completion,
        error: None,
    }
}

/// Encodes one Arrow schema frame and its stable fingerprint.
///
/// # Errors
///
/// Returns query execution failure when Arrow IPC rejects the schema.
fn encode_schema_frame(schema: &SchemaRef) -> Result<QuerySchemaFrame, BifrostError> {
    let mut bytes = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, schema)
        .map_err(|_| BifrostError::QueryExecutionFailed)?;
    writer
        .finish()
        .map_err(|_| BifrostError::QueryExecutionFailed)?;
    Ok(QuerySchemaFrame {
        schema_fingerprint: hex::encode(SchemaFingerprint::from_arrow_schema(schema).0),
        arrow_ipc_schema: bytes,
    })
}

/// Encodes one bounded Arrow record batch frame.
///
/// # Errors
///
/// Returns query execution failure when Arrow IPC rejects the batch.
fn encode_batch_frame(batch: &RecordBatch) -> Result<QueryBatchFrame, BifrostError> {
    let mut bytes = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, &batch.schema())
        .map_err(|_| BifrostError::QueryExecutionFailed)?;
    writer
        .write(batch)
        .and_then(|()| writer.finish())
        .map_err(|_| BifrostError::QueryExecutionFailed)?;
    Ok(QueryBatchFrame {
        arrow_ipc_batch: bytes,
    })
}

/// Maps a late `DataFusion` failure to the closed terminal-code catalog.
fn terminal_error_code(error: &datafusion::error::DataFusionError) -> QueryTerminalErrorCode {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("tenant invariant") {
        QueryTerminalErrorCode::QueryTenantInvariant
    } else if message.contains("reconciliation invariant") {
        QueryTerminalErrorCode::QueryReconciliationInvariant
    } else if message.contains("audit unavailable") {
        QueryTerminalErrorCode::QueryAuditUnavailable
    } else {
        QueryTerminalErrorCode::QueryExecutionFailed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{StringArray, UInt64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::atomic::AtomicUsize;

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

    /// Class ceilings remain independent and analytical work is never downgraded.
    #[test]
    fn admission_class_ceiling_and_demand_are_locked() {
        assert_eq!(admission_limits(10, QueryClass::Interactive), (8, 1));
        assert_eq!(admission_limits(10, QueryClass::Analytical), (4, 2));
        assert_eq!(admission_limits(1, QueryClass::Analytical), (2, 2));
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
        let observed = snapshot.counters.iter().any(|(series, value)| {
            series.starts_with("bifrost_oracle_streams_total{")
                && series.contains("freshness=\"complete\"")
                && series.contains("outcome=\"failed\"")
                && *value == 1
        });
        assert!(
            observed,
            "canonical stream metric was not recorded: {snapshot:?}"
        );
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

    /// Tenant repair insertion coalesces duplicates and saturates at 64 scopes.
    #[test]
    fn oracle_tenant_reconcile_queue_coalesces_and_saturates_without_io() {
        let mut queue = TenantReconcileQueue::default();
        let first = DataTenantId::new_v7();
        assert_eq!(
            queue.enqueue(first, QueryClass::Interactive),
            TenantReconcileInsert::Queued
        );
        assert_eq!(
            queue.enqueue(first, QueryClass::Interactive),
            TenantReconcileInsert::Coalesced
        );
        for _ in 1..64 {
            assert_eq!(
                queue.enqueue(DataTenantId::new_v7(), QueryClass::Interactive),
                TenantReconcileInsert::Queued
            );
        }
        assert_eq!(queue.order.len(), 64);
        assert_eq!(queue.members.len(), 64);
        assert_eq!(
            queue.enqueue(DataTenantId::new_v7(), QueryClass::Analytical),
            TenantReconcileInsert::Saturated
        );
    }

    /// Failed maintenance restores FIFO order and success releases membership.
    #[test]
    fn oracle_tenant_reconcile_queue_requeues_failed_batch_in_order() {
        let mut queue = TenantReconcileQueue::default();
        let tenants = [DataTenantId::new_v7(), DataTenantId::new_v7()];
        for tenant in tenants {
            assert_eq!(
                queue.enqueue(tenant, QueryClass::Analytical),
                TenantReconcileInsert::Queued
            );
        }
        let scopes = queue.take(64);
        assert!(queue.order.is_empty());
        assert_eq!(queue.members.len(), 2);
        queue.requeue(&scopes);
        assert_eq!(queue.take(64), scopes);
        queue.complete(&scopes);
        assert!(queue.members.is_empty());
        assert_eq!(
            queue.enqueue(tenants[0], QueryClass::Analytical),
            TenantReconcileInsert::Queued
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
        let mut attempts: Vec<SealedFragmentFuture<'_>> = Vec::new();
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
        let error = dispatch_fragment_attempts(attempts, 2, &siblings, &mut output)
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
        let error = dispatch_fragment_attempts(vec![first, sibling], 2, &siblings, &mut output)
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
