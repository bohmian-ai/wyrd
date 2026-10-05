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
use arrow::datatypes::Field;
use arrow::datatypes::{DataType, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::datasource::physical_plan::FileScanConfig;
use datafusion::datasource::source::DataSourceExec;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::TaskContext;
use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool, MemoryReservation};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_expr::{EquivalenceProperties, PhysicalExpr};
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
    DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, SendableRecordBatchStream,
};
use datafusion::scalar::ScalarValue;
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
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};
use parquet::arrow::async_reader::{AsyncFileReader, ParquetRecordBatchStreamBuilder};
use parquet::basic::{ConvertedType, LogicalType, Type as PhysicalType};
use parquet::bloom_filter::Sbbf;
use parquet::errors::ParquetError;
use parquet::file::metadata::{ParquetMetaData, ParquetMetaDataReader};
use parquet::schema::types::ColumnDescriptor;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::BifrostError;

use crate::scribe::hot_source::StagedSourceLease;
use crate::storage::error_chain_contains_not_found;
use wyrd_spec::vala::api::{QueryClass, WorkerScanStats};
use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

use super::live::LiveScribeExec;
use super::{AuthorizedQueryContext, OracleMemoryResources, OracleTelemetry};

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
    /// Iceberg's dependency-reported requested-range counters, one per
    /// partition reader that started.
    iceberg: Mutex<Vec<ScanMetrics>>,
    /// Requested bytes from governed hot Parquet range reads.
    hot_bytes: AtomicU64,
    /// Whether at least one hot object read was requested.
    hot_available: AtomicBool,
    /// Number of Iceberg files delivered to the readers, counted once per file.
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
    /// Row groups that survived statistics pruning and a Bloom filter then
    /// proved empty. Also counted in `row_groups_pruned`.
    row_groups_pruned_bloom: AtomicU64,
    /// Rows inside retained row groups that page-index selection skipped.
    rows_pruned_page_index: AtomicU64,
}

impl OracleScanMetricsHandle {
    /// Retains one partition reader's dependency counter before its stream
    /// is consumed; terminal bytes sum every retained counter.
    fn add_iceberg_metrics(&self, metrics: ScanMetrics) {
        if let Ok(mut current) = self.iceberg.lock() {
            current.push(metrics);
        }
    }

    /// Records one task delivered to an Iceberg reader.
    ///
    /// A file split across partitions is counted only by the piece that
    /// begins at the file's first byte, so each file counts once.
    fn record_iceberg_task(&self, task: &FileScanTask) {
        if task.start == 0 {
            self.iceberg_files.fetch_add(1, Ordering::Relaxed);
        }
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

    /// Moves `count` row groups that statistics retained to the pruned side,
    /// attributing them to a Bloom filter.
    ///
    /// Called once per opened hot file after its Bloom probes complete, so the
    /// selected/pruned pair still accounts for every inspected row group.
    pub(super) fn record_bloom_pruned(&self, count: u64) {
        if count == 0 {
            return;
        }
        self.row_groups_selected.fetch_sub(count, Ordering::Relaxed);
        self.row_groups_pruned.fetch_add(count, Ordering::Relaxed);
        self.row_groups_pruned_bloom
            .fetch_add(count, Ordering::Relaxed);
    }

    /// Records rows a page-index row selection skipped in retained groups.
    pub(super) fn record_page_pruned_rows(&self, rows: usize) {
        self.rows_pruned_page_index
            .fetch_add(u64::try_from(rows).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    /// Returns terminal dependency counters without substituting metadata sizes.
    fn terminal_values(&self) -> (Option<u64>, u64, u64) {
        let mut total = 0_u64;
        let mut available = false;
        if let Ok(metrics) = self.iceberg.lock()
            && !metrics.is_empty()
        {
            total = metrics.iter().fold(total, |total, metrics| {
                total.saturating_add(metrics.bytes_read())
            });
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

    /// Returns the terminal per-mechanism pruning counters of both readers.
    ///
    /// Hot counters are this handle's own. Each Iceberg partition reader
    /// reports the row groups it considered and attributes every excluded one
    /// to exactly one of statistics or Bloom, so its retained groups are the
    /// considered groups minus both.
    fn terminal_pruning(&self) -> PruningEvidence {
        let mut evidence = PruningEvidence {
            row_groups_scanned: self.row_groups_selected.load(Ordering::Acquire),
            row_groups_pruned: self.row_groups_pruned.load(Ordering::Acquire),
            row_groups_pruned_bloom: self.row_groups_pruned_bloom.load(Ordering::Acquire),
            rows_pruned_page_index: self.rows_pruned_page_index.load(Ordering::Acquire),
        };
        if let Ok(readers) = self.iceberg.lock() {
            for reader in readers.iter() {
                let statistics = reader.row_groups_pruned_by_statistics();
                let bloom = reader.row_groups_pruned_by_bloom_filter();
                evidence.add(PruningEvidence {
                    row_groups_scanned: reader
                        .row_groups_considered()
                        .saturating_sub(statistics.saturating_add(bloom)),
                    row_groups_pruned: statistics.saturating_add(bloom),
                    row_groups_pruned_bloom: bloom,
                    rows_pruned_page_index: reader.rows_pruned_by_page_index(),
                });
            }
        }
        evidence
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

/// Records one pinned Iceberg task and returns it without altering its delete
/// metadata, schema, predicate, or byte range.
fn retain_iceberg_task(task: FileScanTask, metrics: &Arc<OracleScanMetricsHandle>) -> FileScanTask {
    metrics.record_iceberg_task(&task);
    task
}

/// Returns the byte ranges one partition reads when `sizes` are laid end to end.
///
/// The concatenated bytes are cut into `partitions` equal contiguous ranges and
/// each returned `(file index, range)` is the non-empty intersection of this
/// partition's range with one file. The ranges of all partitions tile every
/// file exactly, so a row group assigned by its midpoint (see
/// [`row_groups_in_byte_range`]) is read by exactly one partition. This is the
/// split Iceberg's own reader honors through `FileScanTask::start`/`length`.
fn partition_byte_ranges(
    sizes: &[u64],
    partition: usize,
    partitions: usize,
) -> Vec<(usize, Range<u64>)> {
    let total: u128 = sizes.iter().map(|size| u128::from(*size)).sum();
    let partitions = u128::try_from(partitions.max(1)).unwrap_or(u128::MAX);
    let partition = u128::try_from(partition).unwrap_or(u128::MAX);
    let bound = |index: u128| total.saturating_mul(index) / partitions;
    let (low, high) = (bound(partition), bound(partition.saturating_add(1)));
    let mut ranges = Vec::new();
    let mut offset = 0_u128;
    for (index, size) in sizes.iter().enumerate() {
        let end = offset + u128::from(*size);
        let (start, stop) = (low.max(offset), high.min(end));
        if start < stop {
            // Both bounds lie within this file, so they fit its u64 size.
            let local = |at: u128| u64::try_from(at - offset).unwrap_or(u64::MAX);
            ranges.push((index, local(start)..local(stop)));
        }
        offset = end;
    }
    ranges
}

/// Returns the row groups of `metadata` whose byte midpoint lies in `range`.
///
/// Row groups are laid out after the four-byte magic header in index order,
/// exactly as Iceberg's reader locates them for a split `FileScanTask`, so a
/// hot file and a published file split identically.
fn row_groups_in_byte_range(
    metadata: &parquet::file::metadata::ParquetMetaData,
    range: &Range<u64>,
) -> Vec<usize> {
    let mut offset = 4_u64;
    let mut owned = Vec::new();
    for (index, row_group) in metadata.row_groups().iter().enumerate() {
        let size = u64::try_from(row_group.compressed_size()).unwrap_or_default();
        if range.contains(&(offset + size / 2)) {
            owned.push(index);
        }
        offset += size;
    }
    owned
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
            self.files_scanned = self.files_scanned.saturating_add(files);
            self.partitions_scanned = self.partitions_scanned.saturating_add(partitions);
            self.pruning.add(handle.terminal_pruning());
            if let Some(bytes) = bytes {
                available = true;
                total = total.saturating_add(bytes);
            }
        }
        for handle in &self.scan_handles {
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
        if let Some(source) = plan.downcast_ref::<LiveScribeExec>() {
            stats.remote_handles.push(Arc::clone(source.scan_metrics()));
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
/// Row groups a follower leaf skipped by statistics or Bloom pruning.
pub(crate) const WYRD_ROW_GROUPS_PRUNED_METRIC: &str = "wyrd_row_groups_pruned";
/// Row groups a follower leaf's Bloom filters skipped after statistics kept them.
pub(crate) const WYRD_ROW_GROUPS_PRUNED_BLOOM_METRIC: &str = "wyrd_row_groups_pruned_bloom";
/// Rows a follower leaf's page-index selection skipped in retained row groups.
pub(crate) const WYRD_ROWS_PRUNED_PAGE_INDEX_METRIC: &str = "wyrd_rows_pruned_page_index";

/// Publishes one resolved follower leaf's terminal scan evidence as metrics.
///
/// The evidence is read through the same collector the leader uses on its own
/// plan, so an Analytical follower reports exactly what an Interactive leader
/// would have reported for the identical scan. Bytes are omitted rather than
/// zeroed when the source never reported them, preserving absent-versus-zero.
pub(crate) fn analytical_leaf_scan_metrics(plan: &dyn ExecutionPlan) -> MetricsSet {
    let mut stats = OracleQueryScanStats::from_plan(plan);
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
    publish(
        WYRD_ROW_GROUPS_SCANNED_METRIC,
        stats.pruning.row_groups_scanned,
    );
    publish(
        WYRD_ROW_GROUPS_PRUNED_METRIC,
        stats.pruning.row_groups_pruned,
    );
    publish(
        WYRD_ROW_GROUPS_PRUNED_BLOOM_METRIC,
        stats.pruning.row_groups_pruned_bloom,
    );
    publish(
        WYRD_ROWS_PRUNED_PAGE_INDEX_METRIC,
        stats.pruning.rows_pruned_page_index,
    );
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
        totals.row_groups_pruned_bloom = totals.row_groups_pruned_bloom.saturating_add(
            sum_named_count(&metrics, WYRD_ROW_GROUPS_PRUNED_BLOOM_METRIC).unwrap_or(0),
        );
        totals.rows_pruned_page_index = totals.rows_pruned_page_index.saturating_add(
            sum_named_count(&metrics, WYRD_ROWS_PRUNED_PAGE_INDEX_METRIC).unwrap_or(0),
        );
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
    /// Alternate spellings of the assigned files, keyed by the form a manifest
    /// may carry and valued by the storage-qualified location it denotes.
    ///
    /// A manifest records whichever path its writer wrote — Forge's promotion
    /// commit records the table-relative object key that `vala.file_list` also
    /// carries — while an assignment is signed in storage-qualified form. The
    /// two are the same object, so drift detection has to compare them in one
    /// spelling or it refuses every promoted file.
    assigned_aliases: std::collections::BTreeMap<String, String>,
    /// Properties copied from the pinned source plan, at this scan's
    /// partition count.
    properties: Arc<PlanProperties>,
    /// Shared terminal metric owner retained by query telemetry.
    metrics: Arc<OracleScanMetricsHandle>,
    /// Governed footer cache the reader loads data-file metadata through;
    /// `None` leaves footer decoding to the Iceberg reader.
    footers: Option<PublishedFooters>,
    /// Pinned file tasks, planned once and shared by every partition.
    planned: Arc<tokio::sync::OnceCell<Vec<FileScanTask>>>,
    /// Iceberg reader built once and cloned by every partition, because
    /// building one probes the host's CPU limits.
    reader: Arc<tokio::sync::OnceCell<iceberg::arrow::ArrowReader>>,
}

/// What a pinned Iceberg scan needs to load data-file footers through the
/// node's one governed metadata cache.
///
/// Built at planning time, before admission, so it retains only the
/// planning-time [`HotParquetPlan`]; the admitted pool, class, and deadline
/// are resolved from the executing task, exactly as a hot leaf resolves them.
#[derive(Clone)]
pub(super) struct PublishedFooters {
    /// The node's one storage owner and its decoded-metadata cache.
    storage: Arc<crate::storage::BifrostStorage>,
    /// Authenticated tenant owning every scanned object.
    tenant_id: wyrd_spec::DataTenantId,
    /// Canonical logical table name scoping every key.
    table: String,
    /// Planning-time governance for the footer ranges a cache miss reads.
    governance: HotParquetPlan,
}

impl PublishedFooters {
    /// Captures the storage owner, tenant, table, and governance mode one
    /// pinned Iceberg scan loads its footers under.
    pub(super) fn new(
        storage: Arc<crate::storage::BifrostStorage>,
        tenant_id: wyrd_spec::DataTenantId,
        table: String,
        governance: HotParquetPlan,
    ) -> Self {
        Self {
            storage,
            tenant_id,
            table,
            governance,
        }
    }

    /// Resolves one executing scan's footer loader.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error when the admitted governance
    /// cannot be resolved from `task`: absent bindings, a cancelled query, or
    /// an elapsed deadline.
    fn loader(
        &self,
        file_io: FileIO,
        task: &TaskContext,
    ) -> DataFusionResult<Arc<PublishedFooterLoader>> {
        let cancel = tokio_util::sync::CancellationToken::new();
        Ok(Arc::new(PublishedFooterLoader {
            storage: Arc::clone(&self.storage),
            file_io,
            tenant_id: self.tenant_id,
            table: self.table.clone(),
            governance: self.governance.resolve(task)?,
            retained: Arc::new(Mutex::new(Vec::new())),
            _cancel_on_drop: cancel.clone().drop_guard(),
            cancel,
        }))
    }
}

/// Serves one pinned Iceberg scan's data-file footers from the node's one
/// governed metadata cache.
///
/// Without it the Iceberg reader decodes every data file's footer and page
/// index again on every query. Through it, one decode per immutable object is
/// shared across queries, concurrent misses single-flight, and retained
/// footers are charged to the one Oracle memory root. Every load's
/// [`crate::storage::RetainedMetadata`] is held for the loader's life, which is
/// the scan stream's life, so evicting an entry never releases the charge for
/// bytes this scan still reads. Dropping the loader cancels any decode it still
/// has outstanding.
struct PublishedFooterLoader {
    /// The node's one storage owner and its decoded-metadata cache.
    storage: Arc<crate::storage::BifrostStorage>,
    /// Pinned table reader, used only when a footer is not cached.
    file_io: FileIO,
    /// Authenticated tenant owning every scanned object.
    tenant_id: wyrd_spec::DataTenantId,
    /// Canonical logical table name scoping every key.
    table: String,
    /// Admitted governance charged for the footer ranges a miss reads.
    governance: HotParquetGovernance,
    /// Every footer this scan loaded, retained until the scan ends.
    retained: Arc<Mutex<Vec<crate::storage::RetainedMetadata>>>,
    /// Cancels outstanding decodes when the loader is dropped.
    _cancel_on_drop: tokio_util::sync::DropGuard,
    /// Token every decode this loader starts is bound to.
    cancel: tokio_util::sync::CancellationToken,
}

impl iceberg::arrow::ParquetMetadataLoader for PublishedFooterLoader {
    /// Loads one data file's footer through the governed cache and proves its
    /// tenant.
    ///
    /// A miss reads the footer and page index through the same governed,
    /// range-reserving reader the hot tier uses; its range observations go to
    /// a scratch handle because they are not hot-tier reads. Every load, hit
    /// or miss, compares the footer tenant with the authenticated tenant before
    /// the reader sees the metadata.
    ///
    /// # Errors
    ///
    /// The returned future fails with the storage owner's closed error as the
    /// Iceberg error's source, so a vanished object keeps its not-found cause
    /// for stale-object classification, and with
    /// [`BifrostError::QueryTenantInvariant`] as the source when the footer
    /// tenant is missing or foreign.
    fn load(
        &self,
        path: &str,
        size: u64,
    ) -> BoxFuture<'static, iceberg::Result<Arc<ParquetMetaData>>> {
        let key = crate::storage::ObjectMetadataKey::published(
            self.tenant_id,
            self.table.clone(),
            path.to_owned(),
            size,
        );
        let build_reader = {
            let file_io = self.file_io.clone();
            let location = path.to_owned();
            let governance = self.governance.clone();
            move || {
                IcebergParquetReader::new(
                    HotObjectSource::Pending {
                        file_io: file_io.clone(),
                        location: location.clone(),
                    },
                    size,
                    governance.clone(),
                    Arc::new(OracleScanMetricsHandle::default()),
                )
            }
        };
        let storage = Arc::clone(&self.storage);
        let cancel = self.cancel.clone();
        let retained = Arc::clone(&self.retained);
        let tenant = self.tenant_id;
        async move {
            let loaded = storage
                .object_metadata(key, build_reader, storage.metadata_deadline(), cancel)
                .await
                .map_err(|error| {
                    iceberg::Error::new(
                        iceberg::ErrorKind::Unexpected,
                        "governed data-file footer load failed",
                    )
                    .with_source((*error).clone())
                })?;
            let metadata = Arc::clone(loaded.metadata());
            // Checked before the metadata reaches the reader, so a foreign or
            // unproven file yields no row group, page, or row.
            verify_scanned_footer_tenant(&metadata, tenant).map_err(|error| {
                iceberg::Error::new(
                    iceberg::ErrorKind::DataInvalid,
                    "scanned data-file footer violates the tenant invariant",
                )
                .with_source(error)
            })?;
            retained
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(loaded);
            Ok(metadata)
        }
        .boxed()
    }
}

/// Routes every pinned Iceberg scan in `plan` through `footers`.
///
/// Used where a plan's Iceberg leaves were built by code that cannot see the
/// storage owner, such as a follower's catalog scan.
///
/// # Errors
///
/// Returns the `DataFusion` error of a failed plan rewrite.
pub(super) fn route_published_footers(
    plan: Arc<dyn ExecutionPlan>,
    footers: &PublishedFooters,
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    use datafusion::common::tree_node::{Transformed, TreeNode as _};
    plan.transform_up(|node| {
        let Some(exec) = node.downcast_ref::<OracleIcebergScanExec>() else {
            return Ok(Transformed::no(node));
        };
        Ok(Transformed::yes(Arc::new(
            exec.clone().with_footers(footers.clone()),
        )))
    })
    .map(|transformed| transformed.data)
}

/// Domain marker for one authenticated Iceberg object lost after cut selection.
#[derive(Debug, thiserror::Error)]
#[error("authenticated Iceberg object disappeared after cut selection")]
struct OracleIcebergStaleObject {
    /// Exact typed storage cause retained for diagnostics and downcast proof.
    #[source]
    source: Box<dyn std::error::Error + Send + Sync>,
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
            assigned_aliases: std::collections::BTreeMap::new(),
            properties: Arc::clone(plan.properties()),
            metrics: Arc::new(OracleScanMetricsHandle::default()),
            footers: None,
            planned: Arc::default(),
            reader: Arc::default(),
        })
    }

    /// Reads this scan across `partitions` byte-range partitions.
    ///
    /// Callers pass their session's target partitions, the way a `DataFusion`
    /// listing scan splits its file groups. See [`partition_byte_ranges`].
    pub(crate) fn with_partitions(mut self, partitions: usize) -> Self {
        self.properties = Arc::new(
            self.properties
                .as_ref()
                .clone()
                .with_partitioning(Partitioning::UnknownPartitioning(partitions.max(1))),
        );
        self
    }

    /// Loads this scan's data-file footers through the governed cache.
    pub(super) fn with_footers(mut self, footers: PublishedFooters) -> Self {
        self.footers = Some(footers);
        self
    }

    /// Restricts this pinned scan to one exact authenticated follower assignment.
    ///
    /// `assigned_aliases` maps every alternate spelling of an assigned path —
    /// the canonical tenant-relative key and the table-relative suffix — onto
    /// the storage-qualified location `assigned_files` names, so a manifest
    /// that records one spelling still resolves to the file the leader signed.
    pub(crate) fn with_assigned_files(
        mut self,
        assigned_files: std::collections::BTreeSet<String>,
        assigned_aliases: std::collections::BTreeMap<String, String>,
    ) -> Self {
        self.assigned_files = Some(assigned_files);
        self.assigned_aliases = assigned_aliases;
        // A restricted scan plans a different task set, never the one an
        // unrestricted clone may already have planned.
        self.planned = Arc::default();
        self
    }

    /// Resolves one planned manifest path to the storage-qualified location an
    /// assignment would name it by.
    ///
    /// A path that is already storage-qualified is its own location; any other
    /// spelling is resolved through the alias map the follower built from the
    /// signed assignment.
    fn assignment_location(&self, planned: &str) -> String {
        self.assigned_aliases
            .get(planned)
            .cloned()
            .unwrap_or_else(|| planned.to_owned())
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

    /// Rebuilds the exact pinned Iceberg scan and plans its file tasks.
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
    /// Returns a typed `DataFusion` error when scan planning, task planning, or
    /// predicate binding fails, and a plan error when an assigned file is
    /// absent from the pinned snapshot.
    async fn plan_tasks(&self) -> DataFusionResult<Vec<FileScanTask>> {
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
        if let Some(assigned) = &self.assigned_files {
            let planned = tasks
                .iter()
                .map(|task| self.assignment_location(&task.data_file_path))
                .collect::<std::collections::BTreeSet<_>>();
            if !assigned.is_subset(&planned) {
                // Counts and the resolved shape only: which objects a tenant
                // owns must not reach a log line any more than it may reach the
                // error that crosses the dispatch boundary. The counts are what
                // separate "the snapshot lost files the leader signed" from
                // "the two sides spelled the same files differently", which is
                // the distinction an operator needs and the one this refusal
                // was previously unable to report at all.
                metrics::counter!(
                    "bifrost_oracle_security_events_total",
                    "event_class" => "assignment_drift"
                )
                .increment(1);
                tracing::warn!(
                    assigned = assigned.len(),
                    planned = planned.len(),
                    unresolved = assigned.difference(&planned).count(),
                    aliased = self.assigned_aliases.len(),
                    "Oracle follower assignment differs from the pinned snapshot's planned files"
                );
                return Err(DataFusionError::Plan(
                    "authenticated Oracle assignment differs from planned files".to_owned(),
                ));
            }
            let row_filter = self.assignment_row_filter()?;
            return Ok(tasks
                .into_iter()
                .filter(|task| assigned.contains(&self.assignment_location(&task.data_file_path)))
                .map(|mut task| {
                    task.predicate.clone_from(&row_filter);
                    task
                })
                .collect());
        }
        Ok(tasks)
    }

    /// Starts one partition's Iceberg reader over its byte ranges.
    ///
    /// Every partition awaits the same planned task list, then reads only the
    /// byte ranges [`partition_byte_ranges`] assigns it; the reader keeps each
    /// row group whose midpoint lies in a task's range. `footers` is the
    /// resolved governed footer loader the reader loads, and tenant-proves,
    /// every data file's metadata through. The first partition to start
    /// builds the one reader every partition clones, so its loader serves the
    /// whole query; every partition of one execution resolves the same
    /// admitted governance, so any partition's loader is equivalent.
    ///
    /// # Errors
    ///
    /// Returns a typed `DataFusion` error when task planning, reader
    /// construction, or object-store reads fail, and
    /// [`BifrostError::QueryTenantInvariant`] when a planned file carries key
    /// metadata, because the reader opens such a file without the loader.
    async fn start_stream(
        &self,
        partition: usize,
        footers: Arc<PublishedFooterLoader>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        let planned = self.planned.get_or_try_init(|| self.plan_tasks()).await?;
        // Bifrost writes no encrypted data file. One that claims key metadata
        // would bypass the tenant-proving loader, so it is refused outright.
        if planned.iter().any(|task| task.key_metadata.is_some()) {
            return Err(DataFusionError::External(Box::new(
                BifrostError::QueryTenantInvariant,
            )));
        }
        let sizes = planned.iter().map(|task| task.length).collect::<Vec<_>>();
        let partitions = self.properties.partitioning.partition_count();
        let tasks = partition_byte_ranges(&sizes, partition, partitions)
            .into_iter()
            .map(|(index, range)| {
                let mut task = planned[index].clone();
                task.start += range.start;
                task.length = range.end - range.start;
                Ok(task)
            })
            .collect::<Vec<_>>();
        let tasks = futures_util::stream::iter(tasks);
        // Row selection turns each task predicate into a page-index selection,
        // so a point lookup decodes the matching pages instead of every page
        // of each surviving row group. The reader defaults it off. The pinned
        // evaluator decodes every physical type it can bound exactly and keeps
        // pages for any other, so it is enabled for every predicate.
        let reader = self
            .reader
            .get_or_init(|| async {
                self.table
                    .reader_builder()
                    .with_row_selection_enabled(true)
                    .with_parquet_metadata_loader(footers)
                    .build()
            })
            .await
            .clone();
        let metrics = reader
            .read(Box::pin(tasks.map_ok({
                let metrics = Arc::clone(&self.metrics);
                move |task| retain_iceberg_task(task, &metrics)
            })))
            .map_err(iceberg_datafusion_error)?;
        self.metrics.add_iceberg_metrics(metrics.metrics().clone());
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

    /// Starts one partition's Iceberg reader stream.
    ///
    /// # Errors
    ///
    /// Returns a typed `DataFusion` error when scan planning or storage setup
    /// fails, when the partition is out of range, or when footer governance
    /// cannot be resolved from the admitted task.
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        if partition >= self.properties.partitioning.partition_count() {
            return Err(DataFusionError::Execution(format!(
                "OracleIcebergScanExec has no partition {partition}"
            )));
        }
        // Resolved here, never at planning time: the admitted pool, class,
        // cancellation, and deadline all arrive with this task. The loader is
        // mandatory because it is what proves every opened footer's tenant; a
        // scan without it would read files no check has seen.
        let footers = self
            .footers
            .as_ref()
            .ok_or_else(|| DataFusionError::External(Box::new(BifrostError::QueryTenantInvariant)))?
            .loader(self.table.file_io().clone(), context.as_ref())?;
        let source = self.clone();
        let future = async move { source.start_stream(partition, footers).await };
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
/// every unchecksummed object collide on one entry. The key's tenant is the
/// authenticated binding's, never the row's, because the scan proves each
/// object's footer against it.
///
/// # Errors
/// Returns `BifrostError::MetadataMismatch` when the row carries no decodable
/// nonzero SHA-256.
pub(super) fn hot_metadata_key(
    tenant: DataTenantId,
    file: &vala_sql::row_types::file_list::HotFileRow,
    size_bytes: usize,
) -> Result<crate::storage::ObjectMetadataKey, BifrostError> {
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
    Ok(crate::storage::ObjectMetadataKey::new(
        tenant,
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
    pub(crate) metadata_key: crate::storage::ObjectMetadataKey,
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
    /// Authenticated request context whose tenant every scanned footer must name.
    pub(crate) context: AuthorizedQueryContext,
    /// Canonical table name used in security diagnostics.
    pub(crate) table_name: String,
    /// Frozen remote owner of this cut's persisted sources, when the cut chose
    /// one. `None` keeps every persisted leaf leader-local.
    pub(crate) remote: Option<OracleRemoteSource>,
    /// Scribe live routes discovered for this table before planning.
    pub(crate) live: Option<super::live::LiveTableRoutes>,
}

/// Complete physical provider for one authenticated table visibility cut.
pub(crate) struct OracleTableProvider {
    /// Pinned Iceberg provider built from immutable table metadata.
    iceberg: IcebergStaticTableProvider,
    /// Pinned hot files absent from the selected Iceberg manifest.
    hot_files: Vec<HotFileSource>,
    /// File reader inherited from the pinned Iceberg table.
    file_io: FileIO,
    /// The node's one storage owner, handed to every hot leaf this builds.
    storage: Arc<crate::storage::BifrostStorage>,
    /// Full physical schema, which is also the caller-visible schema.
    physical_schema: SchemaRef,
    /// Authenticated request context whose tenant every scanned footer must name.
    context: AuthorizedQueryContext,
    /// Canonical table name included in scrubbed security diagnostics.
    table: String,
    /// Frozen remote owner of this cut's persisted sources, when the cut chose
    /// one. `None` keeps every persisted leaf leader-local.
    remote: Option<OracleRemoteSource>,
    /// Scribe live routes discovered for this table before planning.
    live: Option<super::live::LiveTableRoutes>,
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
            storage,
            hot_files,
            context,
            table_name,
            remote,
            live,
        } = inputs;
        let file_io = table.file_io().clone();
        let iceberg = IcebergStaticTableProvider::try_new_from_table(table)
            .await
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        let physical_schema = iceberg.schema();
        Ok(Self {
            iceberg,
            hot_files,
            file_io,
            storage,
            physical_schema,
            context,
            table: table_name,
            remote,
            live,
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
        // Like a `DataFusion` listing scan, each persisted leaf reads across
        // the session's target partitions.
        let partitions = state.config_options().execution.target_partitions;
        let mut inputs: Vec<Arc<dyn ExecutionPlan>> = Vec::new();
        let published_index = inputs.len();
        {
            // `limit` is forwarded only as a per-leaf upper bound; DataFusion's
            // own global limit above this provider remains authoritative. The
            // closure's physical indices are what keep unrequested columns out
            // of the Iceberg reader itself rather than merely out of the result.
            let published = self
                .iceberg
                .scan(
                    state,
                    Some(&scan_projection.physical_indices),
                    supported_filters,
                    limit,
                )
                .await?;
            let published: Arc<dyn ExecutionPlan> = Arc::new(
                OracleIcebergScanExec::from_plan(published.as_ref())?
                    .with_partitions(partitions)
                    .with_footers(PublishedFooters::new(
                        Arc::clone(&self.storage),
                        self.context.data_tenant_id,
                        self.table.clone(),
                        HotParquetPlan::Leader,
                    )),
            );
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
            inputs.push(Arc::new(
                HotParquetExec::new(
                    self.retained_hot_files(supported_predicates),
                    self.file_io.clone(),
                    Arc::clone(&self.storage),
                    Arc::clone(&required_schema),
                    HotParquetPlan::Leader,
                    Arc::new(OracleScanMetricsHandle::default()),
                    supported_predicates.to_vec(),
                )
                .with_partitions(partitions),
            ));
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
            .filter(|file| interval.retains(file.event_time))
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
            if leaves.iter().all(|leaf| {
                physical_schema
                    .column_with_name(leaf.column())
                    .is_some_and(|(_, field)| literal_fits_column(leaf, field.data_type()))
            }) {
                FilterClassification::Supported(leaves)
            } else {
                FilterClassification::Unsupported
            }
        }
        FilterClassification::Unsupported => FilterClassification::Unsupported,
    }
}

/// Reports whether `leaf`'s literal can be materialized against a column of
/// `data_type` without a cast.
///
/// Only a binary literal is constrained here: it must compare against a
/// binary-family column, and a fixed-size column additionally requires the
/// literal's exact width, because a `FixedSizeBinary` scalar of any other width
/// is not a value of that column. Every other literal keeps the classifier's
/// existing contract, where the residual filter decides typed comparisons.
fn literal_fits_column(leaf: &ScanPredicate, data_type: &DataType) -> bool {
    match (leaf.literal(), data_type) {
        (Some(ScanLiteral::Bytes(value)), DataType::FixedSizeBinary(width)) => {
            usize::try_from(*width).is_ok_and(|width| width == value.len())
        }
        (
            Some(ScanLiteral::Bytes(_)),
            DataType::Binary | DataType::LargeBinary | DataType::BinaryView,
        ) => true,
        (Some(ScanLiteral::Bytes(_)), _) => false,
        _ => true,
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

    /// Builds the exact `Iceberg + hot + live` disjoint source.
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
        // One live leaf over the routes safe event-time pruning keeps. It
        // captures routes and the signed closure only; fragments open at
        // execution, after admission and the audited read decision.
        if let Some(live) = self.live.as_ref() {
            let routes = live.select(&supported_predicates);
            if !routes.is_empty() {
                inputs.push(Arc::new(super::live::LiveScribeExec::new(
                    super::live::LiveTableRoutes {
                        routes,
                        ..live.clone()
                    },
                    super::assignment_schema_fingerprint(&self.physical_schema),
                    scan_projection.required_columns.clone(),
                    supported_predicates.clone(),
                    Arc::clone(&required_schema),
                    state.config().target_partitions(),
                )));
            }
        }
        let union = UnionExec::try_new(inputs)?;
        // Provider-local filter over the closed predicates. This is a real
        // pruning aid, not a substitute for correctness: pushdown is reported
        // `Inexact`, so DataFusion still applies its own residual copy above
        // this provider regardless of what happens here.
        let physical_predicates = supported_predicates
            .iter()
            .map(|predicate| scan_predicate_physical_expr(predicate, &union.schema()))
            .collect::<DataFusionResult<Vec<_>>>()?;
        let filtered: Arc<dyn ExecutionPlan> =
            match conjoin_physical_predicates(physical_predicates) {
                Some(predicate) => Arc::new(
                    datafusion::physical_plan::filter::FilterExec::try_new(predicate, union)?,
                ),
                None => union,
            };
        // Resolved by name against the filter's actual output, which is the
        // closure — never against the full-schema ordinals the caller
        // supplied.
        project_plan_by_name(filtered, &scan_projection.output_names)
    }
}

/// Bounded lazy source for pinned hot sealed Parquet files.
#[cfg(test)]
type HotReadOverride = Arc<
    dyn Fn(&str, Range<u64>) -> Pin<Box<dyn Future<Output = DataFusionResult<bytes::Bytes>> + Send>>
        + Send
        + Sync,
>;

/// Owner-backed range bytes whose Oracle reservation follows every clone and slice.
struct AccountedRangeOwner {
    /// Exact bytes returned by the storage range read.
    bytes: bytes::Bytes,
    /// Oracle child and parent capacity retained through the final byte owner.
    reservation: crate::resources::OracleQueryMemoryReservation,
}

impl AsRef<[u8]> for AccountedRangeOwner {
    /// Borrows the immutable storage bytes without copying or changing ownership.
    fn as_ref(&self) -> &[u8] {
        debug_assert_eq!(self.bytes.len(), self.reservation.bytes());
        self.bytes.as_ref()
    }
}

impl Drop for AccountedRangeOwner {
    /// Asserts the coupled reservation still matches the bytes it retains.
    fn drop(&mut self) {
        if self.reservation.bytes() != self.bytes.len() {
            self.reservation.poison();
            tracing::error!("Oracle hot-range reservation no longer matches its bytes");
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
        _reservation: crate::resources::OracleQueryMemoryReservation,
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
            (Self::Leader { .. }, HotRangeReservation::Leader(reservation)) => {
                Ok(bytes::Bytes::from_owner(AccountedRangeOwner {
                    bytes,
                    reservation,
                }))
            }
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
                ..
            } => {
                let reservation = memory
                    .resources
                    .try_split_query_memory(memory_pool, "oracle-hot-decoded-batch", bytes)
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                Ok(HotDecodedReservation::Leader {
                    _reservation: reservation,
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
                    .map_err(|error| ParquetError::External(Box::new(error)))?
            };
            #[cfg(not(test))]
            // Kept typed so the storage owner can tell a vanished object from
            // an outage when this read backs a governed footer decode.
            let bytes = self
                .reader
                .read(range)
                .await
                .map_err(|error| ParquetError::External(Box::new(error)))?;
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
    /// Lease keeping Scribe staged runs on disk while any partition reads them.
    ///
    /// `None` for durable hot objects, which need no lease. Each partition
    /// stream holds a clone, so the lease is released once the plan and every
    /// stream it produced are dropped.
    staged_lease: Option<Arc<StagedSourceLease>>,
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
    /// Creates a hot-file leaf from validated manifests, single-partition
    /// until [`Self::with_partitions`] splits it.
    ///
    /// Callers must have already authenticated the assignment, validated every
    /// object identity and size, and chosen the governance mode that matches
    /// their role; this constructor performs no IO and no authorization.
    /// Each file's metadata key must carry the authenticated binding's tenant:
    /// every opened object's footer is compared with it before a row is
    /// decoded.
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
            staged_lease: None,
            #[cfg(test)]
            reader_override: None,
            properties: plan_properties(Arc::clone(&schema)),
            schema,
        }
    }

    /// Reads this leaf across `partitions` byte-range partitions.
    ///
    /// Callers pass their session's target partitions; see
    /// [`partition_byte_ranges`].
    pub(super) fn with_partitions(mut self, partitions: usize) -> Self {
        self.properties =
            plan_properties_with_partitions(Arc::clone(&self.schema), partitions.max(1));
        self
    }

    /// Holds `lease` for as long as this leaf or any of its streams lives.
    ///
    /// The Scribe follower reads its own staged runs through this leaf; the
    /// lease is what keeps publication from deleting a run mid-read.
    pub(super) fn with_staged_lease(mut self, lease: StagedSourceLease) -> Self {
        self.staged_lease = Some(Arc::new(lease));
        self
    }

    /// Returns the files and byte ranges `partition` owns, in file order.
    ///
    /// Every file's bytes are tiled across this leaf's partition count by
    /// [`partition_byte_ranges`], so the union over partitions covers each byte
    /// exactly once. A size that does not fit `u64` counts as empty.
    fn partition_pieces(&self, partition: usize) -> Vec<(HotFileSource, Range<u64>)> {
        let sizes = self
            .files
            .iter()
            .map(|file| u64::try_from(file.size_bytes).unwrap_or_default())
            .collect::<Vec<_>>();
        partition_byte_ranges(
            &sizes,
            partition,
            self.properties.partitioning.partition_count(),
        )
        .into_iter()
        .map(|(index, range)| (self.files[index].clone(), range))
        .collect()
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

    /// Reads this partition's byte ranges through governed Parquet ranges.
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
        if partition >= self.properties.partitioning.partition_count() {
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
        let stream = hot_stream(
            self,
            partition,
            task.session_config().batch_size(),
            governance,
        );
        Ok(Box::pin(RecordBatchStreamAdapter::new(schema, stream)))
    }
}

/// Opens governed readers for the hot objects of one partition stream.
///
/// Owns the dependencies every reader of the stream shares, so each piece
/// builds its reader from a location and size alone. Cloned into each
/// piece's reader closure, which the metadata owner may call again on retry.
#[derive(Clone)]
struct HotReaderFactory {
    /// File reader inherited from the pinned Iceberg table.
    file_io: FileIO,
    /// Closed governance mode every reader's range reservations use.
    governance: HotParquetGovernance,
    /// Shared physical scan counters retained to terminal query emission.
    metrics: Arc<OracleScanMetricsHandle>,
    /// Deterministic range reader injected only by focused tests.
    #[cfg(test)]
    reader_override: Option<HotReadOverride>,
}

impl HotReaderFactory {
    /// Creates an unopened reader for the object at `location` of `size`
    /// bytes; the object is opened on the reader's first range request.
    fn reader(&self, location: &str, size: u64) -> IcebergParquetReader {
        let reader = IcebergParquetReader::new(
            HotObjectSource::Pending {
                file_io: self.file_io.clone(),
                location: location.to_owned(),
            },
            size,
            self.governance.clone(),
            Arc::clone(&self.metrics),
        );
        #[cfg(test)]
        let reader = if let Some(override_reader) = self.reader_override.as_ref() {
            reader.with_test_reader(location.to_owned(), Arc::clone(override_reader))
        } else {
            reader
        };
        reader
    }
}

/// Builds one partition's hot-file stream after partition validation.
///
/// The partition's byte ranges are read sequentially. The piece holding a
/// file's first byte publishes its file observation before the footer is
/// touched; every piece proves the footer's tenant before decoding anything,
/// then keeps only the row groups whose midpoint lies in its
/// range, prunes those and then pages against the closed predicates, decodes
/// at the session `batch_size`, projects to the
/// authenticated physical schema, and holds one governed reservation for
/// exactly the lifetime of the yielded batch. Dropping the stream releases every retained reservation,
/// which is what makes cancellation return the query's memory.
fn hot_stream(
    exec: &HotParquetExec,
    partition: usize,
    batch_size: usize,
    governance: HotParquetGovernance,
) -> impl Stream<Item = DataFusionResult<RecordBatch>> + Send + 'static {
    let pieces = exec.partition_pieces(partition);
    let readers = HotReaderFactory {
        file_io: exec.file_io.clone(),
        governance,
        metrics: Arc::clone(&exec.metrics),
        #[cfg(test)]
        reader_override: exec.reader_override.clone(),
    };
    let storage = Arc::clone(&exec.storage);
    let schema = Arc::clone(&exec.schema);
    let metrics = Arc::clone(&exec.metrics);
    let predicates = exec.predicates.clone();
    let staged_lease = exec.staged_lease.clone();
    // Cancelling the query drops this stream, which drops the guard and
    // cancels any metadata decode this stream still has outstanding. Owner
    // shutdown cancels the same work through the owner's own token.
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_on_drop = cancel.clone().drop_guard();
    async_stream::try_stream! {
        let _cancel_on_drop = cancel_on_drop;
        let _staged_lease = staged_lease;
        for (file, range) in pieces {
            let size = u64::try_from(file.size_bytes).map_err(|_| {
                DataFusionError::Execution("hot object size exceeds u64".to_owned())
            })?;
            let build_reader = {
                let readers = readers.clone();
                let location = file.location.clone();
                move || readers.reader(&location, size)
            };
            // Recorded before the footer is read so a file observation exists
            // for every attempt on this file, including one whose reader fails
            // or is abandoned mid-open. Row-group pruning is reported
            // separately, so a file whose groups are all pruned still counts as
            // opened rather than vanishing from the scan accounting. Only the
            // piece holding the file's first byte records it, so a file split
            // across partitions counts once.
            if range.start == 0 {
                metrics.record_hot_file();
            }
            // The owner, not this leaf, decides whether this object's footer is
            // decoded again: it owns the node-wide cache, single-flight, request
            // admission, and retry bound for every hot identity.
            let retained = storage
                .object_metadata(
                    file.metadata_key.clone(),
                    build_reader.clone(),
                    storage.metadata_deadline(),
                    cancel.clone(),
                )
                .await
                .map_err(|error| {
                    DataFusionError::External(Box::new((*error).clone()))
                })?;
            let Some((metadata, retained_groups)) = hot_piece_metadata(
                retained.metadata(),
                file.metadata_key.tenant_id(),
                &range,
                &predicates,
                &metrics,
            )?
            else {
                continue;
            };
            let mut builder =
                ParquetRecordBatchStreamBuilder::new_with_metadata(build_reader(), metadata);
            // Bloom filters are probed only for the groups statistics kept, so
            // each excluded group is attributed to exactly one mechanism.
            let bloom = HotBloomProbes::new(builder.metadata(), &predicates)
                .retain(&mut builder, retained_groups)
                .await;
            metrics.record_bloom_pruned(bloom.pruned);
            if bloom.excludes_file() {
                continue;
            }
            let retained_groups = bloom.retained;
            // Selective decode: only the closure's leaves leave storage. The
            // post-decode `project_batch` below then normalizes exact order and
            // types; it is a normalizer, not the thing that avoids the IO.
            let mask = hot_projection_mask(builder.parquet_schema(), schema.as_ref());
            let pages =
                select_pages_for_predicates(builder.metadata(), &retained_groups, &predicates);
            let builder = match pages {
                Some(pages) => {
                    metrics.record_page_pruned_rows(pages.skipped_row_count());
                    builder.with_row_selection(pages)
                }
                None => builder,
            };
            let mut batches = builder
                .with_row_groups(retained_groups)
                .with_batch_size(batch_size)
                .with_projection(mask)
                .build()
                .map_err(|error| DataFusionError::External(Box::new(error)))?;
            while let Some(decoded) = batches.next().await {
                let batch = decoded
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                let batch = project_batch(&batch, Arc::clone(&schema))?;
                let decoded_reservation =
                    readers.governance.reserve_decoded(batch.get_array_memory_size())?;
                yield batch;
                drop(decoded_reservation);
            }
        }
    }
}

/// Decides what one hot piece reads from its object's cached footer.
///
/// The tenant proof runs first, so a missing or foreign footer yields no row
/// group, page, or row. Range ownership and predicate pruning then read only
/// the cached footer, so a piece that owns or keeps no row group never pays the
/// Arrow schema conversion; only a piece with surviving row groups builds its
/// reader metadata. Pruning is recorded in `metrics` for every owning piece.
///
/// # Errors
///
/// Returns an external [`BifrostError::QueryTenantInvariant`] when the footer
/// does not prove `tenant`, or the Parquet error when the footer cannot be
/// projected into Arrow reader metadata.
fn hot_piece_metadata(
    metadata: &Arc<ParquetMetaData>,
    tenant: DataTenantId,
    range: &Range<u64>,
    predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    metrics: &OracleScanMetricsHandle,
) -> DataFusionResult<Option<(ArrowReaderMetadata, Vec<usize>)>> {
    verify_scanned_footer_tenant(metadata, tenant)
        .map_err(|error| DataFusionError::External(Box::new(error)))?;
    let owned = row_groups_in_byte_range(metadata, range);
    if owned.is_empty() {
        return Ok(None);
    }
    let selection = select_row_groups_for_predicates(metadata, owned, predicates);
    metrics.record_row_groups(&selection);
    if selection.excludes_file() {
        return Ok(None);
    }
    let reader_metadata =
        ArrowReaderMetadata::try_new(Arc::clone(metadata), ArrowReaderOptions::new())
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
    Ok(Some((reader_metadata, selection.retained)))
}

/// The equality probes one hot file's Bloom filters can answer.
///
/// Built from a file's cached footer and the scan's closed predicates, it keeps
/// one probe per `column = literal` leaf whose column carries a Bloom filter in
/// a physical encoding this reader hashes exactly. Probing reads each filter
/// through the scan's governed reader, so Bloom bytes are charged like every
/// other range read. Missing filters, unreadable filters, unsupported physical
/// types, and any leaf other than equality keep the row group: a Bloom filter
/// can only prove absence, and a false positive merely retains a group.
struct HotBloomProbes {
    /// `(Parquet leaf column index, hashed probe value)` per supported leaf.
    probes: Vec<(usize, BloomProbe)>,
}

/// One literal encoded exactly as Parquet hashes its column's physical values.
enum BloomProbe {
    /// Raw bytes of a `BYTE_ARRAY` or `FIXED_LEN_BYTE_ARRAY` value.
    Bytes(Vec<u8>),
    /// A plain signed `INT64` value.
    Int64(i64),
}

impl BloomProbe {
    /// Encodes `literal` for the physical column `descriptor`, or `None` when
    /// this reader cannot hash it identically to the writer.
    ///
    /// Byte arrays hash their raw bytes, so a UTF-8 or binary literal maps
    /// directly; a fixed-length column additionally requires the literal's
    /// exact width. `INT64` is accepted only for unannotated or signed-integer
    /// columns, because a timestamp or unsigned annotation changes what the
    /// stored value means. Every other physical type — boolean, floating
    /// point, `INT32`, `INT96` — keeps the group.
    fn for_column(descriptor: &ColumnDescriptor, literal: &ScanLiteral) -> Option<Self> {
        match (descriptor.physical_type(), literal) {
            (PhysicalType::BYTE_ARRAY, ScanLiteral::Utf8(value)) => {
                Some(Self::Bytes(value.as_bytes().to_vec()))
            }
            (PhysicalType::BYTE_ARRAY, ScanLiteral::Bytes(value)) => {
                Some(Self::Bytes(value.clone()))
            }
            (PhysicalType::FIXED_LEN_BYTE_ARRAY, ScanLiteral::Bytes(value))
                if usize::try_from(descriptor.type_length())
                    .is_ok_and(|width| width == value.len()) =>
            {
                Some(Self::Bytes(value.clone()))
            }
            (PhysicalType::INT64, ScanLiteral::I64(value)) => {
                let plain_signed = match descriptor.logical_type_ref() {
                    Some(LogicalType::Integer(int)) => int.is_signed,
                    Some(_) => false,
                    None => descriptor.converted_type() == ConvertedType::NONE,
                };
                plain_signed.then_some(Self::Int64(*value))
            }
            _ => None,
        }
    }

    /// Reports whether `filter` may contain this value.
    fn may_contain(&self, filter: &Sbbf) -> bool {
        match self {
            Self::Bytes(value) => filter.check(value.as_slice()),
            Self::Int64(value) => filter.check(value),
        }
    }
}

impl HotBloomProbes {
    /// Collects the Bloom probes `predicates` allow against `metadata`'s file.
    ///
    /// Only equality leaves become probes; a column the file does not carry or
    /// a literal [`BloomProbe::for_column`] cannot encode is skipped.
    fn new(metadata: &ParquetMetaData, predicates: &[ScanPredicate]) -> Self {
        let schema = metadata.file_metadata().schema_descr();
        let probes = predicates
            .iter()
            .filter_map(|predicate| match predicate {
                ScanPredicate::Eq(column, literal) => {
                    let index = parquet_column_index(metadata, column)?;
                    Some((
                        index,
                        BloomProbe::for_column(&schema.column(index), literal)?,
                    ))
                }
                _ => None,
            })
            .collect();
        Self { probes }
    }

    /// Splits `candidates` into the groups no Bloom filter proves empty, in
    /// order, and the count of groups a filter excluded.
    ///
    /// A group is excluded when any probe's column chunk carries a filter that
    /// reports the probe value absent. A group without a filter for that
    /// column, or whose filter cannot be read, is kept and the read failure is
    /// traced, never surfaced: the filter is an optional pruning aid.
    ///
    /// Cancellation drops the outstanding filter read; nothing is recorded
    /// until the caller receives the completed selection.
    async fn retain<T>(
        &self,
        builder: &mut ParquetRecordBatchStreamBuilder<T>,
        candidates: Vec<usize>,
    ) -> RowGroupSelection
    where
        T: AsyncFileReader + Send + 'static,
    {
        let mut retained = Vec::with_capacity(candidates.len());
        let mut pruned = 0_u64;
        for row_group in candidates {
            let mut absent = false;
            for (column, probe) in &self.probes {
                match builder
                    .get_row_group_column_bloom_filter(row_group, *column)
                    .await
                {
                    Ok(Some(filter)) if !probe.may_contain(&filter) => {
                        absent = true;
                        break;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::debug!(
                            row_group,
                            column,
                            error = %error,
                            "hot Bloom filter unreadable; keeping the row group"
                        );
                    }
                }
            }
            if absent {
                pruned += 1;
            } else {
                retained.push(row_group);
            }
        }
        RowGroupSelection { retained, pruned }
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
/// `UInt64`/`Float64`/`Utf8`/`TimestampMicrosecond` and the binary family, a
/// null literal, or a non-finite `f64`. Every binary spelling — `Binary`,
/// `LargeBinary`, `BinaryView`, and `FixedSizeBinary` — classifies as the same
/// lossless `Bytes` literal: `DataFusion` unwraps the cast it adds around a
/// fixed-size column compared with an `X'..'` literal of matching width into a
/// `FixedSizeBinary` literal, so a trace-id lookup reaches this point as a bare
/// column comparison.
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
        ScalarValue::Binary(Some(inner))
        | ScalarValue::LargeBinary(Some(inner))
        | ScalarValue::BinaryView(Some(inner))
        | ScalarValue::FixedSizeBinary(_, Some(inner)) => Some(ScanLiteral::Bytes(inner.clone())),
        _ => None,
    }
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
    projection: Option<&Vec<usize>>,
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
/// names, followed by the first occurrence of each predicate column in filter
/// order — stably deduplicated.
///
/// A closure is never empty. A request that reads no column, such as
/// `COUNT(*)`, still needs every leaf to report row counts, so it reads the
/// always-present non-null `wyrd_event_time` instead of producing zero-column
/// batches across Iceberg, Parquet, IPC, and distributed frames.
fn required_columns_closure(
    output_names: &[String],
    predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
) -> Vec<String> {
    let mut required = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in output_names.iter().cloned().chain(
        predicates
            .iter()
            .map(|predicate| predicate.column().to_string()),
    ) {
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
    /// Stable-deduplicated, never-empty leaf closure: outputs, then predicate
    /// columns.
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
        projection: Option<&Vec<usize>>,
        predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    ) -> DataFusionResult<Self> {
        let output_names = scan_output_names(physical_schema, projection)?;
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
        ScanLiteral::Bytes(inner) => binary_scalar_for(schema, column, inner),
    }
}

/// Materializes one binary literal in the compared column's own binary type.
///
/// A fixed-size column of the literal's exact width yields a `FixedSizeBinary`
/// scalar, and the variable-width binary types yield their own spelling, so the
/// physical comparison never needs a cast. Any other column — absent, or a
/// width the classifier would have refused — falls back to plain `Binary`,
/// leaving the residual filter authoritative.
fn binary_scalar_for(schema: &SchemaRef, column: &str, value: &[u8]) -> ScalarValue {
    match schema
        .field_with_name(column)
        .map(arrow::datatypes::Field::data_type)
    {
        Ok(DataType::FixedSizeBinary(width))
            if usize::try_from(*width).is_ok_and(|width| width == value.len()) =>
        {
            ScalarValue::FixedSizeBinary(*width, Some(value.to_vec()))
        }
        Ok(DataType::LargeBinary) => ScalarValue::LargeBinary(Some(value.to_vec())),
        Ok(DataType::BinaryView) => ScalarValue::BinaryView(Some(value.to_vec())),
        _ => ScalarValue::Binary(Some(value.to_vec())),
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

/// Compiles one assignment's signed closed predicates into a single physical
/// conjunction over `schema`, or `None` when the assignment carries none.
///
/// The Scribe follower places it in a `FilterExec` above its live leaf, so a
/// selective query ships only matching rows back to the leader. Rows whose
/// conjunction evaluates to `NULL` are dropped, matching SQL `WHERE`.
///
/// # Errors
/// Returns a `DataFusion` plan error when a predicate names a column absent
/// from `schema`.
pub(super) fn scan_predicate_conjunction(
    predicates: &[ScanPredicate],
    schema: &SchemaRef,
) -> DataFusionResult<Option<Arc<dyn PhysicalExpr>>> {
    let compiled = predicates
        .iter()
        .map(|predicate| scan_predicate_physical_expr(predicate, schema))
        .collect::<DataFusionResult<Vec<_>>>()?;
    Ok(conjoin_physical_predicates(compiled))
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
    /// Binary bound compared by unsigned lexicographic byte order, which is
    /// Parquet's order for unannotated and fixed-length byte arrays.
    Bytes(Vec<u8>),
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
        ScanLiteral::Bytes(value) => Some(StatBound::Bytes(value.clone())),
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
        (Statistics::ByteArray(value), StatBound::Bytes(_)) => Some((
            StatBound::Bytes(value.min_opt()?.data().to_vec()),
            StatBound::Bytes(value.max_opt()?.data().to_vec()),
        )),
        (Statistics::FixedLenByteArray(value), StatBound::Bytes(_)) => Some((
            StatBound::Bytes(value.min_opt()?.data().to_vec()),
            StatBound::Bytes(value.max_opt()?.data().to_vec()),
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
    let Some(column_index) = parquet_column_index(metadata, predicate.column()) else {
        return false;
    };
    let row_group = metadata.row_group(row_group_index);
    let Some(stats) = row_group.column(column_index).statistics() else {
        return false;
    };
    leaf_excludes_span(
        predicate,
        |target| statistics_bound(stats, target),
        stats.null_count_opt(),
        u64::try_from(row_group.num_rows()).unwrap_or(0),
    )
}

/// Returns true only when one span's evidence proves no row in it can satisfy
/// `predicate`.
///
/// A span is a row group or one data page; both prune through this one
/// decision so page selection can never disagree with row-group pruning.
/// `bounds` yields the span's min/max matched to the literal's type, and
/// `null_count` with `rows` decides the null checks. Absent evidence always
/// keeps the span, so this never produces a false exclusion.
fn leaf_excludes_span(
    predicate: &wyrd_spec::vala::assignment_authority::ScanPredicate,
    bounds: impl Fn(&StatBound) -> Option<(StatBound, StatBound)>,
    null_count: Option<u64>,
    rows: u64,
) -> bool {
    use std::cmp::Ordering;
    use wyrd_spec::vala::assignment_authority::ScanPredicate;

    match predicate {
        ScanPredicate::IsNull(_) => return null_count == Some(0),
        ScanPredicate::IsNotNull(_) => return null_count == Some(rows),
        _ => {}
    }
    let Some(target) = predicate.literal().and_then(literal_bound) else {
        return false;
    };
    let Some((min, max)) = bounds(&target) else {
        return false;
    };
    match predicate {
        ScanPredicate::Eq(..) => target < min || max < target,
        ScanPredicate::NotEq(..) => min == max && min == target,
        ScanPredicate::Lt(..) => matches!(
            min.partial_cmp(&target),
            Some(Ordering::Equal | Ordering::Greater)
        ),
        ScanPredicate::LtEq(..) => target < min,
        ScanPredicate::Gt(..) => matches!(
            target.partial_cmp(&max),
            Some(Ordering::Equal | Ordering::Greater)
        ),
        ScanPredicate::GtEq(..) => max < target,
        ScanPredicate::IsNull(_) | ScanPredicate::IsNotNull(_) => false,
    }
}

/// Reads one page's typed min/max from a column index as comparable bounds.
///
/// The page-index counterpart of [`statistics_bound`]: returns `None` for an
/// all-null page, a missing index, or a physical type that does not match
/// `target`'s variant, so the caller keeps the page.
fn page_bound(
    index: &parquet::file::page_index::column_index::ColumnIndexMetaData,
    page: usize,
    target: &StatBound,
) -> Option<(StatBound, StatBound)> {
    use parquet::file::page_index::column_index::ColumnIndexMetaData;
    match (index, target) {
        (ColumnIndexMetaData::BOOLEAN(pages), StatBound::Bool(_)) => Some((
            StatBound::Bool(*pages.min_value(page)?),
            StatBound::Bool(*pages.max_value(page)?),
        )),
        (ColumnIndexMetaData::INT64(pages), StatBound::I64(_)) => Some((
            StatBound::I64(*pages.min_value(page)?),
            StatBound::I64(*pages.max_value(page)?),
        )),
        (ColumnIndexMetaData::BYTE_ARRAY(pages), StatBound::Utf8(_)) => Some((
            StatBound::Utf8(String::from_utf8_lossy(pages.min_value(page)?).into_owned()),
            StatBound::Utf8(String::from_utf8_lossy(pages.max_value(page)?).into_owned()),
        )),
        (
            ColumnIndexMetaData::BYTE_ARRAY(pages)
            | ColumnIndexMetaData::FIXED_LEN_BYTE_ARRAY(pages),
            StatBound::Bytes(_),
        ) => Some((
            StatBound::Bytes(pages.min_value(page)?.to_vec()),
            StatBound::Bytes(pages.max_value(page)?.to_vec()),
        )),
        _ => None,
    }
}

/// Selects the pages of `row_groups` whose page index can still satisfy every
/// predicate leaf, as one row selection over those groups in order.
///
/// Row-group pruning alone makes a point lookup decode its whole row group;
/// this narrows the retained groups to the pages that may match, and the
/// reader then fetches and decodes only those pages of every projected column.
/// A page is skipped only when [`leaf_excludes_span`] proves it empty for some
/// leaf, so the selection is always a superset of the matching rows and the
/// plan's own filter still decides exact membership. Returns `None` when the
/// file carries no page index or no page was excluded, leaving the reader
/// unchanged.
fn select_pages_for_predicates(
    metadata: &parquet::file::metadata::ParquetMetaData,
    row_groups: &[usize],
    predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
) -> Option<parquet::arrow::arrow_reader::RowSelection> {
    use parquet::arrow::arrow_reader::{RowSelection, RowSelector};

    let (Some(column_indexes), Some(offset_indexes)) =
        (metadata.column_index(), metadata.offset_index())
    else {
        return None;
    };
    let mut selectors = Vec::new();
    let mut excluded_any = false;
    for &row_group in row_groups {
        let rows = usize::try_from(metadata.row_group(row_group).num_rows()).unwrap_or(0);
        let mut excluded: Vec<Range<usize>> = Vec::new();
        for predicate in predicates {
            let Some(column) = parquet_column_index(metadata, predicate.column()) else {
                continue;
            };
            let (Some(index), Some(offsets)) = (
                column_indexes
                    .get(row_group)
                    .and_then(|group| group.get(column)),
                offset_indexes
                    .get(row_group)
                    .and_then(|group| group.get(column)),
            ) else {
                continue;
            };
            let pages = offsets.page_locations();
            if usize::try_from(index.num_pages()).ok() != Some(pages.len()) {
                continue;
            }
            let first_row = |page: usize| {
                pages.get(page).map_or(rows, |location| {
                    usize::try_from(location.first_row_index).map_or(rows, |row| row.min(rows))
                })
            };
            for page in 0..pages.len() {
                let start = first_row(page);
                let end = first_row(page + 1).max(start);
                let null_count = index
                    .null_count(page)
                    .and_then(|count| u64::try_from(count).ok());
                if leaf_excludes_span(
                    predicate,
                    |target| page_bound(index, page, target),
                    null_count,
                    u64::try_from(end - start).unwrap_or(u64::MAX),
                ) {
                    excluded.push(start..end);
                }
            }
        }
        excluded_any |= !excluded.is_empty();
        excluded.sort_by_key(|range| range.start);
        let mut cursor = 0;
        for range in excluded {
            if range.start > cursor {
                selectors.push(RowSelector::select(range.start - cursor));
            }
            if range.end > cursor {
                selectors.push(RowSelector::skip(range.end - range.start.max(cursor)));
                cursor = range.end;
            }
        }
        if rows > cursor {
            selectors.push(RowSelector::select(rows - cursor));
        }
    }
    excluded_any.then(|| RowSelection::from(selectors))
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

/// Selects the `candidates` row groups of `metadata` whose statistics can
/// still satisfy the closed predicate conjunction, pruning the rest.
///
/// A row group is pruned only when at least one leaf proves it cannot contain a
/// matching row; absent, type-mismatched, or unusable statistics always retain
/// it, so pruning is a pure IO optimization and never changes results. An empty
/// predicate conjunction retains every candidate and prunes none.
pub(super) fn select_row_groups_for_predicates(
    metadata: &parquet::file::metadata::ParquetMetaData,
    candidates: Vec<usize>,
    predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
) -> RowGroupSelection {
    let total = candidates.len();
    let retained: Vec<usize> = candidates
        .into_iter()
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
    use std::io::Cursor;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use super::*;
    use crate::oracle::bindings::{
        FollowerSourceKey, OracleExecutionBindingInputs, OracleExecutionBindings,
    };
    use crate::oracle::codec::RemoteSourcePlaceholderExec;
    use arrow::array::{ArrayRef, FixedSizeBinaryArray, Int32Array, Int64Array, StringArray};
    use async_trait::async_trait;
    use bytes::Bytes;
    use datafusion::common::tree_node::TreeNode;
    use datafusion::datasource::memory::MemorySourceConfig;
    use datafusion::logical_expr::{LogicalPlan, col, lit};
    use datafusion::physical_plan::union::UnionExec;
    use parquet::file::properties::WriterProperties;
    use wyrd_runtime::Principal;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::AuthMethod;
    use wyrd_spec::vala::api::FollowerScanAssignment;
    use wyrd_spec::vala::api::QueryStreamFrame;

    use crate::oracle::live::LiveTableRoutes;

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

    /// The one tenant every fixture object is written for and read as.
    static FIXTURE_TENANT: std::sync::LazyLock<DataTenantId> =
        std::sync::LazyLock::new(DataTenantId::new_v7);

    /// Builds the production writer recipe with a footer proving
    /// [`FIXTURE_TENANT`], the way every Bifrost producer writes.
    fn fixture_writer_properties(bloom_columns: &[String]) -> WriterProperties {
        crate::parquet::writer_properties::bifrost_writer_properties_with_metadata(
            vec![crate::parquet::footer::tenant_key_value(*FIXTURE_TENANT)],
            bloom_columns,
        )
    }

    /// Builds one immutable metadata identity for a fixture object.
    ///
    /// The checksum is derived from the name so two differently named fixture
    /// objects never share a cache entry, which is the same property the
    /// durable writer checksum gives production.
    fn fixture_metadata_key(name: &str, size_bytes: usize) -> crate::storage::ObjectMetadataKey {
        let mut checksum = [0_u8; 32];
        for (slot, byte) in checksum.iter_mut().zip(name.as_bytes()) {
            *slot = *byte;
        }
        checksum[31] = 1;
        crate::storage::ObjectMetadataKey::new(
            *FIXTURE_TENANT,
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
                server_memory_min_bytes: None,
                bifrost_memory_limit_bytes: None,
                scratch_limit_bytes: Some(1024 * 1024 * 1024),
                effective_cpu: None,
                oracle_query_slot_limit: None,
                scratch_root: None,
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
        query_pool: &Arc<dyn MemoryPool>,
        peak_pool: &Arc<AtomicU64>,
    ) -> usize {
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
    /// A completed stream must leave the root governor and the query pool both
    /// back at zero; a nonzero residue is a leaked reservation rather than a
    /// measurement artifact.
    fn assert_hot_scan_baselines(
        roles: &crate::resources::BifrostRoleResources,
        query_pool: &Arc<dyn MemoryPool>,
    ) {
        let snapshot = roles.snapshot().expect("root snapshot");
        assert_eq!(snapshot.oracle_memory_used_bytes, 0);
        assert_eq!(snapshot.governed_memory_used_bytes, 0);
        assert_eq!(query_pool.reserved(), 0);
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
        let mut memory_only = OracleQueryScanStats::from_plan(source.as_ref());
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
        use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

        const ROWS: i64 = 100_000;
        const TARGET: i64 = 54_321;
        let schema = Arc::new(Schema::new(vec![
            Field::new("service_name", DataType::Utf8, false),
            Field::new("value", DataType::Int64, false),
        ]));
        let published =
            write_grouped_fixture(&schema, &[service_block(&schema, "checkout", 0, ROWS)]);
        let metadata = parquet::file::metadata::ParquetMetaDataReader::new()
            .with_page_index_policy(parquet::file::metadata::PageIndexPolicy::Optional)
            .parse_and_finish(&published)
            .expect("valid Parquet footer and page index");
        let predicates = vec![ScanPredicate::Eq(
            "value".to_owned(),
            ScanLiteral::I64(TARGET),
        )];

        let selection = select_pages_for_predicates(&metadata, &[0], &predicates)
            .expect("the sorted column's page index excludes pages");
        let kept = selection.row_count();
        assert!(
            kept > 0 && kept < usize::try_from(ROWS).expect("fixture rows fit usize"),
            "kept {kept} of {ROWS} rows"
        );

        let matching: usize = ParquetRecordBatchReaderBuilder::try_new_with_options(
            published,
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
        assert!(
            select_pages_for_predicates(&metadata, &[0], &[]).is_none(),
            "an empty conjunction leaves the reader unchanged"
        );
    }

    /// Row-group min/max pruning measured on a real two-row-group file written
    /// by the production recipe: the file's low-cardinality `service_name`
    /// column is dictionary-encoded and carries a Bloom filter, but the two
    /// groups hold disjoint values, so the equality leaf retains exactly one
    /// group on min/max statistics alone. No Bloom filter is read here; Bloom
    /// consumption is proved by
    /// `hot_bloom_probes_exclude_an_absent_id_inside_statistics_bounds`. The
    /// retained group's compressed bytes are strictly fewer than the whole
    /// file's, and the decoded rows are exactly the matching rows. A
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

        let predicates = vec![ScanPredicate::Eq(
            "service_name".to_owned(),
            ScanLiteral::Utf8("checkout".to_owned()),
        )];
        let selection = select_row_groups_for_predicates(
            &metadata,
            (0..metadata.num_row_groups()).collect(),
            &predicates,
        );
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
        let schema: SchemaRef = Arc::new(Schema::new(vec![
            Field::new("trace_id", DataType::FixedSizeBinary(16), false),
            Field::new("score", DataType::Float64, false),
        ]));
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

    /// Builds one 16-byte trace id with a constant prefix and `tail` as its
    /// big-endian final two bytes.
    fn trace_id_with_tail(tail: u16) -> [u8; 16] {
        let mut id = [0x5a_u8; 16];
        id[14..].copy_from_slice(&tail.to_be_bytes());
        id
    }

    /// Opens an async stream builder over an in-memory Parquet file.
    ///
    /// # Panics
    ///
    /// Panics when the footer cannot be decoded.
    async fn in_memory_stream_builder(
        published: &Bytes,
    ) -> ParquetRecordBatchStreamBuilder<Cursor<Vec<u8>>> {
        ParquetRecordBatchStreamBuilder::new(Cursor::new(published.to_vec()))
            .await
            .expect("in-memory stream builder")
    }

    /// The hot reader consumes a `FIXED_LEN_BYTE_ARRAY` Bloom filter: an
    /// absent id that lies inside both groups' min/max — so statistics keep
    /// both — is excluded from a group whose filter proves it absent, while a
    /// present id keeps its own group. The absent probe is chosen to be
    /// Bloom-negative in the written filter, so the assertion cannot flake on a
    /// false positive. Unsupported physical types and filterless columns build
    /// no probe and keep every group.
    ///
    /// # Panics
    ///
    /// Panics when a group's Bloom filter cannot be read, when no Bloom-negative id
    /// lies inside both groups' bounds, when statistics alone exclude a group, when
    /// the Bloom probes keep a group for the absent id or drop the group holding a
    /// present id, or when an unsupported or filterless probe drops any group.
    #[tokio::test]
    async fn hot_bloom_probes_exclude_an_absent_id_inside_statistics_bounds() {
        let (published, groups) = write_trace_id_fixture();
        let mut builder = in_memory_stream_builder(&published).await;
        let metadata = Arc::clone(builder.metadata());
        assert_eq!(metadata.num_row_groups(), 2, "fixture must have two groups");
        let mut filters = Vec::new();
        for group in 0..2 {
            filters.push(
                builder
                    .get_row_group_column_bloom_filter(group, 0)
                    .await
                    .expect("readable filter")
                    .expect("trace_id carries a Bloom filter"),
            );
        }
        // An unwritten odd-tailed id inside every group's bounds that both
        // written filters report absent.
        let absent = (1..400_u16)
            .step_by(2)
            .map(trace_id_with_tail)
            .find(|id| {
                groups.iter().all(|ids| {
                    ids.iter().min().is_some_and(|min| min < id)
                        && ids.iter().max().is_some_and(|max| id < max)
                }) && filters.iter().all(|filter| !filter.check(&id[..]))
            })
            .expect("a Bloom-negative id inside both groups' bounds");

        let lookup = |id: &[u8; 16]| {
            vec![ScanPredicate::Eq(
                "trace_id".to_owned(),
                ScanLiteral::Bytes(id.to_vec()),
            )]
        };
        let absent_predicates = lookup(&absent);
        let statistics =
            select_row_groups_for_predicates(&metadata, vec![0, 1], &absent_predicates);
        assert_eq!(
            statistics.retained,
            vec![0, 1],
            "min/max alone must keep both groups, so only a Bloom filter can exclude them"
        );
        let bloom = HotBloomProbes::new(&metadata, &absent_predicates)
            .retain(&mut builder, statistics.retained)
            .await;
        assert_eq!(bloom.retained, Vec::<usize>::new());
        assert_eq!(bloom.pruned, 2);

        let present = groups[1][37];
        let bloom = HotBloomProbes::new(&metadata, &lookup(&present))
            .retain(&mut builder, vec![0, 1])
            .await;
        assert!(
            bloom.retained.contains(&1),
            "the group holding a present id is never excluded"
        );

        // A float column builds no probe; a column absent from the file
        // builds none either, so every candidate survives.
        for predicates in [
            vec![ScanPredicate::Eq(
                "score".to_owned(),
                ScanLiteral::F64Bits(3.0_f64.to_bits()),
            )],
            vec![ScanPredicate::Eq(
                "missing".to_owned(),
                ScanLiteral::Bytes(absent.to_vec()),
            )],
            vec![ScanPredicate::Eq(
                "trace_id".to_owned(),
                ScanLiteral::Bytes(vec![0x5a; 8]),
            )],
        ] {
            let kept = HotBloomProbes::new(&metadata, &predicates)
                .retain(&mut builder, vec![0, 1])
                .await;
            assert_eq!(kept.retained, vec![0, 1], "{predicates:?}");
            assert_eq!(kept.pruned, 0);
        }
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
        let metadata = parquet::file::metadata::ParquetMetaDataReader::new()
            .parse_and_finish(&published)
            .expect("valid Parquet footer");
        let above = vec![ScanPredicate::Eq(
            "trace_id".to_owned(),
            ScanLiteral::Bytes(vec![0xff; 16]),
        )];
        let selection = select_row_groups_for_predicates(&metadata, vec![0, 1], &above);
        assert_eq!(selection.retained, Vec::<usize>::new());
        assert_eq!(selection.pruned, 2);
        // Strictly below the smallest written id, so neither group can match.
        let below = vec![ScanPredicate::Lt(
            "trace_id".to_owned(),
            ScanLiteral::Bytes(trace_id_with_tail(0).to_vec()),
        )];
        assert!(select_row_groups_for_predicates(&metadata, vec![0, 1], &below).excludes_file());
    }

    /// A SQL `X'..'` literal compared with a fixed-size binary column
    /// classifies as one lossless `Bytes` equality after `DataFusion` unwraps
    /// its coercion cast, materializes back as the column's own type, and a
    /// literal of the wrong width stays unsupported.
    ///
    /// # Panics
    ///
    /// Panics when the fixture table cannot be registered or planned, when the
    /// optimized plan has no filter, when the binary equality does not classify as
    /// one `Bytes` leaf, when it does not materialize as a 16-byte
    /// `FixedSizeBinary` scalar, or when a literal of the wrong width is classified
    /// as supported.
    #[tokio::test]
    async fn binary_sql_literal_classifies_as_lossless_bytes() {
        let schema: SchemaRef = Arc::new(Schema::new(vec![Field::new(
            "trace_id",
            DataType::FixedSizeBinary(16),
            false,
        )]));
        let context = datafusion::prelude::SessionContext::new();
        context
            .register_table(
                "spans",
                Arc::new(
                    datafusion::datasource::MemTable::try_new(Arc::clone(&schema), vec![vec![]])
                        .expect("empty table"),
                ),
            )
            .expect("register table");
        let filter_of = |plan: &LogicalPlan| -> Option<Expr> {
            let mut found = None;
            plan.apply(|node| {
                if let LogicalPlan::Filter(filter) = node {
                    found = Some(filter.predicate.clone());
                    return Ok(TreeNodeRecursion::Stop);
                }
                Ok(TreeNodeRecursion::Continue)
            })
            .expect("plan walk");
            found
        };
        let id = "ff00".repeat(8);
        let plan = context
            .sql(&format!(
                "SELECT trace_id FROM spans WHERE trace_id = X'{id}'"
            ))
            .await
            .expect("plan")
            .into_optimized_plan()
            .expect("optimized plan");
        let predicate = filter_of(&plan).expect("filter retained above the memory table");
        let expected = [0xff_u8, 0x00].repeat(8);
        match classify_filter_for_schema(&schema, &predicate) {
            FilterClassification::Supported(leaves) => assert_eq!(
                leaves,
                vec![ScanPredicate::Eq(
                    "trace_id".to_owned(),
                    ScanLiteral::Bytes(expected.clone())
                )]
            ),
            FilterClassification::Unsupported => panic!("binary equality must push down"),
        }
        assert_eq!(
            scan_literal_scalar(&schema, "trace_id", &ScanLiteral::Bytes(expected)),
            datafusion::scalar::ScalarValue::FixedSizeBinary(16, Some([0xff, 0x00].repeat(8)))
        );

        let short = context
            .sql("SELECT trace_id FROM spans WHERE trace_id = X'ff00'")
            .await
            .expect("plan")
            .into_optimized_plan()
            .expect("optimized plan");
        if let Some(predicate) = filter_of(&short) {
            assert!(matches!(
                classify_filter_for_schema(&schema, &predicate),
                FilterClassification::Unsupported
            ));
        }
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
        let properties = fixture_writer_properties(&[]);
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

    /// Every partition count tiles each non-empty file exactly once.
    ///
    /// The union of all partitions' ranges for a file must be `[0, size)`
    /// with no overlap, and an empty file must never be assigned, so each
    /// row group's midpoint lands in exactly one partition.
    #[test]
    fn partition_byte_ranges_tile_every_file_once() {
        let sizes = [10_u64, 3, 0, 20, 1];
        for partitions in 1..=8 {
            let mut covered = vec![Vec::<Range<u64>>::new(); sizes.len()];
            for partition in 0..partitions {
                for (index, range) in partition_byte_ranges(&sizes, partition, partitions) {
                    assert!(range.start < range.end, "ranges are never empty");
                    covered[index].push(range);
                }
            }
            for (index, ranges) in covered.iter_mut().enumerate() {
                ranges.sort_by_key(|range| range.start);
                let mut next = 0;
                for range in ranges.iter() {
                    assert_eq!(range.start, next, "file {index} has a gap or overlap");
                    next = range.end;
                }
                assert_eq!(next, sizes[index], "file {index} is not fully covered");
            }
        }
        assert!(partition_byte_ranges(&[], 0, 4).is_empty());
    }

    /// Writes `values` as a Parquet file with two-row row groups.
    fn write_split_fixture(path: &std::path::Path, schema: &SchemaRef, values: &[i64]) -> usize {
        let batch = RecordBatch::try_new(
            Arc::clone(schema),
            vec![Arc::new(Int64Array::from(values.to_vec())) as ArrayRef],
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

    /// A hot leaf split across partitions reads every row exactly once.
    ///
    /// One multi-row-group file and one small file are read at several
    /// partition counts, including more partitions than row groups. The union
    /// of all partitions must equal the source rows, and each file must count
    /// once in scan telemetry however many partitions read it.
    #[tokio::test]
    async fn hot_parquet_split_partitions_read_every_row_once() {
        let directory = tempfile::tempdir().expect("split fixture directory");
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
                let path = directory.path().join(name);
                let size_bytes = write_split_fixture(&path, &schema, values);
                HotFileSource {
                    metadata_key: fixture_metadata_key(&path.to_string_lossy(), size_bytes),
                    location: path.to_string_lossy().into_owned(),
                    size_bytes,
                    event_time: unusable_event_time(),
                }
            })
            .collect::<Vec<_>>();
        let mut expected = large.iter().chain(&small).copied().collect::<Vec<_>>();
        expected.sort_unstable();
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let telemetry = Arc::new(OracleTelemetry::new());
        for partitions in [1, 2, 3, 4, 16] {
            let metrics = Arc::new(OracleScanMetricsHandle::default());
            let exec = HotParquetExec::new(
                files.clone(),
                FileIO::new_with_fs(),
                fixture_storage(),
                Arc::clone(&schema),
                HotParquetPlan::Leader,
                Arc::clone(&metrics),
                Vec::new(),
            )
            .with_partitions(partitions);
            assert_eq!(exec.properties().partitioning.partition_count(), partitions);
            let context = bound_leader_task(
                oracle_memory_resources(&governor, 1024 * 1024),
                &telemetry,
                crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
            );
            let mut actual = Vec::new();
            for partition in 0..partitions {
                let stream = exec
                    .execute(partition, Arc::clone(&context))
                    .expect("split partition stream");
                actual.extend(collect_int64(stream).await);
            }
            assert!(exec.execute(partitions, context).is_err());
            actual.sort_unstable();
            assert_eq!(actual, expected, "{partitions} partitions");
            let (_, scanned_files, _) = metrics.terminal_values();
            assert_eq!(
                scanned_files, 2,
                "{partitions} partitions count each file once"
            );
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
        let directory = tempfile::tempdir().expect("tenant fixture directory");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let foreign_path = directory.path().join("foreign.parquet");
        let foreign_size = write_split_fixture(&foreign_path, &schema, &[1, 2, 3]);
        let missing_path = directory.path().join("missing.parquet");
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
        let reading_tenant = DataTenantId::new_v7();
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let telemetry = Arc::new(OracleTelemetry::new());
        for (path, size_bytes, tenant) in [
            (&foreign_path, foreign_size, reading_tenant),
            (&missing_path, missing_size, *FIXTURE_TENANT),
        ] {
            let location = path.to_string_lossy().into_owned();
            let file = HotFileSource {
                metadata_key: crate::storage::ObjectMetadataKey::new(
                    tenant,
                    "vala.traces.spans".to_owned(),
                    location.clone(),
                    uuid::Uuid::now_v7(),
                    [7; 32],
                    u64::try_from(size_bytes).expect("fixture size fits u64"),
                ),
                location,
                size_bytes,
                event_time: unusable_event_time(),
            };
            let exec = HotParquetExec::new(
                vec![file],
                FileIO::new_with_fs(),
                fixture_storage(),
                Arc::clone(&schema),
                HotParquetPlan::Leader,
                Arc::new(OracleScanMetricsHandle::default()),
                Vec::new(),
            );
            let context = bound_leader_task(
                oracle_memory_resources(&governor, 1024 * 1024),
                &telemetry,
                crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
            );
            let mut stream = exec.execute(0, context).expect("tenant fixture stream");
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

    /// Byte-range pieces handed to Iceberg's reader return every row once.
    ///
    /// This pins the contract between [`partition_byte_ranges`] and the
    /// reader's midpoint rule for a split `FileScanTask`: the published leaf
    /// relies on it to split one data file across partitions.
    #[tokio::test]
    async fn iceberg_reader_honors_partition_byte_ranges() {
        let directory = tempfile::tempdir().expect("split fixture directory");
        let path = directory.path().join("data.parquet");
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, false).with_metadata(HashMap::from([(
                "PARQUET:field_id".to_owned(),
                "1".to_owned(),
            )])),
        ]));
        let values: Vec<i64> = (0..9).collect();
        let size = u64::try_from(write_split_fixture(&path, &schema, &values))
            .expect("fixture size fits u64");
        let iceberg_schema = Arc::new(
            iceberg::spec::Schema::builder()
                .with_fields(vec![Arc::new(iceberg::spec::NestedField::required(
                    1,
                    "value",
                    iceberg::spec::Type::Primitive(iceberg::spec::PrimitiveType::Long),
                ))])
                .build()
                .expect("Iceberg task schema"),
        );
        let reader = iceberg::arrow::ArrowReaderBuilder::new(
            FileIO::new_with_fs(),
            iceberg::Runtime::current(),
        )
        .build();
        for partitions in [1, 2, 3, 5, 12] {
            let mut actual = Vec::new();
            for partition in 0..partitions {
                for (_, range) in partition_byte_ranges(&[size], partition, partitions) {
                    let task = FileScanTask::builder()
                        .with_file_size_in_bytes(size)
                        .with_start(range.start)
                        .with_length(range.end - range.start)
                        .with_record_count(None)
                        .with_data_file_path(path.to_string_lossy().to_string())
                        .with_data_file_format(iceberg::spec::DataFileFormat::Parquet)
                        .with_schema(Arc::clone(&iceberg_schema))
                        .with_project_field_ids(vec![1])
                        .with_deletes(Vec::new())
                        .with_case_sensitive(false)
                        .build();
                    let batches = reader
                        .clone()
                        .read(Box::pin(futures_util::stream::iter(vec![Ok(task)])))
                        .expect("split reader")
                        .stream()
                        .try_collect::<Vec<RecordBatch>>()
                        .await
                        .expect("split rows");
                    for batch in batches {
                        actual.extend(
                            batch
                                .column(0)
                                .as_any()
                                .downcast_ref::<Int64Array>()
                                .expect("Iceberg split keeps Int64")
                                .values()
                                .iter()
                                .copied(),
                        );
                    }
                }
            }
            actual.sort_unstable();
            assert_eq!(actual, values, "{partitions} partitions");
        }
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
            Some(fixture_writer_properties(&[])),
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
        let telemetry = Arc::new(OracleTelemetry::new());
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
            Some(fixture_writer_properties(&[])),
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
                telemetry: Arc::new(OracleTelemetry::new()),
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
        let telemetry = Arc::new(OracleTelemetry::new());
        let metrics = Arc::new(OracleScanMetricsHandle::default());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&ranges);
        let source = fixture.bytes.clone();
        let peak_pool = Arc::new(AtomicU64::new(0));
        let query_pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
        let observed_pool = Arc::clone(&query_pool);
        let range_peak_pool = Arc::clone(&peak_pool);
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
        let rows =
            drain_projected_int64_batches(&mut batches, &projected, &query_pool, &peak_pool).await;
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
        assert!(peak_pool > 0 && peak_pool <= budget);
        drop(ranges);
        assert_hot_scan_baselines(&governor, &query_pool);
    }

    /// Several accepted compressible requests published into one row group
    /// that decodes above 256 MiB stream back whole under a 32 MiB query pool.
    ///
    /// The leader charges each fetched range and each retained output batch,
    /// never the group's uncompressed total, so the whole group reads as
    /// 8,192-row, 8 MiB batches.
    ///
    /// # Panics
    ///
    /// Panics when the scan is refused, loses a row, or retains a charge.
    #[tokio::test]
    async fn hot_parquet_compressible_group_above_256_mib_streams_under_a_small_pool() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, false),
            Field::new("label", DataType::Utf8, false),
        ]));
        let (requests, rows_per_request) = (5_i64, 60_000_i64);
        let label = "x".repeat(1024);
        let directory = tempfile::tempdir().expect("compressible fixture directory");
        let path = directory.path().join("compressible.parquet");
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            File::create(&path).expect("compressible file"),
            Arc::clone(&schema),
            Some(fixture_writer_properties(&[])),
        )
        .expect("compressible writer");
        for request in 0..requests {
            let start = request * rows_per_request;
            let batch = RecordBatch::try_new(
                Arc::clone(&schema),
                vec![
                    Arc::new(Int64Array::from_iter_values(
                        start..start + rows_per_request,
                    )) as ArrayRef,
                    Arc::new(StringArray::from_iter_values(
                        (0..rows_per_request).map(|_| label.as_str()),
                    )),
                ],
            )
            .expect("compressible request batch");
            writer.write(&batch).expect("compressible request rows");
        }
        let metadata = writer.close().expect("compressible footer");
        assert_eq!(
            metadata.num_row_groups(),
            1,
            "every request shares one group"
        );
        let bytes = bytes::Bytes::from(std::fs::read(&path).expect("compressible bytes"));
        let governor = oracle_test_roles(4 * 1024 * 1024 * 1024);
        let telemetry = Arc::new(OracleTelemetry::new());
        let query_pool = crate::resources::bounded_memory_pool(32 * 1024 * 1024);
        let exec = HotParquetExec::new(
            vec![HotFileSource {
                metadata_key: fixture_metadata_key(&path.to_string_lossy(), bytes.len()),
                location: path.to_string_lossy().into_owned(),
                size_bytes: bytes.len(),
                event_time: unusable_event_time(),
            }],
            FileIO::new_with_fs(),
            fixture_storage(),
            Arc::clone(&schema),
            HotParquetPlan::Leader,
            Arc::new(OracleScanMetricsHandle::default()),
            Vec::new(),
        )
        .with_test_reader(counting_slice_reader(&bytes, &Arc::new(AtomicU64::new(0))));
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
        let mut rows = 0;
        let mut decoded = 0;
        while let Some(batch) = batches.next().await {
            let batch = batch.expect("each batch fits the query pool");
            rows += batch.num_rows();
            decoded += batch.get_array_memory_size();
        }
        drop(batches);
        assert_eq!(rows, 300_000);
        assert!(
            decoded > 256 * 1024 * 1024,
            "the decoded total {decoded} exceeds 256 MiB"
        );
        assert_hot_scan_baselines(&governor, &query_pool);
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

    /// An indivisible range above its ceiling is refused before storage IO
    /// as the admitted query's typed resource exhaustion, not an admission
    /// refusal.
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
            Some(BifrostError::QueryResourcesExhausted)
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
        let telemetry = Arc::new(OracleTelemetry::new());
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
                .governed_memory_used_bytes,
            0
        );
        assert_eq!(query_pool.reserved(), 0);
        assert_eq!(attempts.load(Ordering::Acquire), attempts_at_yield);
    }

    /// Drains a hot plan through the production `OracleQueryStream` terminal owner.
    async fn drain_hot_terminal(telemetry: &Arc<OracleTelemetry>, exec: HotParquetExec) {
        let schema = exec.schema();
        let scan_stats = OracleQueryScanStats::from_plan(&exec);
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
            crate::oracle::test_query_stream_from_physical(&schema, batches, scan_stats);
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
        let telemetry = Arc::new(OracleTelemetry::new());
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
        let scan_stats = OracleQueryScanStats::from_plan(&pending);
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
        let mut pending_stream =
            crate::oracle::test_query_stream_from_physical(&schema, batches, scan_stats);
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
    /// operator; and every closed-subset-violating shape (`OR`, `NOT`,
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

    /// The projection closure is `scan output + predicate columns`, in that
    /// order, stably deduplicated, and never empty.
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
        ]);
        let supported = col("service_name")
            .eq(lit("api"))
            .and(col("duration_ms").is_not_null());

        // Projection-closure order: requested scan output first, then the
        // first occurrence of each predicate column in filter order, stably
        // deduplicated (`duration_ms` appears in both the projection and the predicates).
        let leaves = match classify_filter(&supported) {
            FilterClassification::Supported(leaves) => leaves,
            FilterClassification::Unsupported => unreachable!(),
        };
        let projection = vec![
            physical_schema.index_of("duration_ms").unwrap(),
            physical_schema.index_of("wyrd_event_time").unwrap(),
        ];
        let closure = required_columns_closure(
            &scan_output_names(&physical_schema, Some(&projection)).expect("valid ordinals"),
            &leaves,
        );
        assert_eq!(
            closure,
            vec![
                "duration_ms".to_string(),
                "wyrd_event_time".to_string(),
                "service_name".to_string(),
            ]
        );

        // A `None` projection closes over every table column.
        let full_closure = required_columns_closure(
            &scan_output_names(&physical_schema, None).expect("full projection"),
            &[],
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
            &[],
        );
        assert_eq!(count_closure, vec!["wyrd_event_time".to_string()]);
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

        let physical_schema = Schema::new(vec![Field::new("service_name", DataType::Utf8, true)]);

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

    /// Published data-file footers load once through the governed cache, stay
    /// retained for the scan, and a vanished object keeps its stale cause.
    ///
    /// The second load of the same object must return the identical decoded
    /// metadata, proving the cache served it; both loads are held by the
    /// loader so eviction cannot release bytes the scan still reads. A missing
    /// path must classify as a stale Iceberg object so the query retries
    /// against a fresh snapshot instead of failing as an outage.
    #[tokio::test]
    async fn published_footer_loader_serves_retains_and_classifies_stale() {
        use iceberg::arrow::ParquetMetadataLoader as _;

        // Reads go through the production Iceberg storage adapter, which keeps
        // the owner's typed not-found cause a vanished object reports.
        let fixture = build_hot_batch_fixture();
        let size = u64::try_from(fixture.bytes.len()).expect("fixture size fits u64");
        let root = tempfile::tempdir().expect("published footer root");
        std::fs::write(root.path().join("data.parquet"), &fixture.bytes)
            .expect("published footer object");
        let storage = crate::storage::BifrostStorage::for_test(root.path(), true);
        let warehouse = format!("file://{}", root.path().display());
        let file_io = iceberg::io::FileIOBuilder::new(Arc::new(
            crate::catalog::iceberg_storage::BifrostIcebergStorageFactory::new(
                Arc::clone(&storage),
                &warehouse,
            ),
        ))
        .build();
        let footers = PublishedFooters::new(
            storage,
            *FIXTURE_TENANT,
            "vala.traces.spans".to_owned(),
            HotParquetPlan::Follower {
                memory_pool: crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
            },
        );
        let loader = footers
            .loader(file_io, &task_context_with_batch_size(8))
            .expect("footer loader");
        let path = format!("{warehouse}/data.parquet");

        let first = loader.load(&path, size).await.expect("first footer load");
        let second = loader.load(&path, size).await.expect("cached footer load");
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.num_row_groups(), 1);
        assert_eq!(
            loader
                .retained
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            2
        );

        let error = loader
            .load(&format!("{warehouse}/vanished.parquet"), size)
            .await
            .expect_err("a vanished object must not load");
        assert!(is_stale_iceberg_object_error(&iceberg_datafusion_error(
            error
        )));
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
        )
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
        let (left_source, right_source) = (&left_key, &right_key);
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
        let telemetry = Arc::new(OracleTelemetry::new());
        let pool = crate::resources::bounded_memory_pool(1024 * 1024 * 1024);
        let inputs = |assignments: HashMap<FollowerSourceKey, FollowerScanAssignment>| {
            OracleExecutionBindingInputs {
                grant: crate::oracle::bindings::OracleExecutionGrant::for_test(
                    QueryClass::Interactive,
                    oracle_memory_resources(&governor, 1024 * 1024),
                    Arc::clone(&telemetry),
                ),
                follower_assignments: assignments,
                live: None,
                degraded: Arc::default(),
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
            None,
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
        key: &FollowerSourceKey,
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
        left_key: &'a FollowerSourceKey,
        /// The second occurrence's key, as the retained plan carries it.
        right_key: &'a FollowerSourceKey,
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
        I: Fn(HashMap<FollowerSourceKey, FollowerScanAssignment>) -> OracleExecutionBindingInputs,
        A: Fn() -> HashMap<FollowerSourceKey, FollowerScanAssignment>,
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
    async fn assert_retained_root_binds_once(
        governor: &crate::resources::BifrostRoleResources,
        telemetry: &Arc<OracleTelemetry>,
    ) {
        // A whole retained root is planned on a sentinel planning runtime and
        // executed on a distinct admitted one. Planning must reach no row
        // source, and execution must charge the admitted pool alone.
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let (provider, _live) = projection_closure_provider(tenant, None, None).await;

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

        let bindings = OracleExecutionBindings::try_new(
            OracleExecutionBindingInputs {
                grant: crate::oracle::bindings::OracleExecutionGrant::for_test(
                    QueryClass::Interactive,
                    oracle_memory_resources(governor, 1024 * 1024),
                    Arc::clone(telemetry),
                ),
                follower_assignments: HashMap::new(),
                live: None,
                degraded: Arc::default(),
            },
            &[],
        )
        .expect("a root that planned no delegated source binds nothing");
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
            projection_closure_provider(tenant, None, Some(one_live_route(tenant))).await;
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
        let telemetry = Arc::new(OracleTelemetry::new());

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
            assert_hot_scan_baselines(&governor, &leader_pool);
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
        OracleTableProvider::try_new(OracleTableInputs {
            table: pruning_fixture_table(),
            storage: fixture_storage(),
            hot_files,
            context,
            table_name: "vala.traces.spans".to_owned(),
            remote: None,
            live: None,
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
                    "bifrost_oracle_file_pruning_total{{outcome=\"{outcome}\"}}"
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
        remote: Option<OracleRemoteSource>,
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
            storage: fixture_storage(),
            hot_files: Vec::new(),
            context,
            table_name: "vala.traces.spans".to_owned(),
            remote,
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

    /// One leader-owned closure governs every leaf, the placeholder, the
    /// provider-local filter, and the result.
    ///
    /// This is the production Interactive leader path for
    /// `SELECT duration_ms FROM ... WHERE status_code = 'STATUS_CODE_ERROR'`.
    /// The closure is `[duration_ms, status_code]`: the requested output and
    /// the predicate-only column that must survive to the provider-local
    /// filter. `unused_payload` is requested by nobody and must not appear in
    /// any leaf.
    ///
    /// # Panics
    /// Panics if provider construction, scan planning, or execution violates
    /// the closure contract this owner pins.
    #[tokio::test]
    async fn projected_leaf_union_preserves_predicate_columns() {
        use datafusion::execution::context::SessionContext;
        use datafusion::logical_expr::{col, lit};
        use datafusion::physical_plan::collect;
        use datafusion::physical_plan::filter::FilterExec;

        let tenant = wyrd_spec::DataTenantId::new_v7();
        let (provider, live) =
            projection_closure_provider(tenant, None, Some(one_live_route(tenant))).await;

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
        // The source union sits directly under the provider-local filter.
        let union = Arc::clone(filter.children()[0]);
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

        // The predicate resolves `status_code` against the closure, not against
        // the original full physical schema.
        let predicate_columns =
            datafusion::physical_expr::utils::collect_columns(filter.predicate())
                .into_iter()
                .map(|column| (column.name().to_string(), column.index()))
                .collect::<Vec<_>>();
        assert_eq!(predicate_columns, vec![("status_code".to_string(), 1)]);

        // One ERROR row and one OK row in; only the ERROR duration out. The
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
                datafusion::prelude::SessionConfig::new(),
                crate::resources::bounded_memory_pool(64 * 1024 * 1024),
                crate::oracle::bindings::OracleExecutionGrant::for_test(
                    QueryClass::Interactive,
                    oracle_memory_resources(
                        &oracle_test_roles(2 * 1024 * 1024 * 1024),
                        1024 * 1024,
                    ),
                    Arc::new(OracleTelemetry::new()),
                ),
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
