//! Bounded, spillable staging-to-Iceberg rewrite execution.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use chrono::Datelike;
use datafusion::execution::TaskContext;
use datafusion::execution::memory_pool::MemoryPool;
use datafusion::execution::runtime_env::{RuntimeEnv, RuntimeEnvBuilder};
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_expr::{EquivalenceProperties, LexOrdering, PhysicalSortExpr};
use datafusion::physical_plan::execution_plan::{
    Boundedness, EmissionType, PlanProperties, SchedulingType,
};
use datafusion::physical_plan::metrics::MetricsSet;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, SendableRecordBatchStream,
    execute_stream,
};
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use futures_util::{StreamExt, TryStreamExt};
use iceberg::Catalog;
use iceberg::arrow::NanValueCountVisitor;
#[cfg(test)]
use iceberg::spec::{DataContentType, DataFileBuilder, DataFileFormat};
use iceberg::spec::{DataFile, Literal, SchemaRef as IcebergSchemaRef, Struct};
use iceberg::writer::file_writer::ParquetWriter;
use opendal::Buffer;
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ArrowReaderOptions;
use parquet::arrow::async_reader::{AsyncFileReader, ParquetRecordBatchStreamBuilder};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::compact::{ForgeObjectStore, project_by_name, validate_tenant_column};
use super::error::ForgeError;
use super::lease::ForgeLease;
use super::path::{catalog_path_to_object_key, validate_table_location};
use crate::catalog::TenantTableBinding;
use crate::parquet::writer_properties::{BIFROST_WRITER_RECIPE_VERSION, bifrost_writer_properties};
use vala_sql::OperatorPool;

/// Maximum rows decoded from one Parquet source batch.
const REWRITE_BATCH_ROWS: usize = 8_192;
/// Upper bound for `DataFusion` spill handles to finish dropping after cancel.
const SPILL_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Owned rewrite dependencies shared by every serialized Forge operation.
pub(crate) struct ForgeRewritePipeline {
    /// Process-wide [`DataFusion`] runtime and its owned spill directory.
    runtime: ForgeRewriteRuntime,
    /// Staging operator used for deterministic rewritten-object PUTs.
    staging: Arc<opendal::Operator>,
    /// Iceberg catalog used to re-check live references before cleanup.
    catalog: Arc<dyn Catalog>,
    /// Testable object-store seam used for source reads and failure cleanup.
    object_store: Arc<dyn ForgeObjectStore>,
    /// Upper bound for concurrently open source readers.
    max_concurrent_reads: usize,
    /// Bounds CPU-heavy Parquet encode/finalize tasks.
    blocking_permits: Arc<tokio::sync::Semaphore>,
}

impl fmt::Debug for ForgeRewritePipeline {
    /// Redact dependency internals while retaining useful pipeline limits.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ForgeRewritePipeline")
            .field("max_concurrent_reads", &self.max_concurrent_reads)
            .finish_non_exhaustive()
    }
}

/// Immutable inputs for one deterministic rewrite operation.
pub(crate) struct RewriteRequest<'a> {
    /// Unique `UUIDv7` generation for this concrete execution attempt's paths.
    pub(crate) attempt_generation: ForgeAttemptGeneration,
    /// Validated tenant/table object-storage binding.
    pub(crate) binding: &'a TenantTableBinding,
    /// Physical Arrow schema registered by the destination Iceberg table.
    pub(crate) schema: SchemaRef,
    /// Iceberg schema carrying field IDs required for complete file metrics.
    pub(crate) iceberg_schema: IcebergSchemaRef,
    /// Ordered source files read through the shared bounded rewrite contract.
    pub(crate) source_files: &'a [RewriteSourceFile],
    /// Physical day partition shared by every source file.
    pub(crate) partition_day: chrono::NaiveDate,
    /// Destination Iceberg table location used in `DataFile` paths.
    pub(crate) table_location: &'a str,
    /// Destination partition-spec identifier.
    pub(crate) partition_spec_id: i32,
    /// Destination physical sort-order identifier recorded on every output.
    pub(crate) sort_order_id: i32,
    /// Output target resolved from this operation's Iceberg table metadata.
    pub(crate) target_file_size_bytes: u64,
}

/// `UUIDv7` identity that prevents a later attempt from reusing terminal output paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ForgeAttemptGeneration(Uuid);

impl ForgeAttemptGeneration {
    /// Create a fresh time-ordered generation for one rewrite execution.
    ///
    /// Gated to `test-support`: its only caller is the `test-support`-gated
    /// [`replace_live_group`](super::live_replace) seam. The production task path
    /// derives its generation from the durable attempt via
    /// [`Self::from_attempt`].
    #[cfg(feature = "test-support")]
    #[must_use]
    pub(crate) fn now_v7() -> Self {
        Self(Uuid::now_v7())
    }

    /// Reuses the durable task attempt as the immutable output generation.
    ///
    /// The claim transaction creates a fresh `UUIDv7` for every attempt. Passing
    /// that identity through the rewrite boundary prevents a later retry from
    /// publishing or reclaiming an earlier attempt's output paths.
    #[must_use]
    pub(crate) const fn from_attempt(value: Uuid) -> Self {
        Self(value)
    }

    /// Wrap an explicit generation for deterministic test assertions.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub(crate) const fn for_test(value: Uuid) -> Self {
        Self(value)
    }
}

impl fmt::Display for ForgeAttemptGeneration {
    /// Format the UUID generation used in immutable object names.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Immutable source identity consumed by one bounded rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RewriteSourceFile {
    /// Catalog-owned path used by live replacement's exact delete set.
    pub(crate) catalog_path: String,
    /// Binding-validated object path used for bounded Parquet reads.
    pub(crate) object_path: String,
    /// Manifest or staging metadata size retained for audit and diagnostics.
    pub(crate) file_size_bytes: u64,
    /// Manifest or staging metadata row count retained for validation.
    pub(crate) record_count: u64,
}

/// Complete multi-file result returned after rewrite-owned cleanup transfers.
pub(crate) struct RewriteOutput {
    /// Iceberg metadata for every rotated output in deterministic order.
    pub(crate) files: Vec<DataFile>,
    /// Relative object-store paths corresponding to `files`.
    pub(crate) object_paths: Vec<String>,
    /// Rows accepted from validated source batches.
    pub(crate) input_rows: u64,
    /// Rows encoded across all output files.
    pub(crate) output_rows: u64,
    /// Peak bytes observed in the operation's `DataFusion` spill directory.
    pub(crate) spill_bytes: u64,
}

/// Mutable writer and accounting state for one streamed rewrite operation.
///
/// The state owns every output path as soon as its PUT succeeds and retains
/// that ownership across later batch failures so rewrite finalization can
/// delete the complete accumulated set.
struct RewriteBatchState {
    /// Currently open bounded Parquet writer, if any batch has been accepted.
    writer: Option<ArrowWriter<Vec<u8>>>,
    /// Rows buffered in `writer` and not yet transferred to an output object.
    writer_rows: u64,
    /// Iceberg metadata accumulated for successfully finalized outputs.
    files: Vec<DataFile>,
    /// Object paths still owned by rewrite cleanup.
    output_paths: Vec<String>,
    /// Rows accepted from source batches.
    input_rows: u64,
    /// Rows transferred into successfully finalized output objects.
    output_rows: u64,
    /// Peak spill bytes observed while consuming sorted batches.
    peak_spill_bytes: u64,
    /// Shared `DataFusion` pool reservation for encoded output capacity.
    output_reservation: Option<datafusion::execution::memory_pool::MemoryReservation>,
    /// Per-output NaN counts keyed by Iceberg field ID.
    nan_value_counts: NanValueCountVisitor,
}

impl RewriteBatchState {
    /// Create empty state for isolated batch-state tests and callers without
    /// a shared `DataFusion` reservation.
    #[cfg(test)]
    fn new() -> Self {
        Self::with_reservation(None)
    }

    /// Create state with an optional shared-pool output reservation.
    fn with_reservation(
        reservation: Option<datafusion::execution::memory_pool::MemoryReservation>,
    ) -> Self {
        Self {
            writer: None,
            writer_rows: 0,
            files: Vec::new(),
            output_paths: Vec::new(),
            input_rows: 0,
            output_rows: 0,
            peak_spill_bytes: 0,
            output_reservation: reservation,
            nan_value_counts: NanValueCountVisitor::new(),
        }
    }

    /// Count one non-empty source batch before attempting output rotation.
    ///
    /// Counting first preserves accepted-input diagnostics when the pending
    /// output cannot be fenced or persisted.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the batch row count cannot fit
    /// the rewrite's `u64` counters.
    fn record_input(&mut self, batch: &RecordBatch) -> Result<(), ForgeError> {
        let rows = u64::try_from(batch.num_rows()).map_err(|error| ForgeError::Invariant {
            detail: format!("rewrite row count does not fit u64: {error}"),
        })?;
        self.input_rows = self.input_rows.saturating_add(rows);
        Ok(())
    }

    /// Report whether the active non-empty writer reached the output bound.
    fn should_rotate(&self, output_file_bytes: u64) -> bool {
        self.writer.as_ref().is_some_and(|writer| {
            self.writer_rows > 0
                && u64::try_from(writer.bytes_written() + writer.in_progress_size())
                    .unwrap_or(u64::MAX)
                    >= output_file_bytes
        })
    }

    /// Append one validated batch to the active bounded Parquet writer.
    ///
    /// # Errors
    ///
    /// Returns a Parquet encoding failure or [`ForgeError::Invariant`] when
    /// the batch row count cannot fit the rewrite's `u64` counters.
    fn write_batch(
        &mut self,
        schema: &SchemaRef,
        iceberg_schema: &IcebergSchemaRef,
        batch: &RecordBatch,
    ) -> Result<(), ForgeError> {
        self.nan_value_counts
            .compute(Arc::clone(iceberg_schema), batch.clone())
            .map_err(|error| ForgeError::Invariant {
                detail: format!("Forge NaN metric derivation failed: {error}"),
            })?;
        if let Some(reservation) = &self.output_reservation {
            reservation
                .try_grow(batch.get_array_memory_size())
                .map_err(|error| ForgeError::Invariant {
                    detail: format!("Forge output reservation rejected batch: {error}"),
                })?;
        }
        if self.writer.is_none() {
            self.writer = Some(
                ArrowWriter::try_new(
                    Vec::new(),
                    Arc::clone(schema),
                    Some(bifrost_writer_properties(batch.num_rows())),
                )
                .map_err(|error| ForgeError::Parquet {
                    detail: error.to_string(),
                })?,
            );
        }
        let writer = self.writer.as_mut().ok_or_else(|| ForgeError::Invariant {
            detail: "Forge rewrite failed to retain its active writer".to_owned(),
        })?;
        writer.write(batch).map_err(|error| ForgeError::Parquet {
            detail: error.to_string(),
        })?;
        let rows = u64::try_from(batch.num_rows()).map_err(|error| ForgeError::Invariant {
            detail: format!("rewrite row count does not fit u64: {error}"),
        })?;
        self.writer_rows = self.writer_rows.saturating_add(rows);
        Ok(())
    }

    /// Take the active writer for close, PUT, and metadata derivation.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when rotation is requested without an
    /// active writer.
    fn take_writer(&mut self) -> Result<ArrowWriter<Vec<u8>>, ForgeError> {
        self.writer.take().ok_or_else(|| ForgeError::Invariant {
            detail: "Forge output rotation lost its active writer".to_owned(),
        })
    }

    /// Record one finalized output and clear its rows from the active writer.
    /// Transfer the current output's NaN metrics and reset the visitor state.
    fn take_nan_value_counts(&mut self) -> HashMap<i32, u64> {
        std::mem::take(&mut self.nan_value_counts).nan_value_counts
    }

    /// Record one finalized output and clear its rows and per-file metrics.
    fn record_output(&mut self, file: DataFile, path: String) {
        if let Some(reservation) = &self.output_reservation {
            reservation.free();
        }
        self.output_rows = self.output_rows.saturating_add(self.writer_rows);
        self.writer_rows = 0;
        self.output_paths.push(path);
        self.files.push(file);
        self.nan_value_counts = NanValueCountVisitor::new();
    }

    /// Retain the largest runtime spill observation seen between batches.
    fn observe_spill(&mut self, spill_bytes: u64) {
        self.peak_spill_bytes = self.peak_spill_bytes.max(spill_bytes);
    }

    /// Transfer accumulated output ownership alongside a successful terminal result.
    fn complete(self) -> RewriteBatchResult {
        self.into_result(Ok(()))
    }

    /// Transfer accumulated output ownership alongside the terminal failure.
    fn fail(self, error: ForgeError) -> RewriteBatchResult {
        self.into_result(Err(error))
    }

    /// Drop any unfinished writer and move finalized state into the terminal result.
    fn into_result(mut self, result: Result<(), ForgeError>) -> RewriteBatchResult {
        self.writer.take();
        RewriteBatchResult {
            result,
            files: self.files,
            output_paths: self.output_paths,
            input_rows: self.input_rows,
            output_rows: self.output_rows,
            peak_spill_bytes: self.peak_spill_bytes,
        }
    }
}

/// Accumulated rewrite state retained when a stream stage fails.
struct RewriteBatchResult {
    /// Terminal stream result, preserving errors after output rotation.
    result: Result<(), ForgeError>,
    /// Iceberg metadata accumulated before the terminal result.
    files: Vec<DataFile>,
    /// Object paths still owned by rewrite cleanup.
    output_paths: Vec<String>,
    /// Rows accepted from source batches.
    input_rows: u64,
    /// Rows encoded into output files.
    output_rows: u64,
    /// Peak spill bytes observed while consuming the stream.
    peak_spill_bytes: u64,
}

/// Inputs shared by every output close, PUT, and metadata derivation.
struct RewriteOutputContext<'a, 'b> {
    /// Immutable rewrite identity and destination metadata.
    request: &'a RewriteRequest<'b>,
    /// Cancellation source checked before side effects.
    stop: &'a CancellationToken,
    /// Mutable table lease renewed immediately before each PUT.
    lease: &'a mut ForgeLease,
    /// Operator pool used to validate the lease fence.
    operator_pool: &'a OperatorPool,
}

/// Inputs needed to finalize a completed rewrite after its stream is dropped.
struct RewriteFinalization<'a> {
    /// Terminal batch state, including all rewrite-owned output paths.
    batch: RewriteBatchResult,
    /// Validated target binding used for cleanup ownership checks.
    binding: &'a TenantTableBinding,
    /// Cancellation source checked before cleanup side effects.
    stop: &'a CancellationToken,
    /// Mutable lease that proves cleanup still belongs to this Forge owner.
    lease: &'a mut ForgeLease,
    /// Operator pool used for lease-fence validation.
    operator_pool: &'a OperatorPool,
}

/// Blocking inputs for deriving Iceberg metadata from one completed Parquet output.
struct OutputMetadataRequest {
    /// Closed in-memory Parquet writer whose bytes become the staged object.
    writer: ArrowWriter<Vec<u8>>,
    /// Per-field NaN counts required by Iceberg file metadata.
    nan_value_counts: HashMap<i32, u64>,
    /// Rows encoded into the completed Parquet output.
    rows: u64,
    /// Iceberg schema carrying field identifiers for file metadata.
    iceberg_schema: IcebergSchemaRef,
    /// Catalog-visible Iceberg path for the output file.
    table_path: String,
    /// Day partition assigned to the entire output.
    partition_day: chrono::NaiveDate,
    /// Iceberg partition-spec identifier for the destination table.
    partition_spec_id: i32,
    /// Iceberg sort-order identifier for the destination table.
    sort_order_id: i32,
}

/// Bytes and Iceberg metadata derived together from one completed Parquet output.
struct FinalizedOutput {
    /// Exact Parquet bytes to persist to the staging object store.
    bytes: Bytes,
    /// Iceberg data-file metadata derived from those bytes.
    file: DataFile,
}

/// Close one Parquet writer and derive the matching Iceberg data-file metadata.
///
/// This pure blocking stage owns no `ForgeRewritePipeline` state: it converts a
/// completed writer into bytes and validates that the metadata describes those
/// exact bytes before the async caller persists them.
///
/// # Errors
///
/// Returns Parquet or metadata errors when the writer cannot close, its footer
/// cannot be read, or the derived Iceberg file disagrees with the completed
/// Parquet bytes.
fn finalize_output_metadata(request: OutputMetadataRequest) -> Result<FinalizedOutput, ForgeError> {
    let OutputMetadataRequest {
        writer,
        nan_value_counts,
        rows,
        iceberg_schema,
        table_path,
        partition_day,
        partition_spec_id,
        sort_order_id,
    } = request;
    let bytes = writer.into_inner().map_err(|error| ForgeError::Parquet {
        detail: error.to_string(),
    })?;
    let output_size = u64::try_from(bytes.len()).map_err(|error| ForgeError::Invariant {
        detail: format!("output size does not fit u64: {error}"),
    })?;
    let bytes = Bytes::from(bytes);
    let parquet_metadata = Arc::new(
        parquet::file::metadata::ParquetMetaDataReader::new()
            .parse_and_finish(&bytes)
            .map_err(|error| ForgeError::Parquet {
                detail: format!("Forge output footer parsing failed: {error}"),
            })?,
    );
    let mut file = ParquetWriter::parquet_to_data_file_builder(
        iceberg_schema,
        Arc::clone(&parquet_metadata),
        bytes.len(),
        table_path,
        nan_value_counts,
    )
    .map_err(|error| ForgeError::Invariant {
        detail: format!("Forge output metadata conversion failed: {error}"),
    })?;
    let partition = Struct::from_iter([Some(Literal::date(
        partition_day.num_days_from_ce() - 719_163,
    ))]);
    file.partition(partition)
        .partition_spec_id(partition_spec_id)
        .sort_order_id(sort_order_id);
    let file = file.build().map_err(|error| ForgeError::Invariant {
        detail: format!("Forge output metadata is incomplete: {error}"),
    })?;
    if file.record_count() != rows || file.file_size_in_bytes() != output_size {
        return Err(ForgeError::Invariant {
            detail: "Forge output metadata disagrees with finalized Parquet bytes".to_owned(),
        });
    }
    let row_groups = parquet_metadata.row_groups();
    let split_offsets = file.split_offsets().ok_or_else(|| ForgeError::Invariant {
        detail: "Forge output metadata omitted row-group split offsets".to_owned(),
    })?;
    if split_offsets.len() != row_groups.len() {
        return Err(ForgeError::Invariant {
            detail: "Forge output split offsets do not match row groups".to_owned(),
        });
    }
    Ok(FinalizedOutput { bytes, file })
}

impl ForgeRewritePipeline {
    /// Compose a rewrite pipeline from host-provisioned runtime dependencies.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the concurrency limit is zero.
    pub(crate) fn new(
        runtime: ForgeRewriteRuntime,
        staging: Arc<opendal::Operator>,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
        max_concurrent_reads: usize,
    ) -> Result<Self, ForgeError> {
        if max_concurrent_reads == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "rewrite limits must be positive".to_owned(),
            });
        }
        Ok(Self {
            runtime,
            staging,
            catalog,
            object_store,
            max_concurrent_reads,
            blocking_permits: Arc::new(tokio::sync::Semaphore::new(max_concurrent_reads)),
        })
    }

    /// Confirm the pipeline retained the exact host-supplied staging operator.
    pub(super) fn uses_staging(&self, staging: &Arc<opendal::Operator>) -> bool {
        Arc::ptr_eq(&self.staging, staging)
    }

    /// Stream, validate, sort, and rotate one rewrite bin into Parquet outputs.
    ///
    /// Source batches are bounded and enter a `DataFusion` `SortExec` backed by
    /// the supplied shared memory pool and owned spill directory. Cancellation
    /// is checked between batches and before every output PUT. Before this
    /// method returns success, paths already written remain rewrite-owned and
    /// are deleted best-effort after any error; success transfers all paths to
    /// compaction reconciliation.
    ///
    /// # Errors
    ///
    /// Returns schema, tenant, Parquet, object-store, lease, `DataFusion`,
    /// cancellation, spill-ceiling, or invariant failures. No durable SQL or
    /// Iceberg transition occurs in this method.
    pub(crate) async fn rewrite(
        &self,
        request: RewriteRequest<'_>,
        stop: &CancellationToken,
        lease: &mut ForgeLease,
        operator_pool: &OperatorPool,
    ) -> Result<RewriteOutput, ForgeError> {
        let initial_spill = self.runtime.runtime.spilling_progress();
        if initial_spill.active_files_count != 0 {
            return Err(ForgeError::Invariant {
                detail: "Forge rewrite began with active spill files".to_owned(),
            });
        }
        let (mut stream, sort_metrics) = self.sorted_stream(&request)?;
        let result = self
            .rewrite_batches(&mut stream, &request, stop, lease, operator_pool)
            .await;
        let mut result = result;
        // Dropping the physical stream releases SortExec and its spill handles.
        // This must happen before observing spill progress or returning on cancel.
        drop(stream);
        result.peak_spill_bytes = result.peak_spill_bytes.max(
            u64::try_from(sort_metrics.spilled_bytes().unwrap_or_default()).unwrap_or(u64::MAX),
        );
        self.finalize_rewrite(RewriteFinalization {
            batch: result,
            binding: request.binding,
            stop,
            lease,
            operator_pool,
        })
        .await
    }

    /// Consume sorted batches, rotate bounded writers, and collect output metadata.
    ///
    /// # Errors
    ///
    /// Returns source, Parquet, cancellation, lease, object-store, or invariant
    /// failures encountered while processing the stream.
    ///
    /// # Cancellation
    ///
    /// Cancellation stops before the next batch or output PUT. Previously
    /// finalized paths remain in the returned state for complete-set cleanup.
    async fn rewrite_batches(
        &self,
        stream: &mut SendableRecordBatchStream,
        request: &RewriteRequest<'_>,
        stop: &CancellationToken,
        lease: &mut ForgeLease,
        operator_pool: &OperatorPool,
    ) -> RewriteBatchResult {
        let reservation =
            datafusion::execution::memory_pool::MemoryConsumer::new("forge-rewrite-output")
                .register(&self.runtime.runtime.memory_pool);
        let mut state = RewriteBatchState::with_reservation(Some(reservation));
        while let Some(batch) = tokio::select! {
            () = stop.cancelled() => return state.fail(ForgeError::Shutdown),
            batch = stream.next() => batch,
        } {
            let batch = match batch {
                Ok(batch) => batch,
                Err(error) => return state.fail(self.map_datafusion_error(error)),
            };
            if batch.num_rows() == 0 {
                continue;
            }
            let context = RewriteOutputContext {
                request,
                stop,
                lease,
                operator_pool,
            };
            if let Err(error) = self.process_batch(&mut state, &batch, context).await {
                return state.fail(error);
            }
        }
        let context = RewriteOutputContext {
            request,
            stop,
            lease,
            operator_pool,
        };
        if let Err(error) = self.finish_pending_output(&mut state, context).await {
            return state.fail(error);
        }
        if state.files.is_empty() {
            return state.fail(ForgeError::Invariant {
                detail: "Forge rewrite produced no non-empty output".to_owned(),
            });
        }
        state.complete()
    }

    /// Account for, rotate before, and encode one non-empty sorted batch.
    ///
    /// # Errors
    ///
    /// Returns row-count, Parquet, cancellation, lease, object-store, path, or
    /// metadata failures. A successful rotated PUT remains in `state` for
    /// final ownership transfer or complete-set cleanup.
    ///
    /// # Cancellation
    ///
    /// Cancellation is observed before a rotated output PUT. Outputs completed
    /// by earlier batches remain owned by `state`.
    async fn process_batch(
        &self,
        state: &mut RewriteBatchState,
        batch: &RecordBatch,
        context: RewriteOutputContext<'_, '_>,
    ) -> Result<(), ForgeError> {
        let RewriteOutputContext {
            request,
            stop,
            lease,
            operator_pool,
        } = context;
        state.record_input(batch)?;
        if state.should_rotate(request.target_file_size_bytes) {
            self.rotate_output(
                state,
                RewriteOutputContext {
                    request,
                    stop,
                    lease: &mut *lease,
                    operator_pool,
                },
            )
            .await?;
        }
        let mut detached = std::mem::replace(
            state,
            RewriteBatchState::with_reservation(Some(
                datafusion::execution::memory_pool::MemoryConsumer::new("forge-rewrite-output")
                    .register(&self.runtime.runtime.memory_pool),
            )),
        );
        let schema = Arc::clone(&request.schema);
        let iceberg_schema = Arc::clone(&request.iceberg_schema);
        let clone_reservation =
            datafusion::execution::memory_pool::MemoryConsumer::new("forge-rewrite-batch-clone")
                .register(&self.runtime.runtime.memory_pool);
        clone_reservation
            .try_grow(batch.get_array_memory_size())
            .map_err(|error| ForgeError::Invariant {
                detail: format!("Forge batch clone reservation rejected batch: {error}"),
            })?;
        let batch = batch.clone();
        let permit = tokio::select! {
            () = stop.cancelled() => return Err(ForgeError::Shutdown),
            permit = self.blocking_permits.clone().acquire_owned() => permit.map_err(|error| ForgeError::Invariant {
                detail: format!("Forge blocking permit closed: {error}"),
            })?,
        };
        let join = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _clone_reservation = clone_reservation;
            let result = detached.write_batch(&schema, &iceberg_schema, &batch);
            (detached, result)
        });
        let (returned, result) = join.await.map_err(|error| ForgeError::Invariant {
            detail: format!("Forge output encoding task failed: {error}"),
        })?;
        *state = returned;
        result?;
        state.observe_spill(self.runtime.runtime.spilling_progress().current_bytes);
        Ok(())
    }

    /// Finalize the last non-empty writer after stream exhaustion.
    ///
    /// # Errors
    ///
    /// Returns cancellation, lease, object-store, Parquet, path, or metadata
    /// failures. An empty writer produces no output.
    ///
    /// # Cancellation
    ///
    /// Cancellation before the final PUT returns [`ForgeError::Shutdown`];
    /// earlier output paths remain owned by `state`.
    async fn finish_pending_output(
        &self,
        state: &mut RewriteBatchState,
        context: RewriteOutputContext<'_, '_>,
    ) -> Result<(), ForgeError> {
        if state.writer_rows > 0 {
            self.rotate_output(state, context).await?;
        }
        Ok(())
    }

    /// Close the current writer and transfer its bytes to one output object.
    ///
    /// # Errors
    ///
    /// Returns lease, cancellation, object-store, Parquet, path, or metadata
    /// errors from the output finalization seam.
    ///
    /// # Cancellation
    ///
    /// Cancellation is checked before the PUT and while the bounded blocking
    /// finalization task runs. On cancellation, that task is always joined so
    /// no writer, permit, or metadata buffer is detached; earlier finalized
    /// paths remain in `state` for cleanup.
    async fn rotate_output(
        &self,
        state: &mut RewriteBatchState,
        context: RewriteOutputContext<'_, '_>,
    ) -> Result<(), ForgeError> {
        let writer = state.take_writer()?;
        let nan_value_counts = state.take_nan_value_counts();
        let (file, path) = self
            .finish_output(
                writer,
                nan_value_counts,
                state.writer_rows,
                state.files.len(),
                context,
            )
            .await?;
        state.record_output(file, path);
        Ok(())
    }

    /// Validate cleanup and row invariants before transferring output ownership.
    ///
    /// # Errors
    ///
    /// Returns the original rewrite error, an active-spill invariant, or a row
    /// conservation invariant after deleting rewrite-owned outputs best-effort.
    async fn finalize_rewrite(
        &self,
        finalization: RewriteFinalization<'_>,
    ) -> Result<RewriteOutput, ForgeError> {
        let RewriteFinalization {
            batch,
            binding,
            stop,
            lease,
            operator_pool,
        } = finalization;
        let RewriteBatchResult {
            result,
            files,
            output_paths,
            input_rows,
            output_rows,
            peak_spill_bytes,
        } = batch;
        if let Err(error) = self.await_spill_cleanup().await {
            self.cleanup_paths(&output_paths, binding, stop, lease, operator_pool)
                .await;
            return Err(error);
        }
        if let Err(error) = result {
            self.cleanup_paths(&output_paths, binding, stop, lease, operator_pool)
                .await;
            return Err(error);
        }
        let final_spill = self.runtime.runtime.spilling_progress();
        if final_spill.active_files_count != 0 {
            self.cleanup_paths(&output_paths, binding, stop, lease, operator_pool)
                .await;
            return Err(ForgeError::Invariant {
                detail: "Forge rewrite ended with active spill files".to_owned(),
            });
        }
        if input_rows != output_rows {
            self.cleanup_paths(&output_paths, binding, stop, lease, operator_pool)
                .await;
            return Err(ForgeError::Invariant {
                detail: format!(
                    "Forge rewrite row mismatch: accepted {input_rows}, encoded {output_rows}"
                ),
            });
        }
        Ok(RewriteOutput {
            files,
            object_paths: output_paths,
            input_rows,
            output_rows,
            spill_bytes: peak_spill_bytes,
        })
    }

    /// Wait for `DataFusion`'s dropped physical plan to release all spill files.
    ///
    /// # Errors
    ///
    /// Returns an invariant error when spill handles remain active beyond the
    /// bounded shutdown interval.
    async fn await_spill_cleanup(&self) -> Result<(), ForgeError> {
        let cleanup = async {
            while self.runtime.runtime.spilling_progress().active_files_count != 0 {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        };
        tokio::time::timeout(SPILL_CLEANUP_TIMEOUT, cleanup)
            .await
            .map_err(|_| ForgeError::Invariant {
                detail: "Forge rewrite spill cleanup exceeded its shutdown bound".to_owned(),
            })
    }

    /// Build the single-partition spillable sort stream for one rewrite.
    ///
    /// # Errors
    ///
    /// Returns schema, invariant, or [`DataFusion`] plan-execution errors.
    fn sorted_stream(
        &self,
        request: &RewriteRequest<'_>,
    ) -> Result<(SendableRecordBatchStream, MetricsSet), ForgeError> {
        let source = Arc::new(StagingParquetExec::new(
            request.source_files.to_vec(),
            request.binding.clone(),
            Arc::clone(&request.schema),
            Arc::clone(&self.object_store),
            self.max_concurrent_reads,
        ));
        let tenant_index =
            request
                .schema
                .index_of("data_tenant_id")
                .map_err(|_| ForgeError::Schema {
                    detail: "Iceberg schema lacks data_tenant_id".to_owned(),
                })?;
        let time_index =
            request
                .schema
                .index_of("wyrd_event_time")
                .map_err(|_| ForgeError::Schema {
                    detail: "Iceberg schema lacks wyrd_event_time".to_owned(),
                })?;
        let ordering = LexOrdering::new(vec![
            PhysicalSortExpr::new(
                Arc::new(Column::new("data_tenant_id", tenant_index)),
                arrow::compute::SortOptions {
                    descending: false,
                    nulls_first: false,
                },
            ),
            PhysicalSortExpr::new(
                Arc::new(Column::new("wyrd_event_time", time_index)),
                arrow::compute::SortOptions {
                    descending: false,
                    nulls_first: false,
                },
            ),
        ])
        .ok_or_else(|| ForgeError::Invariant {
            detail: "Forge sort ordering unexpectedly contained no expressions".to_owned(),
        })?;
        let sort = Arc::new(SortExec::new(ordering, source));
        let session = datafusion::prelude::SessionContext::new_with_config_rt(
            datafusion::prelude::SessionConfig::new(),
            self.runtime.runtime(),
        );
        let sort_plan: Arc<dyn ExecutionPlan> = sort.clone();
        let stream =
            execute_stream(sort_plan, session.task_ctx()).map_err(ForgeError::DataFusion)?;
        // DataFusion populates the plan metrics during `execute_stream`; read
        // them only after execution has registered the SortExec metrics set.
        let metrics = sort.metrics().unwrap_or_default();
        Ok((stream, metrics))
    }

    /// Close, PUT, and derive Iceberg metadata for one rotated output.
    ///
    /// # Errors
    ///
    /// Returns cancellation, lease, Parquet, object-store, path, or
    /// metadata-builder failures. The caller remains responsible for deleting
    /// prior outputs.
    ///
    /// # Cancellation
    ///
    /// Cancellation is checked before fence validation and again before PUT.
    /// After PUT succeeds, synchronous metadata construction either returns the
    /// path or deletes that just-written path before returning an error.
    async fn finish_output(
        &self,
        writer: ArrowWriter<Vec<u8>>,
        nan_value_counts: HashMap<i32, u64>,
        rows: u64,
        ordinal: usize,
        context: RewriteOutputContext<'_, '_>,
    ) -> Result<(DataFile, String), ForgeError> {
        let RewriteOutputContext {
            request,
            stop,
            lease,
            operator_pool,
        } = context;
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        let object_path = deterministic_output_path(
            &request.binding.object_prefix,
            request.attempt_generation,
            ordinal,
        );
        self.validate_table_location(request.binding, request.table_location)?;
        request
            .binding
            .validate_object_path(&object_path)
            .ok_or_else(|| ForgeError::Invariant {
                detail: format!("Forge output escaped table prefix: {object_path}"),
            })?;
        self.object_store
            .before_output_put(&object_path)
            .await
            .map_err(ForgeError::ObjectStore)?;
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        lease.require_fence(operator_pool).await?;
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        let table_path = format!(
            "{}/data/forge/{BIFROST_WRITER_RECIPE_VERSION}/{}-{ordinal:05}.parquet",
            request.table_location.trim_end_matches('/'),
            request.attempt_generation
        );
        let partition_day = request.partition_day;
        let partition_spec_id = request.partition_spec_id;
        let sort_order_id = request.sort_order_id;
        let iceberg_schema = Arc::clone(&request.iceberg_schema);
        let permit = tokio::select! {
            () = stop.cancelled() => return Err(ForgeError::Shutdown),
            permit = self.blocking_permits.clone().acquire_owned() => permit.map_err(|error| ForgeError::Invariant {
                detail: format!("Forge blocking permit closed: {error}"),
            })?,
        };
        let join = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            finalize_output_metadata(OutputMetadataRequest {
                writer,
                nan_value_counts,
                rows,
                iceberg_schema,
                table_path,
                partition_day,
                partition_spec_id,
                sort_order_id,
            })
        });
        tokio::pin!(join);
        let finalized = tokio::select! {
            result = &mut join => result,
            () = stop.cancelled() => {
                // The blocking task is never detached: reclaim its result before
                // returning cancellation, so its permits and buffers are dropped.
                let _ = (&mut join).await;
                return Err(ForgeError::Shutdown);
            }
        };
        let FinalizedOutput { bytes, file } =
            finalized.map_err(|error| ForgeError::Invariant {
                detail: format!("Forge output finalization task failed: {error}"),
            })??;
        if catalog_path_to_object_key(
            request.table_location,
            request.binding,
            &self.staging,
            file.file_path(),
        )? != object_path
        {
            return Err(ForgeError::Invariant {
                detail: "Forge DataFile path does not map to its exact object key".to_owned(),
            });
        }
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        self.staging
            .write(&object_path, Buffer::from(bytes))
            .await
            .map_err(ForgeError::ObjectStore)?;
        self.object_store.after_output_put(&object_path).await;
        Ok((file, object_path))
    }

    /// Delete rewrite-owned paths only after binding and fence validation.
    ///
    /// Cleanup is deliberately best effort: the caller's primary rewrite or
    /// metadata error remains authoritative. A cancelled operation, lost
    /// fence, invalid binding, or uncertain ownership leaves all remaining
    /// objects for the fenced orphan-GC pass rather than risking a successor's
    /// committed output.
    async fn cleanup_paths(
        &self,
        paths: &[String],
        binding: &TenantTableBinding,
        stop: &CancellationToken,
        lease: &mut ForgeLease,
        operator_pool: &OperatorPool,
    ) {
        if stop.is_cancelled() {
            return;
        }
        let table = match self.catalog.load_table(&binding.table_ident()).await {
            Ok(table) => table,
            Err(error) => {
                tracing::warn!(error = %error, "Forge cleanup deferred because catalog live set is uncertain");
                return;
            }
        };
        let live_set = match self.current_live_set(binding, &table).await {
            Ok(live_set) => live_set,
            Err(error) => {
                tracing::warn!(error = %error, "Forge cleanup deferred because live-set loading failed");
                return;
            }
        };
        for path in paths {
            let Some(path) = binding.validate_object_path(path) else {
                tracing::warn!(path, "Forge cleanup refused path outside table binding");
                continue;
            };
            if stop.is_cancelled() {
                return;
            }
            if live_set.contains(&path) {
                tracing::warn!(path, "Forge cleanup refused a currently referenced object");
                continue;
            }
            if let Err(fence_error) = lease.require_fence(operator_pool).await {
                tracing::warn!(
                    path,
                    error = %fence_error,
                    "Forge cleanup deferred after fence loss"
                );
                return;
            }
            if stop.is_cancelled() {
                return;
            }
            if let Err(cleanup_error) = self.object_store.delete(&path).await {
                tracing::warn!(
                    path,
                    error = %cleanup_error,
                    "Forge rewrite output cleanup failed"
                );
            }
        }
    }

    /// Reclaim outputs written before a live replacement reaches its durable
    /// `Prepared` audit transition.
    ///
    /// The caller uses this only while the output set is still rewrite-owned.
    /// It delegates to the established live-set and fence-checked cleanup path,
    /// so uncertain catalog state, cancellation, or lost ownership retains the
    /// objects for orphan reconciliation instead of risking a committed file.
    pub(crate) async fn cleanup_unprepared_outputs(
        &self,
        paths: &[String],
        binding: &TenantTableBinding,
        stop: &CancellationToken,
        lease: &mut ForgeLease,
        operator_pool: &OperatorPool,
    ) {
        self.cleanup_paths(paths, binding, stop, lease, operator_pool)
            .await;
    }

    /// Load the current catalog and control-plane references as object keys.
    ///
    /// # Errors
    ///
    /// Returns a catalog, manifest, or binding invariant error when the live
    /// set cannot be established exactly.
    async fn current_live_set(
        &self,
        binding: &TenantTableBinding,
        table: &iceberg::table::Table,
    ) -> Result<std::collections::HashSet<String>, ForgeError> {
        let mut live = std::collections::HashSet::new();
        let mut add = |path: &str| {
            let key = catalog_path_to_object_key(
                table.metadata().location(),
                binding,
                &self.staging,
                path,
            )?;
            live.insert(key);
            Ok::<(), ForgeError>(())
        };
        add(table
            .metadata_location_result()
            .map_err(ForgeError::Catalog)?)?;
        for entry in table.metadata().metadata_log() {
            add(&entry.metadata_file)?;
        }
        for snapshot in table.metadata().snapshots() {
            add(snapshot.manifest_list())?;
            let manifests = table
                .manifest_list_reader(snapshot)
                .load()
                .await
                .map_err(ForgeError::Catalog)?;
            for manifest_file in manifests.entries() {
                add(&manifest_file.manifest_path)?;
                let manifest = manifest_file
                    .load_manifest(table.file_io())
                    .await
                    .map_err(ForgeError::Catalog)?;
                for entry in manifest.entries() {
                    add(entry.data_file().file_path())?;
                }
            }
        }
        Ok(live)
    }

    /// Validate catalog location and object-store identity before any output PUT.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the catalog location is not the
    /// exact configured staging warehouse/table binding.
    fn validate_table_location(
        &self,
        binding: &TenantTableBinding,
        table_location: &str,
    ) -> Result<(), ForgeError> {
        validate_table_location(binding, table_location, &self.staging)
    }

    /// Preserve the public spill-ceiling distinction for resource exhaustion.
    fn map_datafusion_error(&self, error: datafusion::error::DataFusionError) -> ForgeError {
        let detail = error.to_string();
        if detail.contains("temp")
            && detail.contains("limit")
            && detail.contains(&self.runtime.spill_limit_bytes.to_string())
        {
            ForgeError::SpillLimitExceeded {
                limit_bytes: self.runtime.spill_limit_bytes,
            }
        } else {
            ForgeError::DataFusion(error)
        }
    }
}

/// Build one deterministic relative output path from operation identity.
fn deterministic_output_path(
    prefix: &str,
    generation: ForgeAttemptGeneration,
    ordinal: usize,
) -> String {
    format!(
        "{}/data/forge/{BIFROST_WRITER_RECIPE_VERSION}/{generation}-{ordinal:05}.parquet",
        prefix.trim_end_matches('/')
    )
}

/// Builds the production deterministic Forge output path for test fixtures.
#[cfg(feature = "test-support")]
pub fn deterministic_output_path_for_test(
    prefix: &str,
    operation_id: Uuid,
    ordinal: usize,
) -> String {
    deterministic_output_path(
        prefix,
        ForgeAttemptGeneration::for_test(operation_id),
        ordinal,
    )
}

/// `DataFusion` leaf plan that owns bounded staging-file decoding.
#[derive(Debug)]
struct StagingParquetExec {
    /// Ordered durable candidate files.
    files: Vec<RewriteSourceFile>,
    /// Validated tenant/table binding for path and tenant checks.
    binding: TenantTableBinding,
    /// Destination physical schema yielded to `DataFusion`.
    schema: SchemaRef,
    /// Object-store seam used for source reads.
    object_store: Arc<dyn ForgeObjectStore>,
    /// Permits limiting simultaneously open source files.
    permits: Arc<tokio::sync::Semaphore>,
    /// Configured source-stream concurrency ceiling.
    max_concurrent_reads: usize,
    /// `DataFusion` physical properties for one bounded source partition.
    properties: Arc<PlanProperties>,
}

impl StagingParquetExec {
    /// Build one bounded source plan for a deterministic candidate sequence.
    fn new(
        files: Vec<RewriteSourceFile>,
        binding: TenantTableBinding,
        schema: SchemaRef,
        object_store: Arc<dyn ForgeObjectStore>,
        max_concurrent_reads: usize,
    ) -> Self {
        let properties = Arc::new(
            PlanProperties::new(
                EquivalenceProperties::new(Arc::clone(&schema)),
                Partitioning::UnknownPartitioning(1),
                EmissionType::Incremental,
                Boundedness::Bounded,
            )
            .with_scheduling_type(SchedulingType::Cooperative),
        );
        Self {
            files,
            binding,
            schema,
            object_store,
            permits: Arc::new(tokio::sync::Semaphore::new(max_concurrent_reads)),
            max_concurrent_reads,
            properties,
        }
    }

    /// Opens one bounded Parquet stream for this leaf plan's source seam.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error when path validation, metadata,
    /// permit acquisition, Parquet construction, or batch decoding fails.
    async fn open_staging_stream_with_permits(
        &self,
        file: RewriteSourceFile,
        permits: Arc<tokio::sync::Semaphore>,
    ) -> datafusion::error::Result<
        BoxStream<'static, datafusion::error::Result<arrow::record_batch::RecordBatch>>,
    > {
        let path = self
            .binding
            .validate_object_path(&file.object_path)
            .ok_or_else(|| {
                datafusion::error::DataFusionError::Execution(format!(
                    "Forge staging input escaped table prefix: {}",
                    file.object_path
                ))
            })?;
        let permit = permits
            .acquire_owned()
            .await
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
        let size = self
            .object_store
            .stat(&path)
            .await
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?
            .content_length();
        let reader = ForgeParquetReader {
            store: Arc::clone(&self.object_store),
            path,
            size,
        };
        let stream = ParquetRecordBatchStreamBuilder::new(reader)
            .await
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?
            .with_batch_size(REWRITE_BATCH_ROWS)
            .build()
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
        let tenant = self.binding.tenant;
        let schema = Arc::clone(&self.schema);
        let projected = async_stream::try_stream! {
            futures_util::pin_mut!(stream);
            while let Some(batch) = stream.next().await {
                let batch = batch.map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
                validate_tenant_column(&batch, tenant)
                    .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
                crate::schema::managed_columns::row_ordinals(&batch)
                    .map_err(|error| datafusion::error::DataFusionError::Execution(
                        format!("Forge row identity invariant failed: {error}")
                    ))?;
                let projected = project_by_name(&batch, Arc::clone(&schema))
                    .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
                if projected.num_rows() > 0 {
                    yield projected;
                }
            }
            drop(permit);
        };
        Ok(Box::pin(projected))
    }
}

impl DisplayAs for StagingParquetExec {
    /// Render the bounded source without exposing tenant or path values.
    fn fmt_as(
        &self,
        _format: DisplayFormatType,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(formatter, "StagingParquetExec files={}", self.files.len())
    }
}

/// Adapter that turns the Forge ranged-read seam into Parquet's async reader.
#[derive(Debug)]
struct ForgeParquetReader {
    /// Object-store capability used for every footer and row-group range.
    store: Arc<dyn ForgeObjectStore>,
    /// Validated object path being decoded.
    path: String,
    /// Object size required to locate the Parquet footer.
    size: u64,
}

impl AsyncFileReader for ForgeParquetReader {
    /// Fetch exactly the requested Parquet byte range without materializing the file.
    ///
    /// # Errors
    ///
    /// Returns the object-store or Parquet external error for a failed range.
    fn get_bytes(&mut self, range: Range<u64>) -> BoxFuture<'_, parquet::errors::Result<Bytes>> {
        let store = Arc::clone(&self.store);
        let path = self.path.clone();
        Box::pin(async move {
            store
                .read_range(&path, range)
                .await
                .map(|buffer| buffer.to_bytes())
                .map_err(|error| parquet::errors::ParquetError::External(Box::new(error)))
        })
    }

    /// Fetch and decode footer metadata using bounded suffix/range requests.
    ///
    /// # Errors
    ///
    /// Returns the Parquet metadata error when footer decoding fails.
    fn get_metadata<'a>(
        &'a mut self,
        _options: Option<&'a ArrowReaderOptions>,
    ) -> BoxFuture<'a, parquet::errors::Result<Arc<parquet::file::metadata::ParquetMetaData>>> {
        let size = self.size;
        Box::pin(async move {
            let metadata = parquet::file::metadata::ParquetMetaDataReader::new()
                .load_and_finish(&mut *self, size)
                .await?;
            Ok(Arc::new(metadata))
        })
    }
}

impl ExecutionPlan for StagingParquetExec {
    /// Return the stable physical-plan node name.
    fn name(&self) -> &'static str {
        "StagingParquetExec"
    }

    /// Expose this concrete plan for `DataFusion` downcasts.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Return the cached schema, partitioning, and boundedness properties.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    /// This leaf source has no child plans.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        Vec::new()
    }

    /// Rebuild this leaf only when no children are supplied.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error when a caller attempts to attach a child.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> datafusion::error::Result<Arc<dyn ExecutionPlan>> {
        if children.is_empty() {
            Ok(self)
        } else {
            Err(datafusion::error::DataFusionError::Plan(
                "StagingParquetExec is a leaf plan".to_owned(),
            ))
        }
    }

    /// Yield validated, name-projected batches from one bounded source partition.
    ///
    /// Each source file is read under a semaphore permit and decoded in one
    /// bounded blocking task with `REWRITE_BATCH_ROWS` record batches. Dropping
    /// the returned stream cancels future reads and releases every permit.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error for an invalid partition, escaped
    /// path, source read, Parquet decode, schema projection, tenant mismatch, or
    /// blocking-task failure.
    fn execute(
        &self,
        partition: usize,
        _context: Arc<TaskContext>,
    ) -> datafusion::error::Result<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(datafusion::error::DataFusionError::Execution(format!(
                "StagingParquetExec has no partition {partition}"
            )));
        }
        let files = self.files.clone();
        let binding = self.binding.clone();
        let schema = Arc::clone(&self.schema);
        let store = Arc::clone(&self.object_store);
        let permits = Arc::clone(&self.permits);
        let stream_schema = Arc::clone(&schema);
        let max_concurrent_reads = self.max_concurrent_reads;
        let stream = futures_util::stream::iter(files)
            .map(move |file| {
                let binding = binding.clone();
                let schema = Arc::clone(&schema);
                let store = Arc::clone(&store);
                let permits = Arc::clone(&permits);
                async move {
                    StagingParquetExec::new(
                        Vec::new(),
                        binding,
                        schema,
                        store,
                        max_concurrent_reads,
                    )
                    .open_staging_stream_with_permits(file, permits)
                    .await
                }
            })
            .buffer_unordered(self.max_concurrent_reads)
            .try_flatten();
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            stream_schema,
            stream,
        )))
    }
}

/// `DataFusion` runtime with an owned, bounded spill directory.
pub struct ForgeRewriteRuntime {
    /// Runtime environment sharing the host-provisioned memory pool.
    runtime: Arc<RuntimeEnv>,
    /// Current Forge-owned child removed automatically on clean shutdown.
    spill_dir: tempfile::TempDir,
    /// Runtime-wide spill ceiling, equivalent to one operation under `tick`.
    spill_limit_bytes: u64,
}

impl ForgeRewriteRuntime {
    /// Construct a runtime rooted under the pod-owned spill directory.
    ///
    /// Only stale `forge-runtime-*` directories beneath the supplied root are
    /// removed. The root and unrelated entries are never deleted.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] for a zero ceiling and a Parquet
    /// or `DataFusion` error when the owned directory or runtime cannot be built.
    pub fn new(
        memory_pool: Arc<dyn MemoryPool>,
        pod_spill_root: &Path,
        spill_limit_bytes: u64,
    ) -> Result<Self, ForgeError> {
        if spill_limit_bytes == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge spill limit must be positive".to_owned(),
            });
        }
        std::fs::create_dir_all(pod_spill_root).map_err(|error| ForgeError::Parquet {
            detail: error.to_string(),
        })?;
        for entry in std::fs::read_dir(pod_spill_root).map_err(|error| ForgeError::Parquet {
            detail: error.to_string(),
        })? {
            let entry = entry.map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("forge-runtime-")
                && entry
                    .file_type()
                    .map_err(|error| ForgeError::Parquet {
                        detail: error.to_string(),
                    })?
                    .is_dir()
            {
                std::fs::remove_dir_all(entry.path()).map_err(|error| ForgeError::Parquet {
                    detail: error.to_string(),
                })?;
            }
        }
        let spill_dir = tempfile::Builder::new()
            .prefix("forge-runtime-")
            .tempdir_in(pod_spill_root)
            .map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?;
        let runtime = RuntimeEnvBuilder::new()
            .with_memory_pool(memory_pool)
            .with_temp_file_path(spill_dir.path())
            .with_max_temp_directory_size(spill_limit_bytes)
            .build()
            .map_err(ForgeError::DataFusion)?;
        Ok(Self {
            runtime: Arc::new(runtime),
            spill_dir,
            spill_limit_bytes,
        })
    }

    /// Return the configured operation spill ceiling.
    #[must_use]
    pub fn spill_limit_bytes(&self) -> u64 {
        self.spill_limit_bytes
    }

    /// Clone the runtime handle for one `DataFusion` task context.
    #[must_use]
    fn runtime(&self) -> Arc<RuntimeEnv> {
        debug_assert!(self.spill_dir.path().exists());
        Arc::clone(&self.runtime)
    }

    /// Return the owned spill path for diagnostics and tests.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn spill_path(&self) -> &Path {
        self.spill_dir.path()
    }
}

#[cfg(test)]
mod tests {
    use datafusion::execution::memory_pool::GreedyMemoryPool;

    use super::*;

    /// A zero spill ceiling cannot create an unbounded rewrite runtime.
    #[test]
    fn rewrite_runtime_rejects_zero_spill_limit() {
        let root = tempfile::tempdir().expect("test spill root must be created");
        let result =
            ForgeRewriteRuntime::new(Arc::new(GreedyMemoryPool::new(1_024)), root.path(), 0);
        assert!(matches!(result, Err(ForgeError::InvalidConfig { .. })));
    }

    /// Runtime startup removes only stale Forge-owned child directories.
    #[test]
    fn rewrite_runtime_cleans_owned_spill_child() {
        let root = tempfile::tempdir().expect("test spill root must be created");
        let stale = root.path().join("forge-runtime-stale");
        let unrelated = root.path().join("scribe-wal");
        std::fs::create_dir(&stale).expect("stale Forge child must be created");
        std::fs::create_dir(&unrelated).expect("unrelated child must be created");
        let runtime =
            ForgeRewriteRuntime::new(Arc::new(GreedyMemoryPool::new(1_024)), root.path(), 1_024)
                .expect("bounded rewrite runtime must be created");
        let active = runtime.spill_path().to_path_buf();
        assert!(!stale.exists());
        assert!(unrelated.exists());
        assert!(active.exists());
        drop(runtime);
        assert!(!active.exists());
        assert!(unrelated.exists());
    }

    /// Rotated names preserve one operation identity and ordered ordinals.
    #[test]
    fn terminal_attempt_cannot_republish() {
        let terminal_generation = Uuid::now_v7();
        let retry_generation = Uuid::now_v7();
        let terminal = deterministic_output_path(
            "tenants/a/table",
            ForgeAttemptGeneration::for_test(terminal_generation),
            0,
        );
        let retry = deterministic_output_path(
            "tenants/a/table",
            ForgeAttemptGeneration::for_test(retry_generation),
            0,
        );
        assert_ne!(terminal_generation, retry_generation);
        assert_ne!(terminal, retry);
        assert!(terminal.ends_with("-00000.parquet"));
        assert!(retry.ends_with("-00000.parquet"));
    }

    /// A writer recipe change produces a distinct deterministic storage path.
    #[test]
    fn writer_recipe_version_changes_deterministic_path() {
        let operation_id = Uuid::from_u128(42);
        let generation = ForgeAttemptGeneration::for_test(operation_id);
        let path = deterministic_output_path("tenants/a/table", generation, 0);
        assert_eq!(
            path,
            format!(
                "tenants/a/table/data/forge/{BIFROST_WRITER_RECIPE_VERSION}/{operation_id}-00000.parquet"
            )
        );
    }

    /// A later batch failure retains every finalized file and path for cleanup.
    #[test]
    fn batch_failure_preserves_complete_output_ownership() {
        let file = DataFileBuilder::default()
            .content(DataContentType::Data)
            .file_path("table/data/forge.parquet".to_owned())
            .file_format(DataFileFormat::Parquet)
            .partition(Struct::from_iter([Some(Literal::date(0))]))
            .partition_spec_id(0)
            .record_count(7)
            .file_size_in_bytes(128)
            .build()
            .expect("test output metadata must be complete");
        let mut state = RewriteBatchState::new();
        state.writer_rows = 7;
        state.record_output(file, "tenant/table/data/forge.parquet".to_owned());

        let result = state.fail(ForgeError::Shutdown);

        assert!(matches!(result.result, Err(ForgeError::Shutdown)));
        assert_eq!(result.files.len(), 1);
        assert_eq!(
            result.output_paths,
            ["tenant/table/data/forge.parquet".to_owned()]
        );
        assert_eq!(result.output_rows, 7);
    }
}
