//! `DataFusion` physical sources and invariant operators owned by Oracle.
//!
//! Every table enters `DataFusion` through one disjoint source union. Tenant
//! validation surrounds that union so a foreign row cannot influence a filter,
//! join, aggregate, or limit.

use std::any::Any;
use std::fmt;
#[cfg(test)]
use std::fs::File;
#[cfg(test)]
use std::future::Future;
use std::ops::Range;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use arrow::array::Array;
use arrow::compute::cast;
#[cfg(test)]
use arrow::datatypes::{DataType, Field};
use arrow::datatypes::{Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::datasource::physical_plan::FileScanConfig;
use datafusion::datasource::source::DataSourceExec;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::TaskContext;
use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool, MemoryReservation};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_expr::EquivalenceProperties;
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_plan::execution_plan::{
    Boundedness, EmissionType, PlanProperties, SchedulingType,
};
use datafusion::physical_plan::metrics::{ExecutionPlanMetricsSet, MetricValue};
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::union::UnionExec;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, Partitioning,
    SendableRecordBatchStream,
};
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use futures_util::{Stream, StreamExt, TryStreamExt};
use iceberg::arrow::ScanMetrics;
use iceberg::expr::{Bind, BoundPredicate, Predicate};
use iceberg::io::{FileIO, FileRead};
use iceberg::scan::FileScanTask;
use iceberg_datafusion::IcebergStaticTableProvider;
use iceberg_datafusion::physical_plan::IcebergTableScan;
use parquet::arrow::arrow_reader::ArrowReaderOptions;
use parquet::arrow::async_reader::{AsyncFileReader, ParquetRecordBatchStreamBuilder};
use parquet::errors::ParquetError;
use parquet::file::metadata::{ParquetMetaData, ParquetMetaDataReader};
use tracing::Instrument;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::WorkerFooter;
use wyrd_spec::vala::api::{
    BifrostSecurityPhase, BifrostSecurityViolationKind, ClusterRole, FollowerScanAssignment,
    QueryClass, QuerySource, TenantTableBinding,
};
use wyrd_spec::vala::managed_columns::DATA_TENANT_ID;

use super::{
    AccountedMemoryReservation, AuthorizedQueryContext, BifrostSecurityViolation, OracleAudit,
    OracleMemoryKind, OracleMemoryResources, OracleTelemetry, VerifiedSecurityContext,
};

#[cfg(feature = "test-support")]
static REMOTE_PARTITION_ATTEMPTS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Resets the production partition-open counter for one isolated journey.
#[cfg(feature = "test-support")]
pub fn reset_remote_partition_attempts_for_test() {
    REMOTE_PARTITION_ATTEMPTS.store(0, std::sync::atomic::Ordering::SeqCst);
}

/// Returns authenticated production partition opens since the last reset.
#[cfg(feature = "test-support")]
#[must_use]
pub fn remote_partition_attempts_for_test() -> u64 {
    REMOTE_PARTITION_ATTEMPTS.load(std::sync::atomic::Ordering::SeqCst)
}

/// Immutable specialization for one output partition of a distributed scan.
#[derive(Debug, Clone)]
pub(crate) struct RemotePartitionDescriptor {
    /// Exact authenticated participant selected by the immutable query cut.
    pub(crate) candidate: super::dispatcher::DispatchCandidate,
    /// Role-local sources visible to this participant and no other.
    pub(crate) assignments: Vec<FollowerScanAssignment>,
    /// Closed public sources represented by the assignment.
    pub(crate) sources: Vec<QuerySource>,
}

/// Closed partition-local completion selected before aggregate terminal precedence.
#[derive(Debug)]
pub(crate) enum PartitionDisposition {
    /// Pure Oracle partition with no assigned persisted files.
    Empty { ordinal: u32 },
    /// Footer-validated normal completion.
    Complete {
        ordinal: u32,
        batches: Vec<RecordBatch>,
        footer: WorkerFooter,
    },
    /// Partition-local partial retaining every batch decoded before the condition.
    Degraded {
        ordinal: u32,
        decoded: Vec<RecordBatch>,
        reason: PartitionPartialReason,
    },
    /// Fatal authenticated or protocol failure.
    Failed { ordinal: u32, error: BifrostError },
}

/// Closed stable reasons recorded for a degraded partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartitionPartialReason {
    /// Request/channel/ticket construction or open failed before frames.
    Setup,
    /// Authenticated follower deadline or cancellation status.
    Timeout,
    /// Authenticated pinned object disappeared.
    FileNotFound,
    /// Delivered decoder/frame stream ended after retaining prior batches.
    Decoder,
}

/// Network-backed physical node whose output partitions are polled by `DataFusion`.
pub(crate) struct RemoteScanExec {
    /// Schema shared by the common follower plan and every partition stream.
    schema: SchemaRef,
    /// Cached properties exposing exactly one output partition per selected participant.
    properties: Arc<PlanProperties>,
    /// Server-owned authenticated dispatch capability.
    dispatcher: Arc<super::dispatcher::FragmentDispatcher>,
    /// Immutable query, authorization, cancellation, and deadline projection.
    context: super::dispatcher::DispatchContext,
    /// Query-scoped join owner shared by every output partition.
    settlement: Arc<super::admission::DistributedQuerySettlement>,
    /// Common physical plan bytes serialized once for all participants.
    physical_plan_bytes: Arc<[u8]>,
    /// Tenant-qualified binding shared by the common plan.
    binding: TenantTableBinding,
    /// Fingerprint of the common physical plan bytes.
    plan_fingerprint: String,
    /// Absolute request deadline projected into every signed partition request.
    deadline_unix_ms: i64,
    /// Stable per-participant specializations in cut order.
    partitions: Vec<RemotePartitionDescriptor>,
    /// Public freshness rule applied only after exact partition classification.
    freshness: wyrd_spec::vala::api::FreshnessPolicy,
    /// Aggregate-visible ordered degraded source accumulator.
    degraded_sources: super::DegradedSourceAccumulator,
    /// Query-scoped accumulator for participant-reported physical scan volume.
    scan_metrics: Arc<RemoteScanMetrics>,
    /// Test-tier barrier attached to the actual lazy partition-open boundary.
    #[cfg(feature = "test-support")]
    topology_probe: Option<Arc<super::OracleTopologyProbe>>,
}

impl std::fmt::Debug for RemoteScanExec {
    /// Redacts dispatch authority while preserving physical-plan shape.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteScanExec")
            .field("plan_fingerprint", &self.plan_fingerprint)
            .field("partitions", &self.partitions.len())
            .finish_non_exhaustive()
    }
}

/// Complete construction inputs for one distributed [`RemoteScanExec`].
///
/// Every field is fixed once at plan time and immutable for the life of the
/// scan, so they are supplied as one immutable value rather than as eleven
/// positional parameters whose order carries no meaning.
pub(crate) struct RemoteScanConfig {
    /// Output schema common to every participant fragment.
    pub(crate) schema: SchemaRef,
    /// Shared dispatcher that opens authenticated peer streams.
    pub(crate) dispatcher: Arc<super::dispatcher::FragmentDispatcher>,
    /// Signed dispatch facts replayed identically to every participant.
    pub(crate) context: super::dispatcher::DispatchContext,
    /// Query-scoped settlement registry joined at cancellation.
    pub(super) settlement: Arc<super::admission::DistributedQuerySettlement>,
    /// Fragment plan serialized exactly once for byte-identical delivery.
    pub(crate) physical_plan_bytes: Vec<u8>,
    /// Tenant and table this scan is authorized against.
    pub(crate) binding: TenantTableBinding,
    /// Sealed fragment fingerprint validated by each follower.
    pub(crate) plan_fingerprint: String,
    /// One absolute deadline captured at ingress and propagated unchanged.
    pub(crate) deadline_unix_ms: i64,
    /// Stable per-participant partition descriptors in assignment order.
    pub(crate) partitions: Vec<RemotePartitionDescriptor>,
    /// Caller-selected source-loss policy retained through dispatch.
    pub(crate) freshness: wyrd_spec::vala::api::FreshnessPolicy,
    /// Shared accumulator recording ordered degradation reasons.
    pub(crate) degraded_sources: super::DegradedSourceAccumulator,
}

impl RemoteScanExec {
    /// Creates one common distributed scan with stable participant specializations.
    #[must_use]
    pub(crate) fn new(config: RemoteScanConfig) -> Self {
        let RemoteScanConfig {
            schema,
            dispatcher,
            context,
            settlement,
            physical_plan_bytes,
            binding,
            plan_fingerprint,
            deadline_unix_ms,
            partitions,
            freshness,
            degraded_sources,
        } = config;
        let properties = Arc::new(PlanProperties::new(
            EquivalenceProperties::new(Arc::clone(&schema)),
            Partitioning::UnknownPartitioning(partitions.len()),
            EmissionType::Incremental,
            Boundedness::Bounded,
        ));
        Self {
            schema,
            properties,
            dispatcher,
            context,
            settlement,
            scan_metrics: Arc::new(RemoteScanMetrics::default()),
            physical_plan_bytes: physical_plan_bytes.into(),
            binding,
            plan_fingerprint,
            deadline_unix_ms,
            partitions,
            freshness,
            degraded_sources,
            #[cfg(feature = "test-support")]
            topology_probe: None,
        }
    }

    /// Installs the production-path partition-open probe used by concurrency journeys.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub(crate) fn with_topology_probe(
        mut self,
        probe: Option<Arc<super::OracleTopologyProbe>>,
    ) -> Self {
        self.topology_probe = probe;
        self
    }

    /// Opens and decodes exactly one authenticated peer stream for a partition.
    async fn execute_remote_partition(
        self: Arc<Self>,
        partition: usize,
        _partition_guard: super::admission::DistributedPartitionGuard,
    ) -> DataFusionResult<PartitionDisposition> {
        let ordinal = u32::try_from(partition).unwrap_or(u32::MAX);
        let descriptor = self.partitions.get(partition).ok_or_else(|| {
            DataFusionError::Execution(format!("distributed scan has no partition {partition}"))
        })?;
        #[cfg(feature = "test-support")]
        if descriptor.candidate.node_id != self.context.leader_node_id
            && let Some(probe) = &self.topology_probe
        {
            probe
                .pause_first_selection(descriptor.candidate.node_id)
                .await;
        }
        if descriptor.candidate.role == ClusterRole::Oracle
            && descriptor
                .assignments
                .iter()
                .all(|assignment| assignment.persisted.files.is_empty())
        {
            return Ok(PartitionDisposition::Empty { ordinal });
        }
        #[cfg(feature = "test-support")]
        REMOTE_PARTITION_ATTEMPTS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let attempt = self
            .dispatcher
            .execute(
                &self.context,
                super::common_physical_fragment(
                    &self.physical_plan_bytes,
                    descriptor.assignments.clone(),
                    &self.binding,
                    descriptor.candidate.role,
                    &self.plan_fingerprint,
                    self.deadline_unix_ms,
                ),
                std::slice::from_ref(&descriptor.candidate),
            )
            .await;
        let disposition =
            classify_partition_attempt(ordinal, partition, attempt, &descriptor.sources);
        // Fold this participant's physical read volume into the query-scoped
        // accumulator. The leader's own plan scans no storage, so without this
        // a distributed query reports no scan evidence at all.
        if let PartitionDisposition::Complete { footer, .. } = &disposition {
            self.scan_metrics.record_footer(footer.scan_stats);
        }
        Ok(disposition)
    }
}

/// Maps one dispatch outcome onto this partition's closed disposition.
///
/// This is the partial/terminal boundary the leader reacts to, and each arm is
/// chosen so a single participant cannot fail a query that the rest of the cut
/// could still answer:
///
/// - A successful attempt whose batches fail to decode degrades with whatever
///   decoded before the failure rather than discarding them.
/// - A terminal dispatch failure is a security or contract violation, so it
///   fails the partition outright.
/// - A stale object means the pinned cut moved and the query must replan, which
///   is a failure rather than a degradation.
/// - A missing file, a partial attempt, and an unavailable, source-loss, or
///   capacity failure all degrade, preserving any batches already delivered.
fn classify_partition_attempt(
    ordinal: u32,
    partition: usize,
    attempt: Result<super::attempt::ValidatedAttempt, super::dispatcher::DispatchError>,
    sources: &[QuerySource],
) -> PartitionDisposition {
    match attempt {
        Ok(attempt) => {
            let footer = attempt.footer.clone();
            let mut batches = Vec::new();
            if super::decode_attempt_batches(attempt, &mut batches).is_err() {
                return PartitionDisposition::Degraded {
                    ordinal,
                    decoded: batches,
                    reason: PartitionPartialReason::Decoder,
                };
            }
            PartitionDisposition::Complete {
                ordinal,
                batches,
                footer,
            }
        }
        Err(super::dispatcher::DispatchError::Terminal) => PartitionDisposition::Failed {
            ordinal,
            error: BifrostError::QueryPeerSecurity,
        },
        // A foreign-tenant row refused by the physical tripwire fails the
        // partition outright rather than degrading it: the refusal is a
        // property of the scanned data, so no other candidate would succeed
        // and a degraded result would silently drop a tenant-isolation breach.
        Err(super::dispatcher::DispatchError::TenantInvariant) => PartitionDisposition::Failed {
            ordinal,
            error: BifrostError::QueryTenantInvariant,
        },
        Err(super::dispatcher::DispatchError::FileNotFound) => PartitionDisposition::Degraded {
            ordinal,
            decoded: Vec::new(),
            reason: PartitionPartialReason::FileNotFound,
        },
        Err(super::dispatcher::DispatchError::StaleObject) => PartitionDisposition::Failed {
            ordinal,
            error: BifrostError::QueryVisibilityUnavailable,
        },
        Err(super::dispatcher::DispatchError::Partial { attempt, reason }) => {
            let mut batches = Vec::new();
            if let Some(attempt) = attempt {
                for bytes in attempt.batches {
                    let Ok(bytes) = bytes else { break };
                    let Ok(reader) = arrow::ipc::reader::StreamReader::try_new(
                        std::io::Cursor::new(bytes),
                        None,
                    ) else {
                        break;
                    };
                    for batch in reader {
                        let Ok(batch) = batch else { break };
                        batches.push(batch);
                    }
                }
            }
            PartitionDisposition::Degraded {
                ordinal,
                decoded: batches,
                reason: match reason {
                    super::dispatcher::DispatchPartialReason::Setup => {
                        PartitionPartialReason::Setup
                    }
                    super::dispatcher::DispatchPartialReason::Timeout => {
                        PartitionPartialReason::Timeout
                    }
                    super::dispatcher::DispatchPartialReason::Decoder => {
                        PartitionPartialReason::Decoder
                    }
                },
            }
        }
        Err(
            error @ (super::dispatcher::DispatchError::Unavailable
            | super::dispatcher::DispatchError::EligibleSourceLoss { .. }
            | super::dispatcher::DispatchError::Capacity),
        ) => {
            tracing::warn!(
                partition,
                ?error,
                ?sources,
                "distributed Oracle partition completed partial"
            );
            PartitionDisposition::Degraded {
                ordinal,
                decoded: Vec::new(),
                reason: PartitionPartialReason::Setup,
            }
        }
    }
}

impl DisplayAs for RemoteScanExec {
    /// Formats only the common plan fingerprint and partition count.
    fn fmt_as(
        &self,
        _format: DisplayFormatType,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(
            formatter,
            "RemoteScanExec: {}, partitions={}",
            self.plan_fingerprint,
            self.partitions.len()
        )
    }
}

impl ExecutionPlan for RemoteScanExec {
    /// Returns the stable physical operator name.
    fn name(&self) -> &'static str {
        "RemoteScanExec"
    }

    /// Exposes the concrete network node to optimizers and proof tests.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Returns exact output partitioning for the immutable participant cut.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    /// The serialized common child is intentionally not a local execution child.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        Vec::new()
    }

    /// Preserves the immutable leaf only when no child is supplied.
    ///
    /// # Errors
    /// Returns a plan error if an optimizer attempts to attach a local child.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        if children.is_empty() {
            Ok(self)
        } else {
            Err(DataFusionError::Plan(
                "RemoteScanExec is a leaf plan".to_owned(),
            ))
        }
    }

    /// Returns one lazy peer stream; `DataFusion` polls sibling partitions concurrently.
    ///
    /// # Errors
    /// Returns a physical error for an invalid partition or fatal peer outcome.
    fn execute(
        &self,
        partition: usize,
        _context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        if partition >= self.partitions.len() {
            return Err(DataFusionError::Execution(format!(
                "distributed scan has no partition {partition}"
            )));
        }
        let schema = Arc::clone(&self.schema);
        let owner = Arc::new(self.clone_for_execution());
        let degraded_sources = Arc::clone(&self.degraded_sources);
        let partition_sources = self.partitions[partition].sources.clone();
        let stream = futures_util::stream::once(async move {
            // Spawn only when DataFusion first polls this partition. Once started, the task is
            // intentionally independent of the returned stream: dropping the aggregate stream
            // cancels polling without dropping the peer future before reservation release. The
            // query settlement owner joins it before releasing parent admission.
            let partition_guard = owner
                .settlement
                .start(&owner.context.cancellation)
                .ok_or_else(|| {
                    DataFusionError::Execution("distributed query is cancelled".to_owned())
                })?;
            let partition_task = tokio::spawn(async move {
                owner
                    .execute_remote_partition(partition, partition_guard)
                    .await
            });
            let disposition = partition_task.await.map_err(|error| {
                DataFusionError::Execution(format!(
                    "distributed Oracle partition task failed to join: {error}"
                ))
            })??;
            match disposition {
                PartitionDisposition::Empty { ordinal } => {
                    let _ = ordinal;
                    Ok(Vec::new())
                }
                PartitionDisposition::Complete {
                    ordinal,
                    batches,
                    footer,
                } => {
                    let _ = (ordinal, footer);
                    Ok(batches)
                }
                PartitionDisposition::Degraded {
                    ordinal,
                    decoded,
                    reason,
                } => {
                    tracing::warn!(ordinal, ?reason, "distributed Oracle partition degraded");
                    if let Ok(mut entries) = degraded_sources.lock() {
                        entries.push(super::DegradedPartition {
                            ordinal,
                            reason: match reason {
                                PartitionPartialReason::Setup => "setup",
                                PartitionPartialReason::Timeout => "timeout",
                                PartitionPartialReason::FileNotFound => "file_not_found",
                                PartitionPartialReason::Decoder => "decoder",
                            },
                            sources: partition_sources,
                        });
                    }
                    Ok(decoded)
                }
                PartitionDisposition::Failed { ordinal, error } => {
                    tracing::error!(ordinal, ?error, "distributed Oracle partition failed");
                    Err(DataFusionError::External(Box::new(error)))
                }
            }
        })
        .map_ok(|batches| futures_util::stream::iter(batches.into_iter().map(Ok)))
        .try_flatten();
        Ok(Box::pin(RecordBatchStreamAdapter::new(schema, stream)))
    }
}

impl RemoteScanExec {
    /// Clones immutable handles for one lazy partition stream.
    fn clone_for_execution(&self) -> Self {
        Self {
            schema: Arc::clone(&self.schema),
            properties: Arc::clone(&self.properties),
            dispatcher: Arc::clone(&self.dispatcher),
            context: self.context.clone(),
            settlement: Arc::clone(&self.settlement),
            // Shared, not fresh: every partition stream folds into the one
            // accumulator the leader's plan node exposes to query telemetry.
            scan_metrics: Arc::clone(&self.scan_metrics),
            physical_plan_bytes: Arc::clone(&self.physical_plan_bytes),
            binding: self.binding.clone(),
            plan_fingerprint: self.plan_fingerprint.clone(),
            deadline_unix_ms: self.deadline_unix_ms,
            partitions: self.partitions.clone(),
            freshness: self.freshness,
            degraded_sources: Arc::clone(&self.degraded_sources),
            #[cfg(feature = "test-support")]
            topology_probe: self.topology_probe.clone(),
        }
    }
}

/// Shared physical scan state retained by one executing source plan.
#[derive(Debug, Default)]
pub(super) struct OracleScanMetricsHandle {
    /// Iceberg's dependency-reported requested-range counter.
    iceberg: Mutex<Option<ScanMetrics>>,
    /// Requested bytes from governed hot Parquet range reads.
    hot_bytes: AtomicU64,
    /// Whether at least one hot object read was requested.
    hot_available: AtomicBool,
    /// Number of Iceberg file tasks delivered to the reader.
    iceberg_files: AtomicU64,
    /// Whether the Iceberg reader received at least one task.
    iceberg_partitions: AtomicU64,
    /// Number of hot objects delivered to the ranged reader before decode.
    hot_files: AtomicU64,
    /// Whether the ranged reader received at least one hot object.
    hot_partitions: AtomicU64,
    /// Row groups retained after closed-predicate statistics pruning.
    row_groups_selected: AtomicU64,
    /// Row groups excluded by closed-predicate statistics pruning.
    row_groups_pruned: AtomicU64,
}

impl OracleScanMetricsHandle {
    /// Stores the dependency counter before its stream is consumed.
    fn set_iceberg_metrics(&self, metrics: ScanMetrics) {
        if let Ok(mut current) = self.iceberg.lock() {
            *current = Some(metrics);
        }
    }

    /// Records one task delivered to the Iceberg reader.
    fn record_iceberg_task(&self) {
        self.iceberg_files.fetch_add(1, Ordering::Relaxed);
        self.iceberg_partitions.store(1, Ordering::Relaxed);
    }

    /// Records one governed range request immediately before storage await.
    pub(super) fn record_hot_range(&self, bytes: usize) {
        self.hot_bytes
            .fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::Relaxed);
        self.hot_available.store(true, Ordering::Release);
    }

    /// Records one hot file delivered to the ranged Parquet reader.
    pub(super) fn record_hot_file(&self) {
        self.hot_files.fetch_add(1, Ordering::Relaxed);
        self.hot_partitions.store(1, Ordering::Relaxed);
    }

    /// Records one file's closed-predicate row-group pruning outcome.
    ///
    /// Called once per opened hot Parquet file, including files whose row
    /// groups were all pruned, so the selected/pruned pair always accounts
    /// for every row group the reader inspected.
    pub(super) fn record_row_groups(&self, selection: &RowGroupSelection) {
        self.row_groups_selected
            .fetch_add(selection.retained.len() as u64, Ordering::Relaxed);
        self.row_groups_pruned
            .fetch_add(selection.pruned, Ordering::Relaxed);
    }

    /// Returns terminal dependency counters without substituting metadata sizes.
    fn terminal_values(&self) -> (Option<u64>, u64, u64) {
        let mut total = 0_u64;
        let mut available = false;
        if let Ok(metrics) = self.iceberg.lock()
            && let Some(metrics) = metrics.as_ref()
        {
            total = total.saturating_add(metrics.bytes_read());
            available = true;
        }
        if self.hot_available.load(Ordering::Acquire) {
            total = total.saturating_add(self.hot_bytes.load(Ordering::Acquire));
            available = true;
        }
        let files = self
            .iceberg_files
            .load(Ordering::Acquire)
            .saturating_add(self.hot_files.load(Ordering::Acquire));
        let partitions = self
            .iceberg_partitions
            .load(Ordering::Acquire)
            .saturating_add(self.hot_partitions.load(Ordering::Acquire));
        (available.then_some(total), files, partitions)
    }

    /// Returns the terminal `(selected, pruned)` row-group counters.
    fn terminal_row_groups(&self) -> (u64, u64) {
        (
            self.row_groups_selected.load(Ordering::Acquire),
            self.row_groups_pruned.load(Ordering::Acquire),
        )
    }
}

/// Records one pinned Iceberg task and returns it without altering its delete
/// metadata, schema, predicate, or byte range.
fn retain_iceberg_task(task: FileScanTask, metrics: &Arc<OracleScanMetricsHandle>) -> FileScanTask {
    metrics.record_iceberg_task();
    task
}

/// Physical scan volume reported by every participant of one distributed query.
///
/// A distributed leader's plan has only remote leaves, so its local scan
/// counters are legitimately empty. Each follower reports what its own scans
/// touched on its footer, and this owner sums those across the participant cut
/// so the leader can emit the same scan families a single-node query emits.
///
/// Byte availability is tracked separately from the byte total because absent
/// is not zero: a participant whose sources report no physical IO must not make
/// the query look like it scanned zero bytes of storage that it did read.
#[derive(Debug, Default)]
pub(crate) struct RemoteScanMetrics {
    /// Summed physical bytes across every participant that reported them.
    bytes_scanned: AtomicU64,
    /// Whether at least one participant reported physical bytes at all.
    bytes_available: AtomicBool,
    /// Summed file count across every completed participant.
    files_scanned: AtomicU64,
    /// Summed file-partition count across every completed participant.
    partitions_scanned: AtomicU64,
    /// Summed retained row groups across every completed participant.
    row_groups_scanned: AtomicU64,
    /// Summed pruned row groups across every completed participant.
    row_groups_pruned: AtomicU64,
}

impl RemoteScanMetrics {
    /// Folds one completed participant's footer evidence into the running total.
    fn record_footer(&self, stats: wyrd_spec::vala::api::WorkerScanStats) {
        if let Some(bytes) = stats.bytes_scanned {
            self.bytes_scanned.fetch_add(bytes, Ordering::Relaxed);
            self.bytes_available.store(true, Ordering::Release);
        }
        self.files_scanned
            .fetch_add(stats.files_scanned, Ordering::Relaxed);
        self.partitions_scanned
            .fetch_add(stats.partitions_scanned, Ordering::Relaxed);
        self.row_groups_scanned
            .fetch_add(stats.row_groups_scanned, Ordering::Relaxed);
        self.row_groups_pruned
            .fetch_add(stats.row_groups_pruned, Ordering::Relaxed);
    }

    /// Returns the aggregated totals, preserving absent-versus-zero bytes.
    fn terminal_values(&self) -> (Option<u64>, u64, u64) {
        let bytes = self
            .bytes_available
            .load(Ordering::Acquire)
            .then(|| self.bytes_scanned.load(Ordering::Acquire));
        (
            bytes,
            self.files_scanned.load(Ordering::Acquire),
            self.partitions_scanned.load(Ordering::Acquire),
        )
    }

    /// Returns the aggregated retained and pruned row-group totals.
    ///
    /// Reported separately from [`Self::terminal_values`] because row-group
    /// evidence exists only for participants running a Parquet leaf; a cut
    /// with no such participant contributes a true zero, not an absence.
    fn terminal_row_groups(&self) -> (u64, u64) {
        (
            self.row_groups_scanned.load(Ordering::Acquire),
            self.row_groups_pruned.load(Ordering::Acquire),
        )
    }
}

/// Terminal physical scan evidence collected from the executed `DataFusion` plan.
///
/// The collector reads the pinned Parquet `bytes_scanned` metric once after
/// execution has reached a terminal frame. It never substitutes Arrow batch
/// memory for physical IO; non-file sources leave physical bytes unavailable.
#[derive(Debug, Clone, Default)]
pub(crate) struct OracleQueryScanStats {
    /// Physical bytes reported by `DataFusion`'s Parquet scan metric.
    pub(crate) physical_bytes_scanned: Option<u64>,
    /// Number of files represented by executed file scan nodes.
    pub(crate) files_scanned: u64,
    /// Number of file partitions represented by executed scan nodes.
    pub(crate) partitions_scanned: u64,
    /// Row groups retained after closed-predicate statistics pruning.
    pub(crate) row_groups_scanned: u64,
    /// Row groups excluded by closed-predicate statistics pruning.
    pub(crate) row_groups_pruned: u64,
    /// Immutable-cut file sizes selected before physical execution.
    pub(crate) logical_bytes_selected: u64,
    /// Shared file-source metric sets retained until terminal stream drain.
    physical_metrics: Vec<ExecutionPlanMetricsSet>,
    /// Shared dependency counters retained until terminal stream drain.
    scan_handles: Vec<Arc<OracleScanMetricsHandle>>,
    /// Participant-reported scan accumulators from every remote scan leaf.
    remote_handles: Vec<Arc<RemoteScanMetrics>>,
    /// Prevents duplicate terminal aggregation when a stream closes twice.
    finalized: bool,
}

impl OracleQueryScanStats {
    /// Walks one final physical plan and snapshots scan metrics and file counts.
    #[must_use]
    pub(crate) fn from_plan(plan: &dyn ExecutionPlan, logical_bytes_selected: u64) -> Self {
        let mut stats = Self {
            logical_bytes_selected,
            ..Self::default()
        };
        Self::visit(plan, &mut stats);
        stats
    }

    /// Reads the pinned `DataFusion` scan metric once after physical execution.
    pub(crate) fn finalize(&mut self) {
        if self.finalized {
            return;
        }
        self.finalized = true;
        let mut total = 0_u64;
        let mut available = false;
        for metrics in &self.physical_metrics {
            let Some(MetricValue::Count { count, .. }) =
                metrics.clone_inner().sum_by_name("bytes_scanned")
            else {
                continue;
            };
            available = true;
            total = total.saturating_add(count.value() as u64);
        }
        if available {
            self.physical_bytes_scanned = Some(total);
        }
        for handle in &self.remote_handles {
            let (bytes, files, partitions) = handle.terminal_values();
            let (selected_groups, pruned_groups) = handle.terminal_row_groups();
            self.files_scanned = self.files_scanned.saturating_add(files);
            self.partitions_scanned = self.partitions_scanned.saturating_add(partitions);
            self.row_groups_scanned = self.row_groups_scanned.saturating_add(selected_groups);
            self.row_groups_pruned = self.row_groups_pruned.saturating_add(pruned_groups);
            if let Some(bytes) = bytes {
                available = true;
                total = total.saturating_add(bytes);
            }
        }
        for handle in &self.scan_handles {
            let (bytes, files, partitions) = handle.terminal_values();
            let (selected_groups, pruned_groups) = handle.terminal_row_groups();
            self.files_scanned = self.files_scanned.saturating_add(files);
            self.partitions_scanned = self.partitions_scanned.saturating_add(partitions);
            self.row_groups_scanned = self.row_groups_scanned.saturating_add(selected_groups);
            self.row_groups_pruned = self.row_groups_pruned.saturating_add(pruned_groups);
            if let Some(bytes) = bytes {
                available = true;
                total = total.saturating_add(bytes);
            }
        }
        if available {
            self.physical_bytes_scanned = Some(total);
        }
    }

    /// Visits each physical node exactly once, accumulating leaf scan evidence.
    fn visit(plan: &dyn ExecutionPlan, stats: &mut Self) {
        if let Some(source) = plan.as_any().downcast_ref::<DataSourceExec>()
            && let Some(config) = source
                .data_source()
                .as_any()
                .downcast_ref::<FileScanConfig>()
        {
            stats.partitions_scanned = stats
                .partitions_scanned
                .saturating_add(config.file_groups.len() as u64);
            stats.files_scanned = stats.files_scanned.saturating_add(
                config
                    .file_groups
                    .iter()
                    .map(|group| group.len() as u64)
                    .sum::<u64>(),
            );
            stats
                .physical_metrics
                .push(config.file_source.metrics().clone());
        }
        if let Some(source) = plan.as_any().downcast_ref::<OracleIcebergScanExec>() {
            stats.scan_handles.push(Arc::clone(&source.metrics));
        }
        if let Some(source) = plan.as_any().downcast_ref::<HotParquetExec>() {
            stats.scan_handles.push(Arc::clone(source.metrics()));
        }
        if let Some(source) = plan.as_any().downcast_ref::<RemoteScanExec>() {
            stats.remote_handles.push(Arc::clone(&source.scan_metrics));
        }
        for child in plan.children() {
            Self::visit(child.as_ref(), stats);
        }
    }
}

/// Wyrd-owned Iceberg scan that retains the dependency's ranged-read metrics.
#[derive(Clone)]
pub(crate) struct OracleIcebergScanExec {
    /// Pinned table snapshot used by the original Iceberg physical plan.
    table: iceberg::table::Table,
    /// Original time-travel snapshot, if one was selected.
    snapshot_id: Option<i64>,
    /// Original projected Iceberg column names.
    projection: Option<Vec<String>>,
    /// Original pushed Iceberg predicate.
    predicates: Option<Predicate>,
    /// Original row limit applied by the Iceberg source.
    limit: Option<usize>,
    /// Exact storage-qualified files authorized for this follower request.
    assigned_files: Option<std::collections::BTreeSet<String>>,
    /// Cached properties copied from the pinned source plan.
    properties: Arc<PlanProperties>,
    /// Shared terminal metric owner retained by query telemetry.
    metrics: Arc<OracleScanMetricsHandle>,
}

/// Domain marker for one authenticated Iceberg object lost after cut selection.
#[derive(Debug, thiserror::Error)]
#[error("authenticated Iceberg object disappeared after cut selection")]
struct OracleIcebergStaleObject {
    /// Exact typed storage cause retained for diagnostics and downcast proof.
    #[source]
    source: Box<dyn std::error::Error + Send + Sync>,
}

/// Returns whether an error chain contains an exact filesystem or `OpenDAL` not-found cause.
///
/// Object-store backends report a vanished object through their own typed
/// error, wrapped an arbitrary number of times by Iceberg, Parquet, and
/// `DataFusion`. Walking the whole chain and downcasting is the only way to
/// separate a pinned object that disappeared after cut selection from a
/// genuine storage outage, which the caller must classify differently.
pub(super) fn error_chain_contains_not_found(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(source) = current {
        if source
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            || source
                .downcast_ref::<opendal::Error>()
                .is_some_and(|error| error.kind() == opendal::ErrorKind::NotFound)
        {
            return true;
        }
        current = source.source();
    }
    false
}

/// Preserves a typed Iceberg-only stale marker without classifying adjacent IO.
pub(super) fn iceberg_datafusion_error<E>(error: E) -> DataFusionError
where
    E: std::error::Error + Send + Sync + 'static,
{
    if error_chain_contains_not_found(&error) {
        DataFusionError::External(Box::new(OracleIcebergStaleObject {
            source: Box::new(error),
        }))
    } else {
        DataFusionError::External(Box::new(error))
    }
}

/// Detects the tenant tripwire's terminal refusal anywhere in an execution
/// error chain.
///
/// [`TenantTripwireExec`] fails a stream with
/// [`BifrostError::QueryTenantInvariant`] the moment a physically scanned row
/// carries a foreign tenant. That refusal is a security outcome, not a
/// transport failure, so every layer that classifies a stream error must
/// recognize it here rather than collapsing it into a retryable class and
/// losing the reason the query was refused.
pub(crate) fn is_tenant_invariant_error(error: &DataFusionError) -> bool {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(source) = current {
        if matches!(
            source.downcast_ref::<BifrostError>(),
            Some(BifrostError::QueryTenantInvariant)
        ) {
            return true;
        }
        current = source.source();
    }
    false
}

/// Detects only the marker emitted by authenticated Iceberg object access.
pub(crate) fn is_stale_iceberg_object_error(error: &DataFusionError) -> bool {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(source) = current {
        if source.downcast_ref::<OracleIcebergStaleObject>().is_some() {
            return true;
        }
        current = source.source();
    }
    false
}

impl fmt::Debug for OracleIcebergScanExec {
    /// Redacts table paths while preserving the source shape for diagnostics.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OracleIcebergScanExec")
            .field("snapshot_id", &self.snapshot_id)
            .field("projection", &self.projection)
            .field("limit", &self.limit)
            .finish_non_exhaustive()
    }
}

impl OracleIcebergScanExec {
    /// Replaces one pinned `IcebergTableScan` without changing its semantics.
    ///
    /// # Errors
    ///
    /// Returns a planning error when the dependency plan cannot be downcast or
    /// its public projection cannot be represented by the adapter.
    pub(crate) fn from_plan(plan: &dyn ExecutionPlan) -> DataFusionResult<Self> {
        let scan = plan
            .as_any()
            .downcast_ref::<IcebergTableScan>()
            .ok_or_else(|| {
                DataFusionError::Plan(
                    "OracleIcebergScanExec requires the pinned IcebergTableScan".to_owned(),
                )
            })?;
        Ok(Self {
            table: scan.table().clone(),
            snapshot_id: scan.snapshot_id(),
            projection: scan.projection().map(ToOwned::to_owned),
            predicates: scan.predicates().cloned(),
            limit: scan.limit(),
            assigned_files: None,
            properties: Arc::clone(plan.properties()),
            metrics: Arc::new(OracleScanMetricsHandle::default()),
        })
    }

    /// Restricts this pinned scan to one exact authenticated follower assignment.
    pub(crate) fn with_assigned_files(
        mut self,
        assigned_files: std::collections::BTreeSet<String>,
    ) -> Self {
        self.assigned_files = Some(assigned_files);
        self
    }

    /// Binds this scan's predicate to the pinned snapshot's schema, for use as
    /// a per-task row filter on a follower assignment.
    ///
    /// The scan builder does this binding itself when it is given a filter;
    /// a follower withholds the filter from the builder (see
    /// [`Self::start_stream`]) and so has to bind it here against the same
    /// schema the builder would have used — the snapshot's, not the table's
    /// current one, so a scan pinned to an older snapshot binds against the
    /// schema that snapshot was written under.
    ///
    /// Returns `None` when there is no predicate to push, and when the table
    /// has no snapshot to bind against — the latter can only happen on an empty
    /// table, where there are no tasks to attach a filter to.
    ///
    /// # Errors
    ///
    /// Returns a typed `DataFusion` error when the snapshot's schema cannot be
    /// resolved, or when the predicate does not bind to it — a predicate naming
    /// a column the snapshot does not have is a contract failure, not an empty
    /// result.
    fn assignment_row_filter(&self) -> DataFusionResult<Option<BoundPredicate>> {
        let Some(predicate) = &self.predicates else {
            return Ok(None);
        };
        let metadata = self.table.metadata();
        let snapshot = match self.snapshot_id {
            Some(snapshot_id) => metadata.snapshot_by_id(snapshot_id),
            None => metadata.current_snapshot(),
        };
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let schema = snapshot
            .schema(metadata)
            .map_err(iceberg_datafusion_error)?;
        predicate
            .bind(schema, true)
            .map(Some)
            .map_err(iceberg_datafusion_error)
    }

    /// Rebuilds the exact pinned Iceberg scan and starts its reader stream.
    ///
    /// Where the predicate is applied depends on who selected the files.
    ///
    /// A leader-local scan owns its own file selection, so the predicate goes
    /// into the scan builder and prunes manifest entries as usual. A follower
    /// assignment does not: the leader already chose this fragment's files and
    /// signed them, and [`Self::with_assigned_files`] is the authorization
    /// boundary for what this process may read. Handing the predicate to
    /// manifest planning there would prune files *out of* the planned set, and
    /// the assignment check below cannot tell a file the predicate excluded
    /// from a file the follower's snapshot no longer has — so a stale plan
    /// would silently return fewer rows instead of failing and replanning.
    ///
    /// The follower therefore plans against the pinned snapshot unfiltered,
    /// which keeps `assigned ⊆ planned` an exact drift check, and attaches the
    /// bound predicate to each retained task instead. Nothing is lost by that:
    /// row-group and page pruning are driven by `FileScanTask::predicate` at
    /// read time, not by manifest planning, and the file-level pruning given up
    /// here was discarded by `with_assigned_files` anyway.
    ///
    /// # Errors
    ///
    /// Returns a typed `DataFusion` error when scan planning, task planning,
    /// predicate binding, reader construction, or object-store reads fail, and
    /// a plan error when an assigned file is absent from the pinned snapshot.
    async fn start_stream(&self) -> DataFusionResult<SendableRecordBatchStream> {
        let mut builder = self.table.scan();
        if let Some(snapshot_id) = self.snapshot_id {
            builder = builder.snapshot_id(snapshot_id);
        }
        builder = match &self.projection {
            Some(columns) => builder.select(columns.iter().cloned()),
            None => builder.select_all(),
        };
        if self.assigned_files.is_none() {
            if let Some(predicate) = &self.predicates {
                builder = builder.with_filter(predicate.clone());
            }
        }
        let scan = builder.build().map_err(iceberg_datafusion_error)?;
        let tasks = scan.plan_files().await.map_err(iceberg_datafusion_error)?;
        let tasks = tasks
            .try_collect::<Vec<_>>()
            .await
            .map_err(iceberg_datafusion_error)?;
        let tasks = if let Some(assigned) = &self.assigned_files {
            let planned = tasks
                .iter()
                .map(|task| task.data_file_path.clone())
                .collect::<std::collections::BTreeSet<_>>();
            if !assigned.is_subset(&planned) {
                return Err(DataFusionError::Plan(
                    "authenticated Oracle assignment differs from planned files".to_owned(),
                ));
            }
            let row_filter = self.assignment_row_filter()?;
            tasks
                .into_iter()
                .filter(|task| assigned.contains(&task.data_file_path))
                .map(|mut task| {
                    task.predicate = row_filter.clone();
                    task
                })
                .collect::<Vec<_>>()
        } else {
            tasks
        };
        let tasks = futures_util::stream::iter(tasks.into_iter().map(Ok));
        let metrics = self
            .table
            .reader_builder()
            .build()
            .read(Box::pin(tasks.map_ok({
                let metrics = Arc::clone(&self.metrics);
                move |task| retain_iceberg_task(task, &metrics)
            })))
            .map_err(iceberg_datafusion_error)?;
        self.metrics.set_iceberg_metrics(metrics.metrics().clone());
        let stream = metrics
            .stream()
            .map(|result| result.map_err(iceberg_datafusion_error));
        let stream: Pin<Box<dyn Stream<Item = DataFusionResult<RecordBatch>> + Send>> =
            if let Some(limit) = self.limit {
                let mut remaining = limit;
                Box::pin(stream.try_filter_map(move |batch| {
                    futures_util::future::ready(if remaining == 0 {
                        Ok(None)
                    } else if batch.num_rows() <= remaining {
                        remaining -= batch.num_rows();
                        Ok(Some(batch))
                    } else {
                        let limited = batch.slice(0, remaining);
                        remaining = 0;
                        Ok(Some(limited))
                    })
                }))
            } else {
                Box::pin(stream)
            };
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.schema(),
            stream,
        )))
    }
}

impl DisplayAs for OracleIcebergScanExec {
    /// Renders the adapter without exposing storage paths or predicates.
    fn fmt_as(
        &self,
        _format: DisplayFormatType,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(formatter, "OracleIcebergScanExec")
    }
}

impl ExecutionPlan for OracleIcebergScanExec {
    /// Returns the stable physical source name.
    fn name(&self) -> &'static str {
        "OracleIcebergScanExec"
    }

    /// Exposes this concrete adapter for terminal metric collection.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// This source has no child plans.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        Vec::new()
    }

    /// Reuses this source only when no children are supplied.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        if children.is_empty() {
            Ok(self)
        } else {
            Err(DataFusionError::Plan(
                "OracleIcebergScanExec is a leaf plan".to_owned(),
            ))
        }
    }

    /// Returns properties copied from the original Iceberg source.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    /// Starts one single-partition Iceberg reader stream.
    ///
    /// # Errors
    ///
    /// Returns a typed `DataFusion` error when scan planning or storage setup
    /// fails, or when a non-zero partition is requested.
    fn execute(
        &self,
        partition: usize,
        _context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(format!(
                "OracleIcebergScanExec has no partition {partition}"
            )));
        }
        let source = self.clone();
        let future = async move { source.start_stream().await };
        let stream = futures_util::stream::once(future).try_flatten();
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.schema(),
            stream,
        )))
    }
}

/// One validated immutable hot-file source selected by the pinned cut.
#[derive(Debug, Clone)]
pub(crate) struct HotFileSource {
    /// Absolute storage path accepted by the pinned table's `FileIO`.
    pub(crate) location: String,
    /// Manifest size used to reserve parent memory before whole-file decode.
    pub(crate) size_bytes: usize,
}

/// Complete immutable inputs for constructing one authenticated table provider.
pub(crate) struct OracleTableInputs {
    /// Pinned Iceberg table for the sealed cut.
    pub(crate) table: iceberg::table::Table,
    /// Footer-validated distributed Iceberg batches, when peer dispatch was selected.
    pub(crate) distributed_iceberg_batches: Option<Vec<RecordBatch>>,
    /// Leader-local hot files absent from the pinned Iceberg snapshot.
    pub(crate) hot_files: Vec<HotFileSource>,
    /// Footer-validated distributed hot batches.
    pub(crate) distributed_hot_batches: Vec<RecordBatch>,
    /// Drained live batches for the same table cut.
    pub(crate) live_batches: Vec<RecordBatch>,
    /// Authenticated request context retained by the tenant tripwire.
    pub(crate) context: AuthorizedQueryContext,
    /// Canonical table name used in security diagnostics.
    pub(crate) table_name: String,
    /// Mandatory audit collaborator.
    pub(crate) audit: Arc<dyn OracleAudit>,
    /// Parent-governed source and reconciliation memory.
    pub(crate) memory: OracleMemoryResources,
    /// Query-local pool shared by `DataFusion` and Wyrd-owned source buffers.
    pub(crate) query_pool: Arc<dyn MemoryPool>,
    /// Production memory telemetry owner.
    pub(super) telemetry: Arc<OracleTelemetry>,
    /// Admission class charged by this provider.
    pub(crate) query_class: QueryClass,
}

/// Complete physical provider for one authenticated table visibility cut.
pub(crate) struct OracleTableProvider {
    /// Pinned Iceberg provider built from immutable table metadata.
    iceberg: IcebergStaticTableProvider,
    /// Footer-validated distributed Iceberg batches, or `None` for leader-local scanning.
    distributed_iceberg_batches: Option<Vec<RecordBatch>>,
    /// Pinned hot files absent from the selected Iceberg manifest.
    hot_files: Vec<HotFileSource>,
    /// Fully validated local/peer hot batches admitted only after a matching footer.
    distributed_hot_batches: Vec<RecordBatch>,
    /// Already-audited and drained live batches.
    live_batches: Vec<RecordBatch>,
    /// File reader inherited from the pinned Iceberg table.
    file_io: FileIO,
    /// Full physical schema, including the hidden tenant column.
    physical_schema: SchemaRef,
    /// Caller-visible schema after the tripwire removes its tenant column.
    public_schema: SchemaRef,
    /// Authenticated request context used by the security audit.
    context: AuthorizedQueryContext,
    /// Canonical table name included in scrubbed security diagnostics.
    table: String,
    /// Standard read/security audit collaborator.
    audit: Arc<dyn OracleAudit>,
    /// Parent-governed reconciliation and source memory.
    memory: OracleMemoryResources,
    /// Query-local pool shared by every physical operator and source owner.
    query_pool: Arc<dyn MemoryPool>,
    /// Production memory accounting shared with the retained Oracle.
    telemetry: Arc<OracleTelemetry>,
    /// Immutable admission class charged by this table execution.
    query_class: QueryClass,
    /// Optional persisted-source placeholders used by native follower planning.
    remote_sources: RemotePersistedSources,
}

/// Request-local scan identities replacing persisted source leaves before physical planning.
#[derive(Debug, Clone, Default)]
pub(crate) struct RemotePersistedSources {
    /// Pinned Iceberg source identity, when the cut contains published files.
    pub(crate) iceberg_scan_id: Option<String>,
    /// Pinned hot-Parquet source identity, when the cut contains staged files.
    pub(crate) hot_scan_id: Option<String>,
    /// Disjoint Scribe live-tail identities selected for this table.
    pub(crate) scribe_scan_ids: Vec<String>,
}

impl fmt::Debug for OracleTableProvider {
    /// Redacts audit and storage internals while retaining structural diagnostics.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OracleTableProvider")
            .field("table", &self.table)
            .field(
                "distributed_iceberg_batch_count",
                &self
                    .distributed_iceberg_batches
                    .as_ref()
                    .map_or(0, Vec::len),
            )
            .field("hot_file_count", &self.hot_files.len())
            .field(
                "distributed_hot_batch_count",
                &self.distributed_hot_batches.len(),
            )
            .field("live_batch_count", &self.live_batches.len())
            .finish_non_exhaustive()
    }
}

impl OracleTableProvider {
    /// Splits the caller's filters into the closed predicate vocabulary and the
    /// original expressions that produced it.
    ///
    /// The first element is every closed leaf recovered from filters that
    /// decomposed entirely (see [`classify_filter`]); it drives provider-local
    /// pruning and travels to followers as the signed scan closure. The second
    /// is the subset of the caller's own expressions that classified fully
    /// `Supported`, forwarded verbatim to the Iceberg provider so it can build
    /// Iceberg predicates for manifest and row-group pruning. Pushdown is
    /// reported `Inexact`, so `DataFusion` still applies its residual filter
    /// above this provider in both cases.
    fn closed_pushdown(
        &self,
        filters: &[Expr],
    ) -> (
        Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
        Vec<Expr>,
    ) {
        let mut predicates = Vec::new();
        let mut supported = Vec::new();
        for filter in filters {
            if let FilterClassification::Supported(leaves) = self.classify_filter_for_table(filter)
            {
                predicates.extend(leaves);
                supported.push(filter.clone());
            }
        }
        (predicates, supported)
    }

    /// Builds one remote-source placeholder leaf for a dispatched scan id.
    ///
    /// The placeholder stands in for a subtree the splitter will hand to a
    /// follower. It carries the physical schema fingerprint the follower
    /// revalidates, the session's target partitions so the split boundary
    /// lands on the exchange rather than a repartition, and the closed
    /// predicate/projection closure `execute_distributed_session` later
    /// recovers to overwrite that scan id's safe pre-planning default.
    fn remote_placeholder(
        &self,
        scan_id: &str,
        target_partitions: usize,
        required_columns: &[String],
        predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    ) -> Arc<dyn ExecutionPlan> {
        Arc::new(
            super::codec::RemoteSourcePlaceholderExec::new(
                scan_id.to_owned(),
                super::assignment_schema_fingerprint(self.physical_schema.as_ref()),
                Arc::clone(&self.physical_schema),
            )
            .with_partitions(target_partitions)
            .with_closure(required_columns.to_vec(), predicates.to_vec()),
        )
    }

    /// Converts footer-validated distributed batches into the leader memory source.
    pub(super) fn validated_memory_source(
        batches: &Vec<RecordBatch>,
        schema: SchemaRef,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        Ok(MemorySourceConfig::try_new_exec(
            std::slice::from_ref(batches),
            schema,
            None,
        )?)
    }

    /// Builds one provider from an already pinned sealed cut and drained live rows.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when Iceberg cannot construct its static
    /// provider or the physical schema lacks the required tenant column.
    pub(crate) async fn try_new(inputs: OracleTableInputs) -> DataFusionResult<Self> {
        let OracleTableInputs {
            table,
            distributed_iceberg_batches,
            hot_files,
            distributed_hot_batches,
            live_batches,
            context,
            table_name,
            audit,
            memory,
            query_pool,
            telemetry,
            query_class,
        } = inputs;
        let file_io = table.file_io().clone();
        let iceberg = IcebergStaticTableProvider::try_new_from_table(table)
            .await
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        let physical_schema = iceberg.schema();
        let public_schema = schema_without(&physical_schema, DATA_TENANT_ID)?;
        let distributed_iceberg_batches = distributed_iceberg_batches
            .map(|batches| {
                batches
                    .into_iter()
                    .map(|batch| project_batch(&batch, Arc::clone(&physical_schema)))
                    .collect::<DataFusionResult<Vec<_>>>()
            })
            .transpose()?;
        let distributed_hot_batches = distributed_hot_batches
            .into_iter()
            .map(|batch| project_batch(&batch, Arc::clone(&physical_schema)))
            .collect::<DataFusionResult<Vec<_>>>()?;
        let live_batches = live_batches
            .into_iter()
            .map(|batch| project_batch(&batch, Arc::clone(&physical_schema)))
            .collect::<DataFusionResult<Vec<_>>>()?;
        Ok(Self {
            iceberg,
            distributed_iceberg_batches,
            hot_files,
            distributed_hot_batches,
            live_batches,
            file_io,
            physical_schema,
            public_schema,
            context,
            table: table_name,
            audit,
            memory,
            query_pool,
            telemetry,
            query_class,
            remote_sources: RemotePersistedSources::default(),
        })
    }

    /// Builds a provider whose persisted leaves are native remote-scan placeholders.
    ///
    /// # Errors
    /// Returns the same schema and pinned-provider errors as [`Self::try_new`].
    pub(crate) async fn try_new_distributed(
        inputs: OracleTableInputs,
        remote_sources: RemotePersistedSources,
    ) -> DataFusionResult<Self> {
        let mut provider = Self::try_new(inputs).await?;
        provider.remote_sources = remote_sources;
        Ok(provider)
    }
}

impl OracleTableProvider {
    /// Classifies one `DataFusion` filter for pushdown against this table.
    ///
    /// Wraps [`classify_filter`] with the schema check the closed subset
    /// depends on: every leaf column must resolve by name against the complete
    /// physical schema. A filter naming a column this table does not have is
    /// `Unsupported`, so a qualified reference belonging to another relation
    /// can never be pruned against this source.
    fn classify_filter_for_table(&self, filter: &Expr) -> FilterClassification {
        classify_filter_for_schema(&self.physical_schema, filter)
    }
}

/// Classifies one `DataFusion` filter for pushdown against `physical_schema`.
///
/// [`classify_filter`] alone decides only whether the expression shape is in
/// the closed subset. It accepts a column reference whether or not
/// `DataFusion` qualified it, because a planned query always qualifies column
/// references against the registered relation and rejecting qualified
/// references would make pushdown unreachable in practice. The ownership
/// check therefore happens here instead: every leaf column must resolve by
/// name against the complete physical schema, so a reference belonging to a
/// different relation makes the whole filter `Unsupported` and can never
/// prune this source.
fn classify_filter_for_schema(physical_schema: &Schema, filter: &Expr) -> FilterClassification {
    match classify_filter(filter) {
        FilterClassification::Supported(leaves) => {
            if leaves
                .iter()
                .all(|leaf| physical_schema.column_with_name(leaf.column()).is_some())
            {
                FilterClassification::Supported(leaves)
            } else {
                FilterClassification::Unsupported
            }
        }
        FilterClassification::Unsupported => FilterClassification::Unsupported,
    }
}

#[async_trait]
impl TableProvider for OracleTableProvider {
    /// Exposes this provider for `DataFusion` downcasts.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Returns the caller-visible schema with no tenant selector column.
    fn schema(&self) -> SchemaRef {
        Arc::clone(&self.public_schema)
    }

    /// Oracle tables are durable base tables.
    fn table_type(&self) -> TableType {
        TableType::Base
    }

    /// Reports `Inexact` for every filter that decomposes entirely into the
    /// closed predicate vocabulary (see [`classify_filter`]) and
    /// `Unsupported` for anything else. `Inexact` keeps `DataFusion`'s own
    /// residual filter above this provider even though the provider also
    /// applies the same closed predicates locally.
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        Ok(filters
            .iter()
            .map(|filter| match self.classify_filter_for_table(filter) {
                FilterClassification::Supported(_) => TableProviderFilterPushDown::Inexact,
                FilterClassification::Unsupported => TableProviderFilterPushDown::Unsupported,
            })
            .collect())
    }

    /// Builds the exact `Iceberg + hot + live -> tripwire` disjoint source.
    ///
    /// Row IO remains lazy in returned execution plans. Iceberg and hot Parquet
    /// bytes are first accessed only when `DataFusion` executes the already
    /// audited plan.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` planning error when a source or projection cannot
    /// be represented with the pinned physical schema.
    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let (supported_predicates, supported_filters) = self.closed_pushdown(filters);
        let required_columns =
            required_columns_closure(&self.public_schema, projection, &supported_predicates);
        let mut inputs: Vec<Arc<dyn ExecutionPlan>> = Vec::new();
        // Every remote source advertises the session's target partitions so the
        // distributed split boundary lands on the exchange above the partial
        // aggregate rather than on a round-robin repartition inserted to
        // parallelize a single-partition leaf.
        let target_partitions = state.config().target_partitions();
        if let Some(scan_id) = &self.remote_sources.iceberg_scan_id {
            inputs.push(self.remote_placeholder(
                scan_id,
                target_partitions,
                &required_columns,
                &supported_predicates,
            ));
        } else if let Some(batches) = &self.distributed_iceberg_batches {
            let published =
                Self::validated_memory_source(batches, Arc::clone(&self.physical_schema))?;
            inputs.push(published);
        } else {
            // `limit` is forwarded only as a per-leaf upper bound; DataFusion's
            // own global limit above this provider remains authoritative.
            let published = self
                .iceberg
                .scan(state, None, &supported_filters, limit)
                .await?;
            let published = Arc::new(OracleIcebergScanExec::from_plan(published.as_ref())?);
            inputs.push(published);
        }
        if let Some(scan_id) = &self.remote_sources.hot_scan_id {
            inputs.push(self.remote_placeholder(
                scan_id,
                target_partitions,
                &required_columns,
                &supported_predicates,
            ));
        } else if !self.hot_files.is_empty() {
            let hot = Arc::new(HotParquetExec::new(
                self.hot_files.clone(),
                self.file_io.clone(),
                Arc::clone(&self.physical_schema),
                HotParquetGovernance::Leader {
                    memory: self.memory.clone(),
                    memory_pool: Arc::clone(&self.query_pool),
                    telemetry: Arc::clone(&self.telemetry),
                    query_class: self.query_class,
                },
                Arc::new(OracleScanMetricsHandle::default()),
                supported_predicates.clone(),
            ));
            inputs.push(hot);
        }
        if !self.distributed_hot_batches.is_empty() {
            let hot = Self::validated_memory_source(
                &self.distributed_hot_batches,
                Arc::clone(&self.physical_schema),
            )?;
            inputs.push(hot);
        }
        for scan_id in &self.remote_sources.scribe_scan_ids {
            inputs.push(self.remote_placeholder(
                scan_id,
                target_partitions,
                &required_columns,
                &supported_predicates,
            ));
        }
        if !self.live_batches.is_empty() {
            let live = Self::validated_memory_source(
                &self.live_batches,
                Arc::clone(&self.physical_schema),
            )?;
            inputs.push(live);
        }
        let union = UnionExec::try_new(inputs)?;
        let tripwire = Arc::new(TenantTripwireExec::new(
            union,
            self.context.clone(),
            self.table.clone(),
            Arc::clone(&self.audit),
        )?);
        // Provider-local filter over the closed predicates. This is a real
        // pruning aid, not a substitute for correctness: pushdown is reported
        // `Inexact`, so DataFusion still applies its own residual copy above
        // this provider regardless of what happens here.
        let physical_predicates = supported_predicates
            .iter()
            .map(|predicate| scan_predicate_physical_expr(predicate, &tripwire.schema()))
            .collect::<DataFusionResult<Vec<_>>>()?;
        let filtered: Arc<dyn ExecutionPlan> =
            match conjoin_physical_predicates(physical_predicates) {
                Some(predicate) => Arc::new(
                    datafusion::physical_plan::filter::FilterExec::try_new(predicate, tripwire)?,
                ),
                None => tripwire,
            };
        project_plan(filtered, projection)
    }
}

/// Runtime tenant assertion surrounding one complete physical table source union.
pub struct TenantTripwireExec {
    /// Tagged `Iceberg + hot + live` source union.
    input: Arc<dyn ExecutionPlan>,
    /// Authenticated query context retained for standard security audit.
    context: AuthorizedQueryContext,
    /// Canonical table identifier with no object path.
    table: String,
    /// Standard audit collaborator used before a mismatch becomes visible.
    audit: Arc<dyn OracleAudit>,
    /// Output properties after the hidden tenant column is stripped.
    properties: Arc<PlanProperties>,
}

impl fmt::Debug for TenantTripwireExec {
    /// Redacts authenticated and audit state from physical-plan diagnostics.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TenantTripwireExec")
            .field("table", &self.table)
            .finish_non_exhaustive()
    }
}

impl TenantTripwireExec {
    /// Creates a tripwire around one complete table source union.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error when the input lacks `data_tenant_id`.
    pub fn new(
        input: Arc<dyn ExecutionPlan>,
        context: AuthorizedQueryContext,
        table: String,
        audit: Arc<dyn OracleAudit>,
    ) -> DataFusionResult<Self> {
        let schema = schema_without(&input.schema(), DATA_TENANT_ID)?;
        let partition_count = input.output_partitioning().partition_count();
        Ok(Self {
            input,
            context,
            table,
            audit,
            properties: plan_properties_with_partitions(schema, partition_count),
        })
    }

    /// Borrows the authenticated context encoded into a follower subtree.
    pub(crate) const fn context(&self) -> &AuthorizedQueryContext {
        &self.context
    }

    /// Borrows the canonical table label encoded into a follower subtree.
    pub(crate) fn table(&self) -> &str {
        &self.table
    }
}

impl DisplayAs for TenantTripwireExec {
    /// Renders the invariant boundary without tenant values.
    fn fmt_as(
        &self,
        _format: DisplayFormatType,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(formatter, "TenantTripwireExec")
    }
}

impl ExecutionPlan for TenantTripwireExec {
    /// Returns the stable physical operator name.
    fn name(&self) -> &'static str {
        "TenantTripwireExec"
    }

    /// Exposes this concrete invariant node for downcasts.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Returns cached bounded plan properties.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    /// Returns the complete source union as the sole child.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![&self.input]
    }

    /// Rebuilds the tripwire around exactly one replacement union.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error unless exactly one child is supplied.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let [input] = children.try_into().map_err(|_| {
            DataFusionError::Plan("TenantTripwireExec requires one child".to_owned())
        })?;
        Ok(Arc::new(Self::new(
            input,
            self.context.clone(),
            self.table.clone(),
            Arc::clone(&self.audit),
        )?))
    }

    /// Validates every row, audits a mismatch fail-closed, then strips tenant.
    ///
    /// No mismatched batch is yielded. If the security audit also fails, the
    /// query still fails closed and the audit failure is surfaced.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error for source failure, malformed
    /// tenant data, mismatch, security-audit failure, or Arrow projection.
    fn execute(
        &self,
        partition: usize,
        task: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        let mut input = self.input.execute(partition, task)?;
        let schema = self.schema();
        let context = self.context.clone();
        let audit = Arc::clone(&self.audit);
        let stream = async_stream::try_stream! {
            while let Some(batch) = input.next().await {
                let batch = batch?;
                if tenant_mismatch_row(&batch, context.data_tenant_id)?.is_some() {
                    metrics::counter!(
                        "bifrost_oracle_security_events_total",
                        "event_class" => "tenant_row"
                    )
                    .increment(1);
                    let audit_span = tracing::info_span!(
                        "bifrost.oracle.audit",
                        audit_kind = "security_violation",
                        event_class = "tenant_row"
                    );
                    let audit_result = audit
                        .append_security_violation(
                            VerifiedSecurityContext {
                                query: context.clone(),
                                query_digest: None,
                            },
                            BifrostSecurityViolation {
                                violation: BifrostSecurityViolationKind::TenantRow,
                                phase: BifrostSecurityPhase::Source,
                            },
                        )
                        .instrument(audit_span)
                        .await;
                    audit_result
                        .map_err(|error| DataFusionError::External(Box::new(error)))?;
                    Err::<(), _>(DataFusionError::External(Box::new(
                        BifrostError::QueryTenantInvariant,
                    )))?;
                }
                yield remove_column(&batch, DATA_TENANT_ID)?;
            }
        };
        Ok(Box::pin(RecordBatchStreamAdapter::new(schema, stream)))
    }
}

/// Bounded lazy source for pinned hot sealed Parquet files.
#[cfg(test)]
type HotReadOverride = Arc<
    dyn Fn(&str, Range<u64>) -> Pin<Box<dyn Future<Output = DataFusionResult<bytes::Bytes>> + Send>>
        + Send
        + Sync,
>;

/// Owner-backed range bytes whose Oracle charge follows every clone and slice.
struct AccountedRangeOwner {
    /// Exact bytes returned by the storage range read.
    bytes: bytes::Bytes,
    /// Oracle child and parent capacity retained through the final byte owner.
    reservation: crate::resources::OracleQueryMemoryReservation,
    /// Canonical Oracle telemetry owner charged for the same byte lifetime.
    telemetry: Arc<OracleTelemetry>,
    /// Immutable query class used for balanced gauge release.
    query_class: QueryClass,
}

impl AccountedRangeOwner {
    /// Couples exact range capacity to the canonical Oracle memory gauges.
    fn new(
        bytes: bytes::Bytes,
        reservation: crate::resources::OracleQueryMemoryReservation,
        telemetry: Arc<OracleTelemetry>,
        query_class: QueryClass,
    ) -> Self {
        telemetry.charge_memory(bytes.len(), query_class, OracleMemoryKind::Source);
        Self {
            bytes,
            reservation,
            telemetry,
            query_class,
        }
    }
}

impl AsRef<[u8]> for AccountedRangeOwner {
    /// Borrows the immutable storage bytes without copying or changing ownership.
    fn as_ref(&self) -> &[u8] {
        debug_assert_eq!(self.bytes.len(), self.reservation.bytes());
        self.bytes.as_ref()
    }
}

impl Drop for AccountedRangeOwner {
    /// Releases gauge accounting before the coupled governor reservation drops.
    fn drop(&mut self) {
        if self
            .telemetry
            .release_memory(self.query_class, OracleMemoryKind::Source, self.bytes.len())
            .is_err()
        {
            self.reservation.poison();
            tracing::error!("Oracle hot-range wrapper cleanup poisoned accounting");
        }
    }
}

/// Closed memory-governance mode owning one hot-Parquet leaf's reservations.
///
/// Both Oracle roles read the same pinned objects with the same reader, pruning
/// and projection; they differ only in which budget the range and decoded-batch
/// bytes are charged to. The leader couples every byte to the Oracle governor
/// and the canonical memory telemetry for its admitted query class; a follower
/// charges the request-local worker pool retained by its lease and publishes no
/// leader-side gauges. Keeping that difference in one closed enum is what lets
/// a single [`HotParquetExec`] serve both roles.
pub(super) enum HotParquetGovernance {
    /// Leader-local execution charged against the Oracle governor and telemetry.
    Leader {
        /// Pod allocator and compatibility handles used by every range owner.
        memory: OracleMemoryResources,
        /// Query-local pool shared with `DataFusion` operators.
        memory_pool: Arc<dyn MemoryPool>,
        /// Canonical Oracle memory telemetry owner.
        telemetry: Arc<OracleTelemetry>,
        /// Immutable admission class charged by source buffers.
        query_class: QueryClass,
    },
    /// Follower execution charged against the retained request-local worker pool.
    Follower {
        /// Request-local pool backed by the retained Oracle worker lease.
        memory_pool: Arc<dyn MemoryPool>,
    },
}

impl Clone for HotParquetGovernance {
    /// Clones the governance handles so each per-file reader owns its own.
    ///
    /// Every field is a shared handle (`Arc`, a narrow capability, or a `Copy`
    /// class label), so a clone shares one budget rather than duplicating it.
    fn clone(&self) -> Self {
        match self {
            Self::Leader {
                memory,
                memory_pool,
                telemetry,
                query_class,
            } => Self::Leader {
                memory: memory.clone(),
                memory_pool: Arc::clone(memory_pool),
                telemetry: Arc::clone(telemetry),
                query_class: *query_class,
            },
            Self::Follower { memory_pool } => Self::Follower {
                memory_pool: Arc::clone(memory_pool),
            },
        }
    }
}

/// Pre-IO range capacity held by whichever governance mode admitted it.
enum HotRangeReservation {
    /// Governor reservation awaiting its telemetry charge on a successful read.
    Leader(crate::resources::OracleQueryMemoryReservation),
    /// Request-local pool reservation released with the returned bytes.
    Follower(MemoryReservation),
}

/// Decoded-batch capacity released once the yielded batch is consumed.
enum HotDecodedReservation {
    /// Governor reservation with its coupled leader gauge charge.
    Leader {
        /// Capacity and gauge charge released when this value drops.
        _reservation: AccountedMemoryReservation,
    },
    /// Request-local pool reservation held for one yielded batch.
    Follower {
        /// Pool capacity released when this value drops.
        _reservation: MemoryReservation,
    },
}

/// Retained range bytes coupled to a request-local pool reservation.
struct PooledRangeOwner {
    /// Immutable bytes returned by the storage range read.
    bytes: bytes::Bytes,
    /// Capacity retained until the final bytes clone or slice drops.
    _reservation: MemoryReservation,
}

impl AsRef<[u8]> for PooledRangeOwner {
    /// Borrows the retained storage bytes without copying them.
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

impl HotParquetGovernance {
    /// Reserves exactly `requested` range bytes before any storage IO runs.
    ///
    /// Pre-reserving the exact length is what makes a refusal happen before the
    /// object is touched rather than after its bytes are already resident.
    ///
    /// # Errors
    /// Returns a Parquet error when the owning budget refuses the request.
    fn reserve_range(&self, requested: usize) -> parquet::errors::Result<HotRangeReservation> {
        match self {
            Self::Leader {
                memory,
                memory_pool,
                ..
            } => memory
                .resources
                .try_split_query_memory(memory_pool, "oracle-hot-range", requested)
                .map(HotRangeReservation::Leader)
                .map_err(|error| ParquetError::External(Box::new(error))),
            Self::Follower { memory_pool } => {
                let reservation =
                    MemoryConsumer::new("oracle-follower-hot-range").register(memory_pool);
                reservation
                    .try_grow(requested)
                    .map_err(|error| ParquetError::General(error.to_string()))?;
                Ok(HotRangeReservation::Follower(reservation))
            }
        }
    }

    /// Couples read range bytes to their reservation for the whole byte lifetime.
    ///
    /// The leader's gauge charge is applied here, after the exact-length read
    /// succeeded, so a failed or abandoned range releases its governor
    /// reservation without ever publishing leader memory.
    fn own_range(
        &self,
        bytes: bytes::Bytes,
        reservation: HotRangeReservation,
    ) -> parquet::errors::Result<bytes::Bytes> {
        match (self, reservation) {
            (
                Self::Leader {
                    telemetry,
                    query_class,
                    ..
                },
                HotRangeReservation::Leader(reservation),
            ) => Ok(bytes::Bytes::from_owner(AccountedRangeOwner::new(
                bytes,
                reservation,
                Arc::clone(telemetry),
                *query_class,
            ))),
            (Self::Follower { .. }, HotRangeReservation::Follower(reservation)) => {
                Ok(bytes::Bytes::from_owner(PooledRangeOwner {
                    bytes,
                    _reservation: reservation,
                }))
            }
            _ => Err(ParquetError::General(
                "hot Parquet range reservation does not match its governance mode".to_owned(),
            )),
        }
    }

    /// Reserves one decoded batch for exactly as long as it is yielded.
    ///
    /// # Errors
    /// Returns a `DataFusion` error when the owning budget refuses the batch.
    fn reserve_decoded(&self, bytes: usize) -> DataFusionResult<HotDecodedReservation> {
        match self {
            Self::Leader {
                memory,
                memory_pool,
                telemetry,
                query_class,
            } => {
                let reservation = memory
                    .resources
                    .try_split_query_memory(memory_pool, "oracle-hot-decoded-batch", bytes)
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                Ok(HotDecodedReservation::Leader {
                    _reservation: telemetry.account_query_memory(
                        reservation,
                        *query_class,
                        OracleMemoryKind::Source,
                    ),
                })
            }
            Self::Follower { memory_pool } => {
                let reservation =
                    MemoryConsumer::new("oracle-follower-hot-decoded").register(memory_pool);
                reservation
                    .try_grow(bytes)
                    .map_err(|error| DataFusionError::ResourcesExhausted(error.to_string()))?;
                Ok(HotDecodedReservation::Follower {
                    _reservation: reservation,
                })
            }
        }
    }
}

/// Iceberg ranged storage adapted to Parquet with pre-IO Oracle accounting.
struct IcebergParquetReader {
    /// Pinned ranged reader for one immutable hot object.
    reader: Box<dyn FileRead>,
    /// Pinned manifest size used to reject invalid ranges.
    size: u64,
    /// Closed governance mode owning every range reservation this reader takes.
    governance: HotParquetGovernance,
    /// Shared physical scan counters retained to terminal query emission.
    metrics: Arc<OracleScanMetricsHandle>,
    /// Deterministic range reader injected only by focused tests.
    #[cfg(test)]
    reader_override: Option<(String, HotReadOverride)>,
}

impl IcebergParquetReader {
    /// Creates a governed reader for one pinned immutable hot object.
    fn new(
        reader: Box<dyn FileRead>,
        size: u64,
        governance: HotParquetGovernance,
        metrics: Arc<OracleScanMetricsHandle>,
    ) -> Self {
        Self {
            reader,
            size,
            governance,
            metrics,
            #[cfg(test)]
            reader_override: None,
        }
    }

    /// Installs a deterministic range source without changing accounting.
    #[cfg(test)]
    fn with_test_reader(mut self, location: String, reader: HotReadOverride) -> Self {
        self.reader_override = Some((location, reader));
        self
    }
}

impl AsyncFileReader for IcebergParquetReader {
    /// Reads one exact range after validating and reserving its full length.
    ///
    /// # Errors
    ///
    /// Returns a Parquet error for invalid bounds, an indivisible request or
    /// aggregate occupancy refusal, storage failure, or a short range result.
    fn get_bytes(
        &mut self,
        range: Range<u64>,
    ) -> BoxFuture<'_, parquet::errors::Result<bytes::Bytes>> {
        async move {
            let requested_u64 = range.end.checked_sub(range.start).ok_or_else(|| {
                ParquetError::General("hot Parquet range start exceeds end".to_owned())
            })?;
            if range.end > self.size {
                return Err(ParquetError::General(
                    "hot Parquet range exceeds pinned object size".to_owned(),
                ));
            }
            let requested = usize::try_from(requested_u64).map_err(|_| {
                ParquetError::General("hot Parquet range length exceeds usize".to_owned())
            })?;
            let reservation = self.governance.reserve_range(requested)?;
            // Recorded before IO, matching `record_hot_file`: every admitted
            // range publishes one observation even when the read then fails or
            // is abandoned, so scan evidence never silently loses an attempt.
            self.metrics.record_hot_range(requested);
            #[cfg(test)]
            let bytes = if let Some((location, reader)) = self.reader_override.as_ref() {
                reader(location, range.clone())
                    .await
                    .map_err(|error| ParquetError::General(error.to_string()))?
            } else {
                self.reader
                    .read(range)
                    .await
                    .map_err(|error| ParquetError::General(error.to_string()))?
            };
            #[cfg(not(test))]
            let bytes = self
                .reader
                .read(range)
                .await
                .map_err(|error| ParquetError::General(error.to_string()))?;
            if bytes.len() != requested {
                return Err(ParquetError::General(format!(
                    "hot Parquet short range: requested {requested} bytes, received {}",
                    bytes.len()
                )));
            }
            self.governance.own_range(bytes, reservation)
        }
        .boxed()
    }

    /// Loads footer and optional page metadata through the same governed ranges.
    ///
    /// # Errors
    ///
    /// Returns a Parquet error when any governed metadata range is refused,
    /// cannot be read exactly, or does not encode valid Parquet metadata.
    fn get_metadata<'a>(
        &'a mut self,
        _options: Option<&'a ArrowReaderOptions>,
    ) -> BoxFuture<'a, parquet::errors::Result<Arc<ParquetMetaData>>> {
        async move {
            let size = self.size;
            let metadata = ParquetMetaDataReader::new()
                .load_and_finish(self, size)
                .await?;
            Ok(Arc::new(metadata))
        }
        .boxed()
    }
}

/// Bounded lazy source for pinned hot sealed Parquet files.
///
/// This is the only hot-Parquet `ExecutionPlan` in Oracle. Leader-local reads
/// and follower reads of an authenticated hot assignment share its reader,
/// row-group pruning, projection, and scan evidence; the two roles differ only
/// through [`HotParquetGovernance`], which owns where every reserved byte is
/// charged. Visible to sibling `oracle` submodules so the follower resolver can
/// build the follower mode and [`OracleQueryScanStats`] can fold its counters.
pub(super) struct HotParquetExec {
    /// Validated immutable manifest entries.
    files: Vec<HotFileSource>,
    /// Pinned Iceberg storage reader.
    file_io: FileIO,
    /// Complete physical table schema.
    schema: SchemaRef,
    /// Closed governance mode owning every reservation this leaf takes.
    governance: HotParquetGovernance,
    /// Shared terminal metric owner retained by query telemetry.
    metrics: Arc<OracleScanMetricsHandle>,
    /// Closed predicate conjunction used to skip a file whose footer
    /// statistics prove no row group can satisfy every leaf.
    predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    /// Deterministic reader injected only by focused unit tests.
    #[cfg(test)]
    reader_override: Option<HotReadOverride>,
    /// Cached bounded leaf properties.
    properties: Arc<PlanProperties>,
}

impl fmt::Debug for HotParquetExec {
    /// Renders bounded source metadata without exposing object paths.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HotParquetExec")
            .field("file_count", &self.files.len())
            .finish_non_exhaustive()
    }
}

impl HotParquetExec {
    /// Creates a single-partition hot-file leaf from validated manifests.
    ///
    /// Callers must have already authenticated the assignment, validated every
    /// object identity and size, and chosen the governance mode that matches
    /// their role; this constructor performs no IO and no authorization.
    pub(super) fn new(
        files: Vec<HotFileSource>,
        file_io: FileIO,
        schema: SchemaRef,
        governance: HotParquetGovernance,
        metrics: Arc<OracleScanMetricsHandle>,
        predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    ) -> Self {
        Self {
            files,
            file_io,
            governance,
            metrics,
            predicates,
            #[cfg(test)]
            reader_override: None,
            properties: plan_properties(Arc::clone(&schema)),
            schema,
        }
    }

    /// Returns the shared terminal metric owner for this hot leaf.
    pub(super) fn metrics(&self) -> &Arc<OracleScanMetricsHandle> {
        &self.metrics
    }

    /// Injects an existing-interface reader only for deterministic unit tests.
    #[cfg(test)]
    fn with_test_reader(mut self, reader: HotReadOverride) -> Self {
        self.reader_override = Some(reader);
        self
    }
}

impl DisplayAs for HotParquetExec {
    /// Renders only bounded file count, never object paths.
    fn fmt_as(
        &self,
        _format: DisplayFormatType,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(formatter, "HotParquetExec files={}", self.files.len())
    }
}

impl ExecutionPlan for HotParquetExec {
    /// Returns the stable physical leaf name.
    fn name(&self) -> &'static str {
        "HotParquetExec"
    }

    /// Exposes this concrete source for downcasts.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Returns cached bounded plan properties.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    /// This source has no child plans.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        Vec::new()
    }

    /// Reuses this leaf only when no children are supplied.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error when a child is attached.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        if children.is_empty() {
            Ok(self)
        } else {
            Err(DataFusionError::Plan(
                "HotParquetExec is a leaf plan".to_owned(),
            ))
        }
    }

    /// Reads validated hot files sequentially through governed Parquet ranges.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error for an invalid partition, storage
    /// failure, Parquet decode failure, or schema mismatch.
    fn execute(
        &self,
        partition: usize,
        task: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(format!(
                "HotParquetExec has no partition {partition}"
            )));
        }
        let schema = Arc::clone(&self.schema);
        // The hot leaf decodes at the admitted session's batch size, so this
        // path is shaped by the same grant as every other operator in the plan
        // rather than by a fixed constant of its own.
        let stream = hot_stream(self, task.session_config().batch_size());
        Ok(Box::pin(RecordBatchStreamAdapter::new(schema, stream)))
    }
}

/// Builds the hot-file stream after partition validation has completed.
///
/// Files are read sequentially. Each one publishes a file observation before
/// its footer is touched, prunes row groups against the closed predicates,
/// decodes at the admitted `batch_size`, projects to the authenticated physical
/// schema, and holds one governed reservation for exactly the lifetime of the
/// yielded batch. Dropping the stream releases every retained reservation,
/// which is what makes cancellation return the query's memory.
fn hot_stream(
    exec: &HotParquetExec,
    batch_size: usize,
) -> impl Stream<Item = DataFusionResult<RecordBatch>> + Send + 'static {
    let files = exec.files.clone();
    let file_io = exec.file_io.clone();
    let schema = Arc::clone(&exec.schema);
    let governance = exec.governance.clone();
    let metrics = Arc::clone(&exec.metrics);
    let predicates = exec.predicates.clone();
    #[cfg(test)]
    let reader_override = exec.reader_override.clone();
    async_stream::try_stream! {
        for file in files {
            let input = file_io
                .new_input(&file.location)
                .map_err(|error| DataFusionError::External(Box::new(error)))?;
            let reader = input
                .reader()
                .await
                .map_err(|error| DataFusionError::External(Box::new(error)))?;
            let size = u64::try_from(file.size_bytes).map_err(|_| {
                DataFusionError::Execution("hot object size exceeds u64".to_owned())
            })?;
            let reader = IcebergParquetReader::new(
                reader,
                size,
                governance.clone(),
                Arc::clone(&metrics),
            );
            #[cfg(test)]
            let reader = if let Some(override_reader) = reader_override.as_ref() {
                reader.with_test_reader(file.location.clone(), Arc::clone(override_reader))
            } else {
                reader
            };
            // Recorded before the footer is read so a file observation exists
            // for every attempt on this file, including one whose reader fails
            // or is abandoned mid-open. Row-group pruning is reported
            // separately, so a file whose groups are all pruned still counts as
            // opened rather than vanishing from the scan accounting.
            metrics.record_hot_file();
            let builder = ParquetRecordBatchStreamBuilder::new(reader)
                .await
                .map_err(|error| DataFusionError::External(Box::new(error)))?;
            let selection = select_row_groups_for_predicates(builder.metadata(), &predicates);
            metrics.record_row_groups(&selection);
            if selection.excludes_file() {
                continue;
            }
            let mut batches = builder
                .with_row_groups(selection.retained)
                .with_batch_size(batch_size)
                .build()
                .map_err(|error| DataFusionError::External(Box::new(error)))?;
            while let Some(decoded) = batches.next().await {
                let batch = decoded
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                let batch = project_batch(&batch, Arc::clone(&schema))?;
                let decoded_reservation =
                    governance.reserve_decoded(batch.get_array_memory_size())?;
                yield batch;
                drop(decoded_reservation);
            }
        }
    }
}

/// Returns the first tenant mismatch row, or `None` for a valid batch.
///
/// # Errors
///
/// Returns a `DataFusion` error when the managed tenant column is absent or has
/// a non-UTF8 physical type.
fn tenant_mismatch_row(
    batch: &RecordBatch,
    tenant: wyrd_spec::DataTenantId,
) -> DataFusionResult<Option<usize>> {
    let index = batch
        .schema()
        .index_of(DATA_TENANT_ID)
        .map_err(|_| DataFusionError::External(Box::new(BifrostError::QueryTenantInvariant)))?;
    let values = batch
        .column(index)
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .ok_or_else(|| DataFusionError::External(Box::new(BifrostError::QueryTenantInvariant)))?;
    let expected = tenant.to_string();
    Ok((0..values.len()).find(|row| values.is_null(*row) || values.value(*row) != expected))
}

/// Result of classifying one `DataFusion` filter expression against the
/// closed predicate pushdown vocabulary.
enum FilterClassification {
    /// The whole expression decomposed into closed leaves; `DataFusion` still
    /// retains its own residual copy because pushdown is reported `Inexact`.
    Supported(Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>),
    /// At least one leaf fell outside the closed subset.
    Unsupported,
}

/// Classifies one filter expression: recursively flattens `AND`, and
/// classifies each leaf as a closed [`ScanPredicate`](wyrd_spec::vala::assignment_authority::ScanPredicate)
/// comparison or null-check. Any `OR`, `NOT`, cast, function call, arithmetic,
/// column-to-column comparison, qualified/unknown column, non-finite float,
/// or unrecognized literal type makes the entire expression `Unsupported` —
/// classification never partially decomposes one filter.
fn classify_filter(expr: &Expr) -> FilterClassification {
    let mut leaves = Vec::new();
    if flatten_supported_conjunction(expr, &mut leaves) {
        FilterClassification::Supported(leaves)
    } else {
        FilterClassification::Unsupported
    }
}

/// Recursively decomposes `AND` conjunctions into closed leaves.
///
/// Returns `false` (leaving `out` in an unspecified partial state that the
/// caller discards) as soon as one leaf is not representable in the closed
/// subset.
fn flatten_supported_conjunction(
    expr: &Expr,
    out: &mut Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
) -> bool {
    match expr {
        Expr::BinaryExpr(binary) if binary.op == datafusion::logical_expr::Operator::And => {
            flatten_supported_conjunction(&binary.left, out)
                && flatten_supported_conjunction(&binary.right, out)
        }
        Expr::BinaryExpr(binary) => {
            let Some(predicate) = classify_comparison(&binary.left, binary.op, &binary.right)
            else {
                return false;
            };
            out.push(predicate);
            true
        }
        Expr::IsNull(inner) => {
            let Some(column) = column_name_for_pushdown(inner) else {
                return false;
            };
            out.push(wyrd_spec::vala::assignment_authority::ScanPredicate::IsNull(column));
            true
        }
        Expr::IsNotNull(inner) => {
            let Some(column) = column_name_for_pushdown(inner) else {
                return false;
            };
            out.push(wyrd_spec::vala::assignment_authority::ScanPredicate::IsNotNull(column));
            true
        }
        _ => false,
    }
}

/// Returns the bare column name of `expr`, or `None` when `expr` is not a
/// column reference.
///
/// A table qualifier is accepted and discarded: `DataFusion` qualifies every
/// column reference against the registered relation, so a filter reaching a
/// registered provider always arrives as `catalog.schema.table.column`.
/// Rejecting qualified references here would make the closed subset
/// unreachable in practice. The bare name is authoritative because the caller
/// resolves it against the complete physical schema before it can prune, and
/// an unresolvable name classifies the whole filter `Unsupported`.
fn column_name_for_pushdown(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Column(column) => Some(column.name.clone()),
        _ => None,
    }
}

/// Classifies one binary comparison as a closed leaf predicate.
///
/// A literal on the left is normalized by reversing the operator so the
/// returned predicate always carries `(column, literal)`. Returns `None` for
/// any operator outside the closed comparison set, a non-finite float
/// literal, a literal type outside the closed [`ScanLiteral`](wyrd_spec::vala::assignment_authority::ScanLiteral)
/// vocabulary, or an operand pair that is not exactly one unqualified column
/// and one closed literal.
fn classify_comparison(
    left: &Expr,
    op: datafusion::logical_expr::Operator,
    right: &Expr,
) -> Option<wyrd_spec::vala::assignment_authority::ScanPredicate> {
    use wyrd_spec::vala::assignment_authority::ScanPredicate;

    let normalized_op = closed_comparison_op(op)?;
    let (column, literal_expr, normalized_op) = match (
        column_name_for_pushdown(left),
        column_name_for_pushdown(right),
    ) {
        (Some(column), None) => (column, right, normalized_op),
        (None, Some(column)) => (column, left, reverse_comparison_op(normalized_op)),
        _ => return None,
    };
    let literal = classify_literal(literal_expr)?;
    Some(match normalized_op {
        ClosedComparisonOp::Eq => ScanPredicate::Eq(column, literal),
        ClosedComparisonOp::NotEq => ScanPredicate::NotEq(column, literal),
        ClosedComparisonOp::Lt => ScanPredicate::Lt(column, literal),
        ClosedComparisonOp::LtEq => ScanPredicate::LtEq(column, literal),
        ClosedComparisonOp::Gt => ScanPredicate::Gt(column, literal),
        ClosedComparisonOp::GtEq => ScanPredicate::GtEq(column, literal),
    })
}

/// Closed comparison operators reachable through predicate pushdown.
#[derive(Clone, Copy)]
enum ClosedComparisonOp {
    /// `=`
    Eq,
    /// `!=`
    NotEq,
    /// `<`
    Lt,
    /// `<=`
    LtEq,
    /// `>`
    Gt,
    /// `>=`
    GtEq,
}

/// Maps a `DataFusion` operator onto the closed comparison set, or `None`
/// when it falls outside `Eq`/`NotEq`/`Lt`/`LtEq`/`Gt`/`GtEq`.
fn closed_comparison_op(op: datafusion::logical_expr::Operator) -> Option<ClosedComparisonOp> {
    use datafusion::logical_expr::Operator;
    match op {
        Operator::Eq => Some(ClosedComparisonOp::Eq),
        Operator::NotEq => Some(ClosedComparisonOp::NotEq),
        Operator::Lt => Some(ClosedComparisonOp::Lt),
        Operator::LtEq => Some(ClosedComparisonOp::LtEq),
        Operator::Gt => Some(ClosedComparisonOp::Gt),
        Operator::GtEq => Some(ClosedComparisonOp::GtEq),
        _ => None,
    }
}

/// Reverses a closed comparison operator, used when the literal appears on
/// the left of the original expression.
fn reverse_comparison_op(op: ClosedComparisonOp) -> ClosedComparisonOp {
    match op {
        ClosedComparisonOp::Eq => ClosedComparisonOp::Eq,
        ClosedComparisonOp::NotEq => ClosedComparisonOp::NotEq,
        ClosedComparisonOp::Lt => ClosedComparisonOp::Gt,
        ClosedComparisonOp::LtEq => ClosedComparisonOp::GtEq,
        ClosedComparisonOp::Gt => ClosedComparisonOp::Lt,
        ClosedComparisonOp::GtEq => ClosedComparisonOp::LtEq,
    }
}

/// Classifies one literal expression into the closed [`ScanLiteral`](wyrd_spec::vala::assignment_authority::ScanLiteral)
/// vocabulary.
///
/// Returns `None` for any `ScalarValue` variant outside `Boolean`/`Int64`/
/// `UInt64`/`Float64`/`Utf8`/`TimestampMicrosecond`, a null literal, or a
/// non-finite `f64`.
fn classify_literal(expr: &Expr) -> Option<wyrd_spec::vala::assignment_authority::ScanLiteral> {
    use datafusion::scalar::ScalarValue;
    use wyrd_spec::vala::assignment_authority::ScanLiteral;

    let Expr::Literal(value, _) = expr else {
        return None;
    };
    match value {
        ScalarValue::Boolean(Some(inner)) => Some(ScanLiteral::Bool(*inner)),
        ScalarValue::Int64(Some(inner)) => Some(ScanLiteral::I64(*inner)),
        ScalarValue::UInt64(Some(inner)) => Some(ScanLiteral::U64(*inner)),
        ScalarValue::Float64(Some(inner)) if inner.is_finite() => {
            Some(ScanLiteral::F64Bits(inner.to_bits()))
        }
        ScalarValue::Utf8(Some(inner)) => Some(ScanLiteral::Utf8(inner.clone())),
        ScalarValue::TimestampMicrosecond(Some(inner), _) => {
            Some(ScanLiteral::TimestampMicros(*inner))
        }
        _ => None,
    }
}

/// Computes the canonical `required_columns` closure: `DataFusion`'s requested
/// scan output (by name, or every public column when `projection` is
/// `None`), followed by the first occurrence of each predicate column in
/// filter order, followed by the always-present hidden tenant column —
/// stably deduplicated. Names resolve against the full physical schema.
fn required_columns_closure(
    public_schema: &Schema,
    projection: Option<&Vec<usize>>,
    predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
) -> Vec<String> {
    let scan_output_names: Vec<String> = match projection {
        Some(projection) => projection
            .iter()
            .filter_map(|index| public_schema.fields().get(*index))
            .map(|field| field.name().clone())
            .collect(),
        None => public_schema
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect(),
    };
    let mut required = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in scan_output_names
        .into_iter()
        .chain(
            predicates
                .iter()
                .map(|predicate| predicate.column().to_string()),
        )
        .chain(std::iter::once(DATA_TENANT_ID.to_string()))
    {
        if seen.insert(name.clone()) {
            required.push(name);
        }
    }
    required
}

/// Builds the physical predicate for one closed comparison/null-check leaf
/// against `schema`.
///
/// # Errors
/// Returns a `DataFusion` plan error when the predicate's column is absent
/// from `schema`.
fn scan_predicate_physical_expr(
    predicate: &wyrd_spec::vala::assignment_authority::ScanPredicate,
    schema: &SchemaRef,
) -> DataFusionResult<Arc<dyn datafusion::physical_expr::PhysicalExpr>> {
    use datafusion::logical_expr::Operator;
    use datafusion::physical_expr::PhysicalExpr;
    use datafusion::physical_expr::expressions::{BinaryExpr, IsNotNullExpr, IsNullExpr, Literal};
    use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

    let column_expr = |name: &str| -> DataFusionResult<Arc<dyn PhysicalExpr>> {
        Ok(Arc::new(Column::new_with_schema(name, schema)?))
    };
    // A timestamp literal adopts the compared column's own timezone. The
    // durable `ScanLiteral` carries microseconds since the epoch and nothing
    // else, so materializing it as a naive instant would make every comparison
    // against a timezone-carrying column — `wyrd_event_time` among them — an
    // Arrow type error at execution rather than a filter.
    let literal_expr = |column: &str, literal: &ScanLiteral| -> Arc<dyn PhysicalExpr> {
        Arc::new(Literal::new(scan_literal_scalar(schema, column, literal)))
    };
    let comparison = |column: &str, op: Operator, literal: &ScanLiteral| {
        Ok(Arc::new(BinaryExpr::new(
            column_expr(column)?,
            op,
            literal_expr(column, literal),
        )) as Arc<dyn PhysicalExpr>)
    };
    match predicate {
        ScanPredicate::Eq(column, literal) => comparison(column, Operator::Eq, literal),
        ScanPredicate::NotEq(column, literal) => comparison(column, Operator::NotEq, literal),
        ScanPredicate::Lt(column, literal) => comparison(column, Operator::Lt, literal),
        ScanPredicate::LtEq(column, literal) => comparison(column, Operator::LtEq, literal),
        ScanPredicate::Gt(column, literal) => comparison(column, Operator::Gt, literal),
        ScanPredicate::GtEq(column, literal) => comparison(column, Operator::GtEq, literal),
        ScanPredicate::IsNull(column) => Ok(Arc::new(IsNullExpr::new(column_expr(column)?))),
        ScanPredicate::IsNotNull(column) => Ok(Arc::new(IsNotNullExpr::new(column_expr(column)?))),
    }
}

/// Materializes one closed [`ScanLiteral`](wyrd_spec::vala::assignment_authority::ScanLiteral)
/// as the Arrow scalar the compared column expects.
///
/// This is the single authority for turning a durable literal into a value,
/// shared by the physical and logical predicate builders so a leaf rebuilt on
/// a follower compares exactly what the leader planned. A timestamp bound is
/// stored as bare microseconds, so the column's own type supplies the
/// timezone; materializing it naive would make every comparison against a
/// timezone-carrying column an Arrow type error instead of a filter.
fn scan_literal_scalar(
    schema: &SchemaRef,
    column: &str,
    literal: &wyrd_spec::vala::assignment_authority::ScanLiteral,
) -> datafusion::scalar::ScalarValue {
    use datafusion::scalar::ScalarValue;
    use wyrd_spec::vala::assignment_authority::ScanLiteral;

    match literal {
        ScanLiteral::Bool(inner) => ScalarValue::Boolean(Some(*inner)),
        ScanLiteral::I64(inner) => ScalarValue::Int64(Some(*inner)),
        ScanLiteral::U64(inner) => ScalarValue::UInt64(Some(*inner)),
        ScanLiteral::F64Bits(inner) => ScalarValue::Float64(Some(f64::from_bits(*inner))),
        ScanLiteral::Utf8(inner) => ScalarValue::Utf8(Some(inner.clone())),
        ScanLiteral::TimestampMicros(inner) => {
            ScalarValue::TimestampMicrosecond(Some(*inner), timestamp_timezone_of(schema, column))
        }
    }
}

/// Rebuilds the logical filter conjunction a signed assignment closure
/// authorizes, against the full physical `schema`.
///
/// A follower resolves its own leaf rather than receiving one, so the closed
/// predicates the leader classified and signed are the only description of
/// what that leaf may skip. Handing them back to a
/// [`TableProvider`](datafusion::datasource::TableProvider) as logical filters
/// is what lets the underlying source prune files and row groups; without it a
/// follower reads its whole assigned cut and leans on the residual filter
/// above the leaf for correctness alone.
///
/// A predicate whose column is absent from `schema` is skipped rather than
/// failing the scan: pushdown is a pruning aid, and the residual filter stays
/// authoritative for correctness.
pub(super) fn scan_predicate_logical_exprs(
    predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    schema: &SchemaRef,
) -> Vec<Expr> {
    use datafusion::logical_expr::{col, lit};
    use wyrd_spec::vala::assignment_authority::ScanPredicate;

    predicates
        .iter()
        .filter(|predicate| schema.field_with_name(predicate.column()).is_ok())
        .map(|predicate| {
            let scalar = |column: &str, literal| lit(scan_literal_scalar(schema, column, literal));
            match predicate {
                ScanPredicate::Eq(column, literal) => col(column).eq(scalar(column, literal)),
                ScanPredicate::NotEq(column, literal) => {
                    col(column).not_eq(scalar(column, literal))
                }
                ScanPredicate::Lt(column, literal) => col(column).lt(scalar(column, literal)),
                ScanPredicate::LtEq(column, literal) => col(column).lt_eq(scalar(column, literal)),
                ScanPredicate::Gt(column, literal) => col(column).gt(scalar(column, literal)),
                ScanPredicate::GtEq(column, literal) => col(column).gt_eq(scalar(column, literal)),
                ScanPredicate::IsNull(column) => col(column).is_null(),
                ScanPredicate::IsNotNull(column) => col(column).is_not_null(),
            }
        })
        .collect()
}

/// Returns the timezone of one microsecond-timestamp column, or `None` when
/// the column is absent or is not a timezone-carrying timestamp.
///
/// The closed predicate vocabulary stores a timestamp bound as bare
/// microseconds, so the compared column is the only authority on whether that
/// instant is timezone-aware.
fn timestamp_timezone_of(schema: &SchemaRef, column: &str) -> Option<Arc<str>> {
    match schema.field_with_name(column).ok()?.data_type() {
        arrow::datatypes::DataType::Timestamp(_, timezone) => timezone.clone(),
        _ => None,
    }
}

/// Combines one or more physical predicates into a single conjunction, or
/// `None` when the list is empty.
fn conjoin_physical_predicates(
    predicates: Vec<Arc<dyn datafusion::physical_expr::PhysicalExpr>>,
) -> Option<Arc<dyn datafusion::physical_expr::PhysicalExpr>> {
    use datafusion::logical_expr::Operator;
    use datafusion::physical_expr::expressions::BinaryExpr;
    predicates
        .into_iter()
        .reduce(|left, right| Arc::new(BinaryExpr::new(left, Operator::And, right)))
}

/// Compiled conjunction of one assignment's signed closed predicates,
/// evaluated directly over Arrow batches that never pass through a
/// `DataFusion` plan.
///
/// Persisted follower scans get their predicates enforced by the physical
/// plan itself, but the Scribe live-tail snapshot is assembled inside the
/// Scribe pod and shipped back as ready Arrow. Compiling the closure once
/// here lets that path apply the same signed filter to every hot batch
/// before it is returned, so a selective query sends only matching rows
/// into follower attempt encoding instead of the whole tail.
#[derive(Debug)]
pub(crate) struct ScanPredicateFilter {
    /// The conjunction, or `None` when the assignment carries no predicates
    /// and every row is retained unchanged.
    predicate: Option<Arc<dyn datafusion::physical_expr::PhysicalExpr>>,
}

impl ScanPredicateFilter {
    /// Compiles the signed predicate list against the batch schema it will
    /// be evaluated over.
    ///
    /// # Errors
    /// Returns a `DataFusion` error when a predicate names a column absent
    /// from `schema` or compares it against an incompatible literal.
    pub(crate) fn compile(
        schema: &SchemaRef,
        predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    ) -> DataFusionResult<Self> {
        let compiled = predicates
            .iter()
            .map(|predicate| scan_predicate_physical_expr(predicate, schema))
            .collect::<DataFusionResult<Vec<_>>>()?;
        Ok(Self {
            predicate: conjoin_physical_predicates(compiled),
        })
    }

    /// Returns only the rows of `batch` satisfying the conjunction.
    ///
    /// Rows whose predicate evaluates to `NULL` are dropped, matching SQL
    /// `WHERE` semantics and the `FilterExec` the leader would otherwise
    /// have applied above the provider.
    ///
    /// # Errors
    /// Returns a `DataFusion` error when the conjunction cannot be evaluated
    /// against `batch` or does not produce a boolean mask.
    pub(crate) fn retain(
        &self,
        batch: arrow::record_batch::RecordBatch,
    ) -> DataFusionResult<arrow::record_batch::RecordBatch> {
        let Some(predicate) = &self.predicate else {
            return Ok(batch);
        };
        let rows = batch.num_rows();
        let evaluated = predicate.evaluate(&batch)?.into_array(rows)?;
        let mask = datafusion::common::cast::as_boolean_array(&evaluated)?;
        arrow::compute::filter_record_batch(&batch, mask).map_err(Into::into)
    }
}

/// One statistic bound value in the closed subset this pruning path
/// understands. Two bounds are only ever compared after both are derived
/// from the same predicate literal's type, so the derived ordering is exact.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum StatBound {
    /// Boolean bound, ordered `false < true`.
    Bool(bool),
    /// Signed 64-bit bound, shared by `I64` and `TimestampMicros` literals.
    I64(i64),
    /// UTF-8 bound compared by byte order.
    Utf8(String),
}

/// Converts one closed predicate literal into its comparable statistic bound.
/// Returns `None` for `U64`/`F64Bits`, whose Parquet physical encoding this
/// pruning path does not decode; callers must treat that as "never exclude".
fn literal_bound(
    literal: &wyrd_spec::vala::assignment_authority::ScanLiteral,
) -> Option<StatBound> {
    use wyrd_spec::vala::assignment_authority::ScanLiteral;
    match literal {
        ScanLiteral::Bool(value) => Some(StatBound::Bool(*value)),
        ScanLiteral::I64(value) | ScanLiteral::TimestampMicros(value) => {
            Some(StatBound::I64(*value))
        }
        ScanLiteral::Utf8(value) => Some(StatBound::Utf8(value.clone())),
        ScanLiteral::U64(_) | ScanLiteral::F64Bits(_) => None,
    }
}

/// Reads one column chunk's typed min/max as comparable bounds, matched
/// against `target`'s variant. Returns `None` when the physical statistics
/// type does not correspond to `target`, or either bound is unset.
fn statistics_bound(
    stats: &parquet::file::statistics::Statistics,
    target: &StatBound,
) -> Option<(StatBound, StatBound)> {
    use parquet::file::statistics::Statistics;
    match (stats, target) {
        (Statistics::Boolean(value), StatBound::Bool(_)) => Some((
            StatBound::Bool(*value.min_opt()?),
            StatBound::Bool(*value.max_opt()?),
        )),
        (Statistics::Int64(value), StatBound::I64(_)) => Some((
            StatBound::I64(*value.min_opt()?),
            StatBound::I64(*value.max_opt()?),
        )),
        (Statistics::ByteArray(value), StatBound::Utf8(_)) => Some((
            StatBound::Utf8(String::from_utf8_lossy(value.min_opt()?.data()).into_owned()),
            StatBound::Utf8(String::from_utf8_lossy(value.max_opt()?.data()).into_owned()),
        )),
        _ => None,
    }
}

/// Returns the Parquet leaf column index whose name matches `column`, or
/// `None` when the physical schema carries no column by that name.
fn parquet_column_index(
    metadata: &parquet::file::metadata::ParquetMetaData,
    column: &str,
) -> Option<usize> {
    let schema = metadata.file_metadata().schema_descr();
    (0..schema.num_columns()).find(|&index| schema.column(index).name() == column)
}

/// Returns true only when Parquet footer statistics prove no row in
/// `row_group_index` can satisfy `predicate`. An absent, type-mismatched, or
/// undecoded statistic always keeps the row group; this function never
/// produces a false exclusion.
fn leaf_excludes_row_group(
    metadata: &parquet::file::metadata::ParquetMetaData,
    row_group_index: usize,
    predicate: &wyrd_spec::vala::assignment_authority::ScanPredicate,
) -> bool {
    use wyrd_spec::vala::assignment_authority::ScanPredicate;

    let Some(column_index) = parquet_column_index(metadata, predicate.column()) else {
        return false;
    };
    let row_group = metadata.row_group(row_group_index);
    let Some(stats) = row_group.column(column_index).statistics() else {
        return false;
    };
    match predicate {
        ScanPredicate::IsNull(_) => stats.null_count_opt() == Some(0),
        ScanPredicate::IsNotNull(_) => {
            let rows = u64::try_from(row_group.num_rows()).unwrap_or(0);
            stats.null_count_opt() == Some(rows)
        }
        ScanPredicate::Eq(_, literal) => {
            let Some(target) = literal_bound(literal) else {
                return false;
            };
            let Some((min, max)) = statistics_bound(stats, &target) else {
                return false;
            };
            target < min || max < target
        }
        ScanPredicate::NotEq(_, literal) => {
            let Some(target) = literal_bound(literal) else {
                return false;
            };
            let Some((min, max)) = statistics_bound(stats, &target) else {
                return false;
            };
            min == max && min == target
        }
        ScanPredicate::Lt(_, literal) => {
            let Some(target) = literal_bound(literal) else {
                return false;
            };
            let Some((min, _)) = statistics_bound(stats, &target) else {
                return false;
            };
            matches!(
                min.partial_cmp(&target),
                Some(std::cmp::Ordering::Equal | std::cmp::Ordering::Greater)
            )
        }
        ScanPredicate::LtEq(_, literal) => {
            let Some(target) = literal_bound(literal) else {
                return false;
            };
            let Some((min, _)) = statistics_bound(stats, &target) else {
                return false;
            };
            target < min
        }
        ScanPredicate::Gt(_, literal) => {
            let Some(target) = literal_bound(literal) else {
                return false;
            };
            let Some((_, max)) = statistics_bound(stats, &target) else {
                return false;
            };
            matches!(
                target.partial_cmp(&max),
                Some(std::cmp::Ordering::Equal | std::cmp::Ordering::Greater)
            )
        }
        ScanPredicate::GtEq(_, literal) => {
            let Some(target) = literal_bound(literal) else {
                return false;
            };
            let Some((_, max)) = statistics_bound(stats, &target) else {
                return false;
            };
            max < target
        }
    }
}

/// Row groups retained after closed-predicate statistics pruning for one
/// Parquet file, together with how many the pruning removed.
///
/// `retained` is the exact ordered row-group index list a
/// `ParquetRecordBatchStreamBuilder` should be restricted to. An empty
/// `retained` with a non-zero `pruned` means the whole file is excluded and
/// the caller must skip it without opening any data page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RowGroupSelection {
    /// Ordered row-group indices whose statistics can still satisfy every leaf.
    pub(super) retained: Vec<usize>,
    /// Number of row groups excluded by at least one predicate leaf.
    pub(super) pruned: u64,
}

impl RowGroupSelection {
    /// Reports whether the predicates excluded every row group in the file.
    pub(super) fn excludes_file(&self) -> bool {
        self.retained.is_empty() && self.pruned > 0
    }
}

/// Selects the row groups of `metadata` whose statistics can still satisfy the
/// closed predicate conjunction, pruning the rest.
///
/// A row group is pruned only when at least one leaf proves it cannot contain a
/// matching row; absent, type-mismatched, or unusable statistics always retain
/// it, so pruning is a pure IO optimization and never changes results. An empty
/// predicate conjunction retains every row group and prunes none.
pub(super) fn select_row_groups_for_predicates(
    metadata: &parquet::file::metadata::ParquetMetaData,
    predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
) -> RowGroupSelection {
    let total = metadata.num_row_groups();
    if predicates.is_empty() || total == 0 {
        return RowGroupSelection {
            retained: (0..total).collect(),
            pruned: 0,
        };
    }
    let retained: Vec<usize> = (0..total)
        .filter(|row_group_index| {
            !predicates
                .iter()
                .any(|predicate| leaf_excludes_row_group(metadata, *row_group_index, predicate))
        })
        .collect();
    let pruned = (total - retained.len()) as u64;
    RowGroupSelection { retained, pruned }
}

/// Projects one physical batch to the pinned schema by field name.
///
/// # Errors
///
/// Returns a `DataFusion` error when a required field is missing, a cast fails,
/// or Arrow rejects the projected batch.
fn project_batch(batch: &RecordBatch, schema: SchemaRef) -> DataFusionResult<RecordBatch> {
    let columns = schema
        .fields()
        .iter()
        .map(|field| {
            let index = batch.schema().index_of(field.name()).map_err(|_| {
                DataFusionError::Execution(format!(
                    "hot file is missing required field `{}`",
                    field.name()
                ))
            })?;
            let column = batch.column(index);
            if column.data_type() == field.data_type() {
                Ok(Arc::clone(column))
            } else {
                cast(column, field.data_type()).map_err(DataFusionError::from)
            }
        })
        .collect::<DataFusionResult<Vec<_>>>()?;
    RecordBatch::try_new(schema, columns).map_err(DataFusionError::from)
}

/// Removes one named physical field from a schema.
///
/// # Errors
///
/// Returns a `DataFusion` plan error when the field is absent.
fn schema_without(schema: &Schema, name: &str) -> DataFusionResult<SchemaRef> {
    let index = schema
        .index_of(name)
        .map_err(|_| DataFusionError::Plan(format!("physical schema missing `{name}`")))?;
    let fields = schema
        .fields()
        .iter()
        .enumerate()
        .filter(|(ordinal, _)| *ordinal != index)
        .map(|(_, field)| field.as_ref().clone())
        .collect::<Vec<_>>();
    Ok(Arc::new(Schema::new_with_metadata(
        fields,
        schema.metadata().clone(),
    )))
}

/// Removes one named array from a batch without copying retained arrays.
///
/// # Errors
///
/// Returns a `DataFusion` execution error when the field is absent or Arrow
/// rejects the projected batch.
fn remove_column(batch: &RecordBatch, name: &str) -> DataFusionResult<RecordBatch> {
    let index = batch
        .schema()
        .index_of(name)
        .map_err(|_| DataFusionError::Execution(format!("batch missing `{name}`")))?;
    let schema = schema_without(&batch.schema(), name)?;
    let columns = batch
        .columns()
        .iter()
        .enumerate()
        .filter(|(ordinal, _)| *ordinal != index)
        .map(|(_, column)| Arc::clone(column))
        .collect();
    RecordBatch::try_new(schema, columns).map_err(DataFusionError::from)
}

/// Applies the caller-visible projection after tenant validation/reconciliation.
///
/// # Errors
///
/// Returns a `DataFusion` plan error when a projected ordinal is invalid.
fn project_plan(
    input: Arc<dyn ExecutionPlan>,
    projection: Option<&Vec<usize>>,
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    let Some(projection) = projection else {
        return Ok(input);
    };
    let schema = input.schema();
    let expressions = projection
        .iter()
        .map(|index| {
            let field = schema.fields().get(*index).ok_or_else(|| {
                DataFusionError::Plan("table projection ordinal is out of range".to_owned())
            })?;
            Ok((
                Arc::new(Column::new(field.name(), *index))
                    as Arc<dyn datafusion::physical_expr::PhysicalExpr>,
                field.name().clone(),
            ))
        })
        .collect::<DataFusionResult<Vec<_>>>()?;
    Ok(Arc::new(ProjectionExec::try_new(expressions, input)?))
}

/// Creates bounded cooperative properties for one single-partition operator.
fn plan_properties(schema: SchemaRef) -> Arc<PlanProperties> {
    plan_properties_with_partitions(schema, 1)
}

/// Creates bounded cooperative properties preserving an input partition count.
fn plan_properties_with_partitions(
    schema: SchemaRef,
    partition_count: usize,
) -> Arc<PlanProperties> {
    Arc::new(
        PlanProperties::new(
            EquivalenceProperties::new(schema),
            Partitioning::UnknownPartitioning(partition_count),
            EmissionType::Incremental,
            Boundedness::Bounded,
        )
        .with_scheduling_type(SchedulingType::Cooperative),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use super::*;
    use crate::oracle::{BifrostQueryReadDecision, OracleSlotManager};
    use arrow::array::{ArrayRef, Int32Array, Int64Array, StringArray};
    use async_trait::async_trait;
    use datafusion::physical_plan::sorts::sort::SortExec;
    use datafusion::physical_plan::union::UnionExec;
    use wyrd_runtime::Principal;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::AuthMethod;
    use wyrd_spec::vala::api::QueryStreamFrame;

    /// In-memory audit sink used only to inspect physical plan structure.
    struct NoopAudit;

    #[async_trait]
    impl OracleAudit for NoopAudit {
        /// Accepts a read-decision event without persisting it in this plan test.
        async fn append_read_decision(
            &self,
            _context: &AuthorizedQueryContext,
            _decision: BifrostQueryReadDecision,
        ) -> Result<(), BifrostError> {
            Ok(())
        }

        /// Accepts a security-violation event without persisting it in this plan test.
        async fn append_security_violation(
            &self,
            _context: VerifiedSecurityContext,
            _violation: BifrostSecurityViolation,
        ) -> Result<(), BifrostError> {
            Ok(())
        }
    }

    /// Tenant validation finds foreign rows at every batch position.
    #[test]
    fn tripwire_detects_first_middle_and_last_foreign_rows() {
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let foreign = wyrd_spec::DataTenantId::new_v7();
        for position in 0..3 {
            let values = (0..3)
                .map(|row| {
                    if row == position {
                        foreign.to_string()
                    } else {
                        tenant.to_string()
                    }
                })
                .collect::<Vec<_>>();
            let schema = Arc::new(Schema::new(vec![Field::new(
                DATA_TENANT_ID,
                DataType::Utf8,
                false,
            )]));
            let batch = RecordBatch::try_new(
                schema,
                vec![Arc::new(StringArray::from(values)) as ArrayRef],
            )
            .expect("test tenant batch");
            assert_eq!(
                tenant_mismatch_row(&batch, tenant).expect("tenant column is valid"),
                Some(position)
            );
        }
    }

    /// Keeps the tenant tripwire immediately above an unordered source union.
    #[test]
    fn unordered_union_has_no_mandatory_reconciliation_sort() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            DATA_TENANT_ID,
            DataType::Utf8,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(StringArray::from(vec![
                wyrd_spec::DataTenantId::new_v7().to_string(),
            ])) as ArrayRef],
        )
        .expect("source batch");
        let source: Arc<dyn ExecutionPlan> =
            MemorySourceConfig::try_new_exec(std::slice::from_ref(&vec![batch]), schema, None)
                .expect("source batch schema");
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let principal = Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: wyrd_runtime::PrincipalKind::User,
            tenant_id: tenant,
            roles: Vec::new(),
            effective_permissions: PermissionSet::default(),
        };
        let context = AuthorizedQueryContext::try_new(
            principal,
            tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            "bifrost_query:read",
        )
        .expect("query context");
        let source_union =
            UnionExec::try_new(vec![Arc::clone(&source), source]).expect("source union");
        let tripwire = Arc::new(
            TenantTripwireExec::new(
                source_union,
                context,
                "vala.traces.spans".to_owned(),
                Arc::new(NoopAudit),
            )
            .expect("tripwire plan"),
        );
        assert_eq!(tripwire.name(), "TenantTripwireExec");
        assert_eq!(tripwire.children()[0].name(), "UnionExec");
        assert!(tripwire.as_any().downcast_ref::<SortExec>().is_none());
        assert!(
            tripwire.children()[0]
                .as_any()
                .downcast_ref::<SortExec>()
                .is_none()
        );
    }

    /// Composes one Oracle capability for hot-read resource tests.
    fn oracle_test_roles(memory_limit_bytes: usize) -> crate::resources::BifrostRoleResources {
        crate::resources::BifrostRuntimeResources::from_snapshot(
            crate::resources::SystemResourceSnapshot {
                memory_limit_bytes,
                effective_cpu: 2,
                scratch_capacity_bytes: 1024 * 1024 * 1024,
                scratch_available_bytes: 1024 * 1024 * 1024,
                memory_source: crate::resources::ResourceSource::Injected,
                cpu_source: crate::resources::ResourceSource::Injected,
            },
            crate::resources::BifrostResourcePolicy {
                roles: [crate::resources::BifrostRole::Oracle]
                    .into_iter()
                    .collect(),
                memory_limit_bytes: None,
                unmanaged_reserve_bytes: Some(256 * 1024 * 1024),
                scratch_limit_bytes: Some(1024 * 1024 * 1024),
                effective_cpu: None,
                oracle_query_slot_limit: None,
                scratch_root: std::env::temp_dir(),
                volume_roots: None,
            },
        )
        .expect("injected Oracle test resources")
        .compose_roles()
        .expect("Oracle test role composition")
    }

    /// Projects Oracle memory inputs from one production-equivalent composition.
    fn oracle_memory_resources(
        roles: &crate::resources::BifrostRoleResources,
        reconciliation_limit_bytes: usize,
    ) -> OracleMemoryResources {
        OracleMemoryResources {
            resources: roles.oracle().expect("composition enables Oracle"),
            reconciliation_limit_bytes,
        }
    }

    /// Drains a projected hot stream, checking each batch and sampling peaks.
    ///
    /// Every batch must carry exactly the projected single-column schema and
    /// the fixture's values, and the pool and telemetry peaks are sampled once
    /// per batch so the caller can prove the scan never exceeded its budget.
    async fn drain_projected_int64_batches(
        batches: &mut datafusion::execution::SendableRecordBatchStream,
        projected: &Arc<Schema>,
        observed: (&Arc<dyn MemoryPool>, &Arc<OracleTelemetry>),
        peaks: (&Arc<AtomicU64>, &Arc<AtomicU64>),
    ) -> usize {
        let (query_pool, telemetry) = observed;
        let (peak_pool, peak_telemetry) = peaks;
        let mut rows = 0;
        while let Some(batch) = batches.next().await {
            let batch = batch.expect("bounded batch");
            assert_eq!(&batch.schema(), projected);
            assert_eq!(batch.num_columns(), 1);
            let values = batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("projected values remain Int64");
            assert_eq!(values.values().as_ref(), &[1, 2, 3]);
            rows += batch.num_rows();
            peak_pool.fetch_max(query_pool.reserved() as u64, Ordering::AcqRel);
            peak_telemetry.fetch_max(
                telemetry.memory_bytes.load(Ordering::Acquire),
                Ordering::AcqRel,
            );
        }
        rows
    }

    /// Builds a reader that slices the fixture and tallies every requested byte.
    ///
    /// The success and retry attempts of the causal-boundary proof read the
    /// same fixture identically; sharing one constructor keeps their byte
    /// accounting provably equal to the scanned-bytes counter under test.
    fn counting_slice_reader(bytes: &bytes::Bytes, requested: &Arc<AtomicU64>) -> HotReadOverride {
        let bytes = bytes.clone();
        let requested = Arc::clone(requested);
        Arc::new(move |_, range| {
            requested.fetch_add(range.end - range.start, Ordering::AcqRel);
            let bytes = bytes.clone();
            Box::pin(async move {
                Ok(bytes.slice(
                    usize::try_from(range.start).expect("range start")
                        ..usize::try_from(range.end).expect("range end"),
                ))
            })
        })
    }

    /// Asserts the interactive scan counters match the reader's own tallies.
    ///
    /// Every hot attempt — successful, failed, abandoned, and retried — must
    /// publish exactly one file and one partition observation, and the scanned
    /// bytes must equal what the injected reader was actually asked for.
    fn assert_interactive_scan_counters(
        snapshot: &wyrd_bench::BenchmarkMetricSnapshot,
        expected_bytes: u64,
        expected_attempts: u64,
    ) {
        assert_eq!(
            snapshot
                .counters
                .get("oracle_query_bytes_scanned_total{class=\"interactive\"}"),
            Some(&expected_bytes)
        );
        assert_eq!(
            snapshot
                .counters
                .get("oracle_query_files_scanned_total{class=\"interactive\"}"),
            Some(&expected_attempts)
        );
        assert_eq!(
            snapshot
                .counters
                .get("oracle_query_partitions_scanned_total{class=\"interactive\"}"),
            Some(&expected_attempts)
        );
    }

    /// Asserts a finished hot scan returned every governed byte it charged.
    ///
    /// A completed stream must leave the root governor, the query pool, and the
    /// Oracle telemetry gauge all back at zero; a nonzero residue is a leaked
    /// reservation rather than a measurement artifact.
    fn assert_hot_scan_baselines(
        roles: &crate::resources::BifrostRoleResources,
        query_pool: &Arc<dyn MemoryPool>,
        telemetry: &OracleTelemetry,
    ) {
        let snapshot = roles.snapshot().expect("root snapshot");
        assert_eq!(snapshot.oracle_memory_used_bytes, 0);
        assert_eq!(snapshot.elastic_memory_used_bytes, 0);
        assert_eq!(query_pool.reserved(), 0);
        assert_eq!(telemetry.memory_bytes.load(Ordering::Acquire), 0);
    }

    /// In-memory sources remain unavailable while hot reads aggregate actual bytes.
    #[test]
    fn scan_stats_preserve_non_file_absence_and_hot_aggregation() {
        let batch = RecordBatch::new_empty(Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )])));
        let source = MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&vec![batch]),
            Arc::new(Schema::new(vec![Field::new(
                "value",
                DataType::Int64,
                false,
            )])),
            None,
        )
        .expect("memory source is valid");
        let mut memory_only = OracleQueryScanStats::from_plan(source.as_ref(), 17);
        memory_only.finalize();
        assert_eq!(memory_only.physical_bytes_scanned, None);

        let hot = Arc::new(OracleScanMetricsHandle::default());
        hot.record_hot_range(11);
        hot.record_hot_range(7);
        hot.record_hot_file();
        hot.record_hot_file();
        let mut stats = OracleQueryScanStats {
            scan_handles: vec![hot],
            ..OracleQueryScanStats::default()
        };
        stats.finalize();
        assert_eq!(stats.physical_bytes_scanned, Some(18));
        assert_eq!(stats.files_scanned, 2);
        assert_eq!(stats.partitions_scanned, 1);
    }

    /// Terminal aggregation is monotonic and emits exactly once after a drop.
    #[test]
    fn scan_stats_terminal_collection_is_exactly_once() {
        let hot = Arc::new(OracleScanMetricsHandle::default());
        hot.record_hot_range(5);
        hot.record_hot_file();
        let mut stats = OracleQueryScanStats {
            scan_handles: vec![Arc::clone(&hot)],
            ..OracleQueryScanStats::default()
        };
        stats.finalize();
        hot.record_hot_range(9);
        hot.record_hot_file();
        stats.finalize();
        assert_eq!(stats.physical_bytes_scanned, Some(5));
        assert_eq!(stats.files_scanned, 1);
        assert_eq!(stats.partitions_scanned, 1);
    }

    /// Adapter construction fails closed instead of silently changing sources.
    #[test]
    fn iceberg_adapter_requires_pinned_scan_plan() {
        let batch = RecordBatch::new_empty(Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )])));
        let source = MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&vec![batch]),
            Arc::new(Schema::new(vec![Field::new(
                "value",
                DataType::Int64,
                false,
            )])),
            None,
        )
        .expect("memory source is valid");
        let error = OracleIcebergScanExec::from_plan(source.as_ref())
            .expect_err("non-Iceberg plans must be rejected");
        assert!(error.to_string().contains("pinned IcebergTableScan"));
    }

    /// Owns temporary files and metadata for the position-delete adapter proof.
    struct PositionDeleteFixture {
        /// Temporary directory retaining both Parquet files until assertions finish.
        _directory: tempfile::TempDir,
        /// Arrow schema expected from the position-delete reader.
        data_schema: SchemaRef,
        /// Pinned Iceberg task carrying data and position-delete metadata.
        task: FileScanTask,
    }

    /// Writes one Arrow batch to a Parquet path for the position-delete fixture.
    fn write_position_delete_file(
        path: &std::path::Path,
        schema: SchemaRef,
        batch: &RecordBatch,
        context: &str,
    ) {
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            File::create(path).unwrap_or_else(|error| panic!("{context} file: {error}")),
            schema,
            None,
        )
        .unwrap_or_else(|error| panic!("{context} writer: {error}"));
        writer
            .write(batch)
            .unwrap_or_else(|error| panic!("{context} write: {error}"));
        writer
            .close()
            .unwrap_or_else(|error| panic!("{context} close: {error}"));
    }

    /// Builds a data task whose position delete removes the middle row.
    fn build_position_delete_fixture() -> PositionDeleteFixture {
        let directory = tempfile::tempdir().expect("delete fixture directory");
        let data_path = directory.path().join("data.parquet");
        let delete_path = directory.path().join("deletes.parquet");
        let data_schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int32, false).with_metadata(HashMap::from([(
                "PARQUET:field_id".to_owned(),
                "1".to_owned(),
            )])),
        ]));
        let iceberg_schema = Arc::new(
            iceberg::spec::Schema::builder()
                .with_fields(vec![Arc::new(iceberg::spec::NestedField::required(
                    1,
                    "value",
                    iceberg::spec::Type::Primitive(iceberg::spec::PrimitiveType::Int),
                ))])
                .build()
                .expect("Iceberg task schema"),
        );
        let data_batch = RecordBatch::try_new(
            Arc::clone(&data_schema),
            vec![Arc::new(Int32Array::from(vec![10, 20, 30])) as ArrayRef],
        )
        .expect("data batch");
        write_position_delete_file(&data_path, Arc::clone(&data_schema), &data_batch, "data");

        let delete_schema = Arc::new(Schema::new(vec![
            Field::new("file_path", DataType::Utf8, false).with_metadata(HashMap::from([(
                "PARQUET:field_id".to_owned(),
                "2147483546".to_owned(),
            )])),
            Field::new("pos", DataType::Int64, false).with_metadata(HashMap::from([(
                "PARQUET:field_id".to_owned(),
                "2147483545".to_owned(),
            )])),
        ]));
        let delete_batch = RecordBatch::try_new(
            Arc::clone(&delete_schema),
            vec![
                Arc::new(StringArray::from(vec![
                    data_path.to_string_lossy().to_string(),
                ])) as ArrayRef,
                Arc::new(Int64Array::from(vec![1])) as ArrayRef,
            ],
        )
        .expect("delete batch");
        write_position_delete_file(
            &delete_path,
            Arc::clone(&delete_schema),
            &delete_batch,
            "delete",
        );

        let task = FileScanTask::builder()
            .with_file_size_in_bytes(std::fs::metadata(&data_path).expect("data metadata").len())
            .with_start(0)
            .with_length(0)
            .with_record_count(Some(3))
            .with_data_file_path(data_path.to_string_lossy().to_string())
            .with_data_file_format(iceberg::spec::DataFileFormat::Parquet)
            .with_schema(iceberg_schema)
            .with_project_field_ids(vec![1])
            .with_deletes(vec![
                iceberg::scan::FileScanTaskDeleteFile::builder()
                    .with_file_path(delete_path.to_string_lossy().to_string())
                    .with_file_size_in_bytes(
                        std::fs::metadata(&delete_path)
                            .expect("delete metadata")
                            .len(),
                    )
                    .with_file_type(iceberg::spec::DataContentType::PositionDeletes)
                    .with_partition_spec_id(0)
                    .build(),
            ])
            .with_case_sensitive(false)
            .build();
        PositionDeleteFixture {
            _directory: directory,
            data_schema,
            task,
        }
    }

    /// Reads one task through two readers and asserts delete filtering and schema fidelity.
    async fn assert_position_delete_rows(
        reader: &iceberg::arrow::ArrowReader,
        task: FileScanTask,
        data_schema: SchemaRef,
    ) {
        let direct = reader
            .clone()
            .read(Box::pin(futures_util::stream::iter(vec![Ok(task.clone())])))
            .expect("direct reader")
            .stream()
            .try_collect::<Vec<RecordBatch>>()
            .await
            .expect("direct rows");
        let forwarded = reader
            .clone()
            .read(Box::pin(futures_util::stream::iter(vec![Ok(task)])))
            .expect("forwarded reader")
            .stream()
            .try_collect::<Vec<RecordBatch>>()
            .await
            .expect("forwarded rows");
        assert_eq!(direct, forwarded);
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].schema(), data_schema);
        let surviving = direct[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int32Array>()
            .expect("position-delete reader preserves Int32 schema");
        assert_eq!(surviving.values().as_ref(), &[10, 30]);
        assert_eq!(direct.iter().map(RecordBatch::num_rows).sum::<usize>(), 2);
    }

    /// The adapter forwards position-delete task metadata into the pinned
    /// reader, producing the same rows as the unwrapped reader.
    #[tokio::test]
    async fn iceberg_adapter_preserves_position_delete_tasks() {
        let fixture = build_position_delete_fixture();
        let metrics = Arc::new(OracleScanMetricsHandle::default());
        let forwarded = retain_iceberg_task(fixture.task.clone(), &metrics);
        assert_eq!(forwarded, fixture.task);
        assert_eq!(metrics.iceberg_files.load(Ordering::Relaxed), 1);

        let reader = iceberg::arrow::ArrowReaderBuilder::new(
            FileIO::new_with_fs(),
            iceberg::Runtime::current(),
        )
        .build();
        assert_position_delete_rows(&reader, forwarded, fixture.data_schema).await;
    }

    /// Production hot execution records requested bytes once on success,
    /// manifest-size failure, and a retried read before the stream is dropped.
    #[tokio::test]
    async fn hot_exec_records_success_error_drop_and_retry_paths() {
        let directory = tempfile::tempdir().expect("hot fixture directory");
        let path = directory.path().join("hot.parquet");
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, false),
            Field::new("ignored", DataType::Int64, false),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3])) as ArrayRef,
                Arc::new(Int64Array::from(vec![9, 9, 9])) as ArrayRef,
            ],
        )
        .expect("hot batch");
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            File::create(&path).expect("hot file"),
            Arc::clone(&schema),
            None,
        )
        .expect("hot writer");
        writer.write(&batch).expect("hot batch write");
        writer.close().expect("hot close");
        let size = usize::try_from(std::fs::metadata(&path).expect("hot metadata").len())
            .expect("hot size fits usize");
        let memory = OracleMemoryResources {
            resources: crate::resources::BifrostRuntimeResources::composed_for_test(
                1024 * 1024 * 1024,
                1024 * 1024 * 1024,
                [crate::resources::BifrostRole::Oracle],
            )
            .oracle()
            .expect("composition must enable the Oracle capability"),
            reconciliation_limit_bytes: 1024 * 1024,
        };
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let make_exec = |size_bytes| {
            let metrics = Arc::new(OracleScanMetricsHandle::default());
            let exec = HotParquetExec::new(
                vec![HotFileSource {
                    location: path.to_string_lossy().into_owned(),
                    size_bytes,
                }],
                FileIO::new_with_fs(),
                Arc::clone(&schema),
                HotParquetGovernance::Leader {
                    memory: memory.clone(),
                    memory_pool: crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
                    telemetry: Arc::clone(&telemetry),
                    query_class: QueryClass::Interactive,
                },
                Arc::clone(&metrics),
                Vec::new(),
            );
            (exec, metrics)
        };
        let context = datafusion::execution::context::SessionContext::new().task_ctx();
        let (success, success_metrics) = make_exec(size);
        let mut stream = success
            .execute(0, Arc::clone(&context))
            .expect("hot stream");
        let first = stream
            .next()
            .await
            .expect("hot first batch")
            .expect("hot success");
        assert_eq!(first.num_rows(), 3);
        drop(stream);
        let (failed, failed_metrics) = make_exec(size.saturating_add(1));
        let mut stream = failed
            .execute(0, Arc::clone(&context))
            .expect("failed hot stream");
        assert!(stream.next().await.expect("hot error frame").is_err());
        drop(stream);
        let (retry, retry_metrics) = make_exec(size);
        let mut stream = retry.execute(0, context).expect("retry hot stream");
        let _ = stream
            .next()
            .await
            .expect("retry first batch")
            .expect("retry success");
        drop(stream);
        let success_values = success_metrics.terminal_values();
        let failed_values = failed_metrics.terminal_values();
        let retry_values = retry_metrics.terminal_values();
        assert_eq!(success_values, retry_values);
        assert!(matches!(success_values, (Some(bytes), 1, 1) if bytes > 0 && bytes <= size as u64));
        assert!(
            matches!(failed_values, (Some(bytes), 1, 1) if bytes > 0 && bytes <= size.saturating_add(1) as u64)
        );
    }

    /// Owns deterministic Parquet bytes used by the terminal-owner hot test.
    struct HotCausalFixture {
        /// Temporary directory retaining the source file for the fixture lifetime.
        _directory: tempfile::TempDir,
        /// Source location supplied to the production hot execution plan.
        path: std::path::PathBuf,
        /// One-column Arrow schema used to write and decode the source.
        schema: SchemaRef,
        /// Serialized Parquet bytes returned by deterministic reader overrides.
        bytes: bytes::Bytes,
    }

    /// Builds one small Parquet source for terminal-owner hot attempts.
    fn build_hot_causal_fixture() -> HotCausalFixture {
        let directory = tempfile::tempdir().expect("hot causal fixture directory");
        let path = directory.path().join("hot-causal.parquet");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![1, 2, 3])) as ArrayRef],
        )
        .expect("hot causal batch");
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            File::create(&path).expect("hot causal file"),
            Arc::clone(&schema),
            None,
        )
        .expect("hot causal writer");
        writer.write(&batch).expect("hot causal write");
        writer.close().expect("hot causal close");
        let bytes = bytes::Bytes::from(std::fs::read(&path).expect("hot causal bytes"));
        HotCausalFixture {
            _directory: directory,
            path,
            schema,
            bytes,
        }
    }

    /// Deterministic ranged reader that records IO and can inject short reads.
    struct RecordingRangeReader {
        /// Immutable object bytes served by exact ranges.
        bytes: bytes::Bytes,
        /// Ordered range requests observed at the storage boundary.
        ranges: Arc<Mutex<Vec<Range<u64>>>>,
        /// Whether every nonempty request returns one byte too few.
        short: bool,
    }

    #[async_trait]
    impl FileRead for RecordingRangeReader {
        /// Serves one recorded range, optionally truncating its returned bytes.
        ///
        /// # Errors
        ///
        /// This deterministic fixture returns no storage error.
        async fn read(&self, range: Range<u64>) -> iceberg::Result<bytes::Bytes> {
            self.ranges
                .lock()
                .expect("recorded ranges")
                .push(range.clone());
            let start = usize::try_from(range.start).expect("range start fits usize");
            let mut end = usize::try_from(range.end).expect("range end fits usize");
            if self.short && end > start {
                end -= 1;
            }
            Ok(self.bytes.slice(start..end))
        }
    }

    /// Creates a governed reader over deterministic fixture bytes.
    fn governed_fixture_reader(
        fixture: &HotCausalFixture,
        roles: &crate::resources::BifrostRoleResources,
        ranges: Arc<Mutex<Vec<Range<u64>>>>,
        short: bool,
    ) -> (IcebergParquetReader, Arc<dyn MemoryPool>) {
        let resources = roles.oracle().expect("composition must enable Oracle");
        let pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
        let reader = IcebergParquetReader::new(
            Box::new(RecordingRangeReader {
                bytes: fixture.bytes.clone(),
                ranges,
                short,
            }),
            u64::try_from(fixture.bytes.len()).expect("fixture size fits u64"),
            HotParquetGovernance::Leader {
                memory: OracleMemoryResources {
                    resources,
                    reconciliation_limit_bytes: 1024 * 1024,
                },
                memory_pool: Arc::clone(&pool),
                telemetry: Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1)))),
                query_class: QueryClass::Interactive,
            },
            Arc::new(OracleScanMetricsHandle::default()),
        );
        (reader, pool)
    }

    /// Hot Parquet uses strict sub-file requests and never reserves the object size.
    #[tokio::test]
    async fn hot_parquet_reads_ranges_without_whole_file_reservation() {
        let fixture = build_hot_causal_fixture();
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let (reader, pool) =
            governed_fixture_reader(&fixture, &governor, Arc::clone(&ranges), false);
        let mut batches = ParquetRecordBatchStreamBuilder::new(reader)
            .await
            .expect("ranged metadata")
            .with_batch_size(datafusion::prelude::SessionConfig::default().batch_size())
            .build()
            .expect("ranged batches");
        let mut rows = 0;
        while let Some(batch) = batches.next().await {
            rows += batch.expect("ranged batch").num_rows();
        }
        assert_eq!(rows, 3);
        let ranges = ranges.lock().expect("recorded ranges");
        assert!(!ranges.is_empty());
        assert!(ranges.iter().all(|range| {
            range.start != 0
                || range.end != u64::try_from(fixture.bytes.len()).expect("fixture size")
        }));
        assert_eq!(pool.reserved(), 0);
        assert_eq!(
            governor
                .snapshot()
                .expect("root snapshot")
                .oracle_memory_used_bytes,
            0
        );
    }

    /// A logical hot object above the child budget streams when live pieces fit.
    #[tokio::test]
    async fn hot_parquet_larger_than_budget_streams_exact_rows() {
        let fixture = build_hot_causal_fixture();
        let budget = fixture.bytes.len().saturating_sub(1);
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let metrics = Arc::new(OracleScanMetricsHandle::default());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&ranges);
        let source = fixture.bytes.clone();
        let peak_pool = Arc::new(AtomicU64::new(0));
        let peak_telemetry = Arc::new(AtomicU64::new(0));
        let query_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
        let observed_pool = Arc::clone(&query_pool);
        let range_telemetry = Arc::clone(&telemetry);
        let range_peak_pool = Arc::clone(&peak_pool);
        let range_peak_telemetry = Arc::clone(&peak_telemetry);
        let projected = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let exec = HotParquetExec::new(
            vec![HotFileSource {
                location: fixture.path.to_string_lossy().into_owned(),
                size_bytes: fixture.bytes.len(),
            }],
            FileIO::new_with_fs(),
            Arc::clone(&projected),
            HotParquetGovernance::Leader {
                memory: oracle_memory_resources(&governor, 1024),
                memory_pool: Arc::clone(&query_pool),
                telemetry: Arc::clone(&telemetry),
                query_class: QueryClass::Interactive,
            },
            Arc::clone(&metrics),
            Vec::new(),
        )
        .with_test_reader(Arc::new(move |_, range| {
            range_peak_pool.fetch_max(observed_pool.reserved() as u64, Ordering::AcqRel);
            range_peak_telemetry.fetch_max(
                range_telemetry.memory_bytes.load(Ordering::Acquire),
                Ordering::AcqRel,
            );
            recorded
                .lock()
                .expect("recorded ranges")
                .push(range.clone());
            let source = source.clone();
            Box::pin(async move {
                Ok(source.slice(
                    usize::try_from(range.start).expect("range start")
                        ..usize::try_from(range.end).expect("range end"),
                ))
            })
        }));
        let mut batches = exec
            .execute(
                0,
                datafusion::execution::context::SessionContext::new().task_ctx(),
            )
            .expect("production hot stream");
        let rows = drain_projected_int64_batches(
            &mut batches,
            &projected,
            (&query_pool, &telemetry),
            (&peak_pool, &peak_telemetry),
        )
        .await;
        assert_eq!(rows, 3);
        assert!(fixture.bytes.len() > budget);
        let ranges = ranges.lock().expect("recorded ranges");
        let physical = ranges
            .iter()
            .map(|range| range.end - range.start)
            .sum::<u64>();
        let max_range = ranges
            .iter()
            .map(|range| usize::try_from(range.end - range.start).expect("range size"))
            .max()
            .expect("at least one range");
        assert!(max_range <= budget);
        assert_eq!(metrics.terminal_values(), (Some(physical), 1, 1));
        let peak_pool =
            usize::try_from(peak_pool.load(Ordering::Acquire)).expect("query peak fits usize");
        let peak_telemetry = usize::try_from(peak_telemetry.load(Ordering::Acquire))
            .expect("telemetry peak fits usize");
        assert!(peak_pool > 0 && peak_pool <= budget);
        assert!(peak_telemetry > 0 && peak_telemetry <= budget);
        drop(ranges);
        assert_hot_scan_baselines(&governor, &query_pool, &telemetry);
    }

    /// Footer metadata is fetched only through governed ranged IO.
    #[tokio::test]
    async fn hot_parquet_metadata_ranges_use_governed_reader() {
        let fixture = build_hot_causal_fixture();
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let (mut reader, pool) =
            governed_fixture_reader(&fixture, &governor, Arc::clone(&ranges), false);
        let metadata = reader.get_metadata(None).await.expect("governed metadata");
        assert_eq!(metadata.file_metadata().num_rows(), 3);
        let ranges = ranges.lock().expect("recorded ranges");
        assert!(!ranges.is_empty());
        assert!(
            ranges
                .iter()
                .all(|range| range.end <= fixture.bytes.len() as u64)
        );
        assert_eq!(pool.reserved(), 0);
        assert_eq!(
            governor
                .snapshot()
                .expect("root snapshot")
                .oracle_memory_used_bytes,
            0
        );
    }

    /// Clones and slices retain one shared range charge until the final drop.
    #[tokio::test]
    async fn hot_parquet_range_clone_and_slice_retain_charge_until_final_drop() {
        let fixture = build_hot_causal_fixture();
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let (mut reader, pool) =
            governed_fixture_reader(&fixture, &governor, Arc::new(Mutex::new(Vec::new())), false);
        let bytes = reader.get_bytes(0..16).await.expect("governed range");
        let clone = bytes.clone();
        let slice = clone.slice(4..12);
        assert_eq!(pool.reserved(), 16);
        drop(bytes);
        drop(clone);
        assert_eq!(pool.reserved(), 16);
        drop(slice);
        assert_eq!(pool.reserved(), 0);
    }

    /// A short storage result fails closed and releases its pre-IO reservation.
    #[tokio::test]
    async fn hot_parquet_short_range_drops_reservation_and_fails_closed() {
        let fixture = build_hot_causal_fixture();
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let (mut reader, pool) =
            governed_fixture_reader(&fixture, &governor, Arc::new(Mutex::new(Vec::new())), true);
        let error = reader
            .get_bytes(0..16)
            .await
            .expect_err("short range fails");
        assert!(
            error
                .to_string()
                .contains("requested 16 bytes, received 15")
        );
        assert_eq!(
            governor
                .snapshot()
                .expect("root snapshot")
                .oracle_memory_used_bytes,
            0
        );
        assert_eq!(pool.reserved(), 0);
    }

    /// An indivisible range above its ceiling is rejected before storage IO.
    #[tokio::test]
    async fn hot_parquet_oversized_single_range_fails_before_io() {
        let fixture = build_hot_causal_fixture();
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let (mut reader, pool) =
            governed_fixture_reader(&fixture, &governor, Arc::clone(&ranges), false);
        let occupied =
            datafusion::execution::memory_pool::MemoryConsumer::new("oversized-range-sibling")
                .register(&pool);
        occupied
            .try_grow(1024 * 1024 * 1024 - 8)
            .expect("occupy query pool except eight bytes");
        let parquet_error = reader
            .get_bytes(0..9)
            .await
            .expect_err("oversized range fails");
        let error = DataFusionError::External(Box::new(parquet_error));
        assert_eq!(
            crate::oracle::map_first_batch_failure(Some(&Err(error))),
            Some(BifrostError::QueryAdmissionRejected)
        );
        assert!(ranges.lock().expect("recorded ranges").is_empty());
        drop(occupied);
        assert_eq!(
            governor
                .snapshot()
                .expect("root snapshot")
                .oracle_memory_used_bytes,
            0
        );
        assert_eq!(pool.reserved(), 0);
    }

    /// Dropping after one production yield releases decoded and ranged ownership.
    #[tokio::test]
    async fn hot_parquet_drop_stops_io_and_releases_memory() {
        let fixture = build_hot_causal_fixture();
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let attempts = Arc::new(AtomicU64::new(0));
        let observed = Arc::clone(&attempts);
        let source = fixture.bytes.clone();
        let query_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
        let exec = HotParquetExec::new(
            vec![HotFileSource {
                location: fixture.path.to_string_lossy().into_owned(),
                size_bytes: fixture.bytes.len(),
            }],
            FileIO::new_with_fs(),
            Arc::clone(&fixture.schema),
            HotParquetGovernance::Leader {
                memory: oracle_memory_resources(&governor, 1024),
                memory_pool: Arc::clone(&query_pool),
                telemetry: Arc::clone(&telemetry),
                query_class: QueryClass::Interactive,
            },
            Arc::new(OracleScanMetricsHandle::default()),
            Vec::new(),
        )
        .with_test_reader(Arc::new(move |_, range| {
            observed.fetch_add(1, Ordering::AcqRel);
            let source = source.clone();
            Box::pin(async move {
                Ok(source.slice(
                    usize::try_from(range.start).expect("range start")
                        ..usize::try_from(range.end).expect("range end"),
                ))
            })
        }));
        let mut stream = exec
            .execute(
                0,
                datafusion::execution::context::SessionContext::new().task_ctx(),
            )
            .expect("production cancellation stream");
        let batch = stream
            .next()
            .await
            .expect("first yielded frame")
            .expect("first yielded batch");
        assert_eq!(batch.num_rows(), 3);
        assert!(query_pool.reserved() > 0);
        assert!(telemetry.memory_bytes.load(Ordering::Acquire) > 0);
        let attempts_at_yield = attempts.load(Ordering::Acquire);
        drop(stream);
        drop(batch);
        assert_eq!(
            governor
                .snapshot()
                .expect("root snapshot")
                .oracle_memory_used_bytes,
            0
        );
        assert_eq!(
            governor
                .snapshot()
                .expect("root snapshot")
                .elastic_memory_used_bytes,
            0
        );
        assert_eq!(query_pool.reserved(), 0);
        assert_eq!(telemetry.memory_bytes.load(Ordering::Acquire), 0);
        assert_eq!(attempts.load(Ordering::Acquire), attempts_at_yield);
    }

    /// Drains a hot plan through the production `OracleQueryStream` terminal owner.
    async fn drain_hot_terminal(telemetry: &Arc<OracleTelemetry>, exec: HotParquetExec) {
        let schema = exec.schema();
        let scan_stats = OracleQueryScanStats::from_plan(&exec, 0);
        let batches = exec
            .execute(
                0,
                datafusion::execution::context::SessionContext::new().task_ctx(),
            )
            .expect("hot terminal stream");
        let mut stream =
            crate::oracle::test_query_stream_from_physical(telemetry, &schema, batches, scan_stats);
        let mut terminal_seen = false;
        while let Some(frame) = stream.frames.next().await {
            if matches!(frame, Ok(QueryStreamFrame::Terminal(_))) {
                terminal_seen = true;
            }
        }
        assert!(terminal_seen, "hot stream did not reach a terminal frame");
    }

    /// Hot reader attempts finalize through the real query stream guard once.
    #[tokio::test]
    async fn hot_reader_boundary_records_causal_attempts_once() {
        let fixture = build_hot_causal_fixture();
        let requested = fixture.bytes.len();
        let roles = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let memory = oracle_memory_resources(&roles, 1024 * 1024);
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let requested_bytes = Arc::new(AtomicU64::new(0));
        let make_exec = |size_bytes, reader: HotReadOverride| {
            let metrics = Arc::new(OracleScanMetricsHandle::default());
            let exec = HotParquetExec::new(
                vec![HotFileSource {
                    location: fixture.path.to_string_lossy().into_owned(),
                    size_bytes,
                }],
                FileIO::new_with_fs(),
                Arc::clone(&fixture.schema),
                HotParquetGovernance::Leader {
                    memory: memory.clone(),
                    memory_pool: crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
                    telemetry: Arc::clone(&telemetry),
                    query_class: QueryClass::Interactive,
                },
                Arc::clone(&metrics),
                Vec::new(),
            )
            .with_test_reader(reader);
            (exec, metrics)
        };

        let (success, _) = make_exec(
            requested,
            counting_slice_reader(&fixture.bytes, &requested_bytes),
        );
        drain_hot_terminal(&telemetry, success).await;

        let failed_requested = Arc::clone(&requested_bytes);
        let (failed, _) = make_exec(
            7,
            Arc::new(move |_, range| {
                failed_requested.fetch_add(range.end - range.start, Ordering::AcqRel);
                Box::pin(async { Err(DataFusionError::Execution("expected hot error".to_owned())) })
            }),
        );
        drain_hot_terminal(&telemetry, failed).await;

        let pending_requested = Arc::clone(&requested_bytes);
        let (pending, _) = make_exec(
            11,
            Arc::new(move |_, range| {
                pending_requested.fetch_add(range.end - range.start, Ordering::AcqRel);
                Box::pin(async { std::future::pending::<DataFusionResult<bytes::Bytes>>().await })
            }),
        );
        let schema = pending.schema();
        let scan_stats = OracleQueryScanStats::from_plan(&pending, 0);
        let batches = pending
            .execute(
                0,
                datafusion::execution::context::SessionContext::new().task_ctx(),
            )
            .expect("pending hot stream");
        let mut pending_stream = crate::oracle::test_query_stream_from_physical(
            &telemetry, &schema, batches, scan_stats,
        );
        assert!(matches!(
            pending_stream.frames.next().await,
            Some(Ok(QueryStreamFrame::Schema(_)))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), pending_stream.frames.next())
                .await
                .is_err()
        );
        drop(pending_stream);

        let (retry, _) = make_exec(
            requested,
            counting_slice_reader(&fixture.bytes, &requested_bytes),
        );
        drain_hot_terminal(&telemetry, retry).await;

        assert_interactive_scan_counters(
            &recorder.snapshot(),
            requested_bytes.load(Ordering::Acquire),
            4,
        );
    }

    /// Pins the closed predicate classifier, the projection-closure
    /// algorithm, and the `Inexact`/`Unsupported` pushdown report against the
    /// full physical schema and a requested output projection.
    ///
    /// Covers: supported `AND` conjunction of typed comparisons and a null
    /// leaf; a literal-on-the-left comparison normalized by reversing the
    /// operator; the exact stable-dedup closure order (scan output, then
    /// first-occurrence predicate columns, then the always-present hidden
    /// tenant column); and every closed-subset-violating shape (`OR`, `NOT`,
    /// cast, column-to-column, non-finite float) reported `Unsupported`.
    #[test]
    fn closed_predicate_projection_contract() {
        use datafusion::logical_expr::{col, lit};
        use datafusion::scalar::ScalarValue;
        use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

        let physical_schema = Schema::new(vec![
            Field::new("service_name", DataType::Utf8, true),
            Field::new("duration_ms", DataType::Int64, true),
            Field::new("wyrd_event_time", DataType::Int64, true),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
        ]);

        // Supported AND conjunction: comparison + null-check, and a
        // literal-on-the-left comparison normalized by operator reversal.
        let supported = col("service_name")
            .eq(lit("api"))
            .and(col("duration_ms").is_not_null())
            .and(lit(500_i64).gt(col("duration_ms")));
        match classify_filter(&supported) {
            FilterClassification::Supported(leaves) => {
                assert_eq!(
                    leaves,
                    vec![
                        ScanPredicate::Eq(
                            "service_name".to_string(),
                            ScanLiteral::Utf8("api".to_string())
                        ),
                        ScanPredicate::IsNotNull("duration_ms".to_string()),
                        // `500 > duration_ms` normalizes to `duration_ms < 500`.
                        ScanPredicate::Lt("duration_ms".to_string(), ScanLiteral::I64(500)),
                    ]
                );
            }
            FilterClassification::Unsupported => panic!("expected supported classification"),
        }

        // Closed-subset violations: OR, NOT, cast, column-to-column, and a
        // non-finite float literal all classify Unsupported.
        let or_expr = col("service_name")
            .eq(lit("api"))
            .or(col("duration_ms").eq(lit(1_i64)));
        assert!(matches!(
            classify_filter(&or_expr),
            FilterClassification::Unsupported
        ));
        let not_expr = Expr::Not(Box::new(col("duration_ms").is_null()));
        assert!(matches!(
            classify_filter(&not_expr),
            FilterClassification::Unsupported
        ));
        let cast_expr = Expr::Cast(datafusion::logical_expr::Cast::new(
            Box::new(col("duration_ms")),
            DataType::Utf8,
        ))
        .eq(lit("500"));
        assert!(matches!(
            classify_filter(&cast_expr),
            FilterClassification::Unsupported
        ));
        let column_to_column = col("duration_ms").eq(col("wyrd_event_time"));
        assert!(matches!(
            classify_filter(&column_to_column),
            FilterClassification::Unsupported
        ));
        let non_finite = col("duration_ms").eq(Expr::Literal(
            ScalarValue::Float64(Some(f64::INFINITY)),
            None,
        ));
        assert!(matches!(
            classify_filter(&non_finite),
            FilterClassification::Unsupported
        ));

        // `supports_filters_pushdown` reports exactly Inexact/Unsupported per
        // classification, never Exact — DataFusion always keeps its residual.
        let provider_filters = [&supported, &or_expr];
        let pushdown = provider_filters
            .iter()
            .map(
                |filter| match classify_filter_for_schema(&physical_schema, filter) {
                    FilterClassification::Supported(_) => TableProviderFilterPushDown::Inexact,
                    FilterClassification::Unsupported => TableProviderFilterPushDown::Unsupported,
                },
            )
            .collect::<Vec<_>>();
        assert_eq!(
            pushdown,
            vec![
                TableProviderFilterPushDown::Inexact,
                TableProviderFilterPushDown::Unsupported,
            ]
        );
    }

    /// The projection closure is `scan output + predicate columns + hidden
    /// tenant column`, in that order, stably deduplicated.
    ///
    /// Order is part of the contract, not an implementation detail: the
    /// closure is hashed into the assignment-authority digest, so two
    /// components deriving the same set in a different order would produce
    /// different digests and refuse each other's assignments.
    #[test]
    fn closed_predicate_projection_closure_order_is_stable() {
        use datafusion::logical_expr::{col, lit};

        let physical_schema = Schema::new(vec![
            Field::new("service_name", DataType::Utf8, true),
            Field::new("duration_ms", DataType::Int64, true),
            Field::new("wyrd_event_time", DataType::Int64, true),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
        ]);
        let public_schema = schema_without(&physical_schema, DATA_TENANT_ID).unwrap();
        let supported = col("service_name")
            .eq(lit("api"))
            .and(col("duration_ms").is_not_null());

        // Projection-closure order: requested scan output first, then the
        // first occurrence of each predicate column in filter order, then
        // the always-present hidden tenant column, stably deduplicated
        // (`duration_ms` appears in both the projection and the predicates).
        let leaves = match classify_filter(&supported) {
            FilterClassification::Supported(leaves) => leaves,
            FilterClassification::Unsupported => unreachable!(),
        };
        let projection = vec![
            public_schema.index_of("duration_ms").unwrap(),
            public_schema.index_of("wyrd_event_time").unwrap(),
        ];
        let closure = required_columns_closure(&public_schema, Some(&projection), &leaves);
        assert_eq!(
            closure,
            vec![
                "duration_ms".to_string(),
                "wyrd_event_time".to_string(),
                "service_name".to_string(),
                DATA_TENANT_ID.to_string(),
            ]
        );

        // A `None` projection closes over every public column.
        let full_closure = required_columns_closure(&public_schema, None, &[]);
        assert_eq!(
            full_closure,
            vec![
                "service_name".to_string(),
                "duration_ms".to_string(),
                "wyrd_event_time".to_string(),
                DATA_TENANT_ID.to_string(),
            ]
        );
    }

    /// Column ownership, not qualification, decides pushdown eligibility.
    ///
    /// A planned `SELECT ... WHERE value = 'x'` reaches the provider with the
    /// reference already qualified against the registered relation
    /// (`vala.bifrost.<table>.service_name`), never bare. Treating a qualified
    /// reference as outside the closed subset therefore makes pushdown
    /// unreachable for every real query while still passing hand-built
    /// unqualified unit fixtures, so this pins both halves: a qualified
    /// reference to an owned column is `Inexact`, and a reference to a column
    /// this table does not own is `Unsupported` regardless of qualification.
    #[test]
    fn qualified_column_references_push_down_only_for_owned_columns() {
        use datafusion::common::{Column, TableReference};
        use datafusion::logical_expr::{col, lit};
        use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

        let physical_schema = Schema::new(vec![
            Field::new("service_name", DataType::Utf8, true),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
        ]);

        let qualified_owned = Expr::Column(Column::new(
            Some(TableReference::full("vala", "bifrost", "events")),
            "service_name",
        ))
        .eq(lit("api"));
        match classify_filter_for_schema(&physical_schema, &qualified_owned) {
            FilterClassification::Supported(leaves) => assert_eq!(
                leaves,
                vec![ScanPredicate::Eq(
                    "service_name".to_string(),
                    ScanLiteral::Utf8("api".to_string())
                )]
            ),
            FilterClassification::Unsupported => {
                panic!("a qualified reference to an owned column must push down")
            }
        }

        // Same shape, column this table does not own: rejected by the schema
        // check rather than by the expression classifier.
        let qualified_foreign = Expr::Column(Column::new(
            Some(TableReference::full("vala", "bifrost", "other")),
            "other_column",
        ))
        .eq(lit("api"));
        assert!(matches!(
            classify_filter_for_schema(&physical_schema, &qualified_foreign),
            FilterClassification::Unsupported
        ));
        assert!(matches!(
            classify_filter_for_schema(&physical_schema, &col("other_column").eq(lit("api"))),
            FilterClassification::Unsupported
        ));
    }

    /// Only a missing pinned object is classified as a stale-object race; any
    /// other storage failure keeps its outage classification.
    ///
    /// This is the seam `iceberg_datafusion_error` uses to decide whether a
    /// vanished Iceberg object is retryable state or a genuine backend failure,
    /// so both the positive filesystem cause and a negative sibling cause are
    /// pinned here.
    #[test]
    fn oracle_exec_detects_only_not_found_causes_in_an_error_chain() {
        let stale = std::io::Error::from(std::io::ErrorKind::NotFound);
        let outage = std::io::Error::from(std::io::ErrorKind::PermissionDenied);

        assert!(error_chain_contains_not_found(&stale));
        assert!(!error_chain_contains_not_found(&outage));
        assert!(error_chain_contains_not_found(&OracleIcebergStaleObject {
            source: Box::new(std::io::Error::from(std::io::ErrorKind::NotFound)),
        }));
    }

    /// Rows written into one governed hot fixture for batch-shaping proofs.
    ///
    /// Large enough that a floor-shaped session batch size yields many batches
    /// and a maximum-shaped one yields a single batch, which is what separates
    /// "reads at the admitted size" from "reads at a fixed constant".
    const HOT_BATCH_FIXTURE_ROWS: i64 = 64;

    /// Writes one Parquet source of [`HOT_BATCH_FIXTURE_ROWS`] rows in a single
    /// row group, so batch counts depend only on the configured batch size.
    fn build_hot_batch_fixture() -> HotCausalFixture {
        let directory = tempfile::tempdir().expect("hot batch fixture directory");
        let path = directory.path().join("hot-batch.parquet");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(
                (0..HOT_BATCH_FIXTURE_ROWS).collect::<Vec<_>>(),
            )) as ArrayRef],
        )
        .expect("hot batch fixture batch");
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            File::create(&path).expect("hot batch fixture file"),
            Arc::clone(&schema),
            None,
        )
        .expect("hot batch fixture writer");
        writer.write(&batch).expect("hot batch fixture write");
        writer.close().expect("hot batch fixture close");
        let bytes = bytes::Bytes::from(std::fs::read(&path).expect("hot batch fixture bytes"));
        HotCausalFixture {
            _directory: directory,
            path,
            schema,
            bytes,
        }
    }

    /// Builds one hot leaf over `fixture` under an explicit governance mode.
    ///
    /// `size_bytes` is the manifest size the leaf trusts, so a caller can
    /// deliberately mis-state it to drive the storage-failure branch.
    fn hot_exec_for(
        fixture: &HotCausalFixture,
        governance: HotParquetGovernance,
        metrics: &Arc<OracleScanMetricsHandle>,
        size_bytes: usize,
    ) -> HotParquetExec {
        HotParquetExec::new(
            vec![HotFileSource {
                location: fixture.path.to_string_lossy().into_owned(),
                size_bytes,
            }],
            FileIO::new_with_fs(),
            Arc::clone(&fixture.schema),
            governance,
            Arc::clone(metrics),
            Vec::new(),
        )
    }

    /// Builds one task context whose admitted session batch size is `batch_size`.
    fn task_context_with_batch_size(batch_size: usize) -> Arc<TaskContext> {
        datafusion::execution::context::SessionContext::new_with_config(
            datafusion::prelude::SessionConfig::new().with_batch_size(batch_size),
        )
        .task_ctx()
    }

    /// Drains one hot stream into its per-batch row counts.
    async fn drain_hot_row_counts(
        mut stream: datafusion::execution::SendableRecordBatchStream,
    ) -> Vec<usize> {
        let mut counts = Vec::new();
        while let Some(batch) = stream.next().await {
            counts.push(batch.expect("hot batch decodes").num_rows());
        }
        counts
    }

    /// Both governance modes decode at the admitted session batch size and
    /// produce identical rows and scan evidence.
    ///
    /// A floor-shaped grant must split the same file into many small batches
    /// and a maximum-shaped grant must return it as one, through the leader's
    /// governor-backed mode and a follower's request-local pool alike. This is
    /// the proof that the hot leaf no longer carries a batch size of its own.
    #[tokio::test]
    async fn hot_parquet_decodes_at_the_admitted_batch_size_in_both_modes() {
        let fixture = build_hot_batch_fixture();
        let size = fixture.bytes.len();
        let rows = usize::try_from(HOT_BATCH_FIXTURE_ROWS).expect("fixture rows fit usize");
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));

        for (batch_size, expected_batches) in [(8_usize, 8_usize), (8_192, 1)] {
            let leader_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
            let leader_metrics = Arc::new(OracleScanMetricsHandle::default());
            let leader = hot_exec_for(
                &fixture,
                HotParquetGovernance::Leader {
                    memory: oracle_memory_resources(&governor, 1024 * 1024),
                    memory_pool: Arc::clone(&leader_pool),
                    telemetry: Arc::clone(&telemetry),
                    query_class: QueryClass::Interactive,
                },
                &leader_metrics,
                size,
            );
            let leader_counts = drain_hot_row_counts(
                leader
                    .execute(0, task_context_with_batch_size(batch_size))
                    .expect("leader hot stream"),
            )
            .await;

            let follower_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
            let follower_metrics = Arc::new(OracleScanMetricsHandle::default());
            let follower = hot_exec_for(
                &fixture,
                HotParquetGovernance::Follower {
                    memory_pool: Arc::clone(&follower_pool),
                },
                &follower_metrics,
                size,
            );
            let follower_counts = drain_hot_row_counts(
                follower
                    .execute(0, task_context_with_batch_size(batch_size))
                    .expect("follower hot stream"),
            )
            .await;

            assert_eq!(leader_counts, follower_counts);
            assert_eq!(leader_counts.len(), expected_batches);
            assert_eq!(leader_counts.iter().sum::<usize>(), rows);
            assert!(leader_counts.iter().all(|count| *count <= batch_size));
            assert_eq!(
                leader_metrics.terminal_values(),
                follower_metrics.terminal_values()
            );
            assert_hot_scan_baselines(&governor, &leader_pool, &telemetry);
            assert_eq!(follower_pool.reserved(), 0);
        }
    }

    /// Follower governance returns every retained byte to its request-local
    /// pool on success, storage failure, cancellation, and retry.
    ///
    /// A follower holds no leader gauges, so the request-local pool and the
    /// root governor are the only balances that can leak; each attempt is
    /// checked back to zero and the retry reproduces the successful attempt's
    /// scan evidence exactly.
    #[tokio::test]
    async fn hot_parquet_follower_mode_balances_its_request_local_pool() {
        let fixture = build_hot_batch_fixture();
        let size = fixture.bytes.len();
        let rows = usize::try_from(HOT_BATCH_FIXTURE_ROWS).expect("fixture rows fit usize");
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);

        let success_metrics = Arc::new(OracleScanMetricsHandle::default());
        let success = hot_exec_for(
            &fixture,
            HotParquetGovernance::Follower {
                memory_pool: Arc::clone(&pool),
            },
            &success_metrics,
            size,
        );
        let counts = drain_hot_row_counts(
            success
                .execute(0, task_context_with_batch_size(8))
                .expect("follower success stream"),
        )
        .await;
        assert_eq!(counts.iter().sum::<usize>(), rows);
        assert_eq!(pool.reserved(), 0);

        // A mis-stated manifest size drives the storage-failure branch, which
        // must still return its pre-IO range reservation.
        let failed_metrics = Arc::new(OracleScanMetricsHandle::default());
        let failed = hot_exec_for(
            &fixture,
            HotParquetGovernance::Follower {
                memory_pool: Arc::clone(&pool),
            },
            &failed_metrics,
            size.saturating_add(1),
        );
        let mut failed_stream = failed
            .execute(0, task_context_with_batch_size(8))
            .expect("follower failure stream");
        assert!(
            failed_stream
                .next()
                .await
                .expect("follower failure frame")
                .is_err()
        );
        drop(failed_stream);
        assert_eq!(pool.reserved(), 0);

        // Dropping mid-stream releases the decoded batch reservation the
        // yielded batch was still holding.
        let cancelled_metrics = Arc::new(OracleScanMetricsHandle::default());
        let cancelled = hot_exec_for(
            &fixture,
            HotParquetGovernance::Follower {
                memory_pool: Arc::clone(&pool),
            },
            &cancelled_metrics,
            size,
        );
        let mut cancelled_stream = cancelled
            .execute(0, task_context_with_batch_size(8))
            .expect("follower cancellation stream");
        let retained = cancelled_stream
            .next()
            .await
            .expect("follower first frame")
            .expect("follower first batch");
        assert!(pool.reserved() > 0);
        drop(cancelled_stream);
        drop(retained);
        assert_eq!(pool.reserved(), 0);

        let retry_metrics = Arc::new(OracleScanMetricsHandle::default());
        let retry = hot_exec_for(
            &fixture,
            HotParquetGovernance::Follower {
                memory_pool: Arc::clone(&pool),
            },
            &retry_metrics,
            size,
        );
        let retry_counts = drain_hot_row_counts(
            retry
                .execute(0, task_context_with_batch_size(8))
                .expect("follower retry stream"),
        )
        .await;
        assert_eq!(retry_counts, counts);
        assert_eq!(
            retry_metrics.terminal_values(),
            success_metrics.terminal_values()
        );
        assert_eq!(pool.reserved(), 0);
        assert_eq!(
            governor
                .snapshot()
                .expect("root snapshot")
                .oracle_memory_used_bytes,
            0
        );
    }
}
