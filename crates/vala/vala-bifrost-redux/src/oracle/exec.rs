//! `DataFusion` physical sources and invariant operators owned by Oracle.
//!
//! Every table enters `DataFusion` through one complete tagged source union.
//! Tenant validation surrounds that union before exact identity reconciliation,
//! so a foreign row cannot influence a filter, join, aggregate, or limit.

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
use std::task::{Context, Poll};
use std::time::Instant;

use arrow::array::{Array, UInt8Array};
use arrow::compute::cast;
use arrow::compute::kernels::sort::SortOptions;
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::datasource::physical_plan::FileScanConfig;
use datafusion::datasource::source::DataSourceExec;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::TaskContext;
use datafusion::execution::memory_pool::MemoryPool;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_expr::{EquivalenceProperties, LexOrdering, PhysicalSortExpr};
use datafusion::physical_plan::execution_plan::{
    Boundedness, EmissionType, PlanProperties, SchedulingType,
};
use datafusion::physical_plan::metrics::{ExecutionPlanMetricsSet, MetricValue};
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::union::UnionExec;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, Partitioning,
    RecordBatchStream, SendableRecordBatchStream, execute_stream,
};
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use futures_util::{Stream, StreamExt, TryStreamExt};
use iceberg::arrow::ScanMetrics;
use iceberg::expr::Predicate;
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
    AuthorizedQueryContext, BifrostSecurityViolation, OracleAudit, OracleMemoryKind,
    OracleMemoryResources, OracleTelemetry, ReconcileError, RowIdentity, SourceTier,
    VerifiedSecurityContext, query_class_label,
};

/// Shared physical scan state retained by one executing source plan.
#[derive(Debug, Default)]
struct OracleScanMetricsHandle {
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
    fn record_hot_range(&self, bytes: usize) {
        self.hot_bytes
            .fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::Relaxed);
        self.hot_available.store(true, Ordering::Release);
    }

    /// Records one hot file delivered to the ranged Parquet reader.
    fn record_hot_file(&self) {
        self.hot_files.fetch_add(1, Ordering::Relaxed);
        self.hot_partitions.store(1, Ordering::Relaxed);
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
}

/// Records one pinned Iceberg task and returns it without altering its delete
/// metadata, schema, predicate, or byte range.
fn retain_iceberg_task(task: FileScanTask, metrics: &Arc<OracleScanMetricsHandle>) -> FileScanTask {
    metrics.record_iceberg_task();
    task
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
    /// Immutable-cut file sizes selected before physical execution.
    pub(crate) logical_bytes_selected: u64,
    /// Shared file-source metric sets retained until terminal stream drain.
    physical_metrics: Vec<ExecutionPlanMetricsSet>,
    /// Shared dependency counters retained until terminal stream drain.
    scan_handles: Vec<Arc<OracleScanMetricsHandle>>,
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
        for handle in &self.scan_handles {
            let (bytes, files, partitions) = handle.terminal_values();
            self.files_scanned = self.files_scanned.saturating_add(files);
            self.partitions_scanned = self.partitions_scanned.saturating_add(partitions);
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
            stats.scan_handles.push(Arc::clone(&source.metrics));
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
    /// Cached properties copied from the pinned source plan.
    properties: Arc<PlanProperties>,
    /// Shared terminal metric owner retained by query telemetry.
    metrics: Arc<OracleScanMetricsHandle>,
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
    fn from_plan(plan: &dyn ExecutionPlan) -> DataFusionResult<Self> {
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
            properties: Arc::clone(plan.properties()),
            metrics: Arc::new(OracleScanMetricsHandle::default()),
        })
    }

    /// Rebuilds the exact pinned Iceberg scan and starts its reader stream.
    ///
    /// # Errors
    ///
    /// Returns a typed `DataFusion` error when scan planning, task planning,
    /// reader construction, or object-store reads fail.
    async fn start_stream(&self) -> DataFusionResult<SendableRecordBatchStream> {
        let mut builder = self.table.scan();
        if let Some(snapshot_id) = self.snapshot_id {
            builder = builder.snapshot_id(snapshot_id);
        }
        builder = match &self.projection {
            Some(columns) => builder.select(columns.iter().cloned()),
            None => builder.select_all(),
        };
        if let Some(predicate) = &self.predicates {
            builder = builder.with_filter(predicate.clone());
        }
        let scan = builder
            .build()
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        let tasks = scan
            .plan_files()
            .await
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        let metrics = self
            .table
            .reader_builder()
            .build()
            .read(Box::pin(tasks.map_ok({
                let metrics = Arc::clone(&self.metrics);
                move |task| retain_iceberg_task(task, &metrics)
            })))
            .map_err(|error| DataFusionError::External(Box::new(error)))?;
        self.metrics.set_iceberg_metrics(metrics.metrics().clone());
        let stream = metrics
            .stream()
            .map(|result| result.map_err(|error| DataFusionError::External(Box::new(error))));
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

/// Hidden physical column carrying Oracle's closed source precedence.
const SOURCE_TIER_COLUMN: &str = "__wyrd_oracle_source_tier";
/// Record-batch target used while decoding one bounded hot Parquet file.
const HOT_BATCH_ROWS: usize = 8_192;

/// Poll-enclosing lifecycle for one Oracle source or reconciliation stream.
struct OracleStreamLifecycle<S> {
    /// Stream whose entire poll is enclosed by the exact production span.
    inner: Pin<Box<S>>,
    /// Schema forwarded through the `RecordBatchStream` contract.
    schema: SchemaRef,
    /// Exact span entered before every child poll.
    span: tracing::Span,
    /// Optional closed source label for source-operation telemetry.
    source: Option<&'static str>,
    /// Optional reconciliation sort and class used for actual terminal spill evidence.
    reconcile_sort: Option<(Arc<SortExec>, QueryClass)>,
    /// Monotonic start covering pending time and every poll.
    started: Instant,
    /// Whether a terminal outcome was already emitted.
    finished: bool,
}

impl<S> OracleStreamLifecycle<S> {
    /// Wrap one stream before its first poll.
    fn new(
        stream: S,
        schema: SchemaRef,
        span: tracing::Span,
        source: Option<&'static str>,
    ) -> Self {
        Self {
            inner: Box::pin(stream),
            schema,
            span,
            source,
            reconcile_sort: None,
            started: Instant::now(),
            finished: false,
        }
    }

    /// Wraps the reducer and retains its exact `SortExec` metric owner.
    fn new_reconciliation(
        stream: S,
        schema: SchemaRef,
        span: tracing::Span,
        sort: Arc<SortExec>,
        query_class: QueryClass,
    ) -> Self {
        Self {
            inner: Box::pin(stream),
            schema,
            span,
            source: None,
            reconcile_sort: Some((sort, query_class)),
            started: Instant::now(),
            finished: false,
        }
    }

    /// Emit one exact terminal outcome and disarm cancellation-on-drop.
    fn finish(&mut self, outcome: &'static str) {
        if self.finished {
            return;
        }
        self.span.record("outcome", outcome);
        if let Some(source) = self.source {
            metrics::histogram!("bifrost_oracle_source_operation_seconds", "source" => source, "outcome" => outcome)
                .record(self.started.elapsed().as_secs_f64());
        }
        if let Some((sort, query_class)) = &self.reconcile_sort {
            let sort_metrics = sort.metrics().unwrap_or_default();
            let class = query_class_label(*query_class);
            let query_outcome = if outcome == "failed" {
                "error"
            } else {
                outcome
            };
            let spilled_bytes =
                u64::try_from(sort_metrics.spilled_bytes().unwrap_or_default()).unwrap_or(u64::MAX);
            let spill_count =
                u64::try_from(sort_metrics.spill_count().unwrap_or_default()).unwrap_or(u64::MAX);
            metrics::counter!("oracle_query_spill_bytes_total", "class" => class)
                .increment(spilled_bytes);
            metrics::counter!(
                "oracle_query_spill_files_total",
                "class" => class
            )
            .increment(spill_count);
            metrics::counter!(
                "oracle_query_spill_queries_total",
                "class" => class,
                "outcome" => query_outcome
            )
            .increment(1);
        }
        self.finished = true;
    }
}

impl<S> Stream for OracleStreamLifecycle<S>
where
    S: Stream<Item = DataFusionResult<RecordBatch>>,
{
    type Item = DataFusionResult<RecordBatch>;

    /// Poll the complete child operation inside its exact production span.
    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let span = self.span.clone();
        let polled = span.in_scope(|| self.inner.as_mut().poll_next(context));
        match &polled {
            Poll::Ready(Some(Err(_))) => self.finish("failed"),
            Poll::Ready(None) => self.finish("success"),
            Poll::Pending | Poll::Ready(Some(Ok(_))) => {}
        }
        polled
    }
}

impl<S> RecordBatchStream for OracleStreamLifecycle<S>
where
    S: Stream<Item = DataFusionResult<RecordBatch>> + Send,
{
    /// Forward the child stream schema unchanged.
    fn schema(&self) -> SchemaRef {
        Arc::clone(&self.schema)
    }
}

impl<S> Drop for OracleStreamLifecycle<S> {
    /// Record cancellation when the consumer drops before exhaustion or error.
    fn drop(&mut self) {
        if !self.finished {
            self.finish("cancelled");
        }
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
    pub(crate) telemetry: Arc<OracleTelemetry>,
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
        })
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

    /// Reports no pushed filters because Oracle applies complete SQL semantics
    /// above its tenant-tripwired and reconciled table result.
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        Ok(vec![
            TableProviderFilterPushDown::Unsupported;
            filters.len()
        ])
    }

    /// Builds the exact `Iceberg + hot + live -> tripwire -> reconcile` source.
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
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let mut inputs: Vec<Arc<dyn ExecutionPlan>> = Vec::new();
        if let Some(batches) = &self.distributed_iceberg_batches {
            let published =
                Self::validated_memory_source(batches, Arc::clone(&self.physical_schema))?;
            inputs.push(Arc::new(SourceTagExec::new(
                published,
                SourceTier::Iceberg,
            )?));
        } else {
            let published = self.iceberg.scan(state, None, &[], None).await?;
            let published = Arc::new(OracleIcebergScanExec::from_plan(published.as_ref())?);
            inputs.push(Arc::new(SourceTagExec::new(
                published,
                SourceTier::Iceberg,
            )?));
        }
        if !self.hot_files.is_empty() {
            let hot = Arc::new(HotParquetExec::new(
                self.hot_files.clone(),
                self.file_io.clone(),
                Arc::clone(&self.physical_schema),
                HotParquetRuntime {
                    memory: self.memory.clone(),
                    memory_pool: Arc::clone(&self.query_pool),
                    telemetry: Arc::clone(&self.telemetry),
                    query_class: self.query_class,
                    metrics: Arc::new(OracleScanMetricsHandle::default()),
                },
            ));
            inputs.push(Arc::new(SourceTagExec::new(hot, SourceTier::HotSealed)?));
        }
        if !self.distributed_hot_batches.is_empty() {
            let hot = Self::validated_memory_source(
                &self.distributed_hot_batches,
                Arc::clone(&self.physical_schema),
            )?;
            inputs.push(Arc::new(SourceTagExec::new(hot, SourceTier::HotSealed)?));
        }
        if !self.live_batches.is_empty() {
            let live = Self::validated_memory_source(
                &self.live_batches,
                Arc::clone(&self.physical_schema),
            )?;
            inputs.push(Arc::new(SourceTagExec::new(live, SourceTier::Live)?));
        }
        let union = UnionExec::try_new(inputs)?;
        let tripwire = Arc::new(TenantTripwireExec::new(
            union,
            self.context.clone(),
            self.table.clone(),
            Arc::clone(&self.audit),
        )?);
        let reconciled = Arc::new(ReconcileExec::new_with_telemetry(
            tripwire,
            self.memory.clone(),
            Arc::clone(&self.telemetry),
            self.query_class,
        )?);
        project_plan(reconciled, projection)
    }
}

/// Adds the closed physical source tier to every batch from one source.
#[derive(Debug)]
struct SourceTagExec {
    /// Source plan receiving the hidden tag column.
    input: Arc<dyn ExecutionPlan>,
    /// Constant source tier for every yielded row.
    tier: SourceTier,
    /// Cached tagged output properties.
    properties: Arc<PlanProperties>,
}

impl SourceTagExec {
    /// Creates a source tagger over one schema-compatible source.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when the hidden tag field cannot be appended.
    fn new(input: Arc<dyn ExecutionPlan>, tier: SourceTier) -> DataFusionResult<Self> {
        let schema = schema_with_source(input.schema().as_ref())?;
        let partition_count = input.output_partitioning().partition_count();
        Ok(Self {
            input,
            tier,
            properties: plan_properties_with_partitions(schema, partition_count),
        })
    }
}

impl DisplayAs for SourceTagExec {
    /// Renders the closed source tier without paths or tenant identifiers.
    fn fmt_as(
        &self,
        _format: DisplayFormatType,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(formatter, "SourceTagExec tier={:?}", self.tier)
    }
}

impl ExecutionPlan for SourceTagExec {
    /// Returns the stable physical operator name.
    fn name(&self) -> &'static str {
        "SourceTagExec"
    }

    /// Exposes this concrete physical node for downcasts.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Returns cached bounded plan properties.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    /// Returns the single tagged child.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![&self.input]
    }

    /// Rebuilds the tagger around exactly one replacement child.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` planning error unless exactly one child is supplied.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let [input] = children
            .try_into()
            .map_err(|_| DataFusionError::Plan("SourceTagExec requires one child".to_owned()))?;
        Ok(Arc::new(Self::new(input, self.tier)?))
    }

    /// Appends one constant source tag array to each incoming batch.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error for an invalid partition, child
    /// stream failure, or Arrow batch construction failure.
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        let input = self.input.execute(partition, context)?;
        let schema = self.schema();
        let tier = self.tier.as_u8();
        let source = self.tier.label();
        let stream = input.map(move |batch| {
            let batch = batch?;
            let mut columns = batch.columns().to_vec();
            columns.push(Arc::new(UInt8Array::from_value(tier, batch.num_rows())));
            RecordBatch::try_new(Arc::clone(&schema), columns).map_err(DataFusionError::from)
        });
        let output_schema = self.schema();
        let span = tracing::info_span!(
            "bifrost.oracle.source",
            source,
            outcome = tracing::field::Empty
        );
        Ok(Box::pin(OracleStreamLifecycle::new(
            stream,
            output_schema,
            span,
            Some(source),
        )))
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

/// Exact identity reconciliation after tenant validation and before SQL operators.
#[derive(Debug)]
pub struct ReconcileExec {
    /// Identity-sorted, tenant-validated, source-tagged input.
    input: Arc<dyn ExecutionPlan>,
    /// Concrete sort owner retained for terminal spill metrics.
    sort: Arc<SortExec>,
    /// Parent governor and configured reconciliation ceiling.
    memory: OracleMemoryResources,
    /// Production telemetry and class when constructed by the retained Oracle.
    telemetry: Option<(Arc<OracleTelemetry>, QueryClass)>,
    /// Output properties after source bookkeeping is stripped.
    properties: Arc<PlanProperties>,
}

impl ReconcileExec {
    /// Creates the reconciliation operator over one validated table union.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error when source bookkeeping is absent.
    pub fn new(
        input: Arc<dyn ExecutionPlan>,
        memory: OracleMemoryResources,
    ) -> DataFusionResult<Self> {
        Self::new_inner(input, memory, None)
    }

    /// Creates production reconciliation with class-aware memory accounting.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error when source bookkeeping is absent.
    fn new_with_telemetry(
        input: Arc<dyn ExecutionPlan>,
        memory: OracleMemoryResources,
        telemetry: Arc<OracleTelemetry>,
        query_class: QueryClass,
    ) -> DataFusionResult<Self> {
        Self::new_inner(input, memory, Some((telemetry, query_class)))
    }

    /// Builds the common reconciliation plan shape with optional accounting.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error when source bookkeeping is absent.
    fn new_inner(
        input: Arc<dyn ExecutionPlan>,
        memory: OracleMemoryResources,
        telemetry: Option<(Arc<OracleTelemetry>, QueryClass)>,
    ) -> DataFusionResult<Self> {
        let schema = schema_without(&input.schema(), SOURCE_TIER_COLUMN)?;
        let ordering = identity_ordering(input.schema().as_ref())?;
        let sort = Arc::new(SortExec::new(ordering, input));
        Ok(Self {
            input: sort.clone(),
            sort,
            memory,
            telemetry,
            properties: plan_properties(schema),
        })
    }
}

/// Builds the mandatory ascending physical ordering for exact row identity.
///
/// Column indices are resolved by managed-column name so projections or schema
/// evolution cannot silently sort on the wrong physical positions.
///
/// # Errors
///
/// Returns the existing invalid-identity source when either managed identity
/// column is absent, or a plan error if the non-empty ordering cannot be built.
fn identity_ordering(schema: &Schema) -> DataFusionResult<LexOrdering> {
    let batch_index = schema
        .index_of("wyrd_batch_id")
        .map_err(|_| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?;
    let ordinal_index = schema
        .index_of("wyrd_row_ordinal")
        .map_err(|_| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?;
    LexOrdering::new(vec![
        PhysicalSortExpr::new(
            Arc::new(Column::new("wyrd_batch_id", batch_index)),
            SortOptions {
                descending: false,
                nulls_first: false,
            },
        ),
        PhysicalSortExpr::new(
            Arc::new(Column::new("wyrd_row_ordinal", ordinal_index)),
            SortOptions {
                descending: false,
                nulls_first: false,
            },
        ),
    ])
    .ok_or_else(|| DataFusionError::Plan("Oracle identity ordering cannot be empty".to_owned()))
}

impl DisplayAs for ReconcileExec {
    /// Renders the bounded exact-identity operator.
    fn fmt_as(
        &self,
        _format: DisplayFormatType,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(formatter, "ReconcileExec")
    }
}

impl ExecutionPlan for ReconcileExec {
    /// Returns the stable physical operator name.
    fn name(&self) -> &'static str {
        "ReconcileExec"
    }

    /// Exposes this concrete reconciliation node for downcasts.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Returns cached bounded plan properties.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    /// Returns the identity sort as the sole child.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![&self.input]
    }

    /// Rebuilds reconciliation and its identity sort around one replacement child.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error unless exactly one child is supplied.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let [replacement] = children
            .try_into()
            .map_err(|_| DataFusionError::Plan("ReconcileExec requires one child".to_owned()))?;
        let input = replacement
            .as_any()
            .downcast_ref::<SortExec>()
            .map_or_else(|| Arc::clone(&replacement), |sort| Arc::clone(sort.input()));
        Ok(Arc::new(Self::new_inner(
            input,
            self.memory.clone(),
            self.telemetry.clone(),
        )?))
    }

    /// Reduces consecutive sorted identities while retaining one winner.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error for invalid identities, unequal
    /// duplicates, or a sorted-source failure.
    fn execute(
        &self,
        partition: usize,
        task: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(format!(
                "ReconcileExec has no partition {partition}"
            )));
        }
        let mut input = execute_stream(Arc::clone(&self.input), task)?;
        let schema = self.schema();
        let reconcile_span = tracing::info_span!(
            "bifrost.oracle.reconcile",
            operator = "exact_identity",
            outcome = tracing::field::Empty
        );
        let stream = async_stream::try_stream! {
            let mut current: Option<CurrentIdentityWinner> = None;
            while let Some(batch) = input.next().await {
                let batch = batch?;
                for row in 0..batch.num_rows() {
                    let candidate = CurrentIdentityWinner::begin(&batch, row)?;
                    match current.as_mut() {
                        Some(winner) if winner.identity == candidate.identity => {
                            winner.reconcile(candidate)?;
                        }
                        Some(_) => {
                            let prior = current.take().expect("current winner exists in matched branch");
                            yield prior.finish()?;
                            current = Some(candidate);
                        }
                        None => {
                            current = Some(candidate);
                        }
                    }
                }
            }
            if let Some(winner) = current {
                yield winner.finish()?;
            }
        };
        let query_class = self
            .telemetry
            .as_ref()
            .map_or(QueryClass::Analytical, |(_, query_class)| *query_class);
        Ok(Box::pin(OracleStreamLifecycle::new_reconciliation(
            stream,
            schema,
            reconcile_span,
            Arc::clone(&self.sort),
            query_class,
        )))
    }
}

/// One retained winner for the current sorted identity.
struct CurrentIdentityWinner {
    /// Immutable identity shared by consecutive sorted duplicates.
    identity: RowIdentity,
    /// Winning source tier.
    tier: SourceTier,
    /// Shallow one-row batch retaining the source arrays.
    row: RecordBatch,
}

impl CurrentIdentityWinner {
    /// Validates and retains one row as the first winner for its identity.
    ///
    /// # Errors
    ///
    /// Returns the existing invalid-identity error for absent, null, malformed,
    /// negative, or unknown managed identity/source values.
    fn begin(batch: &RecordBatch, row: usize) -> DataFusionResult<Self> {
        let (identity, tier) = row_identity_and_tier(batch, row)?;
        Ok(Self {
            identity,
            tier,
            row: batch.slice(row, 1),
        })
    }

    /// Verifies one equal-identity duplicate and retains the highest-precedence row.
    ///
    /// # Errors
    ///
    /// Returns the reconciliation invariant when logical values differ.
    fn reconcile(&mut self, candidate: Self) -> DataFusionResult<()> {
        if !logical_rows_equal(&self.row, &candidate.row)? {
            metrics::counter!(
                "bifrost_oracle_security_events_total",
                "event_class" => "reconciliation"
            )
            .increment(1);
            return Err(DataFusionError::External(Box::new(
                BifrostError::QueryReconciliationInvariant,
            )));
        }
        let losing_source = if candidate.tier < self.tier {
            self.tier
        } else {
            candidate.tier
        };
        if candidate.tier < self.tier {
            self.tier = candidate.tier;
            self.row = candidate.row;
        }
        metrics::counter!(
            "bifrost_oracle_rows_deduplicated_total",
            "losing_source" => losing_source.label()
        )
        .increment(1);
        Ok(())
    }

    /// Removes the hidden source tier from the winning row.
    ///
    /// # Errors
    ///
    /// Returns a plan error if the internally required source tier is absent.
    fn finish(self) -> DataFusionResult<RecordBatch> {
        remove_column(&self.row, SOURCE_TIER_COLUMN)
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

/// Iceberg ranged storage adapted to Parquet with pre-IO Oracle accounting.
struct IcebergParquetReader {
    /// Pinned ranged reader for one immutable hot object.
    reader: Box<dyn FileRead>,
    /// Pinned manifest size used to reject invalid ranges.
    size: u64,
    /// Shared Oracle child capability used for every metadata and data range.
    memory_pool: Arc<dyn MemoryPool>,
    /// Pod governor poisoned if coupled telemetry diverges from pool ownership.
    resources: crate::resources::BifrostResourceGovernor,
    /// Shared physical scan counters retained to terminal query emission.
    metrics: Arc<OracleScanMetricsHandle>,
    /// Canonical Oracle memory telemetry coupled to range ownership.
    telemetry: Arc<OracleTelemetry>,
    /// Immutable query class used for range accounting.
    query_class: QueryClass,
    /// Deterministic range reader injected only by focused tests.
    #[cfg(test)]
    reader_override: Option<(String, HotReadOverride)>,
}

impl IcebergParquetReader {
    /// Creates a governed reader for one pinned immutable hot object.
    fn new(
        reader: Box<dyn FileRead>,
        size: u64,
        memory_pool: Arc<dyn MemoryPool>,
        resources: crate::resources::BifrostResourceGovernor,
        metrics: Arc<OracleScanMetricsHandle>,
        telemetry: Arc<OracleTelemetry>,
        query_class: QueryClass,
    ) -> Self {
        Self {
            reader,
            size,
            memory_pool,
            resources,
            metrics,
            telemetry,
            query_class,
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
            let reservation = crate::resources::OracleQueryMemoryReservation::try_new(
                &self.memory_pool,
                self.resources.clone(),
                "oracle-hot-range",
                requested,
            )
            .map_err(|error| ParquetError::External(Box::new(error)))?;
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
            Ok(bytes::Bytes::from_owner(AccountedRangeOwner::new(
                bytes,
                reservation,
                Arc::clone(&self.telemetry),
                self.query_class,
            )))
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
struct HotParquetExec {
    /// Validated immutable manifest entries.
    files: Vec<HotFileSource>,
    /// Pinned Iceberg storage reader.
    file_io: FileIO,
    /// Complete physical table schema.
    schema: SchemaRef,
    /// Shared governor used for each range and decoded source batch.
    memory: OracleMemoryResources,
    /// Query-local pool shared with `DataFusion` operators.
    memory_pool: Arc<dyn MemoryPool>,
    /// Production memory accounting shared with the retained Oracle.
    telemetry: Arc<OracleTelemetry>,
    /// Immutable admission class charged by source buffers.
    query_class: QueryClass,
    /// Shared terminal metric owner retained by query telemetry.
    metrics: Arc<OracleScanMetricsHandle>,
    /// Deterministic reader injected only by focused unit tests.
    #[cfg(test)]
    reader_override: Option<HotReadOverride>,
    /// Cached bounded leaf properties.
    properties: Arc<PlanProperties>,
}

/// Query-scoped dependencies shared by every hot Parquet source owner.
struct HotParquetRuntime {
    /// Pod allocator and compatibility memory handles used by range owners.
    memory: OracleMemoryResources,
    /// Shared query-local pool used by operators and source buffers.
    memory_pool: Arc<dyn MemoryPool>,
    /// Canonical Oracle memory telemetry owner.
    telemetry: Arc<OracleTelemetry>,
    /// Immutable scheduling class used only for telemetry labels.
    query_class: QueryClass,
    /// Terminal scan metrics retained through stream completion.
    metrics: Arc<OracleScanMetricsHandle>,
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
    fn new(
        files: Vec<HotFileSource>,
        file_io: FileIO,
        schema: SchemaRef,
        runtime: HotParquetRuntime,
    ) -> Self {
        Self {
            files,
            file_io,
            memory: runtime.memory,
            memory_pool: runtime.memory_pool,
            telemetry: runtime.telemetry,
            query_class: runtime.query_class,
            metrics: runtime.metrics,
            #[cfg(test)]
            reader_override: None,
            properties: plan_properties(Arc::clone(&schema)),
            schema,
        }
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
        _task: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(format!(
                "HotParquetExec has no partition {partition}"
            )));
        }
        let schema = Arc::clone(&self.schema);
        let stream = hot_stream(self);
        Ok(Box::pin(RecordBatchStreamAdapter::new(schema, stream)))
    }
}

/// Builds the hot-file stream after partition validation has completed.
fn hot_stream(
    exec: &HotParquetExec,
) -> impl Stream<Item = DataFusionResult<RecordBatch>> + Send + 'static {
    let files = exec.files.clone();
    let file_io = exec.file_io.clone();
    let schema = Arc::clone(&exec.schema);
    let memory = exec.memory.clone();
    let memory_pool = Arc::clone(&exec.memory_pool);
    let telemetry = Arc::clone(&exec.telemetry);
    let query_class = exec.query_class;
    let metrics = Arc::clone(&exec.metrics);
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
                Arc::clone(&memory_pool),
                memory.resources.governor(),
                Arc::clone(&metrics),
                Arc::clone(&telemetry),
                query_class,
            );
            #[cfg(test)]
            let reader = if let Some(override_reader) = reader_override.as_ref() {
                reader.with_test_reader(file.location.clone(), Arc::clone(override_reader))
            } else {
                reader
            };
            metrics.record_hot_file();
            let mut batches = ParquetRecordBatchStreamBuilder::new(reader)
                .await
                .map_err(|error| DataFusionError::External(Box::new(error)))?
                .with_batch_size(HOT_BATCH_ROWS)
                .build()
                .map_err(|error| DataFusionError::External(Box::new(error)))?;
            while let Some(decoded) = batches.next().await {
                let batch = decoded
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                let batch = project_batch(&batch, Arc::clone(&schema))?;
                let decoded_reservation = crate::resources::OracleQueryMemoryReservation::try_new(
                    &memory_pool,
                    memory.resources.governor(),
                    "oracle-hot-decoded-batch",
                    batch.get_array_memory_size(),
                )
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                let decoded_reservation = telemetry.account_query_memory(
                    decoded_reservation,
                    query_class,
                    OracleMemoryKind::Source,
                );
                yield batch;
                drop(decoded_reservation);
            }
        }
    }
}

/// Reads and validates the exact identity and source tier for one physical row.
///
/// # Errors
///
/// Returns the existing invalid-identity error for missing or incorrectly typed
/// columns, an out-of-range row, null values, a negative ordinal, or an unknown
/// source tier.
fn row_identity_and_tier(
    batch: &RecordBatch,
    row: usize,
) -> DataFusionResult<(RowIdentity, SourceTier)> {
    if row >= batch.num_rows() {
        return Err(DataFusionError::External(Box::new(
            ReconcileError::InvalidIdentity,
        )));
    }
    let batch_index = batch
        .schema()
        .index_of("wyrd_batch_id")
        .map_err(|_| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?;
    let ordinal_index = batch
        .schema()
        .index_of("wyrd_row_ordinal")
        .map_err(|_| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?;
    let source_index = batch
        .schema()
        .index_of(SOURCE_TIER_COLUMN)
        .map_err(|_| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?;
    let batches = batch
        .column(batch_index)
        .as_any()
        .downcast_ref::<arrow::array::FixedSizeBinaryArray>()
        .ok_or_else(|| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?;
    let ordinals = batch
        .column(ordinal_index)
        .as_any()
        .downcast_ref::<arrow::array::Int32Array>()
        .ok_or_else(|| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?;
    let sources = batch
        .column(source_index)
        .as_any()
        .downcast_ref::<UInt8Array>()
        .ok_or_else(|| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?;
    if batches.is_null(row)
        || ordinals.is_null(row)
        || ordinals.value(row) < 0
        || sources.is_null(row)
    {
        return Err(DataFusionError::External(Box::new(
            ReconcileError::InvalidIdentity,
        )));
    }
    let batch_id = batches
        .value(row)
        .try_into()
        .map_err(|_| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?;
    let identity = RowIdentity {
        batch_id,
        ordinal: u32::try_from(ordinals.value(row))
            .map_err(|_| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?,
    };
    let tier = SourceTier::from_u8(sources.value(row))
        .ok_or_else(|| DataFusionError::External(Box::new(ReconcileError::InvalidIdentity)))?;
    Ok((identity, tier))
}

/// Compares one logical row by Arrow values after removing source bookkeeping.
///
/// # Errors
///
/// Returns a `DataFusion` error when either row omits Oracle's source field.
fn logical_rows_equal(left: &RecordBatch, right: &RecordBatch) -> DataFusionResult<bool> {
    let left = remove_column(left, SOURCE_TIER_COLUMN)?;
    let right = remove_column(right, SOURCE_TIER_COLUMN)?;
    Ok(left.schema() == right.schema()
        && left
            .columns()
            .iter()
            .zip(right.columns())
            .all(|(left, right)| left.to_data() == right.to_data()))
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

/// Appends Oracle's source bookkeeping field to one physical schema.
///
/// # Errors
///
/// Returns a `DataFusion` plan error if the reserved private name already exists.
fn schema_with_source(schema: &Schema) -> DataFusionResult<SchemaRef> {
    if schema.index_of(SOURCE_TIER_COLUMN).is_ok() {
        return Err(DataFusionError::Plan(
            "physical schema already contains Oracle source bookkeeping".to_owned(),
        ));
    }
    let mut fields = schema
        .fields()
        .iter()
        .map(|field| field.as_ref().clone())
        .collect::<Vec<_>>();
    fields.push(Field::new(SOURCE_TIER_COLUMN, DataType::UInt8, false));
    Ok(Arc::new(Schema::new_with_metadata(
        fields,
        schema.metadata().clone(),
    )))
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

impl SourceTier {
    /// Returns the physical precedence tag encoded into source batches.
    const fn as_u8(self) -> u8 {
        match self {
            Self::Iceberg => 0,
            Self::HotSealed => 1,
            Self::Live => 2,
        }
    }

    /// Parses one physical precedence tag.
    const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Iceberg),
            1 => Some(Self::HotSealed),
            2 => Some(Self::Live),
            _ => None,
        }
    }

    /// Returns the closed telemetry label for a losing source.
    const fn label(self) -> &'static str {
        match self {
            Self::Iceberg => "iceberg",
            Self::HotSealed => "hot_sealed",
            Self::Live => "live_tail",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::Duration;

    use super::*;
    use crate::oracle::{BifrostQueryReadDecision, OracleSlotManager};
    use arrow::array::{ArrayRef, FixedSizeBinaryBuilder, Int32Array, Int64Array, StringArray};
    use async_trait::async_trait;
    use datafusion::physical_expr::expressions::Column;
    use datafusion::physical_expr::{LexOrdering, PhysicalSortExpr};
    use datafusion::physical_plan::sorts::sort::SortExec;
    use datafusion::physical_plan::union::UnionExec;
    use datafusion::prelude::SessionConfig;
    use wyrd_runtime::Principal;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::AuthMethod;
    use wyrd_spec::vala::api::QueryStreamFrame;

    /// Minimal single-thread subscriber exposing the currently entered span.
    #[derive(Default)]
    struct PollCaptureSubscriber {
        /// Monotonic span identifier source.
        next: AtomicU64,
        /// Metadata retained for each live test span.
        metadata: Mutex<HashMap<u64, &'static tracing::Metadata<'static>>>,
        /// Entered span stack for the manually polled test thread.
        entered: Mutex<Vec<tracing::span::Id>>,
        /// Span names observed on subscriber entry around child polls.
        observed: Arc<Mutex<Vec<String>>>,
        /// Exact field updates retained with their owning span name.
        records: Arc<Mutex<Vec<(String, String, String)>>>,
    }

    /// Visitor retaining exact field values recorded on one Oracle span.
    struct SpanFieldVisitor<'a> {
        /// Owning span name resolved from the subscriber registry.
        span: &'a str,
        /// Shared terminal-record sink.
        records: &'a Mutex<Vec<(String, String, String)>>,
    }

    impl tracing::field::Visit for SpanFieldVisitor<'_> {
        /// Retain string fields without debug quoting.
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.records.lock().expect("span records").push((
                self.span.to_owned(),
                field.name().to_owned(),
                value.to_owned(),
            ));
        }

        /// Retain non-string fields using tracing's canonical debug representation.
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.records.lock().expect("span records").push((
                self.span.to_owned(),
                field.name().to_owned(),
                format!("{value:?}"),
            ));
        }
    }

    impl tracing::Subscriber for PollCaptureSubscriber {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, attributes: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
            self.metadata
                .lock()
                .expect("span metadata")
                .insert(id, attributes.metadata());
            tracing::span::Id::from_u64(id)
        }
        fn record(&self, id: &tracing::span::Id, record: &tracing::span::Record<'_>) {
            let span = self.metadata.lock().expect("span metadata")[&id.into_u64()]
                .name()
                .to_owned();
            record.record(&mut SpanFieldVisitor {
                span: &span,
                records: &self.records,
            });
        }
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, _: &tracing::Event<'_>) {}
        fn enter(&self, id: &tracing::span::Id) {
            let name = self.metadata.lock().expect("span metadata")[&id.into_u64()]
                .name()
                .to_owned();
            self.observed.lock().expect("observed spans").push(name);
            self.entered.lock().expect("entered spans").push(id.clone());
        }
        fn exit(&self, id: &tracing::span::Id) {
            let popped = self.entered.lock().expect("entered spans").pop();
            assert_eq!(popped.as_ref(), Some(id));
        }
    }

    /// Deterministic child stream that exposes one pending poll before its terminal result.
    struct PendingChild {
        /// Child schema forwarded by the lifecycle wrapper.
        schema: SchemaRef,
        /// Terminal item returned after the first pending poll.
        item: Option<DataFusionResult<RecordBatch>>,
        /// Whether the deliberate pending poll already occurred.
        pending_observed: bool,
        /// Test-controlled terminal release.
        ready: Arc<AtomicBool>,
    }

    impl Stream for PendingChild {
        type Item = DataFusionResult<RecordBatch>;

        /// Return one pending poll, then the configured item, then EOF.
        fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            if !self.pending_observed || !self.ready.load(Ordering::Acquire) {
                self.pending_observed = true;
                return Poll::Pending;
            }
            Poll::Ready(self.item.take())
        }
    }

    impl RecordBatchStream for PendingChild {
        /// Forward the deterministic child schema.
        fn schema(&self) -> SchemaRef {
            Arc::clone(&self.schema)
        }
    }

    /// Poll one lifecycle manually without timing-dependent synchronization.
    fn poll_lifecycle<S: Stream<Item = DataFusionResult<RecordBatch>>>(
        lifecycle: Pin<&mut OracleStreamLifecycle<S>>,
    ) -> Poll<Option<DataFusionResult<RecordBatch>>> {
        let waker = futures_util::task::noop_waker();
        lifecycle.poll_next(&mut Context::from_waker(&waker))
    }

    /// Assert one exact terminal outcome was recorded on a named Oracle span.
    fn assert_span_outcome(
        records: &Mutex<Vec<(String, String, String)>>,
        span: &str,
        outcome: &str,
    ) {
        assert!(
            records.lock().expect("span records").iter().any(
                |record| record == &(span.to_owned(), "outcome".to_owned(), outcome.to_owned())
            ),
            "missing {span} outcome={outcome}"
        );
    }

    /// Pending source and reconcile children record exact success spans and elapsed work.
    #[test]
    fn oracle_stream_lifecycle_records_pending_source_and_reconcile_success() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let records = Arc::new(Mutex::new(Vec::new()));
        let subscriber = PollCaptureSubscriber {
            observed: Arc::clone(&observed),
            records: Arc::clone(&records),
            ..PollCaptureSubscriber::default()
        };
        tracing::subscriber::with_default(subscriber, || {
            metrics::with_local_recorder(&recorder, || {
                let schema = Arc::new(Schema::empty());
                let batch = RecordBatch::new_empty(Arc::clone(&schema));
                let source_ready = Arc::new(AtomicBool::new(false));
                let mut successful = Box::pin(OracleStreamLifecycle::new(
                    PendingChild {
                        schema: Arc::clone(&schema),
                        item: Some(Ok(batch)),
                        pending_observed: false,
                        ready: Arc::clone(&source_ready),
                    },
                    Arc::clone(&schema),
                    tracing::info_span!("bifrost.oracle.source", outcome = tracing::field::Empty),
                    Some("live_tail"),
                ));
                assert!(matches!(poll_lifecycle(successful.as_mut()), Poll::Pending));
                source_ready.store(true, Ordering::Release);
                assert!(matches!(
                    poll_lifecycle(successful.as_mut()),
                    Poll::Ready(Some(Ok(_)))
                ));
                assert!(matches!(
                    poll_lifecycle(successful.as_mut()),
                    Poll::Ready(None)
                ));

                let reconcile_ready = Arc::new(AtomicBool::new(false));
                let mut reconciled = Box::pin(OracleStreamLifecycle::new(
                    PendingChild {
                        schema: Arc::clone(&schema),
                        item: None,
                        pending_observed: false,
                        ready: Arc::clone(&reconcile_ready),
                    },
                    Arc::clone(&schema),
                    tracing::info_span!(
                        "bifrost.oracle.reconcile",
                        outcome = tracing::field::Empty
                    ),
                    None,
                ));
                assert!(matches!(poll_lifecycle(reconciled.as_mut()), Poll::Pending));
                reconcile_ready.store(true, Ordering::Release);
                assert!(matches!(
                    poll_lifecycle(reconciled.as_mut()),
                    Poll::Ready(None)
                ));
            });
        });
        let observed = observed.lock().expect("observed poll spans");
        assert!(observed.iter().any(|name| name == "bifrost.oracle.source"));
        assert!(
            observed
                .iter()
                .any(|name| name == "bifrost.oracle.reconcile")
        );
        assert_span_outcome(&records, "bifrost.oracle.source", "success");
        assert_span_outcome(&records, "bifrost.oracle.reconcile", "success");
        let snapshot = recorder.snapshot();
        assert!(
            snapshot
                .histograms
                .get("bifrost_oracle_source_operation_seconds{outcome=\"success\",source=\"live_tail\"}")
                .is_some_and(|value| value.max > 0)
        );
    }

    /// A pending child failure records the exact span outcome and preserves its error.
    #[test]
    fn oracle_stream_lifecycle_records_failed_child() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let records = Arc::new(Mutex::new(Vec::new()));
        let subscriber = PollCaptureSubscriber {
            records: Arc::clone(&records),
            ..PollCaptureSubscriber::default()
        };
        tracing::subscriber::with_default(subscriber, || {
            metrics::with_local_recorder(&recorder, || {
                let schema = Arc::new(Schema::empty());
                let ready = Arc::new(AtomicBool::new(false));
                let mut failed = Box::pin(OracleStreamLifecycle::new(
                    PendingChild {
                        schema: Arc::clone(&schema),
                        item: Some(Err(DataFusionError::Execution("child failure".to_owned()))),
                        pending_observed: false,
                        ready: Arc::clone(&ready),
                    },
                    schema,
                    tracing::info_span!("bifrost.oracle.source", outcome = tracing::field::Empty),
                    Some("hot_sealed"),
                ));
                assert!(matches!(poll_lifecycle(failed.as_mut()), Poll::Pending));
                ready.store(true, Ordering::Release);
                let error = match poll_lifecycle(failed.as_mut()) {
                    Poll::Ready(Some(Err(error))) => error,
                    other => panic!("expected unchanged child error, got {other:?}"),
                };
                assert_eq!(error.to_string(), "Execution error: child failure");
            });
        });
        assert_span_outcome(&records, "bifrost.oracle.source", "failed");
        assert_eq!(
            recorder
                .snapshot()
                .histograms
                .get("bifrost_oracle_source_operation_seconds{outcome=\"failed\",source=\"hot_sealed\"}")
                .map(|value| value.count),
            Some(1)
        );
    }

    /// Dropping a polled pending child records the exact cancelled span outcome.
    #[test]
    fn oracle_stream_lifecycle_records_cancelled_child() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let records = Arc::new(Mutex::new(Vec::new()));
        let subscriber = PollCaptureSubscriber {
            records: Arc::clone(&records),
            ..PollCaptureSubscriber::default()
        };
        tracing::subscriber::with_default(subscriber, || {
            metrics::with_local_recorder(&recorder, || {
                let schema = Arc::new(Schema::empty());
                let mut cancelled = Box::pin(OracleStreamLifecycle::new(
                    PendingChild {
                        schema: Arc::clone(&schema),
                        item: None,
                        pending_observed: false,
                        ready: Arc::new(AtomicBool::new(false)),
                    },
                    schema,
                    tracing::info_span!("bifrost.oracle.source", outcome = tracing::field::Empty),
                    Some("iceberg"),
                ));
                assert!(matches!(poll_lifecycle(cancelled.as_mut()), Poll::Pending));
                drop(cancelled);
            });
        });
        assert_span_outcome(&records, "bifrost.oracle.source", "cancelled");
        assert_eq!(
            recorder
                .snapshot()
                .histograms
                .get(
                    "bifrost_oracle_source_operation_seconds{outcome=\"cancelled\",source=\"iceberg\"}",
                )
                .map(|value| value.count),
            Some(1)
        );
    }

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

    /// Builds one source-tagged physical row for exact reconciliation tests.
    fn tagged_batch(batch_id: [u8; 16], ordinal: i32, value: i64, tier: SourceTier) -> RecordBatch {
        let mut ids = FixedSizeBinaryBuilder::with_capacity(1, 16);
        ids.append_value(batch_id)
            .expect("test identity has fixed width");
        let schema = Arc::new(Schema::new(vec![
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("wyrd_row_ordinal", DataType::Int32, false),
            Field::new("data_tenant_id", DataType::Utf8, false),
            Field::new("value", DataType::Int64, false),
            Field::new(SOURCE_TIER_COLUMN, DataType::UInt8, false),
        ]));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(ids.finish()) as ArrayRef,
                Arc::new(Int32Array::from(vec![ordinal])) as ArrayRef,
                Arc::new(StringArray::from(vec![uuid::Uuid::nil().to_string()])) as ArrayRef,
                Arc::new(Int64Array::from(vec![value])) as ArrayRef,
                Arc::new(UInt8Array::from(vec![tier.as_u8()])) as ArrayRef,
            ],
        )
        .expect("test batch matches its schema")
    }

    /// Builds one bounded multi-row source batch for external-sort tests.
    fn tagged_rows(
        batch_id: [u8; 16],
        ordinals: std::ops::Range<i32>,
        tier: SourceTier,
    ) -> RecordBatch {
        let count = usize::try_from(ordinals.end - ordinals.start).expect("non-negative range");
        let mut ids = FixedSizeBinaryBuilder::with_capacity(count, 16);
        let mut ordinal_values = Vec::with_capacity(count);
        let mut values = Vec::with_capacity(count);
        let mut tenants = Vec::with_capacity(count);
        for ordinal in ordinals {
            ids.append_value(batch_id)
                .expect("test identity has fixed width");
            ordinal_values.push(ordinal);
            values.push(i64::from(ordinal));
            tenants.push(uuid::Uuid::nil().to_string());
        }
        let schema = tagged_batch(batch_id, 0, 0, tier).schema();
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(ids.finish()) as ArrayRef,
                Arc::new(Int32Array::from(ordinal_values)) as ArrayRef,
                Arc::new(StringArray::from(tenants)) as ArrayRef,
                Arc::new(Int64Array::from(values)) as ArrayRef,
                Arc::new(UInt8Array::from(vec![tier.as_u8(); count])) as ArrayRef,
            ],
        )
        .expect("multi-row test batch matches its schema")
    }

    /// Creates the production Oracle `DataFusion` pool with a deterministic test ceiling.
    fn reconciliation_test_pool() -> (
        crate::scribe::memory::BifrostMemoryGovernor,
        Arc<dyn datafusion::execution::memory_pool::MemoryPool>,
    ) {
        let governor = crate::scribe::memory::BifrostMemoryGovernor::new_with_test_child_limits(
            4 * 1024 * 1024 * 1024,
            256 << 20,
            3 << 20,
        )
        .expect("test governor");
        let pool = Arc::new(
            crate::scribe::memory::BifrostDataFusionMemoryPool::for_oracle(governor.clone()),
        );
        (governor, pool)
    }

    /// Executes source-tagged batches through the real sorted reconciliation plan.
    ///
    /// # Errors
    ///
    /// Returns the production plan or stream error unchanged.
    async fn run_sorted_reconciliation(
        batches: Vec<RecordBatch>,
    ) -> DataFusionResult<Vec<RecordBatch>> {
        let source = MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&batches),
            batches[0].schema(),
            None,
        )?;
        let plan = ReconcileExec::new(
            source,
            OracleMemoryResources {
                resources: crate::resources::BifrostRuntimeResources::composed_for_test(
                    1024 * 1024 * 1024,
                    1024 * 1024 * 1024,
                    [crate::resources::BifrostRole::Oracle],
                )
                .oracle()
                .expect("composition must enable the Oracle capability"),
                governor: crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
                    .map_err(|error| DataFusionError::External(Box::new(error)))?,
                reconciliation_limit_bytes: 1024 * 1024,
            },
        )?;
        let mut stream = plan.execute(
            0,
            datafusion::execution::context::SessionContext::new().task_ctx(),
        )?;
        let mut output = Vec::new();
        while let Some(batch) = stream.next().await {
            output.push(batch?);
        }
        Ok(output)
    }

    /// Exact source precedence is independent of source arrival order.
    #[tokio::test]
    async fn reconciliation_precedence_is_arrival_order_independent() {
        let identity = [7_u8; 16];
        for order in [
            [SourceTier::Live, SourceTier::HotSealed, SourceTier::Iceberg],
            [SourceTier::Iceberg, SourceTier::Live, SourceTier::HotSealed],
            [SourceTier::HotSealed, SourceTier::Iceberg, SourceTier::Live],
        ] {
            let recorder = wyrd_bench::BenchmarkRecorder::default();
            let _guard = metrics::set_default_local_recorder(&recorder);
            let output = run_sorted_reconciliation(
                order
                    .into_iter()
                    .map(|tier| tagged_batch(identity, 3, 41, tier))
                    .collect(),
            )
            .await
            .expect("equal duplicates reconcile through production plan");
            assert_eq!(output.len(), 1);
            assert_eq!(winner_key(&output[0]).1, 41);
            let snapshot = recorder.snapshot();
            assert_eq!(
                snapshot
                    .counters
                    .get("bifrost_oracle_rows_deduplicated_total{losing_source=\"live_tail\"}"),
                Some(&1)
            );
            assert_eq!(
                snapshot
                    .counters
                    .get("bifrost_oracle_rows_deduplicated_total{losing_source=\"hot_sealed\"}"),
                Some(&1)
            );
            assert!(
                !snapshot.counters.contains_key(
                    "bifrost_oracle_rows_deduplicated_total{losing_source=\"iceberg\"}"
                ),
                "the highest-precedence Iceberg row must never be the loser"
            );
        }
    }

    /// Unequal logical values for one immutable identity fail closed.
    #[tokio::test]
    async fn reconciliation_rejects_unequal_duplicates() {
        let identity = [9_u8; 16];
        let error = run_sorted_reconciliation(vec![
            tagged_batch(identity, 0, 1, SourceTier::HotSealed),
            tagged_batch(identity, 0, 2, SourceTier::Iceberg),
        ])
        .await
        .expect_err("unequal duplicate must fail");
        let mut source: &(dyn std::error::Error + 'static) = &error;
        let recovered = loop {
            if let Some(bifrost) = source.downcast_ref::<BifrostError>() {
                break bifrost;
            }
            source = source
                .source()
                .expect("reconciliation error retains its typed source");
        };
        assert!(matches!(
            recovered,
            BifrostError::QueryReconciliationInvariant
        ));
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

    /// Keeps tenant tripwire and reconciliation below a global sort operator.
    #[test]
    fn source_invariants_remain_below_global_operator() {
        let batch = tagged_batch([7_u8; 16], 0, 41, SourceTier::Iceberg);
        let source: Arc<dyn ExecutionPlan> = MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&vec![batch]),
            tagged_batch([0_u8; 16], 0, 0, SourceTier::Iceberg).schema(),
            None,
        )
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
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let memory = OracleMemoryResources {
            resources: crate::resources::BifrostRuntimeResources::composed_for_test(
                1024 * 1024 * 1024,
                1024 * 1024 * 1024,
                [crate::resources::BifrostRole::Oracle],
            )
            .oracle()
            .expect("composition must enable the Oracle capability"),
            governor: crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
                .expect("memory governor"),
            reconciliation_limit_bytes: 1024 * 1024,
        };
        let reconciled = Arc::new(
            ReconcileExec::new_with_telemetry(tripwire, memory, telemetry, QueryClass::Interactive)
                .expect("reconcile plan"),
        );
        let ordering = LexOrdering::new(vec![PhysicalSortExpr::new(
            Arc::new(Column::new("value", 3)),
            arrow::compute::SortOptions {
                descending: false,
                nulls_first: false,
            },
        )])
        .expect("sort ordering");
        let global = SortExec::new(ordering, reconciled.clone());
        assert_eq!(global.children()[0].name(), "ReconcileExec");
        assert_eq!(global.children()[0].children()[0].name(), "SortExec");
        assert_eq!(
            global.children()[0].children()[0].children()[0].name(),
            "TenantTripwireExec"
        );
        assert_eq!(
            global.children()[0].children()[0].children()[0].children()[0].name(),
            "UnionExec"
        );

        let rebuilt = reconciled
            .clone()
            .with_new_children(vec![Arc::clone(reconciled.children()[0])])
            .expect("optimizer replacement rebuilds reconciliation");
        let rebuilt = rebuilt
            .as_any()
            .downcast_ref::<ReconcileExec>()
            .expect("replacement preserves reconciliation owner");
        let executed_sort = rebuilt.children()[0]
            .as_any()
            .downcast_ref::<SortExec>()
            .expect("replacement retains one mandatory identity sort");
        assert!(std::ptr::eq(executed_sort, rebuilt.sort.as_ref()));
        assert_eq!(executed_sort.children()[0].name(), "TenantTripwireExec");
        assert_eq!(
            executed_sort.children()[0].children()[0].name(),
            "UnionExec"
        );
        assert!(
            executed_sort.children()[0]
                .as_any()
                .downcast_ref::<SortExec>()
                .is_none(),
            "optimizer rebuild must not nest a second mandatory sort"
        );
    }

    /// Builds one canonical UUIDv7-shaped identity for sorted reconciliation tests.
    fn v7_style_id(tail: u8) -> [u8; 16] {
        let mut id = [
            0x01, 0x93, 0x8a, 0x4c, 0x2f, 0x10, 0x70, 0x00, 0x80, 0, 0, 0, 0, 0, 0, 0,
        ];
        id[7] = tail.wrapping_mul(31).wrapping_add(7);
        id[9] = tail.wrapping_mul(97);
        id[15] = tail;
        id
    }

    /// Extracts the exact identity and value of one reconciled output row.
    fn winner_key(batch: &RecordBatch) -> (RowIdentity, i64) {
        let ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::FixedSizeBinaryArray>()
            .expect("output batch id column");
        let ordinals = batch
            .column(1)
            .as_any()
            .downcast_ref::<Int32Array>()
            .expect("output ordinal column");
        let values = batch
            .column(3)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("output value column");
        (
            RowIdentity {
                batch_id: ids.value(0).try_into().expect("16-byte batch id"),
                ordinal: u32::try_from(ordinals.value(0)).expect("non-negative ordinal"),
            },
            values.value(0),
        )
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
        governor: &crate::scribe::memory::BifrostMemoryGovernor,
        query_pool: &Arc<dyn MemoryPool>,
        telemetry: &OracleTelemetry,
    ) {
        assert_eq!(governor.snapshot().oracle_total_bytes, 0);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 0);
        assert_eq!(query_pool.reserved(), 0);
        assert_eq!(telemetry.memory_bytes.load(Ordering::Acquire), 0);
    }

    /// Asserts one analytical query recorded nonzero spill bytes and files once.
    ///
    /// The spill counters are the operator-visible evidence that the query
    /// actually reached disk under its own lease, so a successful spilling
    /// query must publish positive bytes, positive files, and exactly one
    /// successful spilling-query observation.
    fn assert_analytical_spill_success_counters(snapshot: &wyrd_bench::BenchmarkMetricSnapshot) {
        assert!(
            snapshot
                .counters
                .get("oracle_query_spill_bytes_total{class=\"analytical\"}")
                .is_some_and(|bytes| *bytes > 0)
        );
        assert!(
            snapshot
                .counters
                .get("oracle_query_spill_files_total{class=\"analytical\"}")
                .is_some_and(|files| *files > 0)
        );
        assert_eq!(
            snapshot
                .counters
                .get("oracle_query_spill_queries_total{class=\"analytical\",outcome=\"success\"}"),
            Some(&1)
        );
    }

    /// Composes one Oracle memory capability over the given governor.
    ///
    /// Every exec-level proof leases through the one production composition, so
    /// this helper only injects a raw observation and pairs the issued Oracle
    /// capability with the governor and reconciliation ceiling under test. It
    /// derives no reserve, floor, elastic, or partition value itself.
    fn oracle_memory_resources(
        governor: crate::scribe::memory::BifrostMemoryGovernor,
        reconciliation_limit_bytes: usize,
    ) -> OracleMemoryResources {
        OracleMemoryResources {
            resources: crate::resources::BifrostRuntimeResources::composed_for_test(
                1024 * 1024 * 1024,
                1024 * 1024 * 1024,
                [crate::resources::BifrostRole::Oracle],
            )
            .oracle()
            .expect("composition must enable the Oracle capability"),
            governor,
            reconciliation_limit_bytes,
        }
    }

    /// `DataFusion` external sort spills and yields the exact reference winners.
    #[tokio::test]
    async fn reconciliation_external_sort_spills_with_exact_winners() {
        const BATCHES: i32 = 200;
        const ROWS_PER_BATCH: i32 = 100;
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let identity = v7_style_id(5);
        let batches = (0..BATCHES)
            .rev()
            .flat_map(|batch| {
                let start = batch * ROWS_PER_BATCH;
                [
                    tagged_rows(identity, start..start + ROWS_PER_BATCH, SourceTier::Live),
                    tagged_rows(identity, start..start + ROWS_PER_BATCH, SourceTier::Iceberg),
                ]
            })
            .collect::<Vec<_>>();
        let source = MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&batches),
            batches[0].schema(),
            None,
        )
        .expect("memory source");
        let (governor, pool) = reconciliation_test_pool();
        let plan = ReconcileExec::new(
            source,
            oracle_memory_resources(governor.clone(), 1024 * 1024),
        )
        .expect("reconciliation plan");
        let scratch_root = tempfile::tempdir().expect("scratch root");
        let spill = crate::oracle::spill::OracleSpillRuntime::new(scratch_root.path(), 16 << 20)
            .expect("spill runtime");
        let runtime = spill
            .build_query_runtime(pool, 16 << 20)
            .expect("query runtime");
        let peak_memory = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let peak_disk = Arc::new(AtomicU64::new(0));
        let sampling = Arc::new(AtomicBool::new(true));
        let sampler = tokio::spawn({
            let governor = governor.clone();
            let runtime = Arc::clone(&runtime);
            let peak_memory = Arc::clone(&peak_memory);
            let peak_disk = Arc::clone(&peak_disk);
            let sampling = Arc::clone(&sampling);
            async move {
                while sampling.load(Ordering::Acquire) {
                    peak_memory.fetch_max(governor.snapshot().oracle_total_bytes, Ordering::AcqRel);
                    peak_disk
                        .fetch_max(runtime.spilling_progress().current_bytes, Ordering::AcqRel);
                    tokio::task::yield_now().await;
                }
            }
        });
        let session = datafusion::execution::context::SessionContext::new_with_config_rt(
            SessionConfig::new().with_sort_spill_reservation_bytes(0),
            runtime,
        );
        let mut stream = plan
            .execute(0, session.task_ctx())
            .expect("external sort executes");
        let mut ordinals = Vec::new();
        while let Some(batch) = stream.next().await {
            let batch = batch.expect("external sort reconciles");
            ordinals.push(winner_key(&batch).0.ordinal);
        }
        sampling.store(false, Ordering::Release);
        sampler.await.expect("peak sampler joins");
        drop(stream);
        assert_eq!(
            ordinals.len(),
            usize::try_from(BATCHES * ROWS_PER_BATCH).expect("row count")
        );
        assert!(ordinals.windows(2).all(|pair| pair[0] < pair[1]));
        let metrics = plan.sort.metrics().expect("sort metrics");
        assert!(metrics.spilled_bytes().unwrap_or_default() > 0);
        assert!(metrics.spill_count().unwrap_or_default() > 0);
        assert!(peak_memory.load(Ordering::Acquire) > 0);
        assert!(peak_memory.load(Ordering::Acquire) <= governor.oracle_limit_bytes());
        assert!(peak_disk.load(Ordering::Acquire) > 0);
        assert!(peak_disk.load(Ordering::Acquire) <= 16 << 20);
        assert_analytical_spill_success_counters(&recorder.snapshot());
        assert_eq!(session.runtime_env().memory_pool.reserved(), 0);
        drop(session);
        assert!(
            std::fs::read_dir(spill.spill_path())
                .expect("inspect scratch")
                .next()
                .is_none(),
            "completed sort removes every query spill file"
        );
    }

    /// A one-byte disk ceiling fails structurally and releases sort memory/files.
    #[tokio::test]
    async fn reconciliation_disk_ceiling_fails_typed_and_cleans() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let identity = v7_style_id(6);
        let batches = (0..400)
            .rev()
            .map(|batch| {
                let start = batch * 100;
                tagged_rows(identity, start..start + 100, SourceTier::Live)
            })
            .collect::<Vec<_>>();
        let source = MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&batches),
            batches[0].schema(),
            None,
        )
        .expect("memory source");
        let (governor, pool) = reconciliation_test_pool();
        let baseline = governor.snapshot();
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let plan = ReconcileExec::new_with_telemetry(
            source,
            oracle_memory_resources(governor.clone(), 1024 * 1024),
            Arc::clone(&telemetry),
            QueryClass::Analytical,
        )
        .expect("reconciliation plan");
        let scratch_root = tempfile::tempdir().expect("scratch root");
        let spill = crate::oracle::spill::OracleSpillRuntime::new(scratch_root.path(), 16 << 20)
            .expect("spill runtime");
        let runtime = spill.build_query_runtime(pool, 1).expect("query runtime");
        let session = datafusion::execution::context::SessionContext::new_with_config_rt(
            SessionConfig::new().with_sort_spill_reservation_bytes(0),
            runtime,
        );
        let mut stream = plan
            .execute(0, session.task_ctx())
            .expect("sort starts before disk exhaustion");
        let error = loop {
            match stream.next().await {
                Some(Err(error)) => break error,
                Some(Ok(_)) => {}
                None => panic!("one-byte disk ceiling must fail"),
            }
        };
        assert!(matches!(error, DataFusionError::ResourcesExhausted(_)));
        drop(stream);
        assert_eq!(
            recorder
                .snapshot()
                .counters
                .get("oracle_query_spill_queries_total{class=\"analytical\",outcome=\"error\"}"),
            Some(&1)
        );
        assert_eq!(session.runtime_env().memory_pool.reserved(), 0);
        assert_eq!(governor.snapshot(), baseline);
        assert_eq!(telemetry.memory_bytes.load(Ordering::Acquire), 0);
        drop(session);
        assert!(
            std::fs::read_dir(spill.spill_path())
                .expect("inspect scratch")
                .next()
                .is_none(),
            "failed sort removes every query spill file"
        );
    }

    /// Dropping after one sorted winner releases memory and query spill files.
    #[tokio::test]
    async fn reconciliation_drop_releases_sort_and_spill() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let identity = v7_style_id(10);
        let batches = (0..400)
            .rev()
            .map(|batch| {
                let start = batch * 100;
                tagged_rows(identity, start..start + 100, SourceTier::Live)
            })
            .collect::<Vec<_>>();
        let source = MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&batches),
            batches[0].schema(),
            None,
        )
        .expect("memory source");
        let (governor, pool) = reconciliation_test_pool();
        let baseline = governor.snapshot();
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let plan = ReconcileExec::new_with_telemetry(
            source,
            oracle_memory_resources(governor.clone(), 1024 * 1024),
            Arc::clone(&telemetry),
            QueryClass::Analytical,
        )
        .expect("reconciliation plan");
        let scratch_root = tempfile::tempdir().expect("scratch root");
        let spill = crate::oracle::spill::OracleSpillRuntime::new(scratch_root.path(), 16 << 20)
            .expect("spill runtime");
        let runtime = spill
            .build_query_runtime(pool, 16 << 20)
            .expect("query runtime");
        let session = datafusion::execution::context::SessionContext::new_with_config_rt(
            SessionConfig::new().with_sort_spill_reservation_bytes(0),
            runtime,
        );
        let mut stream = plan
            .execute(0, session.task_ctx())
            .expect("external sort executes");
        assert!(matches!(stream.next().await, Some(Ok(_))));
        assert!(
            plan.sort
                .metrics()
                .expect("sort metrics")
                .spilled_bytes()
                .unwrap_or_default()
                > 0
        );
        drop(stream);
        assert_eq!(
            recorder.snapshot().counters.get(
                "oracle_query_spill_queries_total{class=\"analytical\",outcome=\"cancelled\"}"
            ),
            Some(&1)
        );
        assert_eq!(session.runtime_env().memory_pool.reserved(), 0);
        assert_eq!(governor.snapshot(), baseline);
        assert_eq!(telemetry.memory_bytes.load(Ordering::Acquire), 0);
        drop(session);
        assert!(
            std::fs::read_dir(spill.spill_path())
                .expect("inspect scratch")
                .next()
                .is_none(),
            "cancelled sort removes every query spill file"
        );
    }

    /// Sorted reconciliation carries one identity across source batch boundaries.
    #[tokio::test]
    async fn reconciliation_identity_spans_sorted_batches() {
        let identity = v7_style_id(7);
        let batches = vec![
            tagged_batch(identity, 3, 41, SourceTier::Live),
            tagged_batch(v7_style_id(8), 0, 8, SourceTier::Iceberg),
            tagged_batch(identity, 3, 41, SourceTier::Iceberg),
        ];
        let source = MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&batches),
            batches[0].schema(),
            None,
        )
        .expect("memory source");
        let plan = ReconcileExec::new(
            source,
            OracleMemoryResources {
                resources: crate::resources::BifrostRuntimeResources::composed_for_test(
                    1024 * 1024 * 1024,
                    1024 * 1024 * 1024,
                    [crate::resources::BifrostRole::Oracle],
                )
                .oracle()
                .expect("composition must enable the Oracle capability"),
                governor: crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
                    .expect("test governor"),
                reconciliation_limit_bytes: 1024 * 1024,
            },
        )
        .expect("reconciliation plan");
        let mut stream = plan
            .execute(
                0,
                datafusion::execution::context::SessionContext::new().task_ctx(),
            )
            .expect("sorted reconciliation executes");
        let mut actual = std::collections::BTreeSet::new();
        while let Some(batch) = stream.next().await {
            actual.insert(winner_key(&batch.expect("sorted batch reconciles")));
        }
        assert_eq!(actual.len(), 2);
        assert!(actual.contains(&(
            RowIdentity {
                batch_id: identity,
                ordinal: 3
            },
            41
        )));
    }

    /// One batch-id group with many ordinals completes without hash-partition skew.
    #[tokio::test]
    async fn reconciliation_large_batch_id_group_uses_ordinals_without_skew() {
        let identity = v7_style_id(9);
        let batches = (0..2048)
            .rev()
            .map(|ordinal| tagged_batch(identity, ordinal, i64::from(ordinal), SourceTier::Live))
            .collect::<Vec<_>>();
        let source = MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&batches),
            batches[0].schema(),
            None,
        )
        .expect("memory source");
        let plan = ReconcileExec::new(
            source,
            OracleMemoryResources {
                resources: crate::resources::BifrostRuntimeResources::composed_for_test(
                    1024 * 1024 * 1024,
                    1024 * 1024 * 1024,
                    [crate::resources::BifrostRole::Oracle],
                )
                .oracle()
                .expect("composition must enable the Oracle capability"),
                governor: crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
                    .expect("test governor"),
                reconciliation_limit_bytes: 1024 * 1024,
            },
        )
        .expect("reconciliation plan");
        let mut stream = plan
            .execute(
                0,
                datafusion::execution::context::SessionContext::new().task_ctx(),
            )
            .expect("sorted reconciliation executes");
        let mut rows = 0;
        while let Some(batch) = stream.next().await {
            rows += batch.expect("ordinal group reconciles").num_rows();
        }
        assert_eq!(rows, 2048);
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
            governor: crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
                .expect("hot memory governor"),
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
                HotParquetRuntime {
                    memory: memory.clone(),
                    memory_pool: crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
                    telemetry: Arc::clone(&telemetry),
                    query_class: QueryClass::Interactive,
                    metrics: Arc::clone(&metrics),
                },
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
        governor: &crate::scribe::memory::BifrostMemoryGovernor,
        ranges: Arc<Mutex<Vec<Range<u64>>>>,
        short: bool,
    ) -> (IcebergParquetReader, Arc<dyn MemoryPool>) {
        let resources = crate::resources::BifrostRuntimeResources::composed_for_test(
            768 * 1024 * 1024,
            1024 * 1024 * 1024,
            [crate::resources::BifrostRole::Oracle],
        )
        .oracle()
        .expect("composition must enable the Oracle capability");
        let pool = crate::resources::bounded_memory_pool(governor.oracle_limit_bytes());
        let reader = IcebergParquetReader::new(
            Box::new(RecordingRangeReader {
                bytes: fixture.bytes.clone(),
                ranges,
                short,
            }),
            u64::try_from(fixture.bytes.len()).expect("fixture size fits u64"),
            Arc::clone(&pool),
            resources.governor(),
            Arc::new(OracleScanMetricsHandle::default()),
            Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1)))),
            QueryClass::Interactive,
        );
        (reader, pool)
    }

    /// Hot Parquet uses strict sub-file requests and never reserves the object size.
    #[tokio::test]
    async fn hot_parquet_reads_ranges_without_whole_file_reservation() {
        let fixture = build_hot_causal_fixture();
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let governor = crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
            .expect("hot range governor");
        let (reader, pool) =
            governed_fixture_reader(&fixture, &governor, Arc::clone(&ranges), false);
        let mut batches = ParquetRecordBatchStreamBuilder::new(reader)
            .await
            .expect("ranged metadata")
            .with_batch_size(HOT_BATCH_ROWS)
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
        assert_eq!(governor.snapshot().oracle_total_bytes, 0);
    }

    /// A logical hot object above the child budget streams when live pieces fit.
    #[tokio::test]
    async fn hot_parquet_larger_than_budget_streams_exact_rows() {
        let fixture = build_hot_causal_fixture();
        let budget = fixture.bytes.len().saturating_sub(1);
        let governor = crate::scribe::memory::BifrostMemoryGovernor::new_with_test_child_limits(
            4 * 1024 * 1024 * 1024,
            1024 * 1024,
            budget,
        )
        .expect("small Oracle child");
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let metrics = Arc::new(OracleScanMetricsHandle::default());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&ranges);
        let source = fixture.bytes.clone();
        let peak_pool = Arc::new(AtomicU64::new(0));
        let peak_telemetry = Arc::new(AtomicU64::new(0));
        let query_pool = crate::resources::bounded_memory_pool(governor.oracle_limit_bytes());
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
            HotParquetRuntime {
                memory: oracle_memory_resources(governor.clone(), 1024),
                memory_pool: Arc::clone(&query_pool),
                telemetry: Arc::clone(&telemetry),
                query_class: QueryClass::Interactive,
                metrics: Arc::clone(&metrics),
            },
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
        let governor = crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
            .expect("metadata governor");
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
        assert_eq!(governor.snapshot().oracle_total_bytes, 0);
    }

    /// Clones and slices retain one shared range charge until the final drop.
    #[tokio::test]
    async fn hot_parquet_range_clone_and_slice_retain_charge_until_final_drop() {
        let fixture = build_hot_causal_fixture();
        let governor = crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
            .expect("clone governor");
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
        let governor = crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
            .expect("short-read governor");
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
        assert_eq!(governor.snapshot().oracle_total_bytes, 0);
        assert_eq!(pool.reserved(), 0);
    }

    /// An indivisible range above its ceiling is rejected before storage IO.
    #[tokio::test]
    async fn hot_parquet_oversized_single_range_fails_before_io() {
        let fixture = build_hot_causal_fixture();
        let governor = crate::scribe::memory::BifrostMemoryGovernor::new_with_test_child_limits(
            4 * 1024 * 1024 * 1024,
            1024,
            8,
        )
        .expect("tiny Oracle child");
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let (mut reader, pool) =
            governed_fixture_reader(&fixture, &governor, Arc::clone(&ranges), false);
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
        assert_eq!(governor.snapshot().oracle_total_bytes, 0);
        assert_eq!(pool.reserved(), 0);
    }

    /// Dropping after one production yield releases decoded and ranged ownership.
    #[tokio::test]
    async fn hot_parquet_drop_stops_io_and_releases_memory() {
        let fixture = build_hot_causal_fixture();
        let governor = crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
            .expect("cancellation governor");
        let telemetry = Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1))));
        let attempts = Arc::new(AtomicU64::new(0));
        let observed = Arc::clone(&attempts);
        let source = fixture.bytes.clone();
        let query_pool = crate::resources::bounded_memory_pool(governor.oracle_limit_bytes());
        let exec = HotParquetExec::new(
            vec![HotFileSource {
                location: fixture.path.to_string_lossy().into_owned(),
                size_bytes: fixture.bytes.len(),
            }],
            FileIO::new_with_fs(),
            Arc::clone(&fixture.schema),
            HotParquetRuntime {
                memory: oracle_memory_resources(governor.clone(), 1024),
                memory_pool: Arc::clone(&query_pool),
                telemetry: Arc::clone(&telemetry),
                query_class: QueryClass::Interactive,
                metrics: Arc::new(OracleScanMetricsHandle::default()),
            },
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
        assert_eq!(governor.snapshot().oracle_total_bytes, 0);
        assert_eq!(governor.snapshot().bifrost_total_bytes, 0);
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
        let memory = oracle_memory_resources(
            crate::scribe::memory::BifrostMemoryGovernor::new(4 * 1024 * 1024 * 1024)
                .expect("hot causal governor"),
            1024 * 1024,
        );
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
                HotParquetRuntime {
                    memory: memory.clone(),
                    memory_pool: crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
                    telemetry: Arc::clone(&telemetry),
                    query_class: QueryClass::Interactive,
                    metrics: Arc::clone(&metrics),
                },
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
}
