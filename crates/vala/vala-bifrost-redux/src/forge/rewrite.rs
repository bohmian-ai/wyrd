//! Bounded, spillable staging-to-Iceberg rewrite execution.

use std::any::Any;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

use arrow::datatypes::SchemaRef;
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
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, SendableRecordBatchStream,
    execute_stream,
};
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use futures_util::{StreamExt, TryStreamExt};
use iceberg::spec::{DataContentType, DataFile, DataFileBuilder, DataFileFormat, Literal, Struct};
use opendal::Buffer;
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ArrowReaderOptions;
use parquet::arrow::async_reader::{AsyncFileReader, ParquetRecordBatchStreamBuilder};
use std::ops::Range;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::binpack::{CandidateFile, RewriteBin};
use super::compact::{ForgeObjectStore, project_by_name, validate_tenant_column};
use super::error::ForgeError;
use crate::catalog::TenantTableBinding;
use crate::parquet::writer_properties::bifrost_writer_properties;

/// Maximum rows decoded from one Parquet source batch.
const REWRITE_BATCH_ROWS: usize = 8_192;

/// Owned rewrite dependencies shared by every serialized Forge operation.
pub(crate) struct ForgeRewritePipeline {
    /// Process-wide `DataFusion` runtime and its owned spill directory.
    runtime: ForgeRewriteRuntime,
    /// Staging operator used for deterministic rewritten-object PUTs.
    staging: Arc<opendal::Operator>,
    /// Testable object-store seam used for source reads and failure cleanup.
    object_store: Arc<dyn ForgeObjectStore>,
    /// Upper bound for concurrently open source readers.
    max_concurrent_reads: usize,
    /// Encoded byte threshold checked before writing each next batch.
    output_file_bytes: u64,
}

impl fmt::Debug for ForgeRewritePipeline {
    /// Redact dependency internals while retaining useful pipeline limits.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ForgeRewritePipeline")
            .field("max_concurrent_reads", &self.max_concurrent_reads)
            .field("output_file_bytes", &self.output_file_bytes)
            .finish_non_exhaustive()
    }
}

/// Immutable inputs for one deterministic rewrite operation.
pub(crate) struct RewriteRequest<'a> {
    /// Stable identity shared by output names, Iceberg, and audit transitions.
    pub(crate) operation_id: Uuid,
    /// Validated tenant/table object-storage binding.
    pub(crate) binding: &'a TenantTableBinding,
    /// Physical Arrow schema registered by the destination Iceberg table.
    pub(crate) schema: SchemaRef,
    /// Ordered candidate files assigned to this rewrite bin.
    pub(crate) bin: &'a RewriteBin,
    /// Physical day partition shared by every candidate in the bin.
    pub(crate) partition_day: chrono::NaiveDate,
    /// Destination Iceberg table location used in `DataFile` paths.
    pub(crate) table_location: &'a str,
    /// Destination partition-spec identifier.
    pub(crate) partition_spec_id: i32,
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

impl ForgeRewritePipeline {
    /// Compose a rewrite pipeline from host-provisioned runtime dependencies.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when a concurrency or output limit
    /// is zero.
    pub(crate) fn new(
        runtime: ForgeRewriteRuntime,
        staging: Arc<opendal::Operator>,
        object_store: Arc<dyn ForgeObjectStore>,
        max_concurrent_reads: usize,
        output_file_bytes: u64,
    ) -> Result<Self, ForgeError> {
        if max_concurrent_reads == 0 || output_file_bytes == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "rewrite limits must be positive".to_owned(),
            });
        }
        Ok(Self {
            runtime,
            staging,
            object_store,
            max_concurrent_reads,
            output_file_bytes,
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
    /// Returns schema, tenant, Parquet, object-store, `DataFusion`, cancellation,
    /// spill-ceiling, or invariant failures. No durable SQL or Iceberg
    /// transition occurs in this method.
    pub(crate) async fn rewrite(
        &self,
        request: RewriteRequest<'_>,
        stop: &CancellationToken,
    ) -> Result<RewriteOutput, ForgeError> {
        let initial_spill = self.runtime.runtime.spilling_progress();
        if initial_spill.active_files_count != 0 {
            return Err(ForgeError::Invariant {
                detail: "Forge rewrite began with active spill files".to_owned(),
            });
        }
        let mut stream = self.sorted_stream(&request)?;
        let mut output_paths = Vec::new();
        let mut files = Vec::new();
        let mut writer: Option<ArrowWriter<Vec<u8>>> = None;
        let mut writer_rows = 0_u64;
        let mut input_rows = 0_u64;
        let mut output_rows = 0_u64;
        let mut peak_spill_bytes = 0_u64;

        let result = async {
            while let Some(batch) = stream.next().await {
                if stop.is_cancelled() {
                    return Err(ForgeError::Shutdown);
                }
                let batch = batch.map_err(|error| self.map_datafusion_error(error))?;
                if batch.num_rows() == 0 {
                    continue;
                }
                input_rows = input_rows.saturating_add(u64::try_from(batch.num_rows()).map_err(
                    |error| ForgeError::Invariant {
                        detail: format!("rewrite row count does not fit u64: {error}"),
                    },
                )?);
                let rotate = writer.as_ref().is_some_and(|active| {
                    writer_rows > 0
                        && u64::try_from(active.bytes_written() + active.in_progress_size())
                            .unwrap_or(u64::MAX)
                            >= self.output_file_bytes
                });
                if rotate {
                    let active = writer.take().ok_or_else(|| ForgeError::Invariant {
                        detail: "Forge output rotation lost its active writer".to_owned(),
                    })?;
                    let ordinal = files.len();
                    let (file, path) = self
                        .finish_output(active, writer_rows, ordinal, &request, stop)
                        .await?;
                    output_rows = output_rows.saturating_add(writer_rows);
                    output_paths.push(path);
                    files.push(file);
                    writer_rows = 0;
                }
                let active = writer.get_or_insert(
                    ArrowWriter::try_new(
                        Vec::new(),
                        Arc::clone(&request.schema),
                        Some(bifrost_writer_properties(batch.num_rows())),
                    )
                    .map_err(|error| ForgeError::Parquet {
                        detail: error.to_string(),
                    })?,
                );
                active.write(&batch).map_err(|error| ForgeError::Parquet {
                    detail: error.to_string(),
                })?;
                writer_rows = writer_rows.saturating_add(u64::try_from(batch.num_rows()).map_err(
                    |error| ForgeError::Invariant {
                        detail: format!("rewrite row count does not fit u64: {error}"),
                    },
                )?);
                peak_spill_bytes =
                    peak_spill_bytes.max(self.runtime.runtime.spilling_progress().current_bytes);
            }
            if let Some(active) = writer.take()
                && writer_rows > 0
            {
                let ordinal = files.len();
                let (file, path) = self
                    .finish_output(active, writer_rows, ordinal, &request, stop)
                    .await?;
                output_rows = output_rows.saturating_add(writer_rows);
                output_paths.push(path);
                files.push(file);
            }
            if files.is_empty() {
                return Err(ForgeError::Invariant {
                    detail: "Forge rewrite produced no non-empty output".to_owned(),
                });
            }
            Ok(())
        }
        .await;
        drop(stream);
        self.finalize_rewrite(
            result,
            files,
            output_paths,
            input_rows,
            output_rows,
            peak_spill_bytes,
        )
        .await
    }

    /// Validate cleanup and row invariants before transferring output ownership.
    ///
    /// # Errors
    ///
    /// Returns the original rewrite error, an active-spill invariant, or a row
    /// conservation invariant after deleting rewrite-owned outputs best-effort.
    async fn finalize_rewrite(
        &self,
        result: Result<(), ForgeError>,
        files: Vec<DataFile>,
        output_paths: Vec<String>,
        input_rows: u64,
        output_rows: u64,
        peak_spill_bytes: u64,
    ) -> Result<RewriteOutput, ForgeError> {
        if let Err(error) = result {
            self.cleanup_paths(&output_paths).await;
            return Err(error);
        }
        let final_spill = self.runtime.runtime.spilling_progress();
        if final_spill.active_files_count != 0 {
            self.cleanup_paths(&output_paths).await;
            return Err(ForgeError::Invariant {
                detail: "Forge rewrite ended with active spill files".to_owned(),
            });
        }
        if input_rows != output_rows {
            self.cleanup_paths(&output_paths).await;
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

    /// Build the single-partition spillable sort stream for one rewrite.
    ///
    /// # Errors
    ///
    /// Returns schema, invariant, or `DataFusion` plan-execution errors.
    fn sorted_stream(
        &self,
        request: &RewriteRequest<'_>,
    ) -> Result<SendableRecordBatchStream, ForgeError> {
        let source = Arc::new(StagingParquetExec::new(
            request.bin.files.clone(),
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
        execute_stream(sort, session.task_ctx()).map_err(ForgeError::DataFusion)
    }

    /// Close, PUT, and derive Iceberg metadata for one rotated output.
    ///
    /// # Errors
    ///
    /// Returns cancellation, Parquet, object-store, path, or metadata-builder
    /// failures. The caller remains responsible for deleting prior outputs.
    async fn finish_output(
        &self,
        writer: ArrowWriter<Vec<u8>>,
        rows: u64,
        ordinal: usize,
        request: &RewriteRequest<'_>,
        stop: &CancellationToken,
    ) -> Result<(DataFile, String), ForgeError> {
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        let bytes = writer.into_inner().map_err(|error| ForgeError::Parquet {
            detail: error.to_string(),
        })?;
        let object_path = deterministic_output_path(
            &request.binding.object_prefix,
            request.operation_id,
            ordinal,
        );
        request
            .binding
            .validate_object_path(&object_path)
            .ok_or_else(|| ForgeError::Invariant {
                detail: format!("Forge output escaped table prefix: {object_path}"),
            })?;
        self.staging
            .write(&object_path, Buffer::from(bytes.clone()))
            .await
            .map_err(ForgeError::ObjectStore)?;
        let table_path = format!(
            "{}/data/forge-{}-{ordinal:05}.parquet",
            request.table_location.trim_end_matches('/'),
            request.operation_id
        );
        let partition = Struct::from_iter([Some(Literal::date(
            request.partition_day.num_days_from_ce() - 719_163,
        ))]);
        let file = DataFileBuilder::default()
            .content(DataContentType::Data)
            .file_path(table_path)
            .file_format(DataFileFormat::Parquet)
            .partition(partition)
            .partition_spec_id(request.partition_spec_id)
            .record_count(rows)
            .file_size_in_bytes(u64::try_from(bytes.len()).map_err(|error| {
                ForgeError::Invariant {
                    detail: format!("output size does not fit u64: {error}"),
                }
            })?)
            .build()
            .map_err(|error| ForgeError::Invariant {
                detail: format!("Forge output metadata is incomplete: {error}"),
            });
        let file = match file {
            Ok(file) => file,
            Err(error) => {
                self.cleanup_paths(std::slice::from_ref(&object_path)).await;
                return Err(error);
            }
        };
        Ok((file, object_path))
    }

    /// Delete rewrite-owned paths without masking the original operation error.
    async fn cleanup_paths(&self, paths: &[String]) {
        for path in paths {
            if let Err(cleanup_error) = self.object_store.delete(path).await {
                tracing::warn!(
                    path,
                    error = %cleanup_error,
                    "Forge rewrite output cleanup failed"
                );
            }
        }
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
fn deterministic_output_path(prefix: &str, operation_id: Uuid, ordinal: usize) -> String {
    format!(
        "{}/data/forge-{operation_id}-{ordinal:05}.parquet",
        prefix.trim_end_matches('/')
    )
}

/// `DataFusion` leaf plan that owns bounded staging-file decoding.
#[derive(Debug)]
struct StagingParquetExec {
    /// Ordered durable candidate files.
    files: Vec<CandidateFile>,
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
        files: Vec<CandidateFile>,
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

/// Open one bounded Parquet stream while holding one read permit.
///
/// # Errors
///
/// Returns an execution error for invalid paths, metadata reads, or Parquet
/// stream construction failures.
async fn open_staging_stream(
    file: CandidateFile,
    binding: TenantTableBinding,
    schema: SchemaRef,
    store: Arc<dyn ForgeObjectStore>,
    permits: Arc<tokio::sync::Semaphore>,
) -> datafusion::error::Result<
    BoxStream<'static, datafusion::error::Result<arrow::record_batch::RecordBatch>>,
> {
    let path = binding.validate_object_path(&file.path).ok_or_else(|| {
        datafusion::error::DataFusionError::Execution(format!(
            "Forge staging input escaped table prefix: {}",
            file.path
        ))
    })?;
    let permit = permits
        .acquire_owned()
        .await
        .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
    let size = store
        .stat(&path)
        .await
        .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?
        .content_length();
    let reader = ForgeParquetReader { store, path, size };
    let stream = ParquetRecordBatchStreamBuilder::new(reader)
        .await
        .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?
        .with_batch_size(REWRITE_BATCH_ROWS)
        .build()
        .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
    let tenant = binding.tenant;
    let projected = async_stream::try_stream! {
        futures_util::pin_mut!(stream);
        while let Some(batch) = stream.next().await {
            let batch = batch.map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
            validate_tenant_column(&batch, tenant)
                .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
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
        let stream = futures_util::stream::iter(files)
            .map(move |file| {
                let binding = binding.clone();
                let schema = Arc::clone(&schema);
                let store = Arc::clone(&store);
                let permits = Arc::clone(&permits);
                async move { open_staging_stream(file, binding, schema, store, permits).await }
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
    fn rotation_keeps_one_operation_identity() {
        let operation_id = Uuid::from_u128(42);
        let first = deterministic_output_path("tenants/a/table", operation_id, 0);
        let second = deterministic_output_path("tenants/a/table", operation_id, 1);
        assert!(first.contains(&operation_id.to_string()));
        assert!(second.contains(&operation_id.to_string()));
        assert!(first.ends_with("-00000.parquet"));
        assert!(second.ends_with("-00001.parquet"));
    }
}
