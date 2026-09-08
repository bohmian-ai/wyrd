//! `DataFusion` physical sources and invariant operators owned by Oracle.
//!
//! Every table enters `DataFusion` through one disjoint source union. Tenant
//! validation surrounds that union so a foreign row cannot influence a filter,
//! join, aggregate, or limit.

use datafusion::common::tree_node::TreeNodeRecursion;
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
use datafusion::physical_plan::aggregates::AggregateExec;
use datafusion::physical_plan::execution_plan::{
    Boundedness, EmissionType, PlanProperties, SchedulingType,
};
use datafusion::physical_plan::joins::{HashJoinExec, SortMergeJoinExec};
use datafusion::physical_plan::metrics::{
    Count, ExecutionPlanMetricsSet, Metric, MetricValue, MetricsSet,
};
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::union::UnionExec;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, Partitioning,
    SendableRecordBatchStream,
};
use datafusion_distributed::NetworkBoundaryExt as _;
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
use wyrd_spec::vala::api::{BifrostSecurityPhase, BifrostSecurityViolationKind, QueryClass};
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
        if let Some(source) = plan.downcast_ref::<DataSourceExec>()
            && let Some(config) = source.data_source().downcast_ref::<FileScanConfig>()
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
        if let Some(source) = plan.downcast_ref::<OracleIcebergScanExec>() {
            stats.scan_handles.push(Arc::clone(&source.metrics));
        }
        if let Some(source) = plan.downcast_ref::<HotParquetExec>() {
            stats.scan_handles.push(Arc::clone(source.metrics()));
        }
        // A remote placeholder hides the leaf it substituted from `children`
        // so the distributed planner keeps scaling it as one. The leader still
        // executes that leaf whenever the stage stays in the head, so scan
        // evidence has to descend into it explicitly or every locally served
        // distributed-shaped query would report no source at all.
        if let Some(source) = plan.downcast_ref::<super::codec::RemoteSourcePlaceholderExec>()
            && let Some(local) = source.local_plan()
        {
            Self::visit(local.as_ref(), stats);
        }
        for child in plan.children() {
            Self::visit(child.as_ref(), stats);
        }
    }
}

impl OracleQueryScanStats {
    /// Opens one participant-reported accumulator for a distributed plan.
    ///
    /// The Analytical leader's own plan tree contains no executed scan: every
    /// leaf below a stage boundary runs on a follower. This accumulator is the
    /// same one an Interactive remote scan reports through, so terminal
    /// aggregation stays a single path; only the transport that fills it
    /// differs.
    pub(crate) fn open_distributed_scan(&mut self) -> Arc<RemoteScanMetrics> {
        let handle = Arc::new(RemoteScanMetrics::default());
        self.remote_handles.push(Arc::clone(&handle));
        handle
    }
}

/// Wyrd's own scan-evidence metric names, carried over upstream's metric wire.
///
/// A Wyrd scan does not report physical bytes through a `DataFusion` metric: an
/// Iceberg or hot-Parquet source accounts them in its own
/// [`OracleScanMetricsHandle`], which never leaves the node that owns it. An
/// Analytical follower therefore republishes its terminal scan evidence under
/// these names so the coordinator can read it back from the same metric
/// transport upstream already uses for every other stage metric.
pub(crate) const WYRD_BYTES_SCANNED_METRIC: &str = "wyrd_bytes_scanned";
/// Files a follower leaf actually opened, published as [`WYRD_BYTES_SCANNED_METRIC`] is.
pub(crate) const WYRD_FILES_SCANNED_METRIC: &str = "wyrd_files_scanned";
/// File partitions a follower leaf actually read.
pub(crate) const WYRD_PARTITIONS_SCANNED_METRIC: &str = "wyrd_partitions_scanned";
/// Row groups a follower leaf retained after statistics pruning.
pub(crate) const WYRD_ROW_GROUPS_SCANNED_METRIC: &str = "wyrd_row_groups_scanned";
/// Row groups a follower leaf skipped by statistics pruning.
pub(crate) const WYRD_ROW_GROUPS_PRUNED_METRIC: &str = "wyrd_row_groups_pruned";

/// Publishes one resolved follower leaf's terminal scan evidence as metrics.
///
/// The evidence is read through the same collector the leader uses on its own
/// plan, so an Analytical follower reports exactly what an Interactive leader
/// would have reported for the identical scan. Bytes are omitted rather than
/// zeroed when the source never reported them, preserving absent-versus-zero.
pub(crate) fn analytical_leaf_scan_metrics(plan: &dyn ExecutionPlan) -> MetricsSet {
    let mut stats = OracleQueryScanStats::from_plan(plan, 0);
    stats.finalize();
    let mut published = MetricsSet::new();
    let mut publish = |name: &'static str, value: u64| {
        let count = Count::new();
        count.add(usize::try_from(value).unwrap_or(usize::MAX));
        published.push(Arc::new(Metric::new(
            MetricValue::Count {
                name: name.into(),
                count,
            },
            None,
        )));
    };
    if let Some(bytes) = stats.physical_bytes_scanned {
        publish(WYRD_BYTES_SCANNED_METRIC, bytes);
    }
    publish(WYRD_FILES_SCANNED_METRIC, stats.files_scanned);
    publish(WYRD_PARTITIONS_SCANNED_METRIC, stats.partitions_scanned);
    publish(WYRD_ROW_GROUPS_SCANNED_METRIC, stats.row_groups_scanned);
    publish(WYRD_ROW_GROUPS_PRUNED_METRIC, stats.row_groups_pruned);
    published
}

/// Derives the admitted query class from one already-built physical root.
///
/// The pinned distributed planner is the only classifier: it returns a
/// `DistributedExec` root exactly when it decided the plan needs cross-node
/// stages, and returns the original non-distributed root otherwise. Reading
/// the exact root type is therefore the whole decision — there is no candidate
/// heuristic, operator allowlist, or second build behind it. A `DistributedExec`
/// nested below some other root is not the planner's verdict for this query and
/// stays `Interactive`.
pub(crate) fn query_class_for_root(plan: &dyn ExecutionPlan) -> QueryClass {
    if plan
        .downcast_ref::<datafusion_distributed::DistributedExec>()
        .is_some()
    {
        QueryClass::Analytical
    } else {
        QueryClass::Interactive
    }
}

/// Folds a completed distributed plan's follower scan metrics into `sink`.
///
/// Upstream ships each task's metrics back to the coordinator over its own
/// coordinator channel once that task finishes, and
/// `rewrite_distributed_plan_with_metrics` waits for all of them before
/// attaching them to the coordinator's plan tree. Reading them here is what
/// lets the leader stay the single emitter of a leader-owned physical-scan
/// metric while the reads themselves happened elsewhere.
///
/// A plan that is not distributed, or whose metrics never arrive, contributes
/// nothing rather than a fabricated zero.
///
/// # Errors
///
/// Returns the pinned dependency's rewrite error unchanged. The error is the
/// caller's, not this function's, to interpret: the plan it would otherwise
/// have to fall back on never carried the executed stages' counters, so
/// reporting one as this query's physical evidence would be a fabrication.
/// Nothing is recorded into `sink` on that path.
pub(crate) async fn record_distributed_scan_metrics(
    plan: Arc<dyn ExecutionPlan>,
    sink: Arc<RemoteScanMetrics>,
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    let with_metrics = datafusion_distributed::rewrite_distributed_plan_with_metrics(
        plan,
        datafusion_distributed::DistributedMetricsFormat::Aggregated,
    )
    .await?;
    let mut totals = wyrd_spec::vala::api::WorkerScanStats::default();
    fold_distributed_scan_metrics(&with_metrics, &mut totals);
    sink.record_footer(totals);
    Ok(with_metrics)
}

/// Accumulates one rewritten plan node's scan metrics, then its whole subtree.
///
/// A distributed plan is not one tree. Each stage below a network boundary
/// hangs off that boundary's input stage rather than off its children, so a
/// plain child walk sees only the coordinator's own stage and reports a query
/// that scanned nothing. Descending both edges is what reaches the follower
/// leaves where the reads actually happened.
fn fold_distributed_scan_metrics(
    node: &Arc<dyn ExecutionPlan>,
    totals: &mut wyrd_spec::vala::api::WorkerScanStats,
) {
    if let Some(metrics) = node.metrics() {
        let metrics = metrics.aggregate_by_name();
        for name in ["bytes_scanned", WYRD_BYTES_SCANNED_METRIC] {
            if let Some(bytes) = sum_named_count(&metrics, name) {
                totals.bytes_scanned =
                    Some(totals.bytes_scanned.unwrap_or(0).saturating_add(bytes));
            }
        }
        totals.files_scanned = totals
            .files_scanned
            .saturating_add(sum_named_count(&metrics, WYRD_FILES_SCANNED_METRIC).unwrap_or(0));
        totals.partitions_scanned = totals
            .partitions_scanned
            .saturating_add(sum_named_count(&metrics, WYRD_PARTITIONS_SCANNED_METRIC).unwrap_or(0));
        for name in ROW_GROUPS_MATCHED_METRICS {
            totals.row_groups_scanned = totals
                .row_groups_scanned
                .saturating_add(sum_named_count(&metrics, name).unwrap_or(0));
        }
        for name in ROW_GROUPS_PRUNED_METRICS {
            totals.row_groups_pruned = totals
                .row_groups_pruned
                .saturating_add(sum_named_count(&metrics, name).unwrap_or(0));
        }
    }
    if let Some(boundary) = node.as_network_boundary()
        && let datafusion_distributed::Stage::Local(stage) = boundary.input_stage()
    {
        fold_distributed_scan_metrics(&stage.plan, totals);
    }
    for child in node.children() {
        fold_distributed_scan_metrics(child, totals);
    }
}

/// Reads the completed plan's own physical shape and retained spill evidence.
///
/// The output sort is identified by carrying the root's own output schema,
/// which is unique for this baseline's projection; zero or several matches
/// return `None` rather than guessing, so a caller can never read a partial
/// sort's metrics as the query's. The plan must already have been executed and
/// its stream dropped: `MetricsSet` values are retained on the node, and an
/// absent metric is distinguishable from a zero one only before execution.
///
/// # Panics
///
/// Does not panic.
pub(crate) fn output_sort_evidence(
    root: &Arc<dyn ExecutionPlan>,
) -> Option<super::analytical::AnalyticalPhysicalEvidence> {
    let mut walk = PhysicalEvidenceWalk::default();
    walk.visit(root, root.schema().as_ref());
    let [sort] = walk.sorts.as_slice() else {
        return None;
    };
    let mut aggregate_group_types: Vec<String> = walk.aggregate_group_types.into_iter().collect();
    aggregate_group_types.sort();
    Some(super::analytical::AnalyticalPhysicalEvidence {
        sort_schema: sort.schema.clone(),
        sort_ordering: sort.ordering.clone(),
        spill_count: sort.spill_count,
        spilled_bytes: sort.spilled_bytes,
        spilled_rows: sort.spilled_rows,
        aggregate_group_types,
        join_build_schemas: walk.join_build_schemas,
    })
}

/// One output sort's identity and retained spill counters.
#[derive(Debug, Clone)]
struct OutputSortNode {
    /// Field names the sort emits, in output order.
    schema: Vec<String>,
    /// Ordering the sort holds, rendered exactly as the plan states it.
    ordering: String,
    /// Times this sort spilled a run to scratch.
    spill_count: u64,
    /// Bytes this sort wrote to scratch.
    spilled_bytes: u64,
    /// Rows this sort wrote to scratch.
    spilled_rows: u64,
}

/// Accumulates one plan's physical shape across ordinary and distributed edges.
///
/// A distributed plan hides its remote stages behind network boundaries, so a
/// walk over `children()` alone would see only the coordinator's fragment.
/// Crossing `Stage::Local` reaches the stages this process itself owns, which
/// is where the output sort and its retained metrics live.
#[derive(Debug, Default)]
struct PhysicalEvidenceWalk {
    /// Every sort carrying the root's output schema.
    sorts: Vec<OutputSortNode>,
    /// Distinct grouping-column types across every aggregate in the plan.
    aggregate_group_types: std::collections::BTreeSet<String>,
    /// Field names of each equi-join's left-input child, in visit order.
    join_build_schemas: Vec<Vec<String>>,
    /// Nodes already recorded, identified by address.
    ///
    /// A `Stage::Local` boundary and its ordinary child are the same node in a
    /// distributed plan, so a walk that follows both edges would count one sort
    /// or join twice and then report the plan as ambiguous.
    seen: std::collections::HashSet<*const ()>,
}

impl PhysicalEvidenceWalk {
    /// Records `node`'s own contribution, then descends into everything it owns.
    fn visit(&mut self, node: &Arc<dyn ExecutionPlan>, schema: &Schema) {
        if !self.seen.insert(Arc::as_ptr(node).cast::<()>()) {
            return;
        }
        if let Some(sort) = node.downcast_ref::<SortExec>()
            && sort.schema().fields() == schema.fields()
        {
            // Metrics come from the visited node, never from the downcast one.
            // The distributed metrics rewrite replaces a follower-executed node
            // with a transparent wrapper that delegates its downcast to the
            // original operator: the downcast succeeds, but the operator it
            // yields never executed here and carries an empty `MetricsSet`,
            // while the wrapper holds the counters the follower reported.
            let metrics = node.metrics().unwrap_or_default();
            // Typed first, then by name: a stage that executed on a follower
            // returns its counters through the distributed metrics rewrite,
            // which rebuilds them as plain named counts rather than the typed
            // spill metrics the local accessors recognize. Reading only one of
            // the two forms reports a real spill as zero.
            let named = metrics.aggregate_by_name();
            let spilled = |typed: Option<usize>, name: &str| -> u64 {
                let typed = typed.and_then(|value| u64::try_from(value).ok());
                typed
                    .filter(|value| *value > 0)
                    .or_else(|| sum_named_count(&named, name))
                    .unwrap_or(0)
            };
            self.sorts.push(OutputSortNode {
                schema: field_names(sort.schema().as_ref()),
                ordering: sort.expr().to_string(),
                spill_count: spilled(metrics.spill_count(), "spill_count"),
                spilled_bytes: spilled(metrics.spilled_bytes(), "spilled_bytes"),
                spilled_rows: spilled(metrics.spilled_rows(), "spilled_rows"),
            });
        }
        if let Some(aggregate) = node.downcast_ref::<AggregateExec>() {
            let input = aggregate.input().schema();
            for (expr, _) in aggregate.group_expr().expr() {
                if let Ok(data_type) = expr.data_type(input.as_ref()) {
                    self.aggregate_group_types.insert(data_type.to_string());
                }
            }
        }
        // Both equi-join operators are recorded. Which one a query gets is not
        // a property of the statement: the retained planning shape disables
        // hash joins at the memory floor precisely because `HashJoinExec`
        // cannot spill, so a plan built at the floor carries a sort-merge join
        // instead. Reading only one operator would report a real join as
        // absent on every query the floor shape planned.
        if let Some(join) = node.downcast_ref::<HashJoinExec>() {
            self.join_build_schemas
                .push(field_names(join.left().schema().as_ref()));
        }
        if let Some(join) = node.downcast_ref::<SortMergeJoinExec>() {
            self.join_build_schemas
                .push(field_names(join.left().schema().as_ref()));
        }
        if let Some(boundary) = node.as_network_boundary()
            && let datafusion_distributed::Stage::Local(stage) = boundary.input_stage()
        {
            self.visit(&stage.plan, schema);
        }
        for child in node.children() {
            self.visit(child, schema);
        }
    }
}

/// Renders one schema's field names in output order.
fn field_names(schema: &Schema) -> Vec<String> {
    schema
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect()
}

/// `DataFusion` Parquet metrics naming row groups a scan actually read.
const ROW_GROUPS_MATCHED_METRICS: [&str; 3] = [
    "row_groups_matched_statistics",
    "row_groups_matched_bloom_filter",
    WYRD_ROW_GROUPS_SCANNED_METRIC,
];

/// `DataFusion` Parquet metrics naming row groups a scan skipped.
const ROW_GROUPS_PRUNED_METRICS: [&str; 3] = [
    "row_groups_pruned_statistics",
    "row_groups_pruned_bloom_filter",
    WYRD_ROW_GROUPS_PRUNED_METRIC,
];

/// Reads one named counter from an aggregated metric set.
fn sum_named_count(metrics: &MetricsSet, name: &str) -> Option<u64> {
    match metrics.sum_by_name(name) {
        Some(MetricValue::Count { count, .. }) => Some(count.value() as u64),
        _ => None,
    }
}

/// Records the exact column closure each Iceberg physical scan is built with.
///
/// The compacted read path is only observable from a journey through its
/// physical byte counter, and that counter cannot separate a broad read from a
/// narrow one on a small data file: the dependency prefetches file tail for
/// Parquet metadata and coalesces nearby byte ranges, so a file below those
/// thresholds is fetched whole whatever columns were asked for. That is a real
/// property of the current reader policy, not of Wyrd's projection, and tuning
/// it is a separate production decision with its own evidence requirements.
///
/// This owner therefore exposes the fact the journey actually needs — which
/// columns the signed closure handed to the Iceberg scan — without changing
/// what any metric means and without adding production logging. It exists only
/// under `test-support`; the production build has no recorder and no call site.
#[cfg(feature = "test-support")]
pub mod iceberg_projection_probe {
    use std::sync::Mutex;

    /// Closures observed since the last [`reset`], in scan-construction order.
    ///
    /// `None` is retained rather than skipped: a scan built with no projection
    /// at all is exactly the regression a projection proof must catch, so it
    /// has to be visible to the assertion rather than absent from it.
    static OBSERVED: Mutex<Vec<Option<Vec<String>>>> = Mutex::new(Vec::new());

    /// Discards every previously observed closure.
    ///
    /// A journey calls this immediately before the query it intends to
    /// observe, so the closures it reads back belong to that query alone.
    pub fn reset() {
        if let Ok(mut observed) = OBSERVED.lock() {
            observed.clear();
        }
    }

    /// Returns the closures observed since the last [`reset`].
    #[must_use]
    pub fn observed() -> Vec<Option<Vec<String>>> {
        OBSERVED
            .lock()
            .map(|observed| observed.clone())
            .unwrap_or_default()
    }

    /// Records one Iceberg scan's closure as the scan starts its reader.
    pub(super) fn record(projection: Option<&[String]>) {
        if let Ok(mut observed) = OBSERVED.lock() {
            observed.push(projection.map(<[String]>::to_vec));
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
        let scan = plan.downcast_ref::<IcebergTableScan>().ok_or_else(|| {
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
        if let (None, Some(predicate)) = (&self.assigned_files, &self.predicates) {
            builder = builder.with_filter(predicate.clone());
        }
        #[cfg(feature = "test-support")]
        iceberg_projection_probe::record(self.projection.as_deref());
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
                    task.predicate.clone_from(&row_filter);
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
    /// Visits every physical expression this plan owns.
    ///
    /// This plan owns no `PhysicalExpr`, so the traversal reports
    /// [`TreeNodeRecursion::Continue`] without invoking `f`.
    ///
    /// # Errors
    /// Never returns an error; the signature is fixed by the trait.
    fn apply_expressions(
        &self,
        _f: &mut dyn FnMut(
            &Arc<dyn datafusion::physical_expr::PhysicalExpr>,
        ) -> DataFusionResult<TreeNodeRecursion>,
    ) -> DataFusionResult<TreeNodeRecursion> {
        Ok(TreeNodeRecursion::Continue)
    }

    /// Returns the stable physical source name.
    fn name(&self) -> &'static str {
        "OracleIcebergScanExec"
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

/// Builds one hot object's node-wide decoded-metadata identity.
///
/// The identity is taken from the durable `vala.file_list` row rather than from
/// the object's location, because the location is a path and two distinct
/// durable objects must never share a decode. A row without a usable writer
/// checksum is refused here rather than keyed on a zero digest, which would let
/// every unchecksummed object collide on one entry.
///
/// # Errors
/// Returns `BifrostError::MetadataMismatch` when the row carries no decodable
/// nonzero SHA-256.
pub(super) fn hot_metadata_key(
    file: &vala_sql::row_types::file_list::HotFileRow,
    size_bytes: usize,
) -> Result<crate::storage::HotMetadataKey, BifrostError> {
    let size_bytes_u64 = u64::try_from(size_bytes).map_err(|_| BifrostError::MetadataMismatch {
        detail: "hot file size exceeds process bounds".to_owned(),
    })?;
    let checksum = file
        .file_checksum
        .as_deref()
        .and_then(|hex| hex::decode(hex).ok())
        .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
        .filter(|checksum| checksum != &[0_u8; 32])
        .ok_or_else(|| BifrostError::MetadataMismatch {
            detail: "hot file row carries no usable object checksum".to_owned(),
        })?;
    Ok(crate::storage::HotMetadataKey::new(
        wyrd_spec::DataTenantId::new(file.data_tenant_id).map_err(|_| {
            BifrostError::MetadataMismatch {
                detail: "hot file row carries a non-v7 tenant identity".to_owned(),
            }
        })?,
        file.table_name.clone(),
        file.file_path.clone(),
        file.id,
        checksum,
        size_bytes_u64,
    ))
}

/// One validated immutable hot-file source selected by the pinned cut.
#[derive(Debug, Clone)]
pub(crate) struct HotFileSource {
    /// Absolute storage path accepted by the pinned table's `FileIO`.
    pub(crate) location: String,
    /// Manifest size used to reserve parent memory before whole-file decode.
    pub(crate) size_bytes: usize,
    /// Immutable `wyrd_event_time` interval this object declares, or the exact
    /// reason it declares none.
    ///
    /// Carried from the durable `vala.file_list` row so a scan can decide the
    /// file before it is opened. Unusable statistics retain the file.
    pub(crate) event_time: crate::catalog::event_time::EventTimeStatistics,
    /// Immutable node-wide identity this object's decoded metadata is keyed by.
    ///
    /// Built from the signed `vala.file_list` facts rather than from the
    /// location, so two queries for the same durable object share one decode
    /// and two distinct objects never collide even under the same path prefix.
    pub(crate) metadata_key: crate::storage::HotMetadataKey,
}

/// One table cut's persisted sources delegated to a frozen remote participant.
///
/// Present only when the pinned cut decided this table's published Iceberg
/// objects and leader hot files are read by another Oracle rather than by this
/// leader. It is private stage authority: the destination is the exact
/// participant the roster froze for this source, and the route handler refuses
/// any stage whose leaves do not all name it.
#[derive(Debug, Clone)]
pub(crate) struct OracleRemoteSource {
    /// Canonical table name both sides mint this cut's scan identities from.
    pub(crate) table: String,
    /// The one frozen participant every task of this leaf's stage routes to.
    pub(crate) destination: crate::oracle::dispatcher::DispatchCandidate,
    /// Whether the pinned snapshot names any compacted data file.
    ///
    /// Only the cut knows this; the provider knows its own hot files. A tier
    /// the cut does not hold is not delegated at all, so no stage is dispatched
    /// to read nothing.
    pub(crate) iceberg: bool,
}

/// Complete immutable inputs for constructing one authenticated table provider.
pub(crate) struct OracleTableInputs {
    /// Pinned Iceberg table for the sealed cut.
    pub(crate) table: iceberg::table::Table,
    /// The node's one storage owner, which decodes every hot object's metadata.
    pub(crate) storage: Arc<crate::storage::BifrostStorage>,
    /// Leader-local hot files absent from the pinned Iceberg snapshot.
    pub(crate) hot_files: Vec<HotFileSource>,
    /// Normalized `wyrd_event_time` bounds of every file the pinned Iceberg
    /// snapshot holds, in manifest order.
    ///
    /// Carried so the leader can decide the published tier on the same axis and
    /// the same bounds it decides the hot tier on. Iceberg's manifest planning
    /// applies the identical bounds when it plans the scan, so this is the
    /// leader's own statement of a decision the reader then enforces.
    pub(crate) iceberg_event_times: Vec<crate::catalog::event_time::EventTimeStatistics>,
    /// Authenticated request context retained by the tenant tripwire.
    pub(crate) context: AuthorizedQueryContext,
    /// Canonical table name used in security diagnostics.
    pub(crate) table_name: String,
    /// Mandatory audit collaborator.
    pub(crate) audit: Arc<dyn OracleAudit>,
    /// Frozen remote owner of this cut's persisted sources, when the cut chose
    /// one. `None` keeps every persisted leaf leader-local.
    pub(crate) remote: Option<OracleRemoteSource>,
}

/// Complete physical provider for one authenticated table visibility cut.
pub(crate) struct OracleTableProvider {
    /// Pinned Iceberg provider built from immutable table metadata.
    iceberg: IcebergStaticTableProvider,
    /// Pinned hot files absent from the selected Iceberg manifest.
    hot_files: Vec<HotFileSource>,
    /// Normalized event-time bounds of every pinned Iceberg file, in manifest
    /// order.
    iceberg_event_times: Vec<crate::catalog::event_time::EventTimeStatistics>,
    /// File reader inherited from the pinned Iceberg table.
    file_io: FileIO,
    /// The node's one storage owner, handed to every hot leaf this builds.
    storage: Arc<crate::storage::BifrostStorage>,
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
    /// Frozen remote owner of this cut's persisted sources, when the cut chose
    /// one. `None` keeps every persisted leaf leader-local.
    remote: Option<OracleRemoteSource>,
    /// Request-local ordinal handed to the next physical scan of this table.
    ///
    /// `DataFusion` calls [`TableProvider::scan`] once per physical occurrence,
    /// so a self-join or a repeated CTE over one registered table reaches this
    /// provider more than once. Each occurrence takes its own value, which is
    /// what keeps its remote scan identity — and therefore its assignment —
    /// distinct from its siblings'. `Relaxed` is sufficient: uniqueness is the
    /// invariant, and nothing else is published through this counter.
    scan_occurrences: std::sync::atomic::AtomicU64,
}

impl fmt::Debug for OracleTableProvider {
    /// Redacts audit and storage internals while retaining structural diagnostics.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OracleTableProvider")
            .field("table", &self.table)
            .field("hot_file_count", &self.hot_files.len())
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

    /// Narrows already-validated in-memory batches to the scan's closure schema.
    ///
    /// Distributed and drained live rows arrive at the complete physical schema.
    /// Shallow-projecting them by name before the memory source keeps every
    /// union leaf on one schema, which is what lets the serialized plan's
    /// `Column` indices be closure indices everywhere. An empty batch vector
    /// still yields a source declaring the closure, so an empty branch is
    /// schema-identical to a populated one.
    ///
    /// Rebuilding each batch against the pinned schema also normalizes field
    /// metadata, not only order. A catalog-derived closure carries Iceberg's
    /// `PARQUET:field_id` on every field while rows projected out of a Scribe
    /// memtable do not, and the attempt encoder compares a fragment's declared
    /// schema against each batch exactly — so a leaf that declared the closure
    /// but emitted metadata-free batches would fail the fragment on its first
    /// row rather than serve it.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when a batch is missing a closure column,
    /// a cast fails, or Arrow rejects the projected batch.
    pub(super) fn projected_memory_source(
        batches: &[RecordBatch],
        schema: &SchemaRef,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let projected = batches
            .iter()
            .map(|batch| project_batch(batch, Arc::clone(schema)))
            .collect::<DataFusionResult<Vec<_>>>()?;
        Ok(MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&projected),
            Arc::clone(schema),
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
            storage,
            hot_files,
            iceberg_event_times,
            context,
            table_name,
            audit,
            remote,
        } = inputs;
        let file_io = table.file_io().clone();
        let iceberg = IcebergStaticTableProvider::try_new_from_table(table)
            .await
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        let physical_schema = iceberg.schema();
        let public_schema = schema_without(&physical_schema, DATA_TENANT_ID)?;
        Ok(Self {
            iceberg,
            hot_files,
            iceberg_event_times,
            file_io,
            storage,
            physical_schema,
            public_schema,
            context,
            table: table_name,
            audit,
            remote,
            scan_occurrences: std::sync::atomic::AtomicU64::new(0),
        })
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

    /// Builds this cut's persisted leaves — published Iceberg objects and the
    /// leader's unpublished hot files — under the closure the caller derived.
    ///
    /// When the cut froze a remote owner for those sources the built leaves are
    /// substituted by one destination-bound placeholder that still carries them
    /// as its local plan. Substituting unconditionally is what keeps the
    /// *planner* — not this provider — the thing that decides whether the cut
    /// is read here or by a follower: the leaf reads its own local plan when it
    /// stays in the head stage, and encodes the follower assignment when the
    /// planner puts a boundary above it. Forcing a boundary here instead would
    /// distribute every query over a scannable cut, including one the leader
    /// could answer alone. The placeholder carries no assignment: the files it
    /// may read are bound after admission and the audited drain, never at
    /// planning time.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` planning error when the Iceberg provider refuses
    /// the projection or a leaf cannot be normalized to the closure order.
    async fn persisted_inputs(
        &self,
        state: &dyn Session,
        scan_projection: &OracleScanProjection,
        supported_filters: &[Expr],
        supported_predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
        limit: Option<usize>,
        occurrence: u64,
    ) -> DataFusionResult<Vec<Arc<dyn ExecutionPlan>>> {
        let required_schema = Arc::clone(&scan_projection.required_schema);
        let mut inputs: Vec<Arc<dyn ExecutionPlan>> = Vec::new();
        let published_index = inputs.len();
        {
            // `limit` is forwarded only as a per-leaf upper bound; DataFusion's
            // own global limit above this provider remains authoritative. The
            // closure's physical indices are what keep unrequested columns out
            // of the Iceberg reader itself rather than merely out of the result.
            self.record_iceberg_pruning(supported_predicates);
            let published = self
                .iceberg
                .scan(
                    state,
                    Some(&scan_projection.physical_indices),
                    supported_filters,
                    limit,
                )
                .await?;
            let published: Arc<dyn ExecutionPlan> =
                Arc::new(OracleIcebergScanExec::from_plan(published.as_ref())?);
            // The dependency may return the projected columns in its own
            // physical order. The signed closure is authoritative, so the plan
            // is normalized to it here rather than the closure being reordered
            // to match a source.
            inputs.push(project_plan_by_name(
                published,
                &scan_projection.required_columns,
            )?);
        }
        if !self.hot_files.is_empty() {
            inputs.push(Arc::new(HotParquetExec::new(
                self.retained_hot_files(supported_predicates),
                self.file_io.clone(),
                Arc::clone(&self.storage),
                Arc::clone(&required_schema),
                HotParquetPlan::Leader,
                Arc::new(OracleScanMetricsHandle::default()),
                supported_predicates.to_vec(),
            )));
        }
        let Some(remote) = self.remote.as_ref() else {
            return Ok(inputs);
        };
        // One placeholder per tier the cut actually holds. A follower resolves
        // an assignment's whole descriptor list through a single reader, so a
        // cut holding both compacted and staged output delegates them as two
        // remote sources; both name the same frozen destination, so the stage
        // still routes to exactly one participant.
        let hot_index = published_index + 1;
        let mut remotes: Vec<Arc<dyn ExecutionPlan>> = Vec::new();
        for (index, tier) in [
            (published_index, super::RemotePersistedTier::Iceberg),
            (hot_index, super::RemotePersistedTier::Hot),
        ] {
            let Some(local) = inputs.get(index).filter(|_| match tier {
                super::RemotePersistedTier::Iceberg => remote.iceberg,
                super::RemotePersistedTier::Hot => !self.hot_files.is_empty(),
            }) else {
                continue;
            };
            remotes.push(Arc::new(
                super::codec::RemoteSourcePlaceholderExec::new(
                    super::persisted_follower_scan_id(&remote.table, tier, occurrence),
                    // The fingerprint is of the table's complete physical
                    // schema, not of this scan's closure: the follower resolves
                    // the same catalog provider and compares against
                    // `provider.schema()` before it reads anything. The closure
                    // travels separately, as `required_columns`.
                    super::assignment_schema_fingerprint(&self.physical_schema),
                    Arc::clone(&required_schema),
                )
                .with_closure(
                    scan_projection.required_columns.clone(),
                    supported_predicates.to_vec(),
                )
                .with_source(super::codec::PlannedRemoteSource {
                    destination: remote.destination.clone(),
                    tenant: self.context.data_tenant_id,
                    table: remote.table.clone(),
                    tier,
                })
                .with_local(Arc::clone(local)),
            ) as Arc<dyn ExecutionPlan>);
        }
        // A cut whose only persisted tier is empty has no leaf to substitute,
        // and a placeholder over an empty local plan would claim a source the
        // follower could not read either.
        if remotes.is_empty() {
            return Ok(inputs);
        }
        Ok(remotes)
    }

    /// Records this query's pre-footer event-time decision for every pinned
    /// Iceberg file.
    ///
    /// The published tier is read through Iceberg's own manifest planning,
    /// which applies these same normalized bounds and never opens an excluded
    /// file's footer. Deciding here as well is what makes that exclusion
    /// observable: without it the emitted
    /// `bifrost_oracle_file_pruning_total` counts reconcile against the cut's
    /// hot files alone and report a pruned snapshot as unpruned. The decision
    /// reads the bounds the pin already minted from the manifest entry, so it
    /// cannot disagree with what the reader then does.
    ///
    /// An unconstrained query interval records nothing at all, matching the hot
    /// tier: nothing was considered, so nothing is reported.
    fn record_iceberg_pruning(
        &self,
        predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    ) {
        let interval = crate::oracle::pruning::EventTimeQueryInterval::from_predicates(predicates);
        if interval.is_unbounded() {
            return;
        }
        for statistics in &self.iceberg_event_times {
            let _ = interval.retains(
                crate::oracle::pruning::FilePruningSource::Iceberg,
                *statistics,
            );
        }
    }

    /// Returns the staged hot files this query can still read a row from.
    ///
    /// Staged hot Parquet is the only persisted source a leader-local scan
    /// selects itself: the pinned Iceberg leaf hands its predicate to Iceberg's
    /// own manifest planning, which already prunes on these same bounds and on
    /// every other column's. Deciding here — before `HotParquetExec` is
    /// constructed — is what keeps an excluded object's footer from ever being
    /// opened, and every considered file emits exactly one bounded
    /// `bifrost_oracle_file_pruning_total` observation.
    ///
    /// A file whose durable bounds are absent, unrepresentable, or reversed is
    /// retained, so unusable statistics cost rows from nothing.
    fn retained_hot_files(
        &self,
        predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    ) -> Vec<HotFileSource> {
        let interval = crate::oracle::pruning::EventTimeQueryInterval::from_predicates(predicates);
        if interval.is_unbounded() {
            return self.hot_files.clone();
        }
        self.hot_files
            .iter()
            .filter(|file| {
                interval.retains(
                    crate::oracle::pruning::FilePruningSource::Hot,
                    file.event_time,
                )
            })
            .cloned()
            .collect()
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
    /// be represented with the pinned physical schema, or when this request has
    /// exhausted its scan occurrences.
    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // Exactly one ordinal per physical occurrence, taken before anything
        // below can mint an identity from it. Exhaustion fails planning rather
        // than wrapping, because a wrapped ordinal would reissue a live scan
        // identity to a different read.
        let occurrence = self
            .scan_occurrences
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |current| current.checked_add(1),
            )
            .map_err(|_| {
                DataFusionError::Plan(
                    "Oracle table provider exhausted its request-local scan occurrences".to_owned(),
                )
            })?;
        let (supported_predicates, supported_filters) = self.closed_pushdown(filters);
        // One closure, derived once, governs every leaf below and every
        // operator above. Nothing downstream recomputes a column set or order.
        let scan_projection = OracleScanProjection::try_new(
            &self.physical_schema,
            &self.public_schema,
            projection,
            &supported_predicates,
        )?;
        let required_schema = Arc::clone(&scan_projection.required_schema);
        let mut inputs = self
            .persisted_inputs(
                state,
                &scan_projection,
                &supported_filters,
                &supported_predicates,
                limit,
                occurrence,
            )
            .await?;
        // Planned unconditionally: the Fused drain runs after admission, so
        // planning cannot know whether this table has live rows. The leaf
        // resolves its one bound batch set — possibly empty — from the
        // execution `TaskContext` instead of capturing rows here.
        inputs.push(Arc::new(
            super::analytical_scan::AnalyticalScanExec::local_drained(
                super::bindings::OracleSourceKey::LocalDrained {
                    table: self.table.clone(),
                },
                Arc::clone(&required_schema),
            ),
        ));
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
        // Resolved by name against the filter's actual output, which is the
        // closure minus the tenant column — never against the original
        // full-public-schema ordinals the caller supplied.
        project_plan_by_name(filtered, &scan_projection.output_names)
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
    /// Visits every physical expression this plan owns.
    ///
    /// This plan owns no `PhysicalExpr`, so the traversal reports
    /// [`TreeNodeRecursion::Continue`] without invoking `f`.
    ///
    /// # Errors
    /// Never returns an error; the signature is fixed by the trait.
    fn apply_expressions(
        &self,
        _f: &mut dyn FnMut(
            &Arc<dyn datafusion::physical_expr::PhysicalExpr>,
        ) -> DataFusionResult<TreeNodeRecursion>,
    ) -> DataFusionResult<TreeNodeRecursion> {
        Ok(TreeNodeRecursion::Continue)
    }

    /// Returns the stable physical operator name.
    fn name(&self) -> &'static str {
        "TenantTripwireExec"
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

/// Planning-time governance retained by one hot-Parquet leaf.
///
/// A leaf is built before the query is admitted, so it may not capture a memory
/// pool or an admission class. It retains only the process-owned accounting
/// handles its role needs and resolves the admitted pool, class, cancellation,
/// and deadline from the executing `TaskContext`.
#[derive(Clone)]
pub(super) enum HotParquetPlan {
    /// Leader-local leaf resolving every governance fact from the admitted task.
    Leader,
    /// Follower leaf charged against the request-local pool its lease retains.
    Follower {
        /// Request-local pool backed by the retained Oracle worker lease.
        memory_pool: Arc<dyn MemoryPool>,
    },
}

impl HotParquetPlan {
    /// Resolves the executable governance mode for one admitted task.
    ///
    /// The leader mode reads the once-bound execution bindings installed in the
    /// session configuration before planning, refuses a cancelled or expired
    /// query before any storage IO, and charges every reserved byte to the
    /// admitted runtime's own pool. The follower mode already holds the pool
    /// its stage lease was admitted with and resolves to itself.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error when the leader's bindings are
    /// absent or unbound, the query was cancelled, or its deadline elapsed.
    fn resolve(&self, task: &TaskContext) -> DataFusionResult<HotParquetGovernance> {
        match self {
            Self::Leader => {
                let lock = super::bindings::bindings_for_task(task)?;
                let bindings = lock.get().ok_or_else(|| {
                    DataFusionError::Execution("Oracle execution bindings are not bound".to_owned())
                })?;
                let grant = bindings.grant();
                grant.ensure_live()?;
                Ok(HotParquetGovernance::Leader {
                    memory: grant.memory.clone(),
                    memory_pool: Arc::clone(task.memory_pool()),
                    telemetry: Arc::clone(&grant.telemetry),
                    query_class: grant.query_class,
                })
            }
            Self::Follower { memory_pool } => Ok(HotParquetGovernance::Follower {
                memory_pool: Arc::clone(memory_pool),
            }),
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

/// One hot object's ranged reader, opened on first use.
///
/// The storage owner decodes metadata through a reader *factory*, because a
/// retried decode needs a reader that has not already consumed part of a
/// response. Opening an Iceberg input is asynchronous and a factory is not, so
/// the open is deferred to the first range instead of being performed eagerly
/// by the factory.
enum HotObjectSource {
    /// Not yet opened; carries exactly what opening needs.
    Pending {
        /// File reader inherited from the pinned Iceberg table.
        file_io: FileIO,
        /// Absolute storage path accepted by that `FileIO`.
        location: String,
    },
    /// Opened ranged reader for the pinned object.
    ///
    /// Shared rather than owned because `FileRead::read` borrows for `'static`,
    /// so a range is issued from a cloned handle rather than from a borrow of
    /// this enum.
    Open(Arc<dyn FileRead>),
}

impl HotObjectSource {
    /// Reads one exact range, opening the object on the first call.
    ///
    /// # Errors
    /// Returns the Iceberg failure from opening the input or reading the range.
    async fn read(&mut self, range: Range<u64>) -> Result<bytes::Bytes, iceberg::Error> {
        if let Self::Pending { file_io, location } = self {
            let reader = file_io.new_input(location)?.reader().await?;
            *self = Self::Open(Arc::new(reader));
        }
        match self {
            Self::Open(reader) => Arc::clone(reader).read(range).await,
            Self::Pending { .. } => Err(iceberg::Error::new(
                iceberg::ErrorKind::Unexpected,
                "a hot object source failed to open before its first range",
            )),
        }
    }
}

/// Iceberg ranged storage adapted to Parquet with pre-IO Oracle accounting.
struct IcebergParquetReader {
    /// Pinned ranged reader for one immutable hot object.
    reader: HotObjectSource,
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
    /// Creates a governed reader that opens one pinned object on first range.
    fn new(
        reader: HotObjectSource,
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
    /// The node's one storage owner, asked to decode each object's metadata.
    storage: Arc<crate::storage::BifrostStorage>,
    /// Pinned Iceberg storage reader.
    file_io: FileIO,
    /// Complete physical table schema.
    schema: SchemaRef,
    /// Planning-time governance resolved against the admitted task at execute.
    governance: HotParquetPlan,
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
        storage: Arc<crate::storage::BifrostStorage>,
        schema: SchemaRef,
        governance: HotParquetPlan,
        metrics: Arc<OracleScanMetricsHandle>,
        predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    ) -> Self {
        Self {
            files,
            file_io,
            storage,
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
    /// Visits every physical expression this plan owns.
    ///
    /// This plan owns no `PhysicalExpr`, so the traversal reports
    /// [`TreeNodeRecursion::Continue`] without invoking `f`.
    ///
    /// # Errors
    /// Never returns an error; the signature is fixed by the trait.
    fn apply_expressions(
        &self,
        _f: &mut dyn FnMut(
            &Arc<dyn datafusion::physical_expr::PhysicalExpr>,
        ) -> DataFusionResult<TreeNodeRecursion>,
    ) -> DataFusionResult<TreeNodeRecursion> {
        Ok(TreeNodeRecursion::Continue)
    }

    /// Returns the stable physical leaf name.
    fn name(&self) -> &'static str {
        "HotParquetExec"
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
        // Resolved here, never captured at planning time: the admitted pool,
        // class, cancellation, and deadline all arrive with this task.
        let governance = self.governance.resolve(task.as_ref())?;
        // The hot leaf decodes at the admitted session's batch size, so this
        // path is shaped by the same grant as every other operator in the plan
        // rather than by a fixed constant of its own.
        let stream = hot_stream(self, task.session_config().batch_size(), governance);
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
    governance: HotParquetGovernance,
) -> impl Stream<Item = DataFusionResult<RecordBatch>> + Send + 'static {
    let files = exec.files.clone();
    let file_io = exec.file_io.clone();
    let storage = Arc::clone(&exec.storage);
    let schema = Arc::clone(&exec.schema);
    let metrics = Arc::clone(&exec.metrics);
    let predicates = exec.predicates.clone();
    #[cfg(test)]
    let reader_override = exec.reader_override.clone();
    // Cancelling the query drops this stream, which drops the guard and
    // cancels any metadata decode this stream still has outstanding. Owner
    // shutdown cancels the same work through the owner's own token.
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_on_drop = cancel.clone().drop_guard();
    async_stream::try_stream! {
        let _cancel_on_drop = cancel_on_drop;
        for file in files {
            let size = u64::try_from(file.size_bytes).map_err(|_| {
                DataFusionError::Execution("hot object size exceeds u64".to_owned())
            })?;
            let build_reader = {
                let file_io = file_io.clone();
                let location = file.location.clone();
                let governance = governance.clone();
                let metrics = Arc::clone(&metrics);
                #[cfg(test)]
                let reader_override = reader_override.clone();
                move || {
                    let reader = IcebergParquetReader::new(
                        HotObjectSource::Pending {
                            file_io: file_io.clone(),
                            location: location.clone(),
                        },
                        size,
                        governance.clone(),
                        Arc::clone(&metrics),
                    );
                    #[cfg(test)]
                    let reader = if let Some(override_reader) = reader_override.as_ref() {
                        reader.with_test_reader(location.clone(), Arc::clone(override_reader))
                    } else {
                        reader
                    };
                    reader
                }
            };
            // Recorded before the footer is read so a file observation exists
            // for every attempt on this file, including one whose reader fails
            // or is abandoned mid-open. Row-group pruning is reported
            // separately, so a file whose groups are all pruned still counts as
            // opened rather than vanishing from the scan accounting.
            metrics.record_hot_file();
            // The owner, not this leaf, decides whether this object's footer is
            // decoded again: it owns the node-wide cache, single-flight, request
            // admission, and retry bound for every hot identity.
            let retained = storage
                .hot_metadata(
                    file.metadata_key.clone(),
                    build_reader.clone(),
                    storage.metadata_deadline(),
                    cancel.clone(),
                )
                .await
                .map_err(|error| {
                    DataFusionError::External(Box::new((*error).clone()))
                })?;
            let metadata = parquet::arrow::arrow_reader::ArrowReaderMetadata::try_new(
                Arc::clone(retained.metadata()),
                ArrowReaderOptions::new(),
            )
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
            let builder =
                ParquetRecordBatchStreamBuilder::new_with_metadata(build_reader(), metadata);
            let selection = select_row_groups_for_predicates(builder.metadata(), &predicates);
            metrics.record_row_groups(&selection);
            if selection.excludes_file() {
                continue;
            }
            // Selective decode: only the closure's leaves leave storage. The
            // post-decode `project_batch` below then normalizes exact order and
            // types; it is a normalizer, not the thing that avoids the IO.
            let mask = hot_projection_mask(builder.parquet_schema(), schema.as_ref());
            let mut batches = builder
                .with_row_groups(selection.retained)
                .with_batch_size(batch_size)
                .with_projection(mask)
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

/// Derives the Parquet projection mask that decodes exactly `schema`'s columns.
///
/// Matching is by name against the file's own root fields, because the closure
/// is a name-based contract and a sealed hot file may order or extend its
/// columns independently of the pinned table schema. A closure name the file
/// does not carry is deliberately left out of the mask rather than refused
/// here: [`project_batch`] raises that as a named missing-field error once the
/// batch arrives, which keeps one diagnostic for the condition.
fn hot_projection_mask(
    parquet_schema: &parquet::schema::types::SchemaDescriptor,
    schema: &Schema,
) -> parquet::arrow::ProjectionMask {
    let indices = parquet_schema
        .root_schema()
        .get_fields()
        .iter()
        .enumerate()
        .filter(|(_, field)| schema.column_with_name(field.name()).is_some())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    parquet::arrow::ProjectionMask::roots(parquet_schema, indices)
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

/// Resolves `DataFusion`'s requested scan output to public column names.
///
/// A repeated requested ordinal stays repeated: the caller's output shape is
/// the caller's business, and only the leaf closure derived from these names is
/// deduplicated. A `None` projection means every public column plus the hidden
/// tenant column below it; it never means zero columns.
///
/// # Errors
///
/// Returns a `DataFusion` plan error when a requested ordinal falls outside the
/// public schema, which is a planner contract failure rather than a column the
/// scan may quietly drop.
fn scan_output_names(
    public_schema: &Schema,
    projection: Option<&Vec<usize>>,
) -> DataFusionResult<Vec<String>> {
    let Some(projection) = projection else {
        return Ok(public_schema
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect());
    };
    projection
        .iter()
        .map(|index| {
            public_schema
                .fields()
                .get(*index)
                .map(|field| field.name().clone())
                .ok_or_else(|| {
                    DataFusionError::Plan("table projection ordinal is out of range".to_owned())
                })
        })
        .collect()
}

/// Computes the canonical `required_columns` closure: the requested scan output
/// names, followed by the first occurrence of each predicate column in filter
/// order, followed by the always-present hidden tenant column — stably
/// deduplicated.
fn required_columns_closure(
    output_names: &[String],
    predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
) -> Vec<String> {
    let mut required = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in output_names
        .iter()
        .cloned()
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

/// Selects `names` out of one complete physical schema, by name, in order.
///
/// Returns the narrowed schema — complete `Field` values and the complete
/// schema's own metadata preserved — together with each name's index in the
/// complete schema. The leader's scan planning and every follower resolver
/// derive their leaf schema through this one function, so a signed closure
/// means exactly one Arrow schema everywhere it is validated.
///
/// # Errors
///
/// Returns a `DataFusion` plan error when `schema` contains duplicate field
/// names, which makes name-based selection ambiguous, or when a requested name
/// is absent from it. Neither is silently repaired: a follower that quietly
/// deduplicated or reordered a signed assignment would read something other
/// than what the leader signed.
pub(super) fn select_schema_by_name(
    schema: &Schema,
    names: &[String],
) -> DataFusionResult<(SchemaRef, Vec<usize>)> {
    let mut indices = Vec::with_capacity(names.len());
    let mut fields = Vec::with_capacity(names.len());
    for name in names {
        let matches = schema
            .fields()
            .iter()
            .enumerate()
            .filter(|(_, field)| field.name() == name)
            .collect::<Vec<_>>();
        let [(index, field)] = matches.as_slice() else {
            return Err(DataFusionError::Plan(if matches.is_empty() {
                format!("physical schema is missing required column `{name}`")
            } else {
                format!("physical schema names column `{name}` more than once")
            }));
        };
        indices.push(*index);
        fields.push(Arc::clone(field));
    }
    Ok((
        Arc::new(Schema::new_with_metadata(
            arrow::datatypes::Fields::from(fields),
            schema.metadata().clone(),
        )),
        indices,
    ))
}

/// One leader-owned physical projection closure shared by every scan leaf.
///
/// [`OracleTableProvider::scan`] builds this once from the complete physical
/// schema and the caller's request, then hands the same value to every union
/// leaf, the remote placeholders, the tenant tripwire, the provider-local
/// filter, and the final public projection. No leaf recomputes its own column
/// set or order: a follower revalidates the signed closure against the schema
/// its own catalog resolves, so two components deriving the same set in a
/// different order would refuse each other's assignments.
#[derive(Debug)]
struct OracleScanProjection {
    /// Public column names this scan outputs, in requested order, with a
    /// repeated requested ordinal preserved.
    output_names: Vec<String>,
    /// Stable-deduplicated leaf closure: outputs, predicate columns, tenant.
    required_columns: Vec<String>,
    /// Complete-schema fields selected by `required_columns`, in that order.
    required_schema: SchemaRef,
    /// `required_columns` resolved to complete physical-schema indices.
    physical_indices: Vec<usize>,
}

impl OracleScanProjection {
    /// Derives the closure for one scan request.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error when a requested ordinal is out of
    /// range, when the complete physical schema is ambiguous, or when a closure
    /// name is absent from it.
    fn try_new(
        physical_schema: &Schema,
        public_schema: &Schema,
        projection: Option<&Vec<usize>>,
        predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    ) -> DataFusionResult<Self> {
        let output_names = scan_output_names(public_schema, projection)?;
        let required_columns = required_columns_closure(&output_names, predicates);
        let (required_schema, physical_indices) =
            select_schema_by_name(physical_schema, &required_columns)?;
        Ok(Self {
            output_names,
            required_columns,
            required_schema,
            physical_indices,
        })
    }
}

/// Returns `plan`, or a name-resolved projection of it, exposing exactly
/// `names` in that order.
///
/// Used at every boundary where a dependency may return the right columns in
/// its own order: the closure is authoritative, so the plan is adapted to it
/// rather than the closure being rewritten to match a source. A plan that
/// already matches is returned untouched, which keeps an unprojected scan free
/// of a no-op operator.
///
/// # Errors
///
/// Returns a `DataFusion` plan error when a name does not resolve against the
/// plan's own output schema, or when `DataFusion` rejects the projection.
pub(super) fn project_plan_by_name(
    plan: Arc<dyn ExecutionPlan>,
    names: &[String],
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    let schema = plan.schema();
    if schema.fields().len() == names.len()
        && schema
            .fields()
            .iter()
            .zip(names)
            .all(|(field, name)| field.name() == name)
    {
        return Ok(plan);
    }
    let expressions = names
        .iter()
        .map(|name| {
            let column = Column::new_with_schema(name, &schema)?;
            Ok((
                Arc::new(column) as Arc<dyn datafusion::physical_expr::PhysicalExpr>,
                name.clone(),
            ))
        })
        .collect::<DataFusionResult<Vec<_>>>()?;
    Ok(Arc::new(ProjectionExec::try_new(expressions, plan)?))
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
/// The row count is carried explicitly rather than inferred from the columns,
/// because the pinned schema is legitimately allowed to be empty: `count(*)`
/// requests no output column, so its closure is the hidden tenant column alone
/// and the batch that survives the tripwire has zero columns and a real row
/// count. Arrow cannot recover that count from the columns, so dropping it
/// would turn a valid narrow scan into an execution failure.
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
    RecordBatch::try_new_with_options(
        schema,
        columns,
        &arrow::record_batch::RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
    )
    .map_err(DataFusionError::from)
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
/// The row count is carried explicitly so a batch whose only column was the
/// hidden tenant column — the closure of a `count(*)` scan — survives the
/// tripwire as a zero-column batch with its real row count intact.
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
    RecordBatch::try_new_with_options(
        schema,
        columns,
        &arrow::record_batch::RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
    )
    .map_err(DataFusionError::from)
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
    use crate::oracle::bindings::{
        FollowerSourceKey, OracleExecutionBindingInputs, OracleExecutionBindings, OracleSourceKey,
    };
    use crate::oracle::codec::RemoteSourcePlaceholderExec;
    use crate::oracle::{BifrostQueryReadDecision, OracleSlotManager};
    use arrow::array::{ArrayRef, Int32Array, Int64Array, StringArray};
    use async_trait::async_trait;
    use datafusion::logical_expr::{col, lit};
    use datafusion::physical_plan::sorts::sort::SortExec;
    use datafusion::physical_plan::union::UnionExec;
    use wyrd_runtime::Principal;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::AuthMethod;
    use wyrd_spec::vala::api::FollowerScanAssignment;
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

    /// A `count(*)` closure survives the tripwire as a zero-column batch and
    /// still refuses a foreign row.
    ///
    /// `count(*)` requests no output column, so its signed closure is the
    /// hidden tenant column alone and the batch the tripwire emits has zero
    /// columns. Arrow cannot infer a row count from no columns, so the count
    /// has to be carried explicitly; when it was not, the leaf failed with
    /// `must either specify a row count or at least one column` and the query
    /// surfaced as a degraded partition rather than as the tenant refusal it
    /// actually was. Both halves are pinned here: the owning-tenant scan keeps
    /// its rows, and the foreign row is still classified as a tenant invariant.
    ///
    /// # Panics
    ///
    /// Panics when the fixture plan cannot be built or executed, or when the
    /// tripwire loses the row count or the refusal.
    #[tokio::test]
    async fn count_star_closure_keeps_its_row_count_through_the_tripwire() {
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let foreign = wyrd_spec::DataTenantId::new_v7();
        for (owner_rows, expect_refusal) in [(3_usize, false), (3, true)] {
            let values = (0..owner_rows)
                .map(|row| {
                    if expect_refusal && row == 1 {
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
                Arc::clone(&schema),
                vec![Arc::new(StringArray::from(values)) as ArrayRef],
            )
            .expect("tenant-only closure batch");
            let source =
                MemorySourceConfig::try_new_exec(std::slice::from_ref(&vec![batch]), schema, None)
                    .expect("closure source");
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
            let tripwire = TenantTripwireExec::new(
                source,
                context,
                "vala.traces.spans".to_owned(),
                Arc::new(NoopAudit),
            )
            .expect("tripwire plan");
            assert_eq!(
                tripwire.schema().fields().len(),
                0,
                "a count(*) closure leaves the tripwire with no output column"
            );
            let stream = tripwire
                .execute(0, Arc::new(TaskContext::default()))
                .expect("tripwire stream");
            let collected = futures_util::TryStreamExt::try_collect::<Vec<_>>(stream).await;
            if expect_refusal {
                let error = collected.expect_err("a foreign row must refuse the scan");
                assert!(
                    is_tenant_invariant_error(&error),
                    "the refusal must stay a tenant invariant: {error}"
                );
            } else {
                let batches = collected.expect("an owning-tenant closure scan succeeds");
                let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
                assert_eq!(rows, owner_rows, "the zero-column batch kept its row count");
                assert!(
                    batches.iter().all(|batch| batch.num_columns() == 0),
                    "the tenant column must not survive the tripwire"
                );
            }
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
        let tripwire_plan: Arc<dyn ExecutionPlan> = Arc::clone(&tripwire) as Arc<dyn ExecutionPlan>;
        assert!(tripwire_plan.downcast_ref::<SortExec>().is_none());
        assert!(tripwire.children()[0].downcast_ref::<SortExec>().is_none());
    }

    /// Composes one Oracle capability for hot-read resource tests.
    /// Event-time statistics for a fixture whose pruning decision is not the
    /// behavior under test.
    ///
    /// Unusable is the fail-open value, so a fixture built with it is retained
    /// by every query interval and cannot accidentally disappear from a test
    /// that is measuring something else.
    /// Builds a fixture storage owner that retains decoded metadata.
    ///
    /// Every hot leaf now asks the owner for its metadata, so a leaf fixture
    /// composes one just as a node does. Its backend is never read: the decode
    /// is driven by the fixture's own reader factory.
    ///
    /// # Panics
    /// Panics when the temporary root cannot be created.
    fn fixture_storage() -> Arc<crate::storage::BifrostStorage> {
        let root = tempfile::tempdir().expect("fixture storage root");
        crate::storage::BifrostStorage::for_test(&root.keep(), true)
    }

    /// Builds one immutable metadata identity for a fixture object.
    ///
    /// The checksum is derived from the name so two differently named fixture
    /// objects never share a cache entry, which is the same property the
    /// durable writer checksum gives production.
    fn fixture_metadata_key(name: &str, size_bytes: usize) -> crate::storage::HotMetadataKey {
        let mut checksum = [0_u8; 32];
        for (slot, byte) in checksum.iter_mut().zip(name.as_bytes()) {
            *slot = *byte;
        }
        checksum[31] = 1;
        crate::storage::HotMetadataKey::new(
            wyrd_spec::DataTenantId::new_v7(),
            "vala.traces.spans".to_owned(),
            name.to_owned(),
            uuid::Uuid::now_v7(),
            checksum,
            u64::try_from(size_bytes).unwrap_or(u64::MAX),
        )
    }

    fn unusable_event_time() -> crate::catalog::event_time::EventTimeStatistics {
        crate::catalog::event_time::EventTimeStatistics::Unusable(
            crate::catalog::event_time::EventTimeBoundsDefect::Missing,
        )
    }

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

    /// Writes one real file of `groups` row groups through the production
    /// writer recipe, one row group per supplied value block, and returns its
    /// bytes. Flushing between blocks is what makes the file multi-row-group:
    /// `MAX_ROW_GROUP_ROWS` is 131,072 and is not configurable, so no
    /// row-count-driven fixture could produce two groups at unit scale.
    fn write_grouped_fixture(schema: &SchemaRef, blocks: &[RecordBatch]) -> bytes::Bytes {
        let rows: usize = blocks.iter().map(RecordBatch::num_rows).sum();
        let properties = crate::parquet::writer_properties::bifrost_writer_properties(
            rows,
            &["service_name".to_owned()],
        );
        let mut sink = Vec::new();
        let mut writer =
            parquet::arrow::ArrowWriter::try_new(&mut sink, Arc::clone(schema), Some(properties))
                .expect("grouped fixture writer");
        for block in blocks {
            writer.write(block).expect("grouped fixture write");
            writer.flush().expect("grouped fixture row-group flush");
        }
        writer.close().expect("grouped fixture close");
        bytes::Bytes::from(sink)
    }

    /// Builds one `service_name`/`value` batch of `rows` rows all carrying
    /// `service`, starting the integer column at `first`.
    fn service_block(schema: &SchemaRef, service: &str, first: i64, rows: i64) -> RecordBatch {
        RecordBatch::try_new(
            Arc::clone(schema),
            vec![
                Arc::new(arrow::array::StringArray::from_iter_values(
                    (0..rows).map(|_| service),
                )) as ArrayRef,
                Arc::new(Int64Array::from_iter_values(first..first + rows)) as ArrayRef,
            ],
        )
        .expect("service block")
    }

    /// Assert every row group's `service_name` chunk carries both halves of
    /// the recipe an equality leaf depends on: a dictionary page and a Bloom
    /// filter.
    ///
    /// # Panics
    ///
    /// Panics when a group lacks the column, its dictionary, or its filter.
    fn assert_dictionary_bloom_groups(metadata: &parquet::file::metadata::ParquetMetaData) {
        for group in metadata.row_groups() {
            let column = group
                .columns()
                .iter()
                .find(|column| column.column_path().string() == "service_name")
                .expect("service_name chunk");
            let encodings = column.encodings().collect::<Vec<_>>();
            assert!(
                encodings.contains(&parquet::basic::Encoding::RLE_DICTIONARY)
                    || encodings.contains(&parquet::basic::Encoding::PLAIN_DICTIONARY),
                "service_name must be dictionary-encoded, saw {encodings:?}"
            );
            assert!(
                column.bloom_filter_offset().is_some(),
                "an allowlisted column must carry a Bloom filter"
            );
        }
    }

    /// Decode `groups` of `published` and return the `value` column, asserting
    /// every decoded row carries `service`.
    ///
    /// # Panics
    ///
    /// Panics when the object cannot be decoded, a column is missing or of an
    /// unexpected type, or a decoded row carries a different service.
    fn decoded_values_for_service(
        published: &bytes::Bytes,
        groups: Vec<usize>,
        service: &str,
    ) -> Vec<i64> {
        let decoded = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
            published.clone(),
        )
        .expect("decode builder")
        .with_row_groups(groups)
        .build()
        .expect("decode reader")
        .collect::<Result<Vec<_>, _>>()
        .expect("decode rows");
        let mut decoded_values = Vec::new();
        for batch in &decoded {
            let services = batch
                .column_by_name("service_name")
                .expect("service column")
                .as_any()
                .downcast_ref::<arrow::array::StringArray>()
                .expect("service column is Utf8");
            let values = batch
                .column_by_name("value")
                .expect("value column")
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("value column is Int64");
            for row in 0..batch.num_rows() {
                assert_eq!(services.value(row), service);
                decoded_values.push(values.value(row));
            }
        }
        decoded_values
    }

    /// Prove the same recipe stays lossless once parquet-rs falls back off its
    /// dictionary, by writing `rows` all-distinct strings and reading them back.
    ///
    /// This is the other half of the dictionary decision: enabling dictionaries
    /// by default is only safe because overflow degrades to `PLAIN` rather than
    /// truncating or corrupting the column.
    ///
    /// # Panics
    ///
    /// Panics when the batch cannot be built, encoded, or decoded, or when any
    /// value does not survive the round trip.
    fn assert_high_cardinality_round_trips(schema: &SchemaRef, rows: i64) {
        let unique = (0..rows)
            .map(|row| format!("service-{row}"))
            .collect::<Vec<_>>();
        let batch = RecordBatch::try_new(
            Arc::clone(schema),
            vec![
                Arc::new(arrow::array::StringArray::from_iter_values(unique.iter())) as ArrayRef,
                Arc::new(Int64Array::from_iter_values(0..rows)) as ArrayRef,
            ],
        )
        .expect("high-cardinality batch");
        let wide = write_grouped_fixture(schema, std::slice::from_ref(&batch));
        let wide_rows =
            parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(wide)
                .expect("wide decode builder")
                .build()
                .expect("wide decode reader")
                .collect::<Result<Vec<_>, _>>()
                .expect("wide decode rows");
        let mut wide_services = Vec::with_capacity(unique.len());
        for batch in &wide_rows {
            let services = batch
                .column_by_name("service_name")
                .expect("service column")
                .as_any()
                .downcast_ref::<arrow::array::StringArray>()
                .expect("service column is Utf8");
            for row in 0..batch.num_rows() {
                wide_services.push(services.value(row).to_owned());
            }
        }
        assert_eq!(
            wide_services, unique,
            "dictionary overflow must fall back to PLAIN without losing a row"
        );
    }

    /// Closed-predicate pruning measured on a real two-row-group file written
    /// by the production recipe: the file's low-cardinality `service_name`
    /// column is dictionary-encoded and Bloom-filtered, an equality leaf
    /// retains exactly one of the two groups, the retained group's compressed
    /// bytes are strictly fewer than the whole file's, and the decoded rows are
    /// exactly the matching rows. A high-cardinality column in the same recipe
    /// stays lossless after parquet-rs falls back off its dictionary.
    #[test]
    fn dictionary_bloom_row_group_pruning_contract() {
        use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

        const BLOCK_ROWS: i64 = 2_048;

        let schema: SchemaRef = Arc::new(Schema::new(vec![
            Field::new("service_name", DataType::Utf8, false),
            Field::new("value", DataType::Int64, false),
        ]));
        let published = write_grouped_fixture(
            &schema,
            &[
                service_block(&schema, "checkout", 0, BLOCK_ROWS),
                service_block(&schema, "shipping", BLOCK_ROWS, BLOCK_ROWS),
            ],
        );
        let metadata = parquet::file::metadata::ParquetMetaDataReader::new()
            .parse_and_finish(&published)
            .expect("valid Parquet footer");

        assert_eq!(metadata.num_row_groups(), 2, "fixture must have two groups");
        assert_dictionary_bloom_groups(&metadata);

        let predicates = vec![ScanPredicate::Eq(
            "service_name".to_owned(),
            ScanLiteral::Utf8("checkout".to_owned()),
        )];
        let selection = select_row_groups_for_predicates(&metadata, &predicates);
        assert_eq!(selection.retained, vec![0]);
        assert_eq!(selection.pruned, 1);
        assert!(!selection.excludes_file());

        let group_bytes = |index: usize| -> i64 { metadata.row_group(index).compressed_size() };
        let scanned: i64 = selection
            .retained
            .iter()
            .map(|index| group_bytes(*index))
            .sum();
        let whole_file: i64 = (0..metadata.num_row_groups()).map(group_bytes).sum();
        assert!(
            scanned < whole_file,
            "pruning must scan strictly fewer bytes: {scanned} vs {whole_file}"
        );

        let decoded_values =
            decoded_values_for_service(&published, selection.retained.clone(), "checkout");
        assert_eq!(decoded_values, (0..BLOCK_ROWS).collect::<Vec<_>>());

        assert_high_cardinality_round_trips(&schema, BLOCK_ROWS);
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
        let properties =
            crate::parquet::writer_properties::bifrost_writer_properties(batch.num_rows(), &[]);
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            File::create(path).unwrap_or_else(|error| panic!("{context} file: {error}")),
            schema,
            Some(properties),
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
            Some(
                crate::parquet::writer_properties::bifrost_writer_properties(batch.num_rows(), &[]),
            ),
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
                    metadata_key: fixture_metadata_key(&path.to_string_lossy(), size_bytes),
                    location: path.to_string_lossy().into_owned(),
                    size_bytes,
                    event_time: unusable_event_time(),
                }],
                FileIO::new_with_fs(),
                fixture_storage(),
                Arc::clone(&schema),
                HotParquetPlan::Leader,
                Arc::clone(&metrics),
                Vec::new(),
            );
            (exec, metrics)
        };
        let context = bound_leader_task(
            memory.clone(),
            &telemetry,
            crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
        );
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
            Some(
                crate::parquet::writer_properties::bifrost_writer_properties(batch.num_rows(), &[]),
            ),
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
            HotObjectSource::Open(Arc::new(RecordingRangeReader {
                bytes: fixture.bytes.clone(),
                ranges,
                short,
            })),
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
                metadata_key: fixture_metadata_key(
                    &fixture.path.to_string_lossy(),
                    fixture.bytes.len(),
                ),
                location: fixture.path.to_string_lossy().into_owned(),
                size_bytes: fixture.bytes.len(),
                event_time: unusable_event_time(),
            }],
            FileIO::new_with_fs(),
            fixture_storage(),
            Arc::clone(&projected),
            HotParquetPlan::Leader,
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
                bound_leader_task(
                    oracle_memory_resources(&governor, 1024),
                    &telemetry,
                    Arc::clone(&query_pool),
                ),
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
                metadata_key: fixture_metadata_key(
                    &fixture.path.to_string_lossy(),
                    fixture.bytes.len(),
                ),
                location: fixture.path.to_string_lossy().into_owned(),
                size_bytes: fixture.bytes.len(),
                event_time: unusable_event_time(),
            }],
            FileIO::new_with_fs(),
            fixture_storage(),
            Arc::clone(&fixture.schema),
            HotParquetPlan::Leader,
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
                bound_leader_task(
                    oracle_memory_resources(&governor, 1024),
                    &telemetry,
                    Arc::clone(&query_pool),
                ),
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
                bound_leader_task(
                    oracle_memory_resources(
                        &oracle_test_roles(4 * 1024 * 1024 * 1024),
                        1024 * 1024,
                    ),
                    telemetry,
                    crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
                ),
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
                    metadata_key: fixture_metadata_key(&fixture.path.to_string_lossy(), size_bytes),
                    location: fixture.path.to_string_lossy().into_owned(),
                    size_bytes,
                    event_time: unusable_event_time(),
                }],
                FileIO::new_with_fs(),
                fixture_storage(),
                Arc::clone(&fixture.schema),
                HotParquetPlan::Leader,
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
                bound_leader_task(
                    memory.clone(),
                    &telemetry,
                    crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
                ),
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
        let closure = required_columns_closure(
            &scan_output_names(&public_schema, Some(&projection)).expect("valid ordinals"),
            &leaves,
        );
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
        let full_closure = required_columns_closure(
            &scan_output_names(&public_schema, None).expect("full public projection"),
            &[],
        );
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
            Some(
                crate::parquet::writer_properties::bifrost_writer_properties(batch.num_rows(), &[]),
            ),
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
        governance: HotParquetPlan,
        metrics: &Arc<OracleScanMetricsHandle>,
        size_bytes: usize,
    ) -> HotParquetExec {
        HotParquetExec::new(
            vec![HotFileSource {
                metadata_key: fixture_metadata_key(&fixture.path.to_string_lossy(), size_bytes),
                location: fixture.path.to_string_lossy().into_owned(),
                size_bytes,
                event_time: unusable_event_time(),
            }],
            FileIO::new_with_fs(),
            fixture_storage(),
            Arc::clone(&fixture.schema),
            governance,
            Arc::clone(metrics),
            Vec::new(),
        )
    }

    /// Builds one bound task context for a leader leaf under test.
    ///
    /// Every leader leaf resolves its governance from the task it executes
    /// with, so a leaf-level test has to publish one grant the same way
    /// admission does.
    fn bound_leader_task(
        memory: OracleMemoryResources,
        telemetry: &Arc<OracleTelemetry>,
        memory_pool: Arc<dyn MemoryPool>,
    ) -> Arc<TaskContext> {
        crate::oracle::bindings::bind_test_session(
            datafusion::prelude::SessionConfig::new(),
            memory_pool,
            crate::oracle::bindings::OracleExecutionGrant::for_test(
                QueryClass::Interactive,
                memory,
                Arc::clone(telemetry),
            ),
            std::collections::HashMap::new(),
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

    /// Builds one admitted leader task context at an explicit batch size.
    ///
    /// This is the shape admission publishes: the session carries the admitted
    /// batch size and pool, and the lock carries the grant, so a leaf that
    /// retained nothing still resolves everything it needs.
    fn admitted_hot_task(
        batch_size: usize,
        memory_pool: &Arc<dyn MemoryPool>,
        memory: OracleMemoryResources,
        telemetry: &Arc<OracleTelemetry>,
    ) -> Arc<TaskContext> {
        crate::oracle::bindings::bind_test_session(
            datafusion::prelude::SessionConfig::new().with_batch_size(batch_size),
            Arc::clone(memory_pool),
            crate::oracle::bindings::OracleExecutionGrant::for_test(
                QueryClass::Interactive,
                memory,
                Arc::clone(telemetry),
            ),
            std::collections::HashMap::new(),
        )
    }

    /// Builds the one valid binding set, proving the two refusals on the way.
    ///
    /// A planned key with no binding and a binding no leaf planned are both
    /// mismatches between the retained plan and the admitted sources, and both
    /// must be refused before any task exists to read rows with.
    ///
    /// # Panics
    ///
    /// Panics when either mismatch is accepted, or when the exact planned key
    /// set is refused.
    fn validated_bindings(
        governor: &crate::resources::BifrostRoleResources,
        telemetry: &Arc<OracleTelemetry>,
        table: &str,
        live: &RecordBatch,
        planned_keys: &[crate::oracle::bindings::OracleSourceKey],
    ) -> crate::oracle::bindings::OracleExecutionBindings {
        let inputs = |batches: std::collections::HashMap<String, Vec<RecordBatch>>| {
            crate::oracle::bindings::OracleExecutionBindingInputs {
                grant: crate::oracle::bindings::OracleExecutionGrant::for_test(
                    QueryClass::Interactive,
                    oracle_memory_resources(governor, 1024 * 1024),
                    Arc::clone(telemetry),
                ),
                local_batches: batches,
                follower_assignments: std::collections::HashMap::new(),
                reservations: Vec::new(),
                degraded: false,
            }
        };
        let bound = || std::collections::HashMap::from([(table.to_owned(), vec![live.clone()])]);
        assert!(
            crate::oracle::bindings::OracleExecutionBindings::try_new(
                inputs(std::collections::HashMap::new()),
                planned_keys,
            )
            .is_err(),
            "a planned key with no binding is refused"
        );
        assert!(
            crate::oracle::bindings::OracleExecutionBindings::try_new(inputs(bound()), &[])
                .is_err(),
            "a binding no leaf planned is refused"
        );
        crate::oracle::bindings::OracleExecutionBindings::try_new(inputs(bound()), planned_keys)
            .expect("the exact planned key set binds")
    }

    /// Builds the one frozen participant a delegated fixture cut names.
    fn fixture_destination() -> crate::oracle::dispatcher::DispatchCandidate {
        crate::oracle::dispatcher::DispatchCandidate {
            node_id: wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7()),
            role: wyrd_spec::vala::api::ClusterRole::Oracle,
            worker_fence: 7,
            endpoint: Some("https://oracle-a.internal".to_owned()),
        }
    }

    /// Mints the assignment the binder publishes for one planned occurrence.
    ///
    /// Built from the key rather than from a pinned cut, because what this
    /// owner proves is the key-to-value agreement the binder validates, not the
    /// file projection a cut contributes.
    fn assignment_for(
        key: &crate::oracle::bindings::FollowerSourceKey,
    ) -> wyrd_spec::vala::api::FollowerScanAssignment {
        wyrd_spec::vala::api::FollowerScanAssignment {
            scan_id: key.scan_id.clone(),
            binding: wyrd_spec::vala::api::TenantTableBinding {
                tenant_id: key.tenant,
                namespace: "traces".to_owned(),
                table: "spans".to_owned(),
            },
            persisted: wyrd_spec::vala::api::PersistedFileAssignment {
                files: ["a", "b", "c", "d"]
                    .into_iter()
                    .map(|path| {
                        wyrd_spec::vala::api::PersistedFileDescriptor::Iceberg(
                            wyrd_spec::vala::api::IcebergFileDescriptor {
                                path: format!("s3://fixture/{path}.parquet"),
                                size_bytes: 1,
                                row_count: 1,
                                snapshot_id: 1,
                                min_event_time_micros: None,
                                max_event_time_micros: None,
                            },
                        )
                    })
                    .collect(),
            },
            scribe_provider_cut: None,
            schema_fingerprint: key.schema_fingerprint.clone(),
            required_columns: key.required_columns.clone(),
            predicates: key.predicates.clone(),
        }
    }

    /// Returns the sole remote occurrence key of one planned root.
    ///
    /// # Panics
    ///
    /// Panics when the root planned anything other than exactly one remote
    /// placeholder, which would mean the fixture stopped delegating its cut.
    fn sole_remote_placeholder(root: &Arc<dyn ExecutionPlan>) -> RemoteSourcePlaceholderExec {
        let mut found = crate::oracle::remote_placeholders(root.as_ref());
        assert_eq!(found.len(), 1, "one delegated tier plans one placeholder");
        found.remove(0)
    }

    /// Two occurrences of one delegated table bind independently and exactly.
    ///
    /// A same-table self-join reaches the one registered provider twice, so both
    /// occurrences share a canonical table and persisted tier and differ only in
    /// their projection closure. Split out of
    /// [`retained_plan_uses_admitted_task_context_only`] to keep each half
    /// readable; it owns the occurrence identity, the task-variant
    /// canonicalization, and the six binding refusals.
    ///
    /// # Panics
    ///
    /// Panics when two occurrences collide, when a task variant fails to share
    /// its occurrence's key or narrows a non-disjoint share, or when any
    /// mismatched binding is accepted.
    async fn assert_repeated_scans_bind_exactly() {
        let (left_leaf, right_leaf) = plan_two_delegated_occurrences().await;
        assert_ne!(
            left_leaf.scan_id(),
            right_leaf.scan_id(),
            "each physical occurrence mints its own scan identity"
        );
        let left_key = left_leaf
            .source_key()
            .expect("a delegated leaf names a key");
        let right_key = right_leaf
            .source_key()
            .expect("a delegated leaf names a key");
        assert_ne!(left_key, right_key, "two occurrences are two keys");
        let (Some(left_source), Some(right_source)) = (left_key.follower(), right_key.follower())
        else {
            panic!("both occurrences are remote");
        };
        assert_ne!(
            left_source.required_columns, right_source.required_columns,
            "each occurrence keeps its own projection closure"
        );
        assert_eq!(left_source.tier, right_source.tier);
        assert_eq!(left_source.table, right_source.table);

        let left_assignment = assignment_for(left_source);
        assert_task_variants_share_one_occurrence(&left_leaf, &left_key, &left_assignment);

        let planned = [left_key.clone(), right_key.clone()];
        let assignments = || {
            HashMap::from([
                (left_key.clone(), left_assignment.clone()),
                (right_key.clone(), assignment_for(right_source)),
            ])
        };
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
        let inputs = |assignments: HashMap<OracleSourceKey, FollowerScanAssignment>| {
            OracleExecutionBindingInputs {
                grant: crate::oracle::bindings::OracleExecutionGrant::for_test(
                    QueryClass::Interactive,
                    oracle_memory_resources(&governor, 1024 * 1024),
                    Arc::clone(&telemetry),
                ),
                local_batches: HashMap::new(),
                follower_assignments: assignments,
                reservations: Vec::new(),
                degraded: false,
            }
        };

        let bound = OracleExecutionBindings::try_new(inputs(assignments()), &planned)
            .expect("the exact planned occurrence set binds");
        assert_eq!(
            bound
                .follower_assignment(&left_key)
                .expect("the first occurrence resolves its own assignment")
                .scan_id,
            left_source.scan_id,
        );
        assert_eq!(
            bound
                .follower_assignment(&right_key)
                .expect("the second occurrence resolves its own assignment")
                .scan_id,
            right_source.scan_id,
        );

        assert_exact_binding_refusals(&BindingRefusalCase {
            inputs: &inputs,
            assignments: &assignments,
            left: left_source,
            right: right_source,
            left_key: &left_key,
            right_key: &right_key,
        });
        assert_eq!(
            pool.reserved(),
            0,
            "every binding refusal happens before any leaf opens row IO"
        );
    }

    /// Plans two physical occurrences of one delegated table and tier.
    ///
    /// This is the shape a same-table self-join reaches the single registered
    /// provider with: two `scan` calls that differ only in the alias-level
    /// projection and predicate each side contributes.
    ///
    /// # Panics
    ///
    /// Panics when either occurrence fails to plan its delegated placeholder.
    async fn plan_two_delegated_occurrences()
    -> (RemoteSourcePlaceholderExec, RemoteSourcePlaceholderExec) {
        let (provider, _live) = projection_closure_provider(
            wyrd_spec::DataTenantId::new_v7(),
            Some(OracleRemoteSource {
                table: "vala.traces.spans".to_owned(),
                destination: fixture_destination(),
                iceberg: true,
            }),
        )
        .await;
        let session = datafusion::execution::context::SessionContext::new();
        let left = provider
            .scan(
                &session.state(),
                Some(&vec![1_usize]),
                &[col("status_code").eq(lit("STATUS_CODE_ERROR"))],
                None,
            )
            .await
            .expect("the first occurrence plans");
        let right = provider
            .scan(&session.state(), Some(&vec![0_usize]), &[], None)
            .await
            .expect("the second occurrence plans");
        (
            sole_remote_placeholder(&left),
            sole_remote_placeholder(&right),
        )
    }

    /// One occurrence's task variants share its key and narrow disjoint shares.
    ///
    /// The split handler produces one variant per stage task. A share divides
    /// work, not authority, so every variant must derive the same occurrence
    /// key and reach the one assignment bound under it, while the files each
    /// one actually reads stay disjoint.
    ///
    /// # Panics
    ///
    /// Panics when a variant derives a different key or two variants overlap.
    fn assert_task_variants_share_one_occurrence(
        leaf: &RemoteSourcePlaceholderExec,
        key: &OracleSourceKey,
        assignment: &FollowerScanAssignment,
    ) {
        let first_task = leaf.clone().with_task_share(0, 2);
        let second_task = leaf.clone().with_task_share(1, 2);
        assert_eq!(first_task.source_key().as_ref(), Some(key));
        assert_eq!(second_task.source_key().as_ref(), Some(key));
        let first_share = first_task.narrow(assignment.clone()).persisted.files;
        let second_share = second_task.narrow(assignment.clone()).persisted.files;
        assert_eq!(first_share.len(), 2);
        assert_eq!(second_share.len(), 2);
        assert!(
            first_share.iter().all(|file| !second_share.contains(file)),
            "task variants narrow onto disjoint file shares"
        );
    }

    /// Everything one mismatch-refusal sweep needs to rebuild its inputs.
    ///
    /// The two closures rebuild a fresh grant and a fresh assignment map per
    /// attempt, because [`OracleExecutionBindings::try_new`] consumes both.
    struct BindingRefusalCase<'a, I, A> {
        /// Rebuilds one complete binding input set around a given map.
        inputs: &'a I,
        /// Rebuilds the exact assignment map both planned occurrences bound.
        assignments: &'a A,
        /// The first occurrence's complete planned authority.
        left: &'a FollowerSourceKey,
        /// The second occurrence's complete planned authority.
        right: &'a FollowerSourceKey,
        /// The first occurrence's key, as the retained plan carries it.
        left_key: &'a OracleSourceKey,
        /// The second occurrence's key, as the retained plan carries it.
        right_key: &'a OracleSourceKey,
    }

    /// Refuses every single-fact disagreement between a plan and its bindings.
    ///
    /// One mutated fact at a time; each one is a plan that no longer describes
    /// the admitted read, and each must be refused while the binding set is
    /// still a value — before any leaf exists to open row IO with.
    ///
    /// # Panics
    ///
    /// Panics when any mutated occurrence, repeated occurrence, or swapped
    /// assignment value is accepted.
    fn assert_exact_binding_refusals<I, A>(case: &BindingRefusalCase<'_, I, A>)
    where
        I: Fn(HashMap<OracleSourceKey, FollowerScanAssignment>) -> OracleExecutionBindingInputs,
        A: Fn() -> HashMap<OracleSourceKey, FollowerScanAssignment>,
    {
        let BindingRefusalCase {
            inputs,
            assignments,
            left: left_source,
            right: right_source,
            left_key,
            right_key,
        } = *case;
        let mut destination_role = left_source.clone();
        destination_role.destination.role = wyrd_spec::vala::api::ClusterRole::Scribe;
        let mut destination_fence = left_source.clone();
        destination_fence.destination.worker_fence += 1;
        let mut destination_endpoint = left_source.clone();
        destination_endpoint.destination.endpoint = Some("https://oracle-b.internal".to_owned());
        let mut destination_node = left_source.clone();
        destination_node.destination.node_id =
            wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        let mut other_tenant = left_source.clone();
        other_tenant.tenant = wyrd_spec::DataTenantId::new_v7();
        let mut other_table = left_source.clone();
        other_table.table = "vala.logs.records".to_owned();
        let mut other_tier = left_source.clone();
        other_tier.tier = crate::oracle::RemotePersistedTier::Hot;
        let mut other_schema = left_source.clone();
        other_schema.schema_fingerprint = "not-the-planned-schema".to_owned();
        let mut other_columns = left_source.clone();
        other_columns
            .required_columns
            .push("unused_payload".to_owned());
        let mut other_predicates = left_source.clone();
        other_predicates.predicates.clear();
        for mutated in [
            destination_role,
            destination_fence,
            destination_endpoint,
            destination_node,
            other_tenant,
            other_table,
            other_tier,
            other_schema,
            other_columns,
            other_predicates,
        ] {
            let mutated = OracleSourceKey::Follower(Box::new(mutated));
            assert!(
                OracleExecutionBindings::try_new(
                    inputs(assignments()),
                    &[mutated, right_key.clone()],
                )
                .is_err(),
                "a planned occurrence that differs by one fact resolves nothing"
            );
        }
        // A duplicate canonical occurrence, distinct from the task variants of
        // one occurrence, breaks the exact one-to-one cardinality.
        assert!(
            OracleExecutionBindings::try_new(
                inputs(assignments()),
                &[left_key.clone(), left_key.clone(), right_key.clone()],
            )
            .is_err(),
            "one canonical occurrence may be planned exactly once"
        );
        // A value whose own authority disagrees with the key it was published
        // under is refused even though the key itself resolves.
        let mut replaced = assignments();
        replaced.insert(left_key.clone(), assignment_for(right_source));
        assert!(
            OracleExecutionBindings::try_new(
                inputs(replaced),
                &[left_key.clone(), right_key.clone()],
            )
            .is_err(),
            "one occurrence's assignment cannot answer another's key"
        );
    }

    /// Asserts admission changed no optimizer or memory knob of the retained shape.
    ///
    /// Admission supplies a runtime and a memory pool only, so a grant larger
    /// than the minimum-grant planning shape must leave target partitions,
    /// batch size, hash-join preference, and sort-spill reservation untouched.
    ///
    /// # Panics
    ///
    /// Panics when any of those four knobs differs between the two configs.
    fn assert_same_session_shape(
        planned: &datafusion::prelude::SessionConfig,
        executed: &datafusion::prelude::SessionConfig,
    ) {
        assert_eq!(executed.target_partitions(), planned.target_partitions());
        assert_eq!(executed.batch_size(), planned.batch_size());
        assert_eq!(
            executed.options().optimizer.prefer_hash_join,
            planned.options().optimizer.prefer_hash_join
        );
        assert_eq!(
            executed.options().execution.sort_spill_reservation_bytes,
            planned.options().execution.sort_spill_reservation_bytes
        );
    }

    /// A whole retained root binds once and reads only its admitted task.
    ///
    /// Split out of [`retained_plan_uses_admitted_task_context_only`] to keep
    /// each half readable; it owns the sentinel planning runtime, the binding
    /// validation refusals, and the planning-versus-execution config equality.
    ///
    /// # Panics
    ///
    /// Panics when planning reserves memory, when a mismatched binding is
    /// accepted, when the admitted config drifts from the retained shape, or
    /// when either pool fails to return to zero.
    async fn assert_retained_root_binds_once(
        governor: &crate::resources::BifrostRoleResources,
        telemetry: &Arc<OracleTelemetry>,
    ) {
        // A whole retained root is planned on a sentinel planning runtime and
        // executed on a distinct admitted one. Planning must reach no row
        // source, and the bound Fused rows must arrive through the admitted
        // pool alone.
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let (provider, live) = projection_closure_provider(tenant, None).await;
        let table = "vala.traces.spans".to_owned();
        let planned_keys = [crate::oracle::bindings::OracleSourceKey::LocalDrained {
            table: table.clone(),
        }];

        let shape = crate::resources::OracleSessionShape::for_grant(
            crate::resources::ORACLE_PARTITION_WORKING_MEMORY_BYTES,
            crate::resources::ORACLE_MIN_TARGET_PARTITIONS,
            1,
        );
        let lock = Arc::new(crate::oracle::bindings::OracleExecutionLock::new());
        let retained_config = shape.session_config().with_extension(Arc::clone(&lock));
        let planning_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
        let planning_runtime = Arc::new(
            datafusion::execution::runtime_env::RuntimeEnvBuilder::new()
                .with_memory_pool(Arc::clone(&planning_pool))
                .build()
                .expect("sentinel planning runtime"),
        );
        let planning = datafusion::execution::context::SessionContext::new_with_state(
            datafusion::execution::session_state::SessionStateBuilder::new()
                .with_default_features()
                .with_config(retained_config.clone())
                .with_runtime_env(Arc::clone(&planning_runtime))
                .build(),
        );
        let root = provider
            .scan(&planning.state(), None, &[], None)
            .await
            .expect("retained root plans on the planning session");
        drop(planning);
        assert_eq!(
            planning_pool.reserved(),
            0,
            "planning opens no row source and reserves nothing"
        );

        let bindings = validated_bindings(governor, telemetry, &table, &live, &planned_keys);
        assert!(lock.set(bindings).is_ok(), "a fresh lock binds once");

        let admitted_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
        let admitted = datafusion::execution::context::SessionContext::new_with_state(
            datafusion::execution::session_state::SessionStateBuilder::new()
                .with_default_features()
                .with_config(retained_config.clone())
                .with_runtime_env(Arc::new(
                    datafusion::execution::runtime_env::RuntimeEnvBuilder::new()
                        .with_memory_pool(Arc::clone(&admitted_pool))
                        .build()
                        .expect("admitted query runtime"),
                ))
                .build(),
        );
        let task = admitted.task_ctx();

        assert_same_session_shape(&retained_config, task.session_config());

        let executed = datafusion::physical_plan::collect(Arc::clone(&root), Arc::clone(&task))
            .await
            .expect("the retained root executes on the admitted task");
        assert_eq!(
            executed
                .iter()
                .map(arrow::array::RecordBatch::num_rows)
                .sum::<usize>(),
            2,
            "the bound Fused rows are the retained root's only rows"
        );
        assert_eq!(planning_pool.reserved(), 0, "row IO never touches planning");
        assert_eq!(admitted_pool.reserved(), 0, "settlement returns every byte");

        // A freshly planned root executed on a session carrying no lock refuses
        // at the leaf rather than reading rows from anywhere else.
        let unbound_session =
            datafusion::execution::context::SessionContext::new_with_config(shape.session_config());
        let unbound_root = provider
            .scan(&unbound_session.state(), None, &[], None)
            .await
            .expect("a second root plans");
        assert!(
            datafusion::physical_plan::collect(unbound_root, unbound_session.task_ctx())
                .await
                .is_err(),
            "an unbound session cannot execute a retained root"
        );
        drop(root);
    }

    /// One retained plan reads its runtime, pool, batch size, and bound sources
    /// only from the task it is executed with.
    ///
    /// A physical root is built before admission, so the same hot leaf instance
    /// is executed twice under two different admitted task contexts and must
    /// split at each task's own batch size and charge each task's own pool;
    /// leader and follower modes must agree on rows and scan evidence at every
    /// size. A whole retained root is then planned on a sentinel planning
    /// runtime whose pool must never be reserved against, bound once through the
    /// session `OnceLock`, and executed on a distinct admitted runtime. The
    /// planning and executing `SessionConfig` must still carry the same
    /// minimum-grant target partitions, batch size, hash-join preference, and
    /// sort-spill reservation, because admission supplies capacity rather than
    /// optimizer choices.
    ///
    /// # Panics
    ///
    /// Panics when planning, binding validation, or execution violates any of
    /// those contracts.
    #[tokio::test]
    async fn retained_plan_uses_admitted_task_context_only() {
        let fixture = build_hot_batch_fixture();
        let size = fixture.bytes.len();
        let rows = usize::try_from(HOT_BATCH_FIXTURE_ROWS).expect("fixture rows fit usize");
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));

        for (batch_size, expected_batches) in [(8_usize, 8_usize), (8_192, 1)] {
            let leader_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
            let leader_metrics = Arc::new(OracleScanMetricsHandle::default());
            let leader = hot_exec_for(&fixture, HotParquetPlan::Leader, &leader_metrics, size);
            let leader_counts = drain_hot_row_counts(
                leader
                    .execute(
                        0,
                        admitted_hot_task(
                            batch_size,
                            &leader_pool,
                            oracle_memory_resources(&governor, 1024 * 1024),
                            &telemetry,
                        ),
                    )
                    .expect("leader hot stream"),
            )
            .await;

            let follower_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
            let follower_metrics = Arc::new(OracleScanMetricsHandle::default());
            let follower = hot_exec_for(
                &fixture,
                HotParquetPlan::Follower {
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

        let retained_metrics = Arc::new(OracleScanMetricsHandle::default());
        let retained = hot_exec_for(&fixture, HotParquetPlan::Leader, &retained_metrics, size);
        let split_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
        let split = drain_hot_row_counts(
            retained
                .execute(
                    0,
                    admitted_hot_task(
                        8,
                        &split_pool,
                        oracle_memory_resources(&governor, 1024 * 1024),
                        &telemetry,
                    ),
                )
                .expect("first admitted hot stream"),
        )
        .await;
        let whole_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
        let whole = drain_hot_row_counts(
            retained
                .execute(
                    0,
                    admitted_hot_task(
                        8_192,
                        &whole_pool,
                        oracle_memory_resources(&governor, 1024 * 1024),
                        &telemetry,
                    ),
                )
                .expect("second admitted hot stream"),
        )
        .await;
        assert_eq!(split.len(), 8);
        assert_eq!(whole.len(), 1);
        assert_eq!(split.iter().sum::<usize>(), rows);
        assert_eq!(whole.iter().sum::<usize>(), rows);
        assert_eq!(split_pool.reserved(), 0);
        assert_eq!(whole_pool.reserved(), 0);

        assert_retained_root_binds_once(&governor, &telemetry).await;
        assert_repeated_scans_bind_exactly().await;
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
            HotParquetPlan::Follower {
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
            HotParquetPlan::Follower {
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
            HotParquetPlan::Follower {
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
            HotParquetPlan::Follower {
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

    /// Builds one pinned, snapshot-free Iceberg table carrying the event-time
    /// column the pruning closure is derived from.
    ///
    /// The table has no snapshot, so nothing in this fixture reaches object
    /// storage; it exists to give `OracleTableProvider` the exact physical
    /// schema an event-time predicate must classify against.
    ///
    /// # Panics
    /// Panics if the fixture schema, metadata, or table cannot be constructed.
    fn pruning_fixture_table() -> iceberg::table::Table {
        use iceberg::spec::TableMetadataBuilder;
        use iceberg::spec::{
            FormatVersion, NestedField, PrimitiveType, Schema as IcebergSchema, SortOrder,
            Type as IcebergType, UnboundPartitionSpec,
        };
        use iceberg::{TableIdent, io::FileIO};
        use wyrd_spec::vala::WYRD_EVENT_TIME;

        let schema = IcebergSchema::builder()
            .with_schema_id(0)
            .with_fields(vec![
                Arc::new(NestedField::optional(
                    1,
                    WYRD_EVENT_TIME,
                    IcebergType::Primitive(PrimitiveType::Timestamptz),
                )),
                Arc::new(NestedField::optional(
                    2,
                    "duration_ms",
                    IcebergType::Primitive(PrimitiveType::Long),
                )),
                Arc::new(NestedField::required(
                    3,
                    DATA_TENANT_ID,
                    IcebergType::Primitive(PrimitiveType::String),
                )),
            ])
            .build()
            .expect("fixture Iceberg schema");
        let metadata = TableMetadataBuilder::new(
            schema,
            UnboundPartitionSpec::builder().build(),
            SortOrder::unsorted_order(),
            "memory:///pruning-fixture".to_owned(),
            FormatVersion::V2,
            HashMap::new(),
        )
        .expect("fixture table metadata builder")
        .build()
        .expect("fixture table metadata")
        .metadata;
        iceberg::table::Table::builder()
            .file_io(FileIO::new_with_memory())
            .metadata(metadata)
            .identifier(TableIdent::from_strs(["traces", "spans"]).expect("fixture identifier"))
            .runtime(iceberg::Runtime::current())
            .build()
            .expect("pinned fixture table")
    }

    /// Builds one leader-local provider over the pruning fixture's hot files.
    ///
    /// # Panics
    /// Panics if the authorization context or the provider cannot be built,
    /// since neither is the behavior under test.
    async fn pruning_provider(
        tenant: wyrd_spec::DataTenantId,
        hot_files: Vec<HotFileSource>,
    ) -> OracleTableProvider {
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
        OracleTableProvider::try_new(OracleTableInputs {
            table: pruning_fixture_table(),
            storage: fixture_storage(),
            hot_files,
            iceberg_event_times: Vec::new(),
            context,
            table_name: "vala.traces.spans".to_owned(),
            audit: Arc::new(NoopAudit),
            remote: None,
        })
        .await
        .expect("pruning fixture provider")
    }

    /// Returns the locations the plan's hot leaf will open, in plan order.
    ///
    /// # Panics
    /// Panics when the plan contains no `HotParquetExec`, which would mean the
    /// provider dropped the hot leaf entirely rather than pruning within it.
    fn retained_hot_locations(plan: &Arc<dyn ExecutionPlan>) -> Vec<String> {
        fn walk(plan: &Arc<dyn ExecutionPlan>) -> Option<Vec<String>> {
            if let Some(exec) = plan.downcast_ref::<HotParquetExec>() {
                return Some(
                    exec.files
                        .iter()
                        .map(|file| file.location.clone())
                        .collect(),
                );
            }
            plan.children().into_iter().find_map(walk)
        }
        walk(plan).expect("the plan retains a hot Parquet leaf")
    }

    /// Builds one hot source with the named bounds.
    fn hot_file(
        name: &str,
        event_time: crate::catalog::event_time::EventTimeStatistics,
    ) -> HotFileSource {
        HotFileSource {
            metadata_key: fixture_metadata_key(name, 4_096),
            location: format!("memory:///pruning-fixture/hot/{name}"),
            size_bytes: 4_096,
            event_time,
        }
    }

    /// A supported closed event-time predicate excludes every provably
    /// non-overlapping hot file from the plan the provider builds, and excludes
    /// nothing when the evidence cannot support it.
    ///
    /// This is the production seam for staged hot Parquet, which is the only
    /// persisted source a leader-local scan selects itself: the pinned Iceberg
    /// leaf hands its predicate to Iceberg's own manifest planning, which
    /// already prunes by these same bounds and by every other column's.
    /// `HotParquetExec` opens exactly the files it is given, so a file absent
    /// from that list is a file whose footer is never opened and whose data
    /// ranges never leave storage — the assertion proves exclusion *before*
    /// I/O rather than after it.
    ///
    /// Endpoint overlap, unusable bounds, and an unsupported predicate are
    /// asserted in the same owner because they are the three ways this decision
    /// must decline to exclude; splitting them would let a version that prunes
    /// on a touching endpoint, or on a missing bound, still pass a
    /// "non-overlapping file is excluded" test.
    ///
    /// # Panics
    /// Panics when the scan or its retained file list violates the pruning
    /// contract.
    #[tokio::test]
    async fn supported_event_time_predicate_excludes_hot_files_before_any_footer_open() {
        use datafusion::execution::context::SessionContext;
        use datafusion::logical_expr::{col, lit};
        use datafusion::scalar::ScalarValue;

        let lower_micros = 1_787_493_600_000_000_i64;
        let upper_micros = 1_787_497_200_000_000_i64;
        let bounded =
            |min_micros, max_micros| crate::catalog::event_time::EventTimeStatistics::Bounded {
                min_micros,
                max_micros,
            };
        let hot_files = vec![
            // Spans the window outright.
            hot_file("overlapping.parquet", bounded(lower_micros, upper_micros)),
            // Touches the upper endpoint exactly; closed intervals overlap.
            hot_file("endpoint.parquet", bounded(upper_micros, upper_micros + 9)),
            // Entirely before the window.
            hot_file("disjoint.parquet", bounded(0, lower_micros - 1)),
            // A row whose durable bounds never decoded.
            hot_file("unusable.parquet", unusable_event_time()),
        ];
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let provider = pruning_provider(tenant, hot_files.clone()).await;
        let session = SessionContext::new().state();
        let event_time = || col(wyrd_spec::vala::WYRD_EVENT_TIME);
        let bound = |micros: i64| {
            lit(ScalarValue::TimestampMicrosecond(
                Some(micros),
                Some("UTC".into()),
            ))
        };

        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let guard = metrics::set_default_local_recorder(&recorder);
        let plan = provider
            .scan(
                &session,
                None,
                &[
                    event_time().gt_eq(bound(lower_micros)),
                    event_time().lt_eq(bound(upper_micros)),
                ],
                None,
            )
            .await
            .expect("bounded scan plans");
        drop(guard);

        assert_eq!(
            retained_hot_locations(&plan),
            vec![
                "memory:///pruning-fixture/hot/overlapping.parquet".to_owned(),
                "memory:///pruning-fixture/hot/endpoint.parquet".to_owned(),
                "memory:///pruning-fixture/hot/unusable.parquet".to_owned(),
            ],
            "only the provably disjoint hot file is excluded"
        );

        // Every considered file emits exactly one bounded outcome, so the
        // emitted counts reconcile against the four files the cut named.
        let snapshot = recorder.snapshot();
        let count = |outcome: &str| {
            snapshot
                .counters
                .get(&format!(
                    "bifrost_oracle_file_pruning_total{{outcome=\"{outcome}\",source=\"hot\"}}"
                ))
                .copied()
                .unwrap_or_default()
        };
        assert_eq!(count("included"), 2, "{snapshot:?}");
        assert_eq!(count("excluded"), 1, "{snapshot:?}");
        assert_eq!(count("fail_open_missing_bounds"), 1, "{snapshot:?}");

        // An unsupported predicate constrains nothing, so nothing is excluded.
        let unconstrained = pruning_provider(tenant, hot_files).await;
        let plan = unconstrained
            .scan(&session, None, &[col("duration_ms").gt(lit(1_i64))], None)
            .await
            .expect("unconstrained scan plans");
        assert_eq!(
            retained_hot_locations(&plan).len(),
            4,
            "an unconstrained query excludes no hot file"
        );
    }

    /// Builds one pinned, snapshot-free Iceberg table over the closure fixture
    /// schema, backed entirely by in-memory storage.
    ///
    /// The table carries no snapshot, so no leaf in this fixture ever reaches
    /// object storage; it exists only to give `OracleTableProvider` the exact
    /// complete physical schema the projection closure is derived from.
    ///
    /// # Panics
    /// Panics if the fixture schema, metadata, or table cannot be constructed,
    /// which would mean the fixture no longer has the shape the owner asserts.
    fn projection_fixture_table() -> iceberg::table::Table {
        use iceberg::spec::TableMetadataBuilder;
        use iceberg::spec::{
            FormatVersion, NestedField, PrimitiveType, Schema as IcebergSchema, SortOrder,
            Type as IcebergType, UnboundPartitionSpec,
        };
        use iceberg::{TableIdent, io::FileIO};

        let optional = |id: i32, name: &str, kind: PrimitiveType| {
            Arc::new(NestedField::optional(
                id,
                name,
                IcebergType::Primitive(kind),
            ))
        };
        let schema = IcebergSchema::builder()
            .with_fields(vec![
                optional(1, "unused_payload", PrimitiveType::String),
                optional(2, "duration_ms", PrimitiveType::Long),
                optional(3, "status_code", PrimitiveType::String),
                Arc::new(NestedField::required(
                    4,
                    DATA_TENANT_ID,
                    IcebergType::Primitive(PrimitiveType::String),
                )),
            ])
            .build()
            .expect("fixture Iceberg schema");
        let metadata = TableMetadataBuilder::new(
            schema,
            UnboundPartitionSpec::builder().build(),
            SortOrder::unsorted_order(),
            "memory:///projection-fixture".to_owned(),
            FormatVersion::V2,
            HashMap::new(),
        )
        .expect("fixture table metadata builder")
        .build()
        .expect("fixture table metadata")
        .metadata;
        iceberg::table::Table::builder()
            .file_io(FileIO::new_with_memory())
            .metadata(metadata)
            .identifier(TableIdent::from_strs(["traces", "spans"]).expect("fixture identifier"))
            .runtime(iceberg::Runtime::current())
            .build()
            .expect("pinned fixture table")
    }

    /// Returns every union child's field names, in child and field order.
    ///
    /// # Panics
    /// Panics if `plan` is not the `UnionExec` the fixture builds.
    fn union_child_column_names(plan: &Arc<dyn ExecutionPlan>) -> Vec<Vec<String>> {
        plan.downcast_ref::<UnionExec>()
            .expect("scan builds a union over its disjoint leaves")
            .children()
            .into_iter()
            .map(|child| {
                child
                    .schema()
                    .fields()
                    .iter()
                    .map(|field| field.name().clone())
                    .collect()
            })
            .collect()
    }

    /// Returns one plan's output field names in order.
    fn column_names(plan: &Arc<dyn ExecutionPlan>) -> Vec<String> {
        plan.schema()
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect()
    }

    /// Builds the pinned wide-table provider the closure owner scans.
    ///
    /// The live batch carries the full four-column physical schema —
    /// `unused_payload`, `duration_ms`, `status_code`, `data_tenant_id` — with
    /// one `STATUS_CODE_ERROR` row and one `STATUS_CODE_OK` row, both owned by
    /// `tenant`. Keeping fixture construction here leaves the owning test to
    /// assert only closure behavior.
    ///
    /// # Panics
    /// Panics if the authorization context, fixture batch, or provider cannot
    /// be constructed, since none of those are the behavior under test.
    async fn projection_closure_provider(
        tenant: wyrd_spec::DataTenantId,
        remote: Option<OracleRemoteSource>,
    ) -> (OracleTableProvider, RecordBatch) {
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
        let live_schema = Arc::new(Schema::new(vec![
            Field::new("unused_payload", DataType::Utf8, true),
            Field::new("duration_ms", DataType::Int64, true),
            Field::new("status_code", DataType::Utf8, true),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
        ]));
        let live = RecordBatch::try_new(
            Arc::clone(&live_schema),
            vec![
                Arc::new(StringArray::from(vec!["wide-a", "wide-b"])) as ArrayRef,
                Arc::new(Int64Array::from(vec![41_i64, 97_i64])) as ArrayRef,
                Arc::new(StringArray::from(vec![
                    "STATUS_CODE_ERROR",
                    "STATUS_CODE_OK",
                ])) as ArrayRef,
                Arc::new(StringArray::from(vec![
                    tenant.to_string(),
                    tenant.to_string(),
                ])) as ArrayRef,
            ],
        )
        .expect("live fixture batch");
        let provider = OracleTableProvider::try_new(OracleTableInputs {
            table: projection_fixture_table(),
            storage: fixture_storage(),
            hot_files: Vec::new(),
            iceberg_event_times: Vec::new(),
            context,
            table_name: "vala.traces.spans".to_owned(),
            audit: Arc::new(NoopAudit),
            remote,
        })
        .await
        .expect("pinned fixture provider");
        (provider, live)
    }

    /// One leader-owned closure governs every leaf, the placeholder, the
    /// tripwire, the provider-local filter, and the public result.
    ///
    /// This is the production Interactive leader path for
    /// `SELECT duration_ms FROM ... WHERE status_code = 'STATUS_CODE_ERROR'`.
    /// The closure is `[duration_ms, status_code, data_tenant_id]`: the
    /// requested output, the predicate-only column that must survive to the
    /// provider-local filter, and the hidden tenant column that must survive to
    /// the tripwire. `unused_payload` is requested by nobody and must not
    /// appear in any leaf.
    ///
    /// # Panics
    /// Panics if provider construction, scan planning, or execution violates
    /// the closure contract this owner pins.
    #[tokio::test]
    async fn projected_leaf_union_preserves_predicate_and_tenant_columns() {
        use datafusion::execution::context::SessionContext;
        use datafusion::logical_expr::{col, lit};
        use datafusion::physical_plan::collect;
        use datafusion::physical_plan::filter::FilterExec;

        let tenant = wyrd_spec::DataTenantId::new_v7();
        let (provider, live) = projection_closure_provider(tenant, None).await;

        let public = provider.schema();
        let projection = vec![public.index_of("duration_ms").expect("public duration_ms")];
        let predicate = col("status_code").eq(lit("STATUS_CODE_ERROR"));
        let session = SessionContext::new().state();
        let plan = provider
            .scan(&session, Some(&projection), &[predicate], None)
            .await
            .expect("closure scan plans");

        // The public result is exactly the requested column.
        assert_eq!(column_names(&plan), vec!["duration_ms".to_string()]);

        let filtered = Arc::clone(plan.children()[0]);
        let filter = filtered
            .downcast_ref::<FilterExec>()
            .expect("provider keeps its local filter over the closed predicates");
        let tripwire_plan = Arc::clone(filter.children()[0]);
        let tripwire = tripwire_plan
            .downcast_ref::<TenantTripwireExec>()
            .expect("tripwire sits directly under the provider-local filter");

        // The tripwire consumes the tenant column and never emits it.
        let union = Arc::clone(tripwire.children()[0]);
        let closure = vec![
            "duration_ms".to_string(),
            "status_code".to_string(),
            DATA_TENANT_ID.to_string(),
        ];
        assert_eq!(column_names(&union), closure);
        assert_eq!(
            column_names(&tripwire_plan),
            vec!["duration_ms".to_string(), "status_code".to_string()]
        );

        // Every union child — the published Iceberg leaf and the bound live
        // leaf alike — exposes exactly the closure, in closure order.
        let children = union_child_column_names(&union);
        assert_eq!(children.len(), 2, "published and live leaves both planned");
        for child in &children {
            assert_eq!(child, &closure);
        }

        // The predicate resolves `status_code` against the closure, not against
        // the original full physical schema.
        let predicate_columns =
            datafusion::physical_expr::utils::collect_columns(filter.predicate())
                .into_iter()
                .map(|column| (column.name().to_string(), column.index()))
                .collect::<Vec<_>>();
        assert_eq!(predicate_columns, vec![("status_code".to_string(), 1)]);

        // One ERROR row and one OK row in; only the ERROR duration out.
        let rows = collect(
            plan,
            crate::oracle::bindings::bind_test_session(
                datafusion::prelude::SessionConfig::new(),
                crate::resources::bounded_memory_pool(64 * 1024 * 1024),
                crate::oracle::bindings::OracleExecutionGrant::for_test(
                    QueryClass::Interactive,
                    oracle_memory_resources(&oracle_test_roles(1024 * 1024 * 1024), 1024 * 1024),
                    Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1)))),
                ),
                std::collections::HashMap::from([("vala.traces.spans".to_owned(), vec![live])]),
            ),
        )
        .await
        .expect("closure plan executes");
        let durations = rows
            .iter()
            .flat_map(|batch| {
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("result preserves duration_ms as Int64")
                    .iter()
                    .flatten()
            })
            .collect::<Vec<_>>();
        assert_eq!(durations, vec![41_i64]);
    }

    /// The physical root type alone decides the admitted query class.
    ///
    /// Revision 6 removed every pre-planning classifier: a normal `DataFusion`
    /// root is Interactive, and only the exact `DistributedExec` root the
    /// pinned distributed planner returns is Analytical. Both roots asserted
    /// here come from real planning — the ordinary one from `DataFusion`'s own
    /// planner and the distributed one from the pinned planner — so the
    /// assertion covers exactly the value production derives its class from.
    /// Cut, attempt, build-count, terminal, and ownership behavior belong to
    /// the public routing journey and are deliberately absent here.
    ///
    /// # Panics
    ///
    /// Panics when either statement cannot be lowered, or when the pinned
    /// planner returns no `DistributedExec` root for the grouped fixture.
    #[tokio::test]
    async fn physical_root_alone_selects_query_class() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("key", DataType::Int64, false),
            Field::new("value", DataType::Int64, false),
        ]));
        let batch = |offset: i64| {
            RecordBatch::try_new(
                Arc::clone(&schema),
                vec![
                    Arc::new(Int64Array::from(vec![offset, offset + 1])) as ArrayRef,
                    Arc::new(Int64Array::from(vec![offset * 10, offset * 20])) as ArrayRef,
                ],
            )
            .expect("root-class fixture batch")
        };
        let partitions = vec![vec![batch(0)], vec![batch(2)]];

        let ordinary = datafusion::prelude::SessionContext::new();
        ordinary
            .register_table(
                "rows",
                Arc::new(
                    datafusion::datasource::MemTable::try_new(
                        Arc::clone(&schema),
                        partitions.clone(),
                    )
                    .expect("ordinary fixture provider"),
                ),
            )
            .expect("register ordinary fixture");
        let ordinary_root = ordinary
            .sql("SELECT key, sum(value) FROM rows GROUP BY key")
            .await
            .expect("lower ordinary statement")
            .create_physical_plan()
            .await
            .expect("build ordinary physical root");
        assert!(
            ordinary_root
                .downcast_ref::<datafusion_distributed::DistributedExec>()
                .is_none(),
            "the ordinary planner must not return a distributed root"
        );
        assert_eq!(
            query_class_for_root(ordinary_root.as_ref()),
            QueryClass::Interactive
        );

        let mut config = datafusion::prelude::SessionConfig::new();
        datafusion_distributed::DistributedExt::set_distributed_worker_resolver(
            &mut config,
            RootClassWorkers,
        );
        datafusion_distributed::DistributedExt::set_distributed_desired_task_count_handler(
            &mut config,
            RootClassTaskCount,
        );
        let state = <datafusion::execution::session_state::SessionStateBuilder as datafusion_distributed::SessionStateBuilderExt>::with_distributed_planner(
            datafusion::execution::session_state::SessionStateBuilder::new()
                .with_default_features()
                .with_config(config),
        )
        .build();
        let distributed = datafusion::prelude::SessionContext::new_with_state(state);
        distributed
            .register_table(
                "rows",
                Arc::new(
                    datafusion::datasource::MemTable::try_new(Arc::clone(&schema), partitions)
                        .expect("distributed fixture provider"),
                ),
            )
            .expect("register distributed fixture");
        let distributed_root = distributed
            .sql("SELECT key, sum(value) FROM rows GROUP BY key")
            .await
            .expect("lower distributed statement")
            .create_physical_plan()
            .await
            .expect("build distributed physical root");
        assert!(
            distributed_root
                .downcast_ref::<datafusion_distributed::DistributedExec>()
                .is_some(),
            "the pinned planner must return a distributed root for the grouped fixture"
        );
        assert_eq!(
            query_class_for_root(distributed_root.as_ref()),
            QueryClass::Analytical
        );

        // A distributed subtree that is not the exact root stays Interactive:
        // the class is read from the root alone, never from the tree beneath it.
        let wrapped: Arc<dyn ExecutionPlan> = Arc::new(
            datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec::new(
                Arc::clone(&distributed_root),
            ),
        );
        assert_eq!(
            query_class_for_root(wrapped.as_ref()),
            QueryClass::Interactive
        );
    }

    /// Forces every leaf stage of the root-class fixture onto two tasks.
    ///
    /// The pinned planner elides a boundary it judges unnecessary, and a two-row
    /// in-memory table is always judged unnecessary. Oracle's production planner
    /// answers this same handler from the frozen cut rather than from scanned
    /// bytes, so answering it here plans the shape production plans instead of
    /// forcing an outcome the planner would otherwise refuse.
    #[derive(Debug)]
    struct RootClassTaskCount;

    #[async_trait::async_trait]
    impl datafusion_distributed::DesiredTaskCountHandler for RootClassTaskCount {
        /// Requests two tasks for every leaf node and defers on inner nodes.
        ///
        /// # Errors
        ///
        /// Never fails: the count is a constant.
        async fn handle(
            &self,
            ev: datafusion_distributed::DesiredTaskCountEvent<'_>,
        ) -> Option<DataFusionResult<datafusion_distributed::DesiredTaskCountEventResponse>>
        {
            if ev.plan.children().is_empty() {
                Some(Ok(
                    datafusion_distributed::DesiredTaskCountEventResponse::desired(2),
                ))
            } else {
                None
            }
        }
    }

    /// Frozen worker set the root-class fixture plans its distributed root over.
    #[derive(Debug)]
    struct RootClassWorkers;

    impl datafusion_distributed::WorkerResolver for RootClassWorkers {
        /// Returns two fixed peers so the pinned planner may form a real stage.
        ///
        /// # Errors
        ///
        /// Never fails: both URLs are constant and already valid.
        fn get_urls(&self) -> DataFusionResult<Vec<url::Url>> {
            Ok(vec![
                url::Url::parse("http://worker-0.invalid:50051").expect("fixture worker URL"),
                url::Url::parse("http://worker-1.invalid:50051").expect("fixture worker URL"),
            ])
        }
    }
}
