//! `DataFusion` physical sources and invariant operators owned by Oracle.
//!
//! Every table enters `DataFusion` through one disjoint source union. Tenant
//! validation surrounds that union so a foreign row cannot influence a filter,
//! join, aggregate, or limit.

use std::error::Error;
use std::fmt;
#[cfg(test)]
use std::fs::File;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use arrow::array::Array;
use arrow::compute::cast;
#[cfg(test)]
use arrow::datatypes::Field;
use arrow::datatypes::{Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use bytes::Bytes;
use datafusion::catalog::Session;
use datafusion::common::Statistics;
use datafusion::common::config::TableParquetOptions;
use datafusion::common::stats::Precision;
use datafusion::config::ConfigOptions;
use datafusion::datasource::listing::PartitionedFile;
use datafusion::datasource::physical_plan::parquet::metadata::DFParquetMetadata;
use datafusion::datasource::physical_plan::parquet::{
    CachedParquetFileReaderFactory, DefaultParquetFileReaderFactory, ParquetFileReaderFactory,
};
use datafusion::datasource::physical_plan::{
    FileGroup, FileScanConfig, FileScanConfigBuilder, ParquetSource,
};
use datafusion::datasource::source::DataSourceExec;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::cache::cache_manager::FileMetadataCache;
use datafusion::execution::object_store::ObjectStoreUrl;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::object_store::local::LocalFileSystem;
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::aggregates::AggregateExec;
use datafusion::physical_plan::joins::{HashJoinExec, SortMergeJoinExec};
use datafusion::physical_plan::metrics::{ExecutionPlanMetricsSet, MetricValue, MetricsSet};
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::union::UnionExec;
use datafusion_datasource::compute_all_files_statistics;
use datafusion_distributed::NetworkBoundaryExt as _;
use futures_util::future::BoxFuture;
use futures_util::{FutureExt, TryFutureExt};
use futures_util::{StreamExt as _, TryStreamExt};
use iceberg::scan::FileScanTask;
use iceberg_datafusion::IcebergStaticTableProvider;
use iceberg_datafusion::physical_plan::IcebergTableScan;
use parquet::arrow::arrow_reader::ArrowReaderOptions;
use parquet::arrow::async_reader::AsyncFileReader;
use parquet::errors::ParquetError;
use parquet::file::metadata::ParquetMetaData;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::BifrostError;

use crate::catalog::iceberg_storage::BifrostIcebergStorage;
use crate::scribe::hot_source::StagedSourceLease;
use crate::storage::error_chain_contains_not_found;
use wyrd_spec::vala::api::{QueryClass, WorkerScanStats};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

use super::live::LiveScribeExec;
use super::{AuthorizedQueryContext, QueryCatalogError};

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

/// Reads one `DataFusion` Parquet scan's pruning metrics as
/// [`PruningEvidence`].
///
/// `DataFusion` records every row group it considers as either pruned or
/// matched by statistics, then, of those statistics kept, as pruned or matched
/// by Bloom filters; with no predicate both stages record every group as
/// matched. The groups read are therefore the Bloom stage's matches.
fn parquet_pruning(metrics: &MetricsSet) -> PruningEvidence {
    let pruning = |name: &str| match metrics.sum_by_name(name) {
        Some(MetricValue::PruningMetrics {
            pruning_metrics, ..
        }) => (
            pruning_metrics.pruned() as u64,
            pruning_metrics.matched() as u64,
        ),
        _ => (0, 0),
    };
    let (statistics_pruned, _) = pruning("row_groups_pruned_statistics");
    let (bloom_pruned, bloom_matched) = pruning("row_groups_pruned_bloom_filter");
    let (page_rows_pruned, _) = pruning("page_index_rows_pruned");
    PruningEvidence {
        row_groups_scanned: bloom_matched,
        row_groups_pruned: statistics_pruned.saturating_add(bloom_pruned),
        row_groups_pruned_bloom: bloom_pruned,
        rows_pruned_page_index: page_rows_pruned,
    }
}

/// Per-mechanism row-group and page pruning evidence of one or more scans.
///
/// `row_groups_pruned` counts every excluded row group whatever excluded it;
/// `row_groups_pruned_bloom` is the subset a Bloom filter excluded after
/// statistics kept it, so statistics-only exclusion is the difference.
/// `rows_pruned_page_index` counts rows skipped inside retained row groups.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PruningEvidence {
    /// Row groups read after every row-group pruning mechanism.
    pub(crate) row_groups_scanned: u64,
    /// Row groups excluded by statistics or Bloom filters.
    pub(crate) row_groups_pruned: u64,
    /// Row groups a Bloom filter excluded after statistics retained them.
    pub(crate) row_groups_pruned_bloom: u64,
    /// Rows in retained row groups that page-index selection skipped.
    pub(crate) rows_pruned_page_index: u64,
}

impl PruningEvidence {
    /// Adds `other` into this total, saturating every counter.
    pub(crate) fn add(&mut self, other: Self) {
        self.row_groups_scanned = self
            .row_groups_scanned
            .saturating_add(other.row_groups_scanned);
        self.row_groups_pruned = self
            .row_groups_pruned
            .saturating_add(other.row_groups_pruned);
        self.row_groups_pruned_bloom = self
            .row_groups_pruned_bloom
            .saturating_add(other.row_groups_pruned_bloom);
        self.rows_pruned_page_index = self
            .rows_pruned_page_index
            .saturating_add(other.rows_pruned_page_index);
    }
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
    /// Summed Bloom-excluded row groups across every completed participant.
    row_groups_pruned_bloom: AtomicU64,
    /// Summed page-index-skipped rows across every completed participant.
    rows_pruned_page_index: AtomicU64,
}

impl RemoteScanMetrics {
    /// Folds one completed participant's footer evidence into the running total.
    ///
    /// Called once per completed participant: a distributed fold records its
    /// whole cut once, and a live Scribe fragment records its footer or
    /// in-process completion once, when that completion validates.
    pub(super) fn record_footer(&self, stats: WorkerScanStats) {
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
        self.row_groups_pruned_bloom
            .fetch_add(stats.row_groups_pruned_bloom, Ordering::Relaxed);
        self.rows_pruned_page_index
            .fetch_add(stats.rows_pruned_page_index, Ordering::Relaxed);
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

    /// Returns the aggregated per-mechanism pruning totals.
    ///
    /// Reported separately from [`Self::terminal_values`] because pruning
    /// evidence exists only for participants running a Parquet leaf; a cut
    /// with no such participant contributes a true zero, not an absence.
    fn terminal_pruning(&self) -> PruningEvidence {
        PruningEvidence {
            row_groups_scanned: self.row_groups_scanned.load(Ordering::Acquire),
            row_groups_pruned: self.row_groups_pruned.load(Ordering::Acquire),
            row_groups_pruned_bloom: self.row_groups_pruned_bloom.load(Ordering::Acquire),
            rows_pruned_page_index: self.rows_pruned_page_index.load(Ordering::Acquire),
        }
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
    /// Per-mechanism row-group and page pruning across every executed leaf.
    pub(crate) pruning: PruningEvidence,
    /// Shared file-source metric sets retained until terminal stream drain.
    physical_metrics: Vec<ExecutionPlanMetricsSet>,
    /// Participant-reported scan accumulators from every remote scan leaf.
    remote_handles: Vec<Arc<RemoteScanMetrics>>,
    /// Prevents duplicate terminal aggregation when a stream closes twice.
    finalized: bool,
}

impl OracleQueryScanStats {
    /// Walks one final physical plan and snapshots scan metrics and file counts.
    #[must_use]
    pub(crate) fn from_plan(plan: &dyn ExecutionPlan) -> Self {
        let mut stats = Self::default();
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
            let metrics = metrics.clone_inner();
            self.pruning.add(parquet_pruning(&metrics));
            let Some(MetricValue::Count { count, .. }) = metrics.sum_by_name("bytes_scanned")
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
            self.files_scanned = self.files_scanned.saturating_add(files);
            self.partitions_scanned = self.partitions_scanned.saturating_add(partitions);
            self.pruning.add(handle.terminal_pruning());
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
            stats.files_scanned = stats
                .files_scanned
                .saturating_add(distinct_files(config) as u64);
            stats
                .physical_metrics
                .push(config.file_source.metrics().clone());
        }
        if let Some(source) = plan.downcast_ref::<LiveScribeExec>() {
            stats.remote_handles.push(Arc::clone(source.scan_metrics()));
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
    let mut totals = WorkerScanStats::default();
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
fn fold_distributed_scan_metrics(node: &Arc<dyn ExecutionPlan>, totals: &mut WorkerScanStats) {
    if let Some(metrics) = node.metrics() {
        let metrics = metrics.aggregate_by_name();
        if let Some(bytes) = sum_named_count(&metrics, "bytes_scanned") {
            totals.bytes_scanned = Some(totals.bytes_scanned.unwrap_or(0).saturating_add(bytes));
        }
        let pruning = parquet_pruning(&metrics);
        totals.row_groups_scanned = totals
            .row_groups_scanned
            .saturating_add(pruning.row_groups_scanned);
        totals.row_groups_pruned = totals
            .row_groups_pruned
            .saturating_add(pruning.row_groups_pruned);
        totals.row_groups_pruned_bloom = totals
            .row_groups_pruned_bloom
            .saturating_add(pruning.row_groups_pruned_bloom);
        totals.rows_pruned_page_index = totals
            .rows_pruned_page_index
            .saturating_add(pruning.rows_pruned_page_index);
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
/// columns the signed closure handed to each Parquet scan, published or hot,
/// as [`hot_parquet_scan`] builds it — without changing
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

    /// Records one Parquet scan's closure as the scan is built.
    pub(super) fn record(projection: Option<&[String]>) {
        if let Ok(mut observed) = OBSERVED.lock() {
            observed.push(projection.map(<[String]>::to_vec));
        }
    }
}

/// Domain marker for one authenticated warehouse object lost after cut
/// selection, raised by Iceberg file planning or by the Parquet read.
#[derive(Debug, thiserror::Error)]
#[error("authenticated Iceberg object disappeared after cut selection")]
struct OracleIcebergStaleObject {
    /// Exact typed storage cause retained for diagnostics and downcast proof.
    #[source]
    source: Box<dyn std::error::Error + Send + Sync>,
}

/// Maps an Iceberg scan failure into the `DataFusion` error the scan raises.
///
/// A catalogued [`BifrostError`] in the chain, such as the footer tenant
/// refusal, is raised through [`QueryCatalogError`] so it keeps its identity
/// across peers. Otherwise a typed Iceberg-only stale marker is preserved
/// without classifying adjacent IO.
pub(super) fn iceberg_datafusion_error<E>(error: E) -> DataFusionError
where
    E: Error + Send + Sync + 'static,
{
    if let Some(catalog) = QueryCatalogError::find(&error) {
        QueryCatalogError::external(catalog)
    } else if error_chain_contains_not_found(&error) {
        DataFusionError::External(Box::new(OracleIcebergStaleObject {
            source: Box::new(error),
        }))
    } else {
        DataFusionError::External(Box::new(error))
    }
}

/// Proves one scanned object's footer names the authenticated tenant.
///
/// This is the one tenant check of the shared Parquet scan: published Iceberg
/// footers, hot objects, and Scribe staged runs all reach it before any row
/// group is decoded. The comparison happens once per opened object, so its
/// cost is independent of row count.
///
/// # Errors
///
/// Returns [`BifrostError::QueryTenantInvariant`] when the footer carries no
/// tenant, more than one tenant, or a tenant other than `tenant`.
fn verify_scanned_footer_tenant(
    metadata: &ParquetMetaData,
    tenant: DataTenantId,
) -> Result<(), BifrostError> {
    crate::parquet::footer::verify_footer_tenant(metadata.file_metadata(), tenant).map_err(
        |detail| {
            tracing::error!(
                detail = %detail,
                "scanned object footer does not prove the authenticated tenant"
            );
            BifrostError::QueryTenantInvariant
        },
    )
}

/// Detects the footer tenant refusal anywhere in an execution error chain.
///
/// [`verify_scanned_footer_tenant`] fails a scan with
/// [`BifrostError::QueryTenantInvariant`] the moment an opened object's footer
/// is missing or names a foreign tenant. That refusal is a security outcome, not a
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

/// Opens every object an Oracle Parquet scan reads through `DataFusion`'s own
/// reader factory, and proves each footer's tenant before a row is decoded.
///
/// This is the one Wyrd-specific step of a hot or staged scan; the reads,
/// footer caching, pruning, and decoding are all `DataFusion`'s.
#[derive(Debug)]
struct TenantParquetReaderFactory {
    /// `DataFusion`'s reader factory the objects are opened through.
    inner: Arc<dyn ParquetFileReaderFactory>,
    /// Authenticated tenant every footer must name.
    tenant: DataTenantId,
}

impl ParquetFileReaderFactory for TenantParquetReaderFactory {
    /// Opens `file` through the inner factory behind the tenant check.
    ///
    /// # Errors
    ///
    /// Returns the inner factory's error.
    fn create_reader(
        &self,
        partition_index: usize,
        file: PartitionedFile,
        metadata_size_hint: Option<usize>,
        metrics: &ExecutionPlanMetricsSet,
    ) -> DataFusionResult<Box<dyn AsyncFileReader + Send>> {
        Ok(Box::new(TenantParquetReader {
            inner: self
                .inner
                .create_reader(partition_index, file, metadata_size_hint, metrics)?,
            tenant: self.tenant,
        }))
    }
}

/// One opened object whose footer is tenant-proved when it is loaded.
struct TenantParquetReader {
    /// `DataFusion`'s reader for the object.
    inner: Box<dyn AsyncFileReader + Send>,
    /// Authenticated tenant the footer must name.
    tenant: DataTenantId,
}

/// Marks a read failure caused by a vanished object as stale.
///
/// A pinned cut can name an object that Forge removed after the cut was
/// chosen. That race is the one failure a caller may replan, so it carries the
/// typed [`OracleIcebergStaleObject`] marker; every other failure is returned
/// unchanged.
fn mark_stale(error: ParquetError) -> ParquetError {
    if error_chain_contains_not_found(&error) {
        ParquetError::External(Box::new(OracleIcebergStaleObject {
            source: Box::new(error),
        }))
    } else {
        error
    }
}

impl AsyncFileReader for TenantParquetReader {
    /// Reads one byte range through the inner reader.
    ///
    /// # Errors
    ///
    /// The returned future fails with the inner reader's error, marked stale
    /// when the object no longer exists.
    fn get_bytes(&mut self, range: Range<u64>) -> BoxFuture<'_, parquet::errors::Result<Bytes>> {
        self.inner.get_bytes(range).map_err(mark_stale).boxed()
    }

    /// Reads several byte ranges through the inner reader.
    ///
    /// # Errors
    ///
    /// The returned future fails with the inner reader's error, marked stale
    /// when the object no longer exists.
    fn get_byte_ranges(
        &mut self,
        ranges: Vec<Range<u64>>,
    ) -> BoxFuture<'_, parquet::errors::Result<Vec<Bytes>>> {
        self.inner
            .get_byte_ranges(ranges)
            .map_err(mark_stale)
            .boxed()
    }

    /// Loads the footer through the inner reader and proves its tenant.
    ///
    /// # Errors
    ///
    /// The returned future fails with the inner reader's error (marked stale
    /// when the object no longer exists), or with
    /// [`BifrostError::QueryTenantInvariant`] in its catalog envelope as the
    /// external source when the footer tenant is missing or foreign; the
    /// envelope is what keeps the code when a worker forwards the error.
    fn get_metadata<'a>(
        &'a mut self,
        options: Option<&'a ArrowReaderOptions>,
    ) -> BoxFuture<'a, parquet::errors::Result<Arc<ParquetMetaData>>> {
        async move {
            let metadata = self.inner.get_metadata(options).await.map_err(mark_stale)?;
            verify_scanned_footer_tenant(&metadata, self.tenant).map_err(|error| {
                ParquetError::External(Box::new(super::QueryCatalogError(error)))
            })?;
            Ok(metadata)
        }
        .boxed()
    }
}

/// Where a hot leaf's Parquet objects live.
pub(crate) enum HotObjects {
    /// Immutable objects under the warehouse, read through the node's storage
    /// owner and footer cache.
    Warehouse(Arc<BifrostIcebergStorage>),
    /// Scribe staged runs on this node's disk. They are rewritable, so their
    /// footers are never cached; the lease, when present, keeps them on disk
    /// for as long as the plan holds its files.
    Staged(Option<StagedSourceLease>),
}

/// Builds one hot leaf: a plain `DataFusion` Parquet scan of `files`.
///
/// The leader's hot files, a follower's hot assignment, and a Scribe
/// follower's staged runs all use it. `DataFusion` owns the read, the footer
/// cache, pruning, the row filter, splitting, and memory; Wyrd adds only the
/// tenant proof of every footer and the Variant expression adapter. Each
/// file's row count, when the manifest records one, is reported as inexact
/// statistics so `DataFusion` decides splitting as it does for a listing table.
/// Inexact, because an exact count lets `DataFusion` answer `count(*)` without
/// opening the file, which would skip that file's footer tenant proof.
///
/// The scan's `ObjectStoreUrl` is the always-registered local one; it is never
/// resolved to a store, because the reader factory carries the real store.
///
/// # Errors
///
/// Returns the `DataFusion` error raised when a warehouse location is outside
/// the warehouse or the file statistics cannot be merged.
///
/// The scan carries no predicate; `DataFusion`'s `FilterPushdown` places the
/// query's predicate in it, and the file pruner then skips a file whose
/// event-time bounds (see [`HotFileSource::statistics`]) exclude it.
pub(super) fn hot_parquet_scan(
    files: Vec<HotFileSource>,
    objects: HotObjects,
    tenant: DataTenantId,
    schema: &SchemaRef,
    options: &Arc<ConfigOptions>,
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    #[cfg(feature = "test-support")]
    iceberg_projection_probe::record(Some(
        &schema
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect::<Vec<_>>(),
    ));
    let (inner, warehouse, lease): (Arc<dyn ParquetFileReaderFactory>, _, _) = match objects {
        HotObjects::Warehouse(store) => {
            let objects = Arc::clone(store.storage().objects());
            let inner: Arc<dyn ParquetFileReaderFactory> = match store.storage().metadata_cache() {
                Some(cache) => Arc::new(CachedParquetFileReaderFactory::new(
                    objects,
                    Arc::clone(cache),
                )),
                None => Arc::new(DefaultParquetFileReaderFactory::new(objects)),
            };
            (inner, Some(store), None)
        }
        HotObjects::Staged(lease) => (
            Arc::new(DefaultParquetFileReaderFactory::new(Arc::new(
                LocalFileSystem::new(),
            ))),
            None,
            lease.map(Arc::new),
        ),
    };
    let source = ParquetSource::new(Arc::clone(schema))
        .with_table_parquet_options(TableParquetOptions {
            global: options.execution.parquet.clone(),
            ..TableParquetOptions::default()
        })
        .with_parquet_file_reader_factory(Arc::new(TenantParquetReaderFactory { inner, tenant }));
    let files = files
        .into_iter()
        .map(|file| {
            let statistics = file.statistics(schema);
            let path = match &warehouse {
                Some(store) => store
                    .relativize(&file.location)
                    .map_err(|error| DataFusionError::External(Box::new(error)))?,
                None => file.location,
            };
            let mut partitioned =
                PartitionedFile::new(path, u64::try_from(file.size_bytes).unwrap_or(u64::MAX));
            if let Some(statistics) = statistics {
                partitioned = partitioned.with_statistics(Arc::new(statistics));
            }
            if let Some(lease) = &lease {
                partitioned = partitioned.with_extension(Arc::clone(lease));
            }
            Ok(partitioned)
        })
        .collect::<DataFusionResult<Vec<_>>>()?;
    let mut groups = FileGroup::new(files).split_files(options.execution.target_partitions.max(1));
    if groups.is_empty() {
        groups.push(FileGroup::new(Vec::new()));
    }
    // Only the file pruner reads a file's exact bounds. Every merged statistic
    // is inexact, so no aggregate is answered without opening (and proving)
    // a footer.
    let (mut groups, statistics) =
        compute_all_files_statistics(groups, Arc::clone(schema), true, true)?;
    for group in &mut groups {
        if let Some(statistics) = group.statistics_mut() {
            *statistics = std::mem::take(statistics).to_inexact();
        }
    }
    let config = FileScanConfigBuilder::new(ObjectStoreUrl::local_filesystem(), Arc::new(source))
        .with_file_groups(groups)
        .with_statistics(statistics)
        .with_expr_adapter(Some(
            super::variant_sql::OracleVariantSql::shared().file_adapter(),
        ))
        .build();
    Ok(DataSourceExec::from_data_source(config))
}

/// The exact files a follower may read from a pinned snapshot.
///
/// `files` holds the storage-qualified locations the leader signed.
/// `aliases` maps every other spelling a manifest may record for one of them —
/// the canonical tenant-relative key and the table-relative suffix — onto that
/// location, so a file recorded in either spelling resolves to the file the
/// leader signed.
pub(crate) struct PublishedAssignment {
    /// Storage-qualified locations the leader signed.
    pub(crate) files: std::collections::BTreeSet<String>,
    /// Alternate manifest spellings, keyed by spelling, valued by location.
    pub(crate) aliases: std::collections::BTreeMap<String, String>,
}

impl PublishedAssignment {
    /// Resolves one planned manifest path to the storage-qualified location an
    /// assignment would name it by.
    fn location(&self, planned: &str) -> String {
        self.aliases
            .get(planned)
            .cloned()
            .unwrap_or_else(|| planned.to_owned())
    }
}

/// Lists the data files Iceberg plans for the pinned snapshot `scan` names,
/// as sources [`hot_parquet_scan`] reads exactly like hot files.
///
/// Iceberg only lists the files. The leader passes no `assigned` and plans
/// with the scan's pushed predicate, so manifest statistics drop files. A
/// follower passes the files its leader signed and plans the snapshot
/// unfiltered: every assigned file must be in that plan, or the snapshot has
/// drifted and the read is refused rather than silently returning fewer rows.
/// Only assigned files are read. A leader's scan of a table with no snapshot
/// returns no file without building an Iceberg scan.
///
/// Wyrd writes no delete file and no encrypted file. A planned file with
/// either is refused: the reader would ignore its deletes, and an encrypted
/// file would bypass the tenant proof.
///
/// # Errors
///
/// Returns the mapped Iceberg error when file planning fails (a vanished
/// manifest is marked stale), [`BifrostError::QueryTenantInvariant`] for a
/// file with key metadata, a not-implemented error for a file with deletes,
/// and a plan error when an assigned file is not in the snapshot.
pub(super) async fn published_files(
    scan: &IcebergTableScan,
    assigned: Option<&PublishedAssignment>,
) -> DataFusionResult<Vec<HotFileSource>> {
    // A leader's scan of a table with no snapshot plans no file; skip
    // building Iceberg's scan, which probes the host's parallelism each time.
    if assigned.is_none()
        && scan.snapshot_id().is_none()
        && scan.table().metadata().current_snapshot().is_none()
    {
        return Ok(Vec::new());
    }
    let mut builder = scan.table().scan();
    if let Some(snapshot_id) = scan.snapshot_id() {
        builder = builder.snapshot_id(snapshot_id);
    }
    if let (None, Some(predicate)) = (assigned, scan.predicates()) {
        builder = builder.with_filter(predicate.clone());
    }
    let mut tasks = builder
        .build()
        .map_err(iceberg_datafusion_error)?
        .plan_files()
        .await
        .map_err(iceberg_datafusion_error)?
        .try_collect::<Vec<FileScanTask>>()
        .await
        .map_err(iceberg_datafusion_error)?;
    if let Some(assigned) = assigned {
        let planned = tasks
            .iter()
            .map(|task| assigned.location(&task.data_file_path))
            .collect::<std::collections::BTreeSet<_>>();
        if !assigned.files.is_subset(&planned) {
            // Counts only: which objects a tenant owns must not reach a log
            // line. They separate "the snapshot lost files the leader signed"
            // from "the two sides spelled the same files differently".
            metrics::counter!(
                "bifrost_oracle_security_events_total",
                "event_class" => "assignment_drift"
            )
            .increment(1);
            tracing::warn!(
                assigned = assigned.files.len(),
                planned = planned.len(),
                unresolved = assigned.files.difference(&planned).count(),
                aliased = assigned.aliases.len(),
                "Oracle follower assignment differs from the pinned snapshot's planned files"
            );
            return Err(DataFusionError::Plan(
                "authenticated Oracle assignment differs from planned files".to_owned(),
            ));
        }
        tasks.retain(|task| {
            assigned
                .files
                .contains(&assigned.location(&task.data_file_path))
        });
    }
    if tasks.iter().any(|task| task.key_metadata.is_some()) {
        return Err(QueryCatalogError::external(
            BifrostError::QueryTenantInvariant,
        ));
    }
    if tasks.iter().any(|task| !task.deletes.is_empty()) {
        return Err(DataFusionError::NotImplemented(
            "a published snapshot carries delete files, which Oracle does not apply".to_owned(),
        ));
    }
    Ok(tasks
        .into_iter()
        .map(|task| HotFileSource {
            location: task.data_file_path,
            size_bytes: usize::try_from(task.file_size_in_bytes).unwrap_or(usize::MAX),
            row_count: task.record_count,
            event_time: crate::catalog::event_time::EventTimeStatistics::Unusable(
                crate::catalog::event_time::EventTimeBoundsDefect::Missing,
            ),
        })
        .collect())
}

/// One validated immutable hot-file source selected by the pinned cut.
#[derive(Debug, Clone)]
pub(crate) struct HotFileSource {
    /// Absolute warehouse location of a hot object, or the local filesystem
    /// path of a Scribe staged run.
    pub(crate) location: String,
    /// The object's exact size, from the manifest or the staged run's file.
    pub(crate) size_bytes: usize,
    /// Exact row count the manifest records, or `None` for a staged run
    /// whose count is not known before its footer is read.
    ///
    /// Reported as the file's exact statistics, so `DataFusion` decides
    /// whether to split a hot scan as it does for a listing table.
    pub(crate) row_count: Option<u64>,
    /// Immutable `wyrd_event_time` interval this object declares, or the exact
    /// reason it declares none.
    ///
    /// Carried from the durable `vala.file_list` row so a scan can decide the
    /// file before it is opened. Unusable statistics retain the file.
    pub(crate) event_time: crate::catalog::event_time::EventTimeStatistics,
}

impl HotFileSource {
    /// `DataFusion` statistics for this file under the scan's `schema`, or
    /// `None` when the file declares neither a row count nor usable bounds.
    ///
    /// The row count is inexact, so `count(*)` still opens the file. Bounded
    /// event time becomes exact `wyrd_event_time` min/max, which
    /// `DataFusion`'s file pruner requires to skip the file before its footer
    /// is read; [`hot_parquet_scan`] makes every merged statistic inexact so
    /// only that pruner sees them. A `schema` without `wyrd_event_time` as
    /// microsecond timestamps carries no bounds, and the file is kept.
    fn statistics(&self, schema: &Schema) -> Option<Statistics> {
        let mut statistics = Statistics::new_unknown(schema);
        if let Some(rows) = self.row_count {
            statistics.num_rows = Precision::Inexact(usize::try_from(rows).unwrap_or(usize::MAX));
        }
        if let crate::catalog::event_time::EventTimeStatistics::Bounded {
            min_micros,
            max_micros,
        } = self.event_time
            && let Ok(index) = schema.index_of(WYRD_EVENT_TIME)
            && let arrow::datatypes::DataType::Timestamp(
                arrow::datatypes::TimeUnit::Microsecond,
                zone,
            ) = schema.field(index).data_type()
        {
            let column = &mut statistics.column_statistics[index];
            column.min_value = Precision::Exact(datafusion::scalar::ScalarValue::TimestampMicrosecond(
                Some(min_micros),
                zone.clone(),
            ));
            column.max_value = Precision::Exact(datafusion::scalar::ScalarValue::TimestampMicrosecond(
                Some(max_micros),
                zone.clone(),
            ));
        }
        (statistics != Statistics::new_unknown(schema)).then_some(statistics)
    }
}

/// Complete immutable inputs for constructing one authenticated table provider.
pub(crate) struct OracleTableInputs {
    /// Pinned Iceberg table for the sealed cut.
    pub(crate) table: iceberg::table::Table,
    /// The warehouse store every published footer and hot object is read through.
    pub(crate) objects: Arc<BifrostIcebergStorage>,
    /// Leader-local hot files absent from the pinned Iceberg snapshot.
    pub(crate) hot_files: Vec<HotFileSource>,
    /// Authenticated request context whose tenant every scanned footer must name.
    pub(crate) context: AuthorizedQueryContext,
    /// Canonical table name used in security diagnostics.
    pub(crate) table_name: String,
    /// Scribe live routes discovered for this table before planning.
    pub(crate) live: Option<super::live::LiveTableRoutes>,
}

/// Complete physical provider for one authenticated table visibility cut.
pub(crate) struct OracleTableProvider {
    /// Pinned Iceberg provider built from immutable table metadata.
    iceberg: IcebergStaticTableProvider,
    /// Pinned hot files absent from the selected Iceberg manifest.
    hot_files: Vec<HotFileSource>,
    /// The warehouse store every published footer and hot object is read through.
    objects: Arc<BifrostIcebergStorage>,
    /// Full physical schema, which is also the caller-visible schema.
    physical_schema: SchemaRef,
    /// Authenticated request context whose tenant every scanned footer must name.
    context: AuthorizedQueryContext,
    /// Canonical table name included in scrubbed security diagnostics.
    table: String,
    /// Scribe live routes discovered for this table before planning.
    live: Option<super::live::LiveTableRoutes>,
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
    /// Narrows already-validated in-memory batches to the scan's closure schema.
    ///
    /// Test fixtures use it to stand in for a live leaf. Live rows arrive at
    /// the complete physical schema.
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
    #[cfg(test)]
    pub(super) fn projected_memory_source(
        batches: &[RecordBatch],
        schema: &SchemaRef,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let projected = batches
            .iter()
            .map(|batch| project_batch(batch, Arc::clone(schema)))
            .collect::<DataFusionResult<Vec<_>>>()?;
        Ok(
            datafusion::datasource::memory::MemorySourceConfig::try_new_exec(
                std::slice::from_ref(&projected),
                Arc::clone(schema),
                None,
            )?,
        )
    }

    /// Builds one provider from an already pinned sealed cut and its discovered live routes.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when Iceberg cannot construct its static
    /// provider.
    pub(crate) async fn try_new(inputs: OracleTableInputs) -> DataFusionResult<Self> {
        let OracleTableInputs {
            table,
            objects,
            hot_files,
            context,
            table_name,
            live,
        } = inputs;
        let iceberg = IcebergStaticTableProvider::try_new_from_table(table)
            .await
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        let physical_schema = iceberg.schema();
        Ok(Self {
            iceberg,
            hot_files,
            objects,
            physical_schema,
            context,
            table: table_name,
            live,
        })
    }
}

impl OracleTableProvider {
    /// Loads each of `files`' footer and page index into the node's footer
    /// cache, as `DataFusion`'s `ListingTable` does per file per scan when it
    /// infers statistics; a file already cached costs one lookup.
    ///
    /// The scan itself asks the cache for the footer alone, then loads the page
    /// index outside the cache, so without this every query reads and decodes
    /// the page index again. Fetches run `meta_fetch_concurrency` at a time,
    /// as `DataFusion`'s do. A failed fetch is left to the scan, which reports
    /// it with its stale-object marker; a node without a cache does nothing.
    async fn cache_footers(&self, files: &[HotFileSource], options: &ConfigOptions) {
        let Some(cache) = self.objects.storage().metadata_cache() else {
            return;
        };
        let fetches: Vec<_> = files
            .iter()
            .map(|file| self.cache_footer(file, cache, options))
            .collect();
        let _ = futures_util::stream::iter(fetches)
            .buffer_unordered(options.execution.meta_fetch_concurrency.get())
            .try_collect::<Vec<()>>()
            .await;
    }

    /// Fetches one warehouse file's footer and page index through
    /// `DataFusion`'s `DFParquetMetadata` into `cache`, keyed exactly as the
    /// scan's own reader keys it.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error of a location outside the warehouse or
    /// a failed read.
    async fn cache_footer(
        &self,
        file: &HotFileSource,
        cache: &Arc<FileMetadataCache>,
        options: &ConfigOptions,
    ) -> DataFusionResult<()> {
        let path = self
            .objects
            .relativize(&file.location)
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        let meta = PartitionedFile::new(path, u64::try_from(file.size_bytes).unwrap_or(u64::MAX))
            .object_meta;
        DFParquetMetadata::new(self.objects.storage().objects().as_ref(), &meta)
            .with_metadata_size_hint(options.execution.parquet.metadata_size_hint)
            .with_file_metadata_cache(Some(Arc::clone(cache)))
            .fetch_metadata()
            .await
            .map(drop)
    }

    /// Builds this cut's persisted leaf: one `DataFusion` Parquet scan over
    /// the published Iceberg objects and the unpublished hot files, as a
    /// listing table plans one `DataSourceExec` over all of its files.
    ///
    /// The scan carries no predicate of its own; `DataFusion`'s filter
    /// pushdown places the query's predicate in it.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when Iceberg cannot list the snapshot's
    /// files, a listed file is refused (see [`published_files`]), or the scan
    /// cannot be built.
    async fn persisted_input(
        &self,
        state: &dyn Session,
        required_schema: &SchemaRef,
        filters: &[Expr],
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let options = state.config().options();
        // Iceberg lists the snapshot's files with the pushed filters.
        let published = self.iceberg.scan(state, None, filters, None).await?;
        let published = published
            .downcast_ref::<IcebergTableScan>()
            .ok_or_else(|| {
                DataFusionError::Plan("Oracle requires the pinned IcebergTableScan".to_owned())
            })?;
        let mut files = published_files(published, None).await?;
        files.extend(self.hot_files.iter().cloned());
        self.cache_footers(&files, options).await;
        hot_parquet_scan(
            files,
            HotObjects::Warehouse(Arc::clone(&self.objects)),
            self.context.data_tenant_id,
            required_schema,
            options,
        )
    }
}

#[async_trait]
impl TableProvider for OracleTableProvider {
    /// Returns the table's complete physical schema, which is also the
    /// caller-visible schema.
    fn schema(&self) -> SchemaRef {
        Arc::clone(&self.physical_schema)
    }

    /// Oracle tables are durable base tables.
    fn table_type(&self) -> TableType {
        TableType::Base
    }

    /// Reports pushdown exactly as the pinned Iceberg provider does, so the
    /// filters Iceberg can plan with reach its manifest pruning.
    ///
    /// # Errors
    ///
    /// Returns the Iceberg provider's error.
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        self.iceberg.supports_filters_pushdown(filters)
    }

    /// Builds the exact `Iceberg + hot + live` disjoint source.
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
        projection: Option<&[usize]>,
        filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // One closure, derived once, governs every leaf below and every
        // operator above. Nothing downstream recomputes a column set or order.
        let scan_projection = OracleScanProjection::try_new(&self.physical_schema, projection)?;
        let required_schema = Arc::clone(&scan_projection.required_schema);
        let mut inputs = vec![
            self.persisted_input(state, &required_schema, filters)
                .await?,
        ];
        // One live leaf over the discovered routes. It captures routes and the
        // closure only; fragments open at execution, after admission and the
        // audited read decision.
        if let Some(live) = self.live.as_ref()
            && !live.routes.is_empty()
        {
            inputs.push(Arc::new(super::live::LiveScribeExec::new(
                live.clone(),
                super::assignment_schema_fingerprint(&self.physical_schema),
                scan_projection.required_columns.clone(),
                Arc::clone(&required_schema),
                state.config().target_partitions(),
            )));
        }
        let union = UnionExec::try_new(inputs)?;
        // Resolved by name against the union's actual output, which is the
        // closure — never against the full-schema ordinals the caller
        // supplied.
        project_plan_by_name(union, &scan_projection.output_names)
    }
}

/// Counts the distinct files one file scan reads.
///
/// `DataFusion`'s byte-range repartitioning lists one file in several groups,
/// so the count deduplicates by object location.
fn distinct_files(config: &FileScanConfig) -> usize {
    config
        .file_groups
        .iter()
        .flat_map(FileGroup::iter)
        .map(|file| &file.object_meta.location)
        .collect::<std::collections::HashSet<_>>()
        .len()
}

/// Resolves `DataFusion`'s requested scan output to public column names.
///
/// A repeated requested ordinal stays repeated: the caller's output shape is
/// the caller's business, and only the leaf closure derived from these names is
/// deduplicated. A `None` projection means every table column; it never means
/// zero columns.
///
/// # Errors
///
/// Returns a `DataFusion` plan error when a requested ordinal falls outside the
/// table schema, which is a planner contract failure rather than a column the
/// scan may quietly drop.
fn scan_output_names(
    schema: &Schema,
    projection: Option<&[usize]>,
) -> DataFusionResult<Vec<String>> {
    let Some(projection) = projection else {
        return Ok(schema
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect());
    };
    projection
        .iter()
        .map(|index| {
            schema
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
/// names, stably deduplicated.
///
/// A closure is never empty. A request that reads no column, such as
/// `COUNT(*)`, still needs every leaf to report row counts, so it reads the
/// always-present non-null `wyrd_event_time` instead of producing zero-column
/// batches across Iceberg, Parquet, IPC, and distributed frames.
fn required_columns_closure(output_names: &[String]) -> Vec<String> {
    let mut required = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in output_names.iter().cloned() {
        if seen.insert(name.clone()) {
            required.push(name);
        }
    }
    if required.is_empty() {
        required.push(WYRD_EVENT_TIME.to_owned());
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
/// leaf, the remote placeholders, the provider-local filter, and the final
/// output projection. No leaf recomputes its own column
/// set or order: a follower revalidates the signed closure against the schema
/// its own catalog resolves, so two components deriving the same set in a
/// different order would refuse each other's assignments.
#[derive(Debug)]
struct OracleScanProjection {
    /// Column names this scan outputs, in requested order, with a repeated
    /// requested ordinal preserved.
    output_names: Vec<String>,
    /// Stable-deduplicated, never-empty leaf closure of the outputs.
    required_columns: Vec<String>,
    /// Complete-schema fields selected by `required_columns`, in that order.
    required_schema: SchemaRef,
}

impl OracleScanProjection {
    /// Derives the closure for one scan request.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error when a requested ordinal is out of
    /// range, when the complete physical schema is ambiguous, or when a closure
    /// name is absent from it.
    fn try_new(physical_schema: &Schema, projection: Option<&[usize]>) -> DataFusionResult<Self> {
        let output_names = scan_output_names(physical_schema, projection)?;
        let required_columns = required_columns_closure(&output_names);
        let (required_schema, _) = select_schema_by_name(physical_schema, &required_columns)?;
        Ok(Self {
            output_names,
            required_columns,
            required_schema,
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

/// Projects one physical batch to the pinned schema by field name.
///
/// The row count is carried explicitly rather than inferred from the columns,
/// because a projected schema is legitimately allowed to be empty: an output
/// projection for `count(*)` has zero columns and a real row count. Arrow
/// cannot recover that count from the columns, so dropping it would turn a
/// valid narrow scan into an execution failure.
///
/// # Errors
///
/// Returns a `DataFusion` error when a required field is missing, a cast fails,
/// or Arrow rejects the projected batch.
pub(super) fn project_batch(
    batch: &RecordBatch,
    schema: SchemaRef,
) -> DataFusionResult<RecordBatch> {
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

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::oracle::bindings::{OracleExecutionBindingInputs, OracleExecutionBindings};
    use crate::oracle::nested_pushdown::FileReadPlan;
    use arrow::array::{ArrayRef, FixedSizeBinaryArray, Int64Array, StringArray};
    use arrow::datatypes::DataType;
    use bytes::Bytes;
    use datafusion::common::tree_node::TreeNode;
    use datafusion::datasource::memory::MemorySourceConfig;
    use datafusion::execution::TaskContext;
    use datafusion::logical_expr::{col, lit};
    use datafusion::physical_expr::PhysicalExpr;
    use datafusion::physical_plan::union::UnionExec;
    use futures_util::StreamExt;
    use parquet::arrow::arrow_reader::ArrowReaderMetadata;
    use parquet::file::properties::WriterProperties;
    use std::collections::HashMap;
    use wyrd_runtime::Principal;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::AuthMethod;

    use crate::oracle::live::LiveTableRoutes;

    /// Event-time statistics for a fixture whose pruning decision is not the
    /// behavior under test.
    ///
    /// Unusable is the fail-open value, so a fixture built with it is retained
    /// by every query interval and cannot accidentally disappear from a test
    /// that is measuring something else.
    fn unusable_event_time() -> crate::catalog::event_time::EventTimeStatistics {
        crate::catalog::event_time::EventTimeStatistics::Unusable(
            crate::catalog::event_time::EventTimeBoundsDefect::Missing,
        )
    }

    /// Builds a warehouse store over a fresh temporary root, composed the way
    /// an Oracle-serving node composes it.
    ///
    /// # Panics
    /// Panics when the temporary root cannot be created.
    fn fixture_objects() -> Arc<BifrostIcebergStorage> {
        warehouse_objects(&tempfile::tempdir().expect("fixture storage root").keep())
    }

    /// Builds the warehouse store rooted at `root`, with the node footer cache.
    pub(in crate::oracle) fn warehouse_objects(
        root: &std::path::Path,
    ) -> Arc<BifrostIcebergStorage> {
        Arc::new(BifrostIcebergStorage::new(
            crate::storage::BifrostStorage::for_test(root, true),
            &format!("file://{}", root.display()),
        ))
    }

    /// The one tenant every fixture object is written for and read as.
    pub(in crate::oracle) static FIXTURE_TENANT: std::sync::LazyLock<DataTenantId> =
        std::sync::LazyLock::new(DataTenantId::new_v7);

    /// Builds the production writer recipe with a footer proving
    /// [`FIXTURE_TENANT`], the way every Bifrost producer writes.
    pub(in crate::oracle) fn fixture_writer_properties(
        bloom_columns: &[String],
    ) -> WriterProperties {
        crate::parquet::writer_properties::bifrost_writer_properties_with_metadata(
            vec![crate::parquet::footer::tenant_key_value(*FIXTURE_TENANT)],
            bloom_columns,
        )
    }

    /// In-memory sources remain unavailable while scan accumulators aggregate
    /// actual bytes.
    #[test]
    fn scan_stats_preserve_non_file_absence_and_scan_aggregation() {
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
        let mut memory_only = OracleQueryScanStats::from_plan(source.as_ref());
        memory_only.finalize();
        assert_eq!(memory_only.physical_bytes_scanned, None);

        let scans = Arc::new(RemoteScanMetrics::default());
        for bytes in [11, 7] {
            scans.record_footer(WorkerScanStats {
                bytes_scanned: Some(bytes),
                files_scanned: 1,
                ..WorkerScanStats::default()
            });
        }
        let mut stats = OracleQueryScanStats {
            remote_handles: vec![scans],
            ..OracleQueryScanStats::default()
        };
        stats.finalize();
        assert_eq!(stats.physical_bytes_scanned, Some(18));
        assert_eq!(stats.files_scanned, 2);
    }

    /// Terminal aggregation is monotonic and emits exactly once after a drop.
    #[test]
    fn scan_stats_terminal_collection_is_exactly_once() {
        let scans = Arc::new(RemoteScanMetrics::default());
        let footer = |bytes| WorkerScanStats {
            bytes_scanned: Some(bytes),
            files_scanned: 1,
            partitions_scanned: 1,
            ..WorkerScanStats::default()
        };
        scans.record_footer(footer(5));
        let mut stats = OracleQueryScanStats {
            remote_handles: vec![Arc::clone(&scans)],
            ..OracleQueryScanStats::default()
        };
        stats.finalize();
        scans.record_footer(footer(9));
        stats.finalize();
        assert_eq!(stats.physical_bytes_scanned, Some(5));
        assert_eq!(stats.files_scanned, 1);
        assert_eq!(stats.partitions_scanned, 1);
    }

    /// Writes one real file of `groups` row groups through the production
    /// writer recipe, one row group per supplied value block, and returns its
    /// bytes. Flushing between blocks is what makes the file multi-row-group:
    /// the production row-group target is 128 MiB of encoded bytes, so no
    /// size-driven fixture could produce two groups at unit scale.
    fn write_grouped_fixture(schema: &SchemaRef, blocks: &[RecordBatch]) -> bytes::Bytes {
        let properties = fixture_writer_properties(&["service_name".to_owned()]);
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

    /// Page selection measured on one production-recipe row group: an
    /// equality leaf on a sorted column skips every page whose index proves it
    /// cannot match, keeps strictly fewer rows than the group, and still
    /// yields the one matching row when the reader applies the selection.
    #[test]
    fn page_index_selects_only_the_matching_pages() {
        use parquet::arrow::arrow_reader::{ArrowReaderOptions, ParquetRecordBatchReaderBuilder};
        const ROWS: i64 = 100_000;
        const TARGET: i64 = 54_321;
        let schema = Arc::new(Schema::new(vec![
            Field::new("service_name", DataType::Utf8, false),
            Field::new("value", DataType::Int64, false),
        ]));
        let published =
            write_grouped_fixture(&schema, &[service_block(&schema, "checkout", 0, ROWS)]);
        let filter = col("value").eq(lit(TARGET));

        let (groups, selection, skipped) = hot_file_plan(&published, &schema, Some(&filter))
            .select_pages(vec![0])
            .expect("page selection");
        assert_eq!(groups, vec![0]);
        let selection = selection.expect("the sorted column's page index excludes pages");
        let kept = selection.row_count();
        assert!(
            kept > 0 && kept < usize::try_from(ROWS).expect("fixture rows fit usize"),
            "kept {kept} of {ROWS} rows"
        );

        let matching: usize = ParquetRecordBatchReaderBuilder::try_new_with_options(
            published.clone(),
            ArrowReaderOptions::new()
                .with_page_index_policy(parquet::file::metadata::PageIndexPolicy::Optional),
        )
        .expect("reader builder")
        .with_row_selection(selection)
        .build()
        .expect("selected reader")
        .map(|batch| {
            let batch = batch.expect("selected batch");
            batch
                .column(1)
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("value column")
                .iter()
                .filter(|value| *value == Some(TARGET))
                .count()
        })
        .sum();
        assert_eq!(matching, 1, "the selection must keep the matching row");
        let (groups, unchanged, none_skipped) = hot_file_plan(&published, &schema, None)
            .select_pages(vec![0])
            .expect("page selection");
        assert!(
            groups == vec![0] && unchanged.is_none() && none_skipped == 0,
            "an empty conjunction leaves the reader unchanged"
        );
        assert_eq!(
            skipped,
            usize::try_from(ROWS).expect("fixture rows fit usize") - kept
        );
    }

    /// Row-group min/max pruning measured on a real two-row-group file written
    /// by the production recipe: the file's low-cardinality `service_name`
    /// column is dictionary-encoded and carries a Bloom filter, but the two
    /// groups hold disjoint values, so the equality leaf retains exactly one
    /// group on min/max statistics alone. No Bloom filter is read here;
    /// `DataFusion`'s Parquet scan owns Bloom pruning. The retained group's
    /// compressed bytes are strictly fewer than the whole file's, and the
    /// decoded rows are exactly the matching rows. A
    /// high-cardinality column in the same recipe stays lossless after
    /// parquet-rs falls back off its dictionary.
    ///
    /// # Panics
    ///
    /// Panics when the fixture footer does not decode, when the file is not two
    /// dictionary-encoded, Bloom-filtered groups, when the equality leaf does not
    /// retain exactly the first group, when the retained bytes are not strictly
    /// fewer than the file's, or when the decoded or high-cardinality rows differ
    /// from what was written.
    #[test]
    fn dictionary_recipe_row_group_min_max_pruning_contract() {
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

        let filter = col("service_name").eq(lit("checkout"));
        let selection = hot_file_plan(&published, &schema, Some(&filter))
            .select_row_groups((0..metadata.num_row_groups()).collect());
        assert_eq!(selection, vec![0]);

        let group_bytes = |index: usize| -> i64 { metadata.row_group(index).compressed_size() };
        let scanned: i64 = selection.iter().map(|index| group_bytes(*index)).sum();
        let whole_file: i64 = (0..metadata.num_row_groups()).map(group_bytes).sum();
        assert!(
            scanned < whole_file,
            "pruning must scan strictly fewer bytes: {scanned} vs {whole_file}"
        );

        let decoded_values = decoded_values_for_service(&published, selection.clone(), "checkout");
        assert_eq!(decoded_values, (0..BLOCK_ROWS).collect::<Vec<_>>());

        assert_high_cardinality_round_trips(&schema, BLOCK_ROWS);
    }

    /// Writes one production-recipe file of two row groups whose 16-byte
    /// `trace_id` values interleave, so each group's min/max spans the other
    /// group's ids, with a Bloom filter on `trace_id` only.
    ///
    /// Returns the encoded file and every written id, group by group.
    ///
    /// # Panics
    ///
    /// Panics when the fixture batch cannot be built or encoded.
    fn write_trace_id_fixture() -> (Bytes, Vec<Vec<[u8; 16]>>) {
        let schema = trace_id_schema();
        // Group `g` holds the ids whose big-endian tail is `4k + 2g`: both
        // groups span nearly the same range and every odd tail is unwritten,
        // so statistics alone retain both groups for any id between them.
        let groups = (0..2_u16)
            .map(|group| {
                (0..100_u16)
                    .map(|k| trace_id_with_tail(4 * k + 2 * group))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let blocks = groups
            .iter()
            .map(|ids| {
                RecordBatch::try_new(
                    Arc::clone(&schema),
                    vec![
                        Arc::new(
                            FixedSizeBinaryArray::try_from_iter(ids.iter())
                                .expect("16-byte trace ids"),
                        ) as ArrayRef,
                        Arc::new(arrow::array::Float64Array::from_iter_values(
                            ids.iter().map(|id| f64::from(id[15])),
                        )) as ArrayRef,
                    ],
                )
                .expect("trace id block")
            })
            .collect::<Vec<_>>();
        let properties = fixture_writer_properties(&["trace_id".to_owned()]);
        let mut sink = Vec::new();
        let mut writer =
            parquet::arrow::ArrowWriter::try_new(&mut sink, Arc::clone(&schema), Some(properties))
                .expect("trace id fixture writer");
        for block in &blocks {
            writer.write(block).expect("trace id fixture write");
            writer.flush().expect("trace id row-group flush");
        }
        writer.close().expect("trace id fixture close");
        (bytes::Bytes::from(sink), groups)
    }

    /// The `trace_id`/`score` schema [`write_trace_id_fixture`] writes.
    fn trace_id_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("trace_id", DataType::FixedSizeBinary(16), false),
            Field::new("score", DataType::Float64, false),
        ]))
    }

    /// Plans reading every `schema` column of the in-memory file `published`
    /// under `predicates`, through the same [`FileReadPlan`] the hot reader
    /// builds after footer discovery, page index included.
    ///
    /// # Panics
    ///
    /// Panics when the footer does not decode or the predicates do not compile
    /// against `schema`.
    fn hot_file_plan(published: &Bytes, schema: &SchemaRef, filter: Option<&Expr>) -> FileReadPlan {
        let reader = ArrowReaderMetadata::load(
            published,
            ArrowReaderOptions::new()
                .with_page_index_policy(parquet::file::metadata::PageIndexPolicy::Optional),
        )
        .expect("valid Parquet footer");
        let filter = filter.map(|filter| physical_filter(filter, schema));
        FileReadPlan::new(
            &reader,
            schema,
            super::super::leaf_paths::LeafPaths::default().projection(schema),
            filter,
            20,
        )
        .expect("per-file read plan")
    }

    /// Builds one 16-byte trace id with a constant prefix and `tail` as its
    /// big-endian final two bytes.
    fn trace_id_with_tail(tail: u16) -> [u8; 16] {
        let mut id = [0x5a_u8; 16];
        id[14..].copy_from_slice(&tail.to_be_bytes());
        id
    }

    /// Binary statistics and page bounds prune by unsigned byte order: a
    /// `Bytes` leaf outside a group's `FIXED_LEN_BYTE_ARRAY` min/max excludes
    /// it, and invalid UTF-8 bytes compare as bytes rather than failing.
    ///
    /// # Panics
    ///
    /// Panics when the fixture footer does not decode or when a `Bytes` leaf
    /// outside both groups' binary bounds retains either group.
    #[test]
    fn binary_min_max_prunes_fixed_len_row_groups() {
        let (published, _) = write_trace_id_fixture();
        let bytes = |id: Vec<u8>| {
            lit(datafusion::scalar::ScalarValue::FixedSizeBinary(
                16,
                Some(id),
            ))
        };
        let above = col("trace_id").eq(bytes(vec![0xff; 16]));
        let plan = |filter: &Expr| {
            hot_file_plan(&published, &trace_id_schema(), Some(filter))
                .select_row_groups(vec![0, 1])
        };
        assert_eq!(plan(&above), Vec::<usize>::new());
        // Strictly below the smallest written id, so neither group can match.
        let below = col("trace_id").lt(bytes(trace_id_with_tail(0).to_vec()));
        assert_eq!(plan(&below), Vec::<usize>::new());
    }

    /// Builds the production hot scan over `files` stored under the
    /// warehouse `root`, read as [`FIXTURE_TENANT`].
    ///
    /// # Panics
    /// Panics when the scan does not plan.
    fn fixture_hot_scan(
        root: &std::path::Path,
        files: Vec<HotFileSource>,
        tenant: DataTenantId,
        schema: &SchemaRef,
        options: &ConfigOptions,
    ) -> Arc<dyn ExecutionPlan> {
        hot_parquet_scan(
            files,
            HotObjects::Warehouse(warehouse_objects(root)),
            tenant,
            schema,
            &Arc::new(options.clone()),
        )
        .expect("hot scan plans")
    }

    /// Compiles the logical `filter` against `schema` into the physical
    /// expression `DataFusion` would plan for it.
    ///
    /// # Panics
    /// Panics when `filter` does not compile against `schema`.
    fn physical_filter(filter: &Expr, schema: &SchemaRef) -> Arc<dyn PhysicalExpr> {
        datafusion::prelude::SessionContext::new()
            .create_physical_expr(
                filter.clone(),
                &datafusion::common::DFSchema::try_from(schema.as_ref().clone())
                    .expect("filter schema"),
            )
            .expect("physical filter")
    }

    /// Returns `scan` with `filter` pushed into it by `DataFusion`'s own
    /// `FilterPushdown`, the way an Oracle session plans a filtered read.
    ///
    /// # Panics
    /// Panics when the filter does not compile or push down.
    fn pushed_filter(
        scan: Arc<dyn ExecutionPlan>,
        filter: &Expr,
        options: &ConfigOptions,
    ) -> Arc<dyn ExecutionPlan> {
        use datafusion::physical_optimizer::PhysicalOptimizerRule as _;

        let predicate = physical_filter(filter, &scan.schema());
        datafusion::physical_optimizer::filter_pushdown::FilterPushdown::new()
            .optimize(
                Arc::new(
                    datafusion::physical_plan::filter::FilterExec::try_new(predicate, scan)
                        .expect("filter plans"),
                ),
                options,
            )
            .expect("filter pushdown")
    }

    /// Describes one fixture object at the warehouse key `name`.
    fn split_file(name: &str, size_bytes: usize) -> HotFileSource {
        HotFileSource {
            location: name.to_owned(),
            size_bytes,
            row_count: None,
            event_time: unusable_event_time(),
        }
    }

    /// A hot scan split across partitions reads every row exactly once.
    ///
    /// One multi-row-group file and one small file are planned, then split by
    /// `DataFusion`'s own file-scan repartitioning at several partition
    /// counts, including more partitions than row groups. The union of all
    /// partitions must equal the source rows.
    ///
    /// # Panics
    /// Panics when a row is missing or read twice at any partition count.
    #[tokio::test]
    async fn hot_parquet_split_partitions_read_every_row_once() {
        let root = tempfile::tempdir().expect("split fixture directory");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let large: Vec<i64> = (0..11).collect();
        let small: Vec<i64> = vec![100, 101];
        let files = [("large.parquet", &large), ("small.parquet", &small)]
            .into_iter()
            .map(|(name, values)| {
                split_file(
                    name,
                    write_split_fixture(&root.path().join(name), &schema, values),
                )
            })
            .collect::<Vec<_>>();
        let mut expected = large.iter().chain(&small).copied().collect::<Vec<_>>();
        expected.sort_unstable();
        for partitions in [1, 2, 3, 4, 16] {
            let mut options = crate::resources::OracleSessionShape::for_test().config_options();
            options.execution.target_partitions = partitions;
            options.optimizer.repartition_file_min_size = 0;
            let scan = fixture_hot_scan(
                root.path(),
                files.clone(),
                *FIXTURE_TENANT,
                &schema,
                &options,
            );
            let scan = scan
                .repartitioned(partitions, &options)
                .expect("repartition")
                .unwrap_or(scan);
            let context = Arc::new(TaskContext::default());
            let mut actual = Vec::new();
            for partition in 0..scan.properties().partitioning.partition_count() {
                let stream = scan
                    .execute(partition, Arc::clone(&context))
                    .expect("split partition stream");
                actual.extend(collect_int64(stream).await);
            }
            actual.sort_unstable();
            assert_eq!(actual, expected, "{partitions} partitions");
        }
    }

    /// A hot object whose footer is missing or names a foreign tenant fails
    /// the scan with the tenant invariant before any row is yielded.
    ///
    /// The first file is written by the fixture tenant but read under another
    /// tenant's authenticated binding; the second carries no footer tenant at
    /// all. Both must refuse with [`BifrostError::QueryTenantInvariant`] and
    /// yield zero rows, so no per-row check is needed downstream.
    ///
    /// # Panics
    ///
    /// Panics when either object yields a row, completes without error, or
    /// fails with anything other than the tenant invariant.
    #[tokio::test]
    async fn hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row() {
        let root = tempfile::tempdir().expect("tenant fixture directory");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let foreign_size =
            write_split_fixture(&root.path().join("foreign.parquet"), &schema, &[1, 2, 3]);
        let missing_path = root.path().join("missing.parquet");
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            File::create(&missing_path).expect("missing-tenant fixture file"),
            Arc::clone(&schema),
            None,
        )
        .expect("missing-tenant fixture writer");
        writer
            .write(
                &RecordBatch::try_new(
                    Arc::clone(&schema),
                    vec![Arc::new(Int64Array::from(vec![4, 5])) as ArrayRef],
                )
                .expect("missing-tenant fixture batch"),
            )
            .expect("missing-tenant fixture write");
        writer.close().expect("missing-tenant fixture close");
        let missing_size = usize::try_from(
            std::fs::metadata(&missing_path)
                .expect("missing-tenant fixture size")
                .len(),
        )
        .expect("fixture size fits usize");
        for (name, size_bytes, tenant) in [
            ("foreign.parquet", foreign_size, DataTenantId::new_v7()),
            ("missing.parquet", missing_size, *FIXTURE_TENANT),
        ] {
            let scan = fixture_hot_scan(
                root.path(),
                vec![split_file(name, size_bytes)],
                tenant,
                &schema,
                &crate::resources::OracleSessionShape::for_test().config_options(),
            );
            let mut stream = scan
                .execute(0, Arc::new(TaskContext::default()))
                .expect("tenant fixture stream");
            let mut rows = 0;
            let mut refusal = None;
            while let Some(batch) = stream.next().await {
                match batch {
                    Ok(batch) => rows += batch.num_rows(),
                    Err(error) => {
                        refusal = Some(error);
                        break;
                    }
                }
            }
            assert_eq!(rows, 0, "an unproven file yields no row");
            let refusal = refusal.expect("an unproven file fails the scan");
            assert!(
                is_tenant_invariant_error(&refusal),
                "an unproven file is a tenant refusal, saw {refusal}"
            );
        }
    }

    /// A recorded row count is inexact scan statistics, so `DataFusion` can
    /// never answer `count(*)` from it without opening the file and proving
    /// its footer tenant.
    ///
    /// # Panics
    ///
    /// Panics when the scan does not plan or reports the count as exact.
    #[test]
    fn hot_scan_row_counts_never_answer_a_query_unopened() {
        let root = tempfile::tempdir().expect("statistics fixture directory");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let size = write_split_fixture(&root.path().join("counted.parquet"), &schema, &[1, 2, 3]);
        let scan = fixture_hot_scan(
            root.path(),
            vec![HotFileSource {
                row_count: Some(3),
                ..split_file("counted.parquet", size)
            }],
            *FIXTURE_TENANT,
            &schema,
            &crate::resources::OracleSessionShape::for_test().config_options(),
        );
        assert_eq!(
            datafusion::physical_plan::StatisticsContext::new()
                .compute(
                    scan.as_ref(),
                    &datafusion::physical_plan::StatisticsArgs::new()
                )
                .expect("scan statistics")
                .num_rows,
            Precision::Inexact(3)
        );
    }

    /// A field of a null struct is null on the hot read path: the required
    /// child's placeholder under a null parent is never stored in Parquet, so
    /// neither statistics pruning nor the decoder row filter may observe it.
    /// `point.x IS NULL` keeps exactly the parentless row and `point.x = 0`
    /// keeps none, whether row groups are pruned by statistics first or every
    /// group is decoded through the row filter alone.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot be written or read, or when either
    /// predicate keeps a row other than the expected ones.
    #[tokio::test]
    async fn struct_fields_of_null_parents_are_null_with_and_without_pruning() {
        let point = Field::new(
            "point",
            DataType::Struct(vec![Field::new("x", DataType::Int64, false)].into()),
            true,
        );
        let schema: SchemaRef = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            point,
        ]));
        let points = arrow::array::StructArray::new(
            vec![Field::new("x", DataType::Int64, false)].into(),
            vec![Arc::new(Int64Array::from(vec![5, 0])) as ArrayRef],
            Some(arrow::buffer::NullBuffer::from(vec![true, false])),
        );
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![1, 2])), Arc::new(points)],
        )
        .expect("fixture batch");
        let root = tempfile::tempdir().expect("struct fixture directory");
        let path = root.path().join("points.parquet");
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            File::create(&path).expect("fixture file"),
            Arc::clone(&schema),
            Some(fixture_writer_properties(&[])),
        )
        .expect("fixture writer");
        writer.write(&batch).expect("fixture write");
        writer.close().expect("fixture close");
        let size = usize::try_from(std::fs::metadata(&path).expect("fixture size").len())
            .expect("fixture size fits usize");

        let leaf = || datafusion::functions::core::expr_fn::get_field(col("point"), "x");
        for (predicate, expected) in [
            (leaf().is_null(), vec![2_i64]),
            (leaf().eq(lit(0_i64)), vec![]),
        ] {
            for prune in [true, false] {
                let mut options = crate::resources::OracleSessionShape::for_test().config_options();
                options.execution.parquet.pruning = prune;
                let scan = pushed_filter(
                    fixture_hot_scan(
                        root.path(),
                        vec![split_file("points.parquet", size)],
                        *FIXTURE_TENANT,
                        &schema,
                        &options,
                    ),
                    &predicate,
                    &options,
                );
                let ids = collect_int64(
                    scan.execute(0, Arc::new(TaskContext::default()))
                        .expect("struct fixture stream"),
                )
                .await;
                assert_eq!(ids, expected, "{predicate} prune={prune}");
            }
        }
    }

    /// One retained plan reads its batch size and bound sources only from the
    /// task it is executed with.
    ///
    /// A physical root is built before admission, so the same hot scan
    /// instance, reset between runs as `DataFusion` requires, is executed
    /// under two task contexts and must split at each task's own batch size. A
    /// whole retained root is then planned on a sentinel planning runtime,
    /// bound once through the session lock, and executed on a distinct
    /// admitted runtime.
    ///
    /// # Panics
    ///
    /// Panics when planning, binding validation, or execution violates any of
    /// those contracts.
    #[tokio::test]
    async fn retained_plan_uses_admitted_task_context_only() {
        let fixture = build_hot_batch_fixture();
        let rows = usize::try_from(HOT_BATCH_FIXTURE_ROWS).expect("fixture rows fit usize");
        let root = fixture.path.parent().expect("fixture directory");
        let name = fixture
            .path
            .file_name()
            .expect("fixture name")
            .to_string_lossy()
            .into_owned();
        let retained = fixture_hot_scan(
            root,
            vec![split_file(&name, fixture.bytes.len())],
            *FIXTURE_TENANT,
            &fixture.schema,
            &crate::resources::OracleSessionShape::for_test().config_options(),
        );
        for (batch_size, expected_batches) in [(8_usize, 8_usize), (8_192, 1)] {
            let task = datafusion::execution::context::SessionContext::new_with_config(
                crate::resources::OracleSessionShape::for_test()
                    .session_config()
                    .with_batch_size(batch_size),
            )
            .task_ctx();
            // A file scan owns one execution's shared work queue, so each run
            // starts from `DataFusion`'s own reset of the same plan instance.
            let batches = datafusion::physical_plan::common::collect(
                Arc::clone(&retained)
                    .reset_state()
                    .expect("reset retained scan")
                    .execute(0, task)
                    .expect("admitted hot stream"),
            )
            .await
            .expect("admitted hot rows");
            assert_eq!(batches.len(), expected_batches, "batch size {batch_size}");
            assert_eq!(
                batches.iter().map(RecordBatch::num_rows).sum::<usize>(),
                rows
            );
        }

        assert_retained_root_binds_once().await;
    }

    /// A closed event-time predicate skips every hot file whose bounds
    /// exclude it before its footer is read, and keeps a file whose bounds
    /// touch the window or are unusable.
    ///
    /// The disjoint file does not exist, so reading its footer would fail the
    /// scan: a clean result proves `DataFusion`'s file pruner skipped it from
    /// the statistics [`HotFileSource::statistics`] attaches.
    ///
    /// # Panics
    /// Panics when the scan fails, returns the wrong rows, or prunes other
    /// than the one disjoint file.
    #[tokio::test]
    async fn event_time_predicate_skips_disjoint_hot_files_before_any_footer_open() {
        use datafusion::scalar::ScalarValue;

        let lower = 1_787_493_600_000_000_i64;
        let upper = 1_787_497_200_000_000_i64;
        let zone: Arc<str> = "UTC".into();
        let schema = Arc::new(Schema::new(vec![Field::new(
            WYRD_EVENT_TIME,
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some(zone.clone())),
            false,
        )]));
        let root = tempfile::tempdir().expect("event-time fixture directory");
        let bounded = |min_micros, max_micros| {
            crate::catalog::event_time::EventTimeStatistics::Bounded {
                min_micros,
                max_micros,
            }
        };
        let written = |name: &str, values: &[i64], event_time| HotFileSource {
            event_time,
            ..split_file(
                name,
                write_split_fixture(&root.path().join(name), &schema, values),
            )
        };
        let files = vec![
            written("overlapping.parquet", &[lower, upper], bounded(lower, upper)),
            // Touches the upper endpoint exactly; closed intervals overlap.
            written("endpoint.parquet", &[upper, upper + 9], bounded(upper, upper + 9)),
            written("unusable.parquet", &[lower + 1, 0], unusable_event_time()),
            HotFileSource {
                event_time: bounded(0, lower - 1),
                ..split_file("disjoint.parquet", 4_096)
            },
        ];
        let options = crate::resources::OracleSessionShape::for_test().config_options();
        let bound = |micros| lit(ScalarValue::TimestampMicrosecond(Some(micros), Some(zone.clone())));
        let filter = col(WYRD_EVENT_TIME)
            .gt_eq(bound(lower))
            .and(col(WYRD_EVENT_TIME).lt_eq(bound(upper)));
        let plan = pushed_filter(
            fixture_hot_scan(root.path(), files, *FIXTURE_TENANT, &schema, &options),
            &filter,
            &options,
        );
        let batches = datafusion::physical_plan::collect(Arc::clone(&plan), Arc::new(TaskContext::default()))
            .await
            .expect("event-time scan reads");
        let mut values = batches
            .iter()
            .flat_map(|batch| {
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<arrow::array::TimestampMicrosecondArray>()
                    .expect("event time stays a microsecond timestamp")
                    .values()
                    .to_vec()
            })
            .collect::<Vec<_>>();
        values.sort_unstable();
        assert_eq!(values, vec![lower, lower + 1, upper, upper]);

        // The filter either sits in the scan or stays directly above it.
        let scan = match plan.downcast_ref::<DataSourceExec>() {
            Some(_) => &plan,
            None => plan.children()[0],
        };
        let files_pruned = match scan
            .metrics()
            .and_then(|metrics| metrics.sum_by_name("files_ranges_pruned_statistics"))
        {
            Some(MetricValue::PruningMetrics {
                pruning_metrics, ..
            }) => pruning_metrics.pruned(),
            _ => 0,
        };
        assert_eq!(files_pruned, 1, "only the disjoint file is skipped");
        // Exact merged bounds would let `min`/`max` be answered unopened,
        // skipping the footer tenant proof.
        let statistics = datafusion::physical_plan::StatisticsContext::new()
            .compute(scan.as_ref(), &datafusion::physical_plan::StatisticsArgs::new())
            .expect("scan statistics");
        assert!(
            !statistics.column_statistics[0].min_value.is_exact().unwrap_or(false),
            "{statistics:?}"
        );
    }

    /// Writes `values` as a Parquet file with two-row row groups.
    fn write_split_fixture(path: &std::path::Path, schema: &SchemaRef, values: &[i64]) -> usize {
        let batch = RecordBatch::try_new(
            Arc::clone(schema),
            vec![
                cast(&Int64Array::from(values.to_vec()), schema.field(0).data_type())
                    .expect("split fixture values cast to the schema"),
            ],
        )
        .expect("split fixture batch");
        let properties = parquet::file::properties::WriterProperties::builder()
            .set_max_row_group_row_count(Some(2))
            .set_key_value_metadata(Some(vec![crate::parquet::footer::tenant_key_value(
                *FIXTURE_TENANT,
            )]))
            .build();
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            File::create(path).expect("split fixture file"),
            Arc::clone(schema),
            Some(properties),
        )
        .expect("split fixture writer");
        writer.write(&batch).expect("split fixture write");
        writer.close().expect("split fixture close");
        usize::try_from(std::fs::metadata(path).expect("split fixture size").len())
            .expect("split fixture size fits usize")
    }

    /// Collects every `Int64` value one stream yields.
    async fn collect_int64(
        mut stream: datafusion::execution::SendableRecordBatchStream,
    ) -> Vec<i64> {
        let mut values = Vec::new();
        while let Some(batch) = stream.next().await {
            let batch = batch.expect("split partition batch");
            values.extend(
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("split values remain Int64")
                    .values()
                    .iter()
                    .copied(),
            );
        }
        values
    }

    /// Owns deterministic Parquet bytes used by the terminal-owner hot test.
    pub(in crate::oracle) struct HotCausalFixture {
        /// Temporary directory retaining the source file for the fixture lifetime.
        pub(in crate::oracle) _directory: tempfile::TempDir,
        /// Source location supplied to the production hot execution plan.
        pub(in crate::oracle) path: std::path::PathBuf,
        /// Arrow schema used to write and decode the source.
        pub(in crate::oracle) schema: SchemaRef,
        /// Serialized Parquet bytes returned by deterministic reader overrides.
        pub(in crate::oracle) bytes: bytes::Bytes,
    }

    /// The projection closure is the scan output, in that order, stably
    /// deduplicated, and never empty.
    ///
    /// Order is part of the contract, not an implementation detail: the
    /// closure is hashed into the assignment-authority digest, so two
    /// components deriving the same set in a different order would produce
    /// different digests and refuse each other's assignments.
    #[test]
    fn projection_closure_order_is_stable() {
        let physical_schema = Schema::new(vec![
            Field::new("service_name", DataType::Utf8, true),
            Field::new("duration_ms", DataType::Int64, true),
            Field::new("wyrd_event_time", DataType::Int64, true),
        ]);
        // Projection-closure order: requested scan output, stably deduplicated.
        let projection = vec![
            physical_schema.index_of("duration_ms").unwrap(),
            physical_schema.index_of("wyrd_event_time").unwrap(),
            physical_schema.index_of("duration_ms").unwrap(),
        ];
        let closure = required_columns_closure(
            &scan_output_names(&physical_schema, Some(&projection)).expect("valid ordinals"),
        );
        assert_eq!(
            closure,
            vec!["duration_ms".to_string(), "wyrd_event_time".to_string()]
        );

        // A `None` projection closes over every table column.
        let full_closure = required_columns_closure(
            &scan_output_names(&physical_schema, None).expect("full projection"),
        );
        assert_eq!(
            full_closure,
            vec![
                "service_name".to_string(),
                "duration_ms".to_string(),
                "wyrd_event_time".to_string(),
            ]
        );

        // A `count(*)` scan requests no column, so its closure reads the
        // always-present event time rather than producing zero-column leaves.
        let count_closure = required_columns_closure(
            &scan_output_names(&physical_schema, Some(&Vec::new())).expect("empty projection"),
        );
        assert_eq!(count_closure, vec!["wyrd_event_time".to_string()]);
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
            Some(fixture_writer_properties(&[])),
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

    /// A warehouse object that vanished after the cut was chosen fails the
    /// scan with the typed stale marker, so the query can be retried against
    /// a fresh snapshot instead of failing as an outage.
    ///
    /// # Panics
    /// Panics when the scan yields a row or fails without the stale marker.
    #[tokio::test]
    async fn vanished_warehouse_object_fails_as_stale() {
        let root = tempfile::tempdir().expect("stale fixture root");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let scan = fixture_hot_scan(
            root.path(),
            vec![split_file("vanished.parquet", 1024)],
            *FIXTURE_TENANT,
            &schema,
            &crate::resources::OracleSessionShape::for_test().config_options(),
        );
        let error = datafusion::physical_plan::common::collect(
            scan.execute(0, Arc::new(TaskContext::default()))
                .expect("stale fixture stream"),
        )
        .await
        .expect_err("a vanished object must not read");
        assert!(is_stale_iceberg_object_error(&error), "{error}");
    }

    /// Asserts admission changed no planning knob of the retained shape.
    ///
    /// Admission supplies a runtime, a memory pool, and the execution-time
    /// sort-merge reservation only, so target partitions, batch size, and
    /// hash-join preference stay exactly as planned.
    ///
    /// # Panics
    ///
    /// Panics when any of those three knobs differs between the two configs.
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
    }

    /// A whole retained root binds once and reads only its admitted task.
    ///
    /// Split out of [`retained_plan_uses_admitted_task_context_only`] to keep
    /// each half readable; it owns the sentinel planning runtime, the empty binding
    /// set, and the planning-versus-execution config equality.
    ///
    /// # Panics
    ///
    /// Panics when planning reserves memory, when a mismatched binding is
    /// accepted, when the admitted config drifts from the retained shape, or
    /// when either pool fails to return to zero.
    async fn assert_retained_root_binds_once() {
        // A whole retained root is planned on a sentinel planning runtime and
        // executed on a distinct admitted one. Planning must reach no row
        // source, and execution must charge the admitted pool alone.
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let (provider, _live) = projection_closure_provider(tenant, None).await;

        let shape = crate::resources::OracleSessionShape::new(
            crate::resources::ORACLE_MIN_TARGET_PARTITIONS,
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

        let bindings = OracleExecutionBindings::new(OracleExecutionBindingInputs {
            grant: crate::oracle::bindings::OracleExecutionGrant::for_test(QueryClass::Interactive),
            live: None,
            degraded: Arc::default(),
        });
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
            0,
            "the unpublished fixture table has no rows"
        );
        assert_eq!(planning_pool.reserved(), 0, "row IO never touches planning");
        assert_eq!(admitted_pool.reserved(), 0, "settlement returns every byte");

        // A freshly planned root with a governed live leaf, executed on a
        // session carrying no lock, refuses at the leaf rather than dialing.
        let (live_provider, _live) =
            projection_closure_provider(tenant, Some(one_live_route(tenant))).await;
        let unbound_session =
            datafusion::execution::context::SessionContext::new_with_config(shape.session_config());
        let unbound_root = live_provider
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
    /// The live batch carries the full three-column physical schema —
    /// `unused_payload`, `duration_ms`, `status_code` — with one
    /// `STATUS_CODE_ERROR` row and one `STATUS_CODE_OK` row, read as `tenant`. Keeping fixture construction here leaves the owning test to
    /// assert only closure behavior.
    ///
    /// # Panics
    /// Panics if the authorization context, fixture batch, or provider cannot
    /// be constructed, since none of those are the behavior under test.
    async fn projection_closure_provider(
        tenant: wyrd_spec::DataTenantId,
        live_routes: Option<crate::oracle::live::LiveTableRoutes>,
    ) -> (OracleTableProvider, RecordBatch) {
        let principal = Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: wyrd_runtime::PrincipalKind::User,
            tenant_id: tenant,
            roles: Vec::new(),
            effective_permissions: PermissionSet::default(),
            credential_id: None,
        };
        let context = AuthorizedQueryContext::try_new(
            principal,
            tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            wyrd_runtime::Permission::bifrost_query_read(),
        )
        .expect("query context");
        let live_schema = Arc::new(Schema::new(vec![
            Field::new("unused_payload", DataType::Utf8, true),
            Field::new("duration_ms", DataType::Int64, true),
            Field::new("status_code", DataType::Utf8, true),
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
            ],
        )
        .expect("live fixture batch");
        let provider = OracleTableProvider::try_new(OracleTableInputs {
            table: projection_fixture_table(),
            objects: fixture_objects(),
            hot_files: Vec::new(),
            context,
            table_name: "vala.traces.spans".to_owned(),
            live: live_routes,
        })
        .await
        .expect("pinned fixture provider");
        (provider, live)
    }

    /// Builds one live Scribe route over the closure fixture's table.
    ///
    /// The route is only planned, never dialed: the closure owner substitutes
    /// the live leaf with the fixture rows before execution.
    ///
    /// # Panics
    ///
    /// Panics if the fixed hour boundary is rejected, which is a fixture bug.
    fn one_live_route(tenant: DataTenantId) -> LiveTableRoutes {
        LiveTableRoutes {
            binding: wyrd_spec::vala::api::TenantTableBinding {
                tenant_id: tenant,
                namespace: "traces".to_owned(),
                table: "spans".to_owned(),
            },
            routes: vec![crate::oracle::live::LiveScribeRoute {
                node_id: wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7()),
                writer_epoch: 1,
                endpoint: "https://scribe.internal".to_owned(),
                time_partition: wyrd_spec::vala::api::TimePartitionWire::new(
                    wyrd_spec::vala::api::TimeGranularityWire::Hour,
                    chrono::DateTime::from_timestamp_micros(1_787_493_600_000_000)
                        .expect("instant"),
                )
                .expect("hour boundary"),
            }],
        }
    }

    /// One leader-owned closure governs every leaf, the placeholder, and the
    /// result.
    ///
    /// This is the production Interactive leader path for
    /// `SELECT duration_ms FROM ... WHERE status_code = 'STATUS_CODE_ERROR'`.
    /// The closure is `[duration_ms, status_code]`: the requested output and
    /// the predicate-only column the leaves prune with. `unused_payload` is
    /// requested by nobody and must not appear in any leaf. Pushdown is
    /// `Inexact`, so the provider returns every row and `DataFusion` filters
    /// above it.
    ///
    /// # Panics
    /// Panics if provider construction, scan planning, or execution violates
    /// the closure contract this owner pins.
    #[tokio::test]
    async fn projected_leaf_union_preserves_predicate_columns() {
        use datafusion::execution::context::SessionContext;
        use datafusion::logical_expr::{col, lit};
        use datafusion::physical_plan::collect;

        let tenant = wyrd_spec::DataTenantId::new_v7();
        let (provider, live) =
            projection_closure_provider(tenant, Some(one_live_route(tenant))).await;

        let public = provider.schema();
        let projection = vec![public.index_of("duration_ms").expect("public duration_ms")];
        let predicate = col("status_code").eq(lit("STATUS_CODE_ERROR"));
        let session = SessionContext::new_with_config(
            crate::resources::OracleSessionShape::for_test().session_config(),
        )
        .state();
        let plan = provider
            .scan(&session, Some(&projection), &[predicate], None)
            .await
            .expect("closure scan plans");

        // The public result is exactly the requested column.
        assert_eq!(column_names(&plan), vec!["duration_ms".to_string()]);

        // The source union sits directly under the output projection.
        let union = Arc::clone(plan.children()[0]);
        assert!(union.downcast_ref::<UnionExec>().is_some());
        let closure = vec!["duration_ms".to_string(), "status_code".to_string()];
        assert_eq!(column_names(&union), closure);

        // Every union child — the published Iceberg leaf and the live Scribe
        // leaf alike — exposes exactly the closure, in closure order.
        let children = union_child_column_names(&union);
        assert_eq!(children.len(), 2, "published and live leaves both planned");
        for child in &children {
            assert_eq!(child, &closure);
        }

        // One ERROR row and one OK row in, both out as `duration_ms`. The
        // live leaf's peer stream is substituted by its rows, projected onto
        // the same closure the leaf planned.
        let plan = plan
            .transform_up(|node| {
                if node
                    .downcast_ref::<super::super::live::LiveScribeExec>()
                    .is_none()
                {
                    return Ok(datafusion::common::tree_node::Transformed::no(node));
                }
                OracleTableProvider::projected_memory_source(
                    std::slice::from_ref(&live),
                    &node.schema(),
                )
                .map(datafusion::common::tree_node::Transformed::yes)
            })
            .expect("live leaf substitutes")
            .data;
        let rows = collect(
            plan,
            crate::oracle::bindings::bind_test_session(
                crate::resources::OracleSessionShape::for_test().session_config(),
                crate::resources::bounded_memory_pool(64 * 1024 * 1024),
                crate::oracle::bindings::OracleExecutionGrant::for_test(QueryClass::Interactive),
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
        assert_eq!(durations, vec![41_i64, 97_i64]);
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

        let mut config = crate::resources::OracleSessionShape::for_test().session_config();
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
