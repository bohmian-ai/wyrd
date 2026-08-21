//! Bounded, spillable staging-to-Iceberg rewrite execution.

use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use chrono::Datelike;
use datafusion::execution::TaskContext;
use datafusion::execution::memory_pool::{
    MemoryConsumer, MemoryLimit, MemoryPool, MemoryReservation,
};
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
use iceberg::arrow::NanValueCountVisitor;
use sha2::{Digest, Sha256};

use crate::parquet::object_uploader::{
    BifrostParquetUploader, BifrostUploadRole, ParquetObjectIdentity,
};

/// Attempt-local aggregate pool view recording the root reservation peak.
#[derive(Debug)]
struct ForgeAttemptMemoryPool {
    /// Aggregate production pool issued by the root lease.
    inner: Arc<dyn MemoryPool>,
    /// Largest aggregate reservation observed after successful growth.
    peak_bytes: Arc<AtomicU64>,
}

impl ForgeAttemptMemoryPool {
    /// Wraps the one aggregate attempt lease without creating child ledgers.
    fn new(inner: Arc<dyn MemoryPool>, peak_bytes: Arc<AtomicU64>) -> Self {
        Self { inner, peak_bytes }
    }

    /// Retains the largest aggregate reservation after successful pool growth.
    fn observe_peak(&self) {
        self.peak_bytes.fetch_max(
            u64::try_from(self.inner.reserved()).unwrap_or(u64::MAX),
            Ordering::AcqRel,
        );
    }
}

impl MemoryPool for ForgeAttemptMemoryPool {
    /// Delegates registration to the aggregate pool.
    fn register(&self, consumer: &MemoryConsumer) {
        self.inner.register(consumer);
    }

    /// Delegates removal to the aggregate pool.
    fn unregister(&self, consumer: &MemoryConsumer) {
        self.inner.unregister(consumer);
    }

    /// Grows the one aggregate reservation and records its peak.
    fn grow(&self, reservation: &MemoryReservation, additional: usize) {
        self.inner.grow(reservation, additional);
        self.observe_peak();
    }

    /// Releases bytes from the one aggregate reservation.
    fn shrink(&self, reservation: &MemoryReservation, shrink: usize) {
        self.inner.shrink(reservation, shrink);
    }

    /// Attempts aggregate growth without claiming a fungible child ledger.
    fn try_grow(
        &self,
        reservation: &MemoryReservation,
        additional: usize,
    ) -> datafusion::error::Result<()> {
        self.inner.try_grow(reservation, additional)?;
        self.observe_peak();
        Ok(())
    }

    /// Returns aggregate live ownership.
    fn reserved(&self) -> usize {
        self.inner.reserved()
    }

    /// Returns the aggregate finite limit.
    fn memory_limit(&self) -> MemoryLimit {
        self.inner.memory_limit()
    }
}
/// Lease-owned rewrite resources for exactly one Forge attempt.
///
/// The attempt's `DataFusion` runtime is built from the pool and scratch bytes
/// the root governor granted, so no attempt can execute against capacity it did
/// not acquire. Field order is load-bearing: `runtime` is declared before
/// `lease`, so the attempt drops its runtime — releasing the spill child — before
/// the exact root lease is returned to the governor.
pub(crate) struct ForgeAttemptResources {
    /// Physical table projection charged to and retained by this attempt.
    projection: Option<
        crate::catalog::PhysicalTableProjection<crate::resources::ForgeRewriteMemoryReservation>,
    >,
    /// Attempt runtime shared with the attempt-local execution pipeline view.
    runtime: Option<Arc<ForgeRewriteRuntime>>,
    /// Exact memory and scratch lease returned to the root governor on drop.
    lease: Option<crate::resources::ForgeRewriteResources>,
    /// First finalization result retained for idempotent explicit and drop paths.
    release_result: Option<crate::resources::ForgeResourceReleaseResult>,
    /// Attempt-local resident peak shared with the leased pool wrapper.
    peak_memory_bytes: Arc<AtomicU64>,
    /// Persisted and acquired totals recorded once at finalization.
    observation: super::metrics::ForgeResourceObservation,
    /// Fixed-cardinality metric owner receiving the final lifecycle event.
    telemetry: Arc<super::metrics::ForgeTelemetry>,
}

/// Splits and materializes one binding beneath an admitted Forge attempt.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when logical validation or binding
/// reconciliation fails and [`ForgeError::Capacity`] when the admitted attempt
/// pool refuses the exact projection bytes.
fn project_forge_binding(
    lease: &crate::resources::ForgeRewriteResources,
    binding: &crate::catalog::TenantTableBinding,
) -> Result<
    crate::catalog::PhysicalTableProjection<crate::resources::ForgeRewriteMemoryReservation>,
    ForgeError,
> {
    let logical = crate::catalog::LogicalTableIdentity::try_new(
        &binding.tenant,
        &binding.tenant,
        &binding.table_ref,
    )
    .map_err(|error| ForgeError::Invariant {
        detail: format!("Forge logical table identity is invalid: {error:?}"),
    })?;
    let projection = logical
        .try_project(crate::catalog::PhysicalProjectionRole::Forge, |facts| {
            lease.try_split_memory("forge_physical_table_projection", facts.material_bytes)
        })
        .map_err(|error| ForgeError::Capacity {
            detail: format!("Forge physical table projection refused: {error}"),
        })?;
    if projection.object_prefix() != binding.object_prefix
        || projection.namespace() != binding.iceberg_namespace.to_string()
    {
        return Err(ForgeError::Invariant {
            detail: "Forge projection diverged from the validated table binding".to_owned(),
        });
    }
    Ok(projection)
}

impl ForgeAttemptResources {
    /// Atomically acquires the exact request, then builds its attempt runtime.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Capacity`] when the root governor refuses either
    /// counter, a runtime construction error when the owned spill child cannot
    /// be created, and [`ForgeError::Invariant`] when the constructed runtime
    /// did not retain the lease-issued pool.
    pub(crate) fn acquire(
        resources: &crate::resources::ForgeResources,
        request: crate::resources::ForgeRewriteRequest,
        binding: &crate::catalog::TenantTableBinding,
        pod_spill_root: &Path,
        task_id: Uuid,
        attempt_id: Uuid,
        telemetry: Arc<super::metrics::ForgeTelemetry>,
    ) -> Result<Self, ForgeError> {
        let lease =
            resources
                .try_acquire_rewrite(request)
                .map_err(|error| ForgeError::Capacity {
                    detail: format!("{error}; request={request:?}"),
                })?;
        let projection = project_forge_binding(&lease, binding)?;
        let decoded_batch_bytes =
            usize::try_from(request.envelope.decoded_batch_bytes).map_err(|_| {
                ForgeError::Capacity {
                    detail: "decoded batch exceeds this platform".to_owned(),
                }
            })?;
        let output_allowance = usize::try_from(
            request
                .envelope
                .encoder_buffer_bytes
                .checked_add(request.envelope.upload_chunk_bytes)
                .ok_or_else(|| ForgeError::Invariant {
                    detail: "Forge output allowance overflows".to_owned(),
                })?,
        )
        .map_err(|_| ForgeError::Capacity {
            detail: "Forge output allowance exceeds this platform".to_owned(),
        })?;
        let peak_memory_bytes = Arc::new(AtomicU64::new(0));
        let pool: Arc<dyn MemoryPool> = Arc::new(ForgeAttemptMemoryPool::new(
            lease.memory_pool(),
            Arc::clone(&peak_memory_bytes),
        ));
        let sort_spill_bytes = request.envelope.sort_spill_bytes;
        let runtime = ForgeRewriteRuntime::new_attempt(
            Arc::clone(&pool),
            pod_spill_root,
            task_id,
            attempt_id,
            ForgeAttemptEnvelope {
                spill_limit: lease.scratch_bytes(),
                sort_spill_limit_bytes: sort_spill_bytes,
                output_allowance_bytes: output_allowance,
                decoded_batch_bytes,
                sort_merge_reservation_bytes: usize::try_from(
                    request.envelope.sort_merge_reservation_bytes,
                )
                .map_err(|_| ForgeError::Capacity {
                    detail: "sort merge reservation exceeds this platform".to_owned(),
                })?,
                encoder_flush_bytes: usize::try_from(request.envelope.encoder_buffer_bytes / 2)
                    .map_err(|_| ForgeError::Capacity {
                        detail: "encoder flush threshold exceeds this platform".to_owned(),
                    })?,
                upload_chunk_bytes: usize::try_from(request.envelope.upload_chunk_bytes).map_err(
                    |_| ForgeError::Capacity {
                        detail: "upload chunk exceeds this platform".to_owned(),
                    },
                )?,
                footer_encoded_bytes: usize::try_from(request.envelope.footer_encoded_bytes)
                    .map_err(|_| ForgeError::Capacity {
                        detail: "encoded footer exceeds this platform".to_owned(),
                    })?,
                footer_decode_workspace_bytes: usize::try_from(
                    request.envelope.footer_decode_workspace_bytes,
                )
                .map_err(|_| ForgeError::Capacity {
                    detail: "footer decode workspace exceeds this platform".to_owned(),
                })?,
            },
        )?;
        if !runtime.uses_memory_pool(&pool) {
            return Err(ForgeError::Invariant {
                detail: "Forge rewrite runtime did not retain its operation lease pool".to_owned(),
            });
        }
        Ok(Self {
            projection: Some(projection),
            runtime: Some(Arc::new(runtime)),
            lease: Some(lease),
            release_result: None,
            peak_memory_bytes,
            observation: super::metrics::ForgeResourceObservation {
                planned_memory: u64::try_from(request.memory_bytes).unwrap_or(u64::MAX),
                acquired_memory: u64::try_from(request.memory_bytes).unwrap_or(u64::MAX),
                peak_memory: 0,
                planned_scratch: request.scratch_bytes,
                acquired_scratch: request.scratch_bytes,
                peak_scratch: 0,
            },
            telemetry,
        })
    }

    /// Creates the attempt-local execution view backed by this leased runtime.
    pub(crate) fn pipeline(&self, base: &ForgeRewritePipeline) -> ForgeRewritePipeline {
        base.for_runtime(Arc::clone(
            self.runtime
                .as_ref()
                .expect("invariant: active attempt retains its runtime"),
        ))
    }

    /// Returns the runtime this attempt lends to its execution pipeline view.
    #[cfg(test)]
    pub(crate) fn runtime(&self) -> &ForgeRewriteRuntime {
        self.runtime
            .as_ref()
            .expect("invariant: active attempt retains its runtime")
    }

    /// Returns the exact pool this attempt leased, for lifecycle assertions.
    #[cfg(test)]
    pub(crate) fn memory_pool(&self) -> Arc<dyn MemoryPool> {
        Arc::clone(
            &self
                .runtime
                .as_ref()
                .expect("invariant: active attempt retains its runtime")
                .runtime
                .memory_pool,
        )
    }

    /// Drops runtime ownership before explicitly releasing or poisoning the lease.
    fn finish(&mut self) -> crate::resources::ForgeResourceReleaseResult {
        if let Some(result) = self.release_result {
            return result;
        }
        let runtime = self
            .runtime
            .take()
            .expect("invariant: unfinished attempt retains its runtime");
        self.observation.peak_memory = self.peak_memory_bytes.load(Ordering::Acquire);
        self.observation.peak_scratch = runtime.scratch_peak_bytes();
        let surviving_handle = Arc::strong_count(&runtime) != 1;
        drop(runtime);
        drop(
            self.projection
                .take()
                .expect("invariant: unfinished attempt retains its physical projection"),
        );
        let mut lease = self
            .lease
            .take()
            .expect("invariant: unfinished attempt retains its lease");
        let result = if surviving_handle {
            lease.poison_without_release()
        } else if let Err(error) = lease.release() {
            tracing::error!(%error, "Forge attempt resource release failed");
            crate::resources::ForgeResourceReleaseResult::Poisoned
        } else {
            crate::resources::ForgeResourceReleaseResult::Released
        };
        self.telemetry.record_attempt_resources(self.observation);
        self.telemetry.record_attempt_release(result);
        self.release_result = Some(result);
        result
    }
}

impl Drop for ForgeAttemptResources {
    /// Verifies the execution view released the runtime before the lease returns.
    ///
    /// A surviving pipeline view would keep the spill child alive past the
    /// governor's scratch release, so a violation is reported rather than
    /// silently tolerated. Dropping proceeds either way; the lease must return.
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
use iceberg::spec::{DataContentType, DataFileBuilder, DataFileFormat};
use iceberg::spec::{DataFile, Literal, SchemaRef as IcebergSchemaRef, Struct};
use iceberg::writer::file_writer::ParquetWriter;
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ArrowReaderOptions;
use parquet::arrow::async_reader::{AsyncFileReader, ParquetRecordBatchStreamBuilder};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::compact::{ForgeObjectStore, project_by_name, validate_tenant_column};
use super::error::ForgeError;
use super::lease::ForgeLease;
use super::path::{catalog_path_to_object_key, validate_table_location};
#[cfg(feature = "test-support")]
use super::planner::ForgePlanCandidate;
use super::planner::{ForgeCapacity, ForgePlanner};
use crate::catalog::TenantTableBinding;
use crate::parquet::memory::{
    BifrostArrowLogicalSizer, BifrostParquetMemoryEnvelope, BoundedRowSlice, MAX_FILE_BYTES,
    MAX_FILE_ROW_GROUPS, MAX_LOGICAL_ROW_GROUP_BYTES, MAX_ROW_GROUP_ROWS,
};
use crate::parquet::writer_properties::{
    BIFROST_WRITER_RECIPE_VERSION, bifrost_writer_properties_with_metadata,
};
use crate::resources::ForgeRewriteRequest;
use vala_sql::OperatorPool;
use vala_sql::row_types::forge_tasks::ForgeTaskEstimates;

/// Maximum rows decoded from one Parquet source batch.
const REWRITE_BATCH_ROWS: usize = 8_192;
/// Byte threshold that forces the current Parquet row group to flush.
#[cfg(test)]
pub(crate) const ROW_GROUP_FLUSH_BYTES: usize = 32 * 1024 * 1024;
/// Target decoded batch bytes used by the envelope and footer refusal rule.
#[cfg(test)]
pub(crate) const DECODED_BATCH_TARGET_BYTES: usize = 16 * 1024 * 1024;
/// Fixed working set every rewrite plan needs regardless of input size.
///
/// The rewrite executes a `DataFusion` sort/merge into one Parquet writer.
/// That plan reserves whole decoded batches for `ExternalSorterMerge`, a sort
/// spill reservation, and the output writer's buffers before it can make any
/// progress, so a demand derived purely from selected input bytes starves a
/// small-file compaction — the most common maintenance case. Requests are
/// therefore raised to this floor and still bounded by the validated
/// [`ForgeCapacity`] memory ceiling.
pub(crate) const REWRITE_WORKING_SET_FLOOR_BYTES: u64 =
    crate::resources::FORGE_MEMORY_FLOOR_BYTES as u64;

/// Upper bound for `DataFusion` spill handles to finish dropping after cancel.
const SPILL_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Largest combined sort/output scratch observation in serialized production tests.
#[cfg(feature = "test-support")]
static TEST_SCRATCH_PEAK_BYTES: AtomicU64 = AtomicU64::new(0);

/// Resets the production scratch high-water observation used by integration tests.
#[cfg(feature = "test-support")]
pub fn reset_scratch_peak_for_test() {
    TEST_SCRATCH_PEAK_BYTES.store(0, Ordering::Release);
}

/// Returns the production scratch high-water observation since the last reset.
#[cfg(feature = "test-support")]
#[must_use]
pub fn scratch_peak_for_test() -> u64 {
    TEST_SCRATCH_PEAK_BYTES.load(Ordering::Acquire)
}

impl ForgeRewriteRequest {
    /// Forms the one exact rewrite demand from a durable claim's estimates.
    ///
    /// The persisted estimates already passed planner capacity classification,
    /// so this operation only re-proves the invariants the resource root
    /// depends on: positive demand, representability in this platform's
    /// `usize`, and conformance to the same validated [`ForgeCapacity`]
    /// ceilings. The durable estimate is then raised only to the executable
    /// rewrite working set (see [`rewrite_working_set_bytes`]); it is never
    /// inflated toward a configuration ceiling by the estimate itself.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Capacity`] when either estimate is zero, exceeds
    /// its validated ceiling, or exceeds this platform's `usize` domain.
    pub(crate) fn from_claim(
        estimates: &ForgeTaskEstimates,
        capacity: ForgeCapacity,
    ) -> Result<Self, ForgeError> {
        estimates
            .validate()
            .map_err(|error| ForgeError::Invariant {
                detail: error.to_string(),
            })?;
        ForgePlanner::new(capacity)
            .validate_candidate_estimates(estimates.memory_bytes, estimates.spill_bytes)?;
        let envelope = estimates.envelope.ok_or_else(|| ForgeError::Invariant {
            detail: "legacy Forge envelope is not executable".to_owned(),
        })?;
        envelope.validate().map_err(|error| ForgeError::Invariant {
            detail: error.to_string(),
        })?;
        Ok(Self {
            envelope,
            memory_bytes: usize::try_from(envelope.memory_bytes().map_err(|error| {
                ForgeError::Invariant {
                    detail: error.to_string(),
                }
            })?)
            .map_err(|_| ForgeError::Capacity {
                detail: "planned memory bytes exceed this platform".to_owned(),
            })?,
            scratch_bytes: envelope
                .scratch_bytes()
                .map_err(|error| ForgeError::Invariant {
                    detail: error.to_string(),
                })?,
            reader_permits: envelope.reader_permits,
        })
    }

    /// Forms the exact rewrite demand a directly replaced live group requires.
    ///
    /// The group is first mapped through the sole production candidate
    /// estimation algorithm ([`ForgePlanCandidate::from_live_group`]), so the
    /// resulting request is byte-identical to the one
    /// [`Self::from_claim`] would form for the task the planner would have
    /// persisted for the same group, including the shared working-set floor.
    /// Configuration ceilings bound the request; they are never themselves the
    /// requested demand.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the group's input bytes overflow
    /// or its file count exceeds the durable parallelism domain, and
    /// [`ForgeError::Capacity`] for the same conditions as [`Self::from_claim`].
    #[cfg(feature = "test-support")]
    pub(crate) fn from_live_group(
        group: &super::right_size::IcebergRewriteGroup,
        capacity: ForgeCapacity,
    ) -> Result<Self, ForgeError> {
        let candidate = ForgePlanCandidate::from_live_group(
            group,
            usize::from(capacity.max_parallelism),
            capacity,
        )?;
        ForgePlanner::new(capacity)
            .validate_candidate_estimates(candidate.memory_bytes, candidate.spill_bytes)?;
        let envelope = super::planner::ForgeEnvelopeSizer::size(
            candidate.bytes,
            candidate.inputs.len(),
            usize::from(candidate.parallelism),
            capacity,
        )?;
        Ok(Self {
            envelope,
            memory_bytes: usize::try_from(envelope.memory_bytes().map_err(|error| {
                ForgeError::Invariant {
                    detail: error.to_string(),
                }
            })?)
            .map_err(|_| ForgeError::Capacity {
                detail: "planned memory bytes exceed this platform".to_owned(),
            })?,
            scratch_bytes: envelope
                .scratch_bytes()
                .map_err(|error| ForgeError::Invariant {
                    detail: error.to_string(),
                })?,
            reader_permits: candidate.parallelism,
        })
    }
}

/// Owned rewrite dependencies shared by every serialized Forge operation.
pub(crate) struct ForgeRewritePipeline {
    /// Attempt-local runtime installed only on the worker-owned execution view.
    runtime: Option<Arc<ForgeRewriteRuntime>>,
    /// Staging operator used for deterministic rewritten-object PUTs.
    staging: Arc<opendal::Operator>,
    /// Shared physical-evidence producer for deterministic rewritten objects.
    uploader: Arc<BifrostParquetUploader>,
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
    /// Returns the durable generation used for operation ancestry.
    #[must_use]
    pub(crate) const fn as_uuid(self) -> Uuid {
        self.0
    }

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

/// Complete multi-file result retaining verified paths for publication settlement.
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

/// Buffered attempt-owned file writer that refuses physical scratch overrun.
///
/// The limit is checked before bytes reach the inner [`BufWriter`], so neither
/// its buffer nor the filesystem can grow beyond the output sibling term.
struct ScratchBoundedWriter {
    /// Buffered file receiving admitted Parquet bytes.
    inner: BufWriter<std::fs::File>,
    /// Bytes already accepted by this writer.
    written: u64,
    /// Exact pending-output sibling ceiling.
    limit: u64,
}

impl ScratchBoundedWriter {
    /// Wraps one newly created output file with its pre-reserved byte ceiling.
    fn new(file: std::fs::File, limit: u64) -> Self {
        Self {
            inner: BufWriter::new(file),
            written: 0,
            limit,
        }
    }

    /// Returns the file handle used to fsync the sealed output.
    #[must_use]
    fn file(&self) -> &std::fs::File {
        self.inner.get_ref()
    }
}

impl Write for ScratchBoundedWriter {
    /// Writes one encoder buffer only when it fits the pre-reserved ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::ErrorKind::StorageFull`] without forwarding any bytes
    /// when the complete buffer would cross the ceiling, or returns the inner
    /// scratch-file error.
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let requested = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        if self.written.saturating_add(requested) > self.limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::StorageFull,
                "Forge output scratch ceiling exceeded",
            ));
        }
        let written = self.inner.write(buffer)?;
        self.written = self
            .written
            .saturating_add(u64::try_from(written).unwrap_or(u64::MAX));
        Ok(written)
    }

    /// Flushes admitted bytes to the attempt-owned file.
    ///
    /// # Errors
    ///
    /// Returns the inner scratch-file flush error.
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Mutable writer and accounting state for one streamed rewrite operation.
///
/// The state owns every output path as soon as its PUT succeeds and retains
/// that ownership across later batch failures so rewrite finalization can
/// delete the complete accumulated set.
struct RewriteBatchState {
    /// Currently open bounded Parquet writer, if any batch has been accepted.
    writer: Option<ArrowWriter<ScratchBoundedWriter>>,
    /// Footer-phase capability retained for the lifetime of the active writer.
    footer_phase: Option<ForgeFooterPhaseGuard>,
    /// Attempt-owned directory containing disposable encoded outputs.
    scratch_dir: PathBuf,
    /// Path of the active encoded output, present exactly when `writer` exists.
    scratch_path: Option<PathBuf>,
    /// Rows buffered in `writer` and not yet transferred to an output object.
    writer_rows: u64,
    /// Flushed row groups retained by the active writer.
    writer_row_groups: usize,
    /// Logical bytes accumulated in the active row group.
    writer_row_group_logical_bytes: u64,
    /// Rows accumulated in the active row group.
    writer_row_group_rows: usize,
    /// Iceberg metadata accumulated for successfully finalized outputs.
    files: Vec<DataFile>,
    /// Verified object paths retained for publication settlement or orphan GC.
    output_paths: Vec<String>,
    /// Rows accepted from source batches.
    input_rows: u64,
    /// Rows transferred into successfully finalized output objects.
    output_rows: u64,
    /// Peak spill bytes observed while consuming sorted batches.
    peak_spill_bytes: u64,
    /// Shared `DataFusion` pool reservation for encoded output capacity.
    _output_reservation: Option<datafusion::execution::memory_pool::MemoryReservation>,
    /// Attempt scratch held before the first output file can be created.
    _output_scratch: Option<OutputScratchReservation>,
    /// Physical byte ceiling enforced before every scratch-file write.
    output_scratch_limit_bytes: u64,
    /// Attempt-local conservative scratch peak shared with the resource owner.
    scratch_peak_bytes: Option<Arc<AtomicU64>>,
    /// Per-output NaN counts keyed by Iceberg field ID.
    nan_value_counts: NanValueCountVisitor,
}

impl RewriteBatchState {
    /// Create empty state for isolated batch-state tests and callers without
    /// a shared `DataFusion` reservation.
    #[cfg(test)]
    fn new() -> Self {
        Self::with_reservation(None, None, std::env::temp_dir())
    }

    /// Create state with an optional shared-pool output reservation.
    fn with_reservation(
        reservation: Option<datafusion::execution::memory_pool::MemoryReservation>,
        output_scratch: Option<OutputScratchReservation>,
        scratch_dir: PathBuf,
    ) -> Self {
        let output_scratch_limit_bytes = output_scratch
            .as_ref()
            .map_or(u64::MAX, |value| value.bytes);
        let scratch_peak_bytes = output_scratch
            .as_ref()
            .map(|value| Arc::clone(&value.peak_bytes));
        Self {
            writer: None,
            footer_phase: None,
            scratch_dir,
            scratch_path: None,
            writer_rows: 0,
            writer_row_groups: 0,
            writer_row_group_logical_bytes: 0,
            writer_row_group_rows: 0,
            files: Vec::new(),
            output_paths: Vec::new(),
            input_rows: 0,
            output_rows: 0,
            peak_spill_bytes: 0,
            _output_reservation: reservation,
            _output_scratch: output_scratch,
            output_scratch_limit_bytes,
            scratch_peak_bytes,
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
                && (self.writer_row_groups >= MAX_FILE_ROW_GROUPS
                    || u64::try_from(writer.bytes_written() + writer.in_progress_size())
                        .unwrap_or(u64::MAX)
                        >= output_file_bytes)
        })
    }

    /// Report whether appending another physical group would cross the hard
    /// writer-v2 row-group ceiling.
    fn would_exceed_row_groups(&self, slices: &[BoundedRowSlice]) -> bool {
        if self.writer.is_none() {
            return false;
        }
        let mut groups = self.writer_row_groups;
        let mut logical_bytes = self.writer_row_group_logical_bytes;
        let mut rows = self.writer_row_group_rows;
        for slice in slices {
            let crosses_bytes = logical_bytes
                .checked_add(slice.logical_bytes)
                .is_none_or(|bytes| bytes > MAX_LOGICAL_ROW_GROUP_BYTES);
            let crosses_rows = rows
                .checked_add(slice.len)
                .is_none_or(|rows| rows > MAX_ROW_GROUP_ROWS);
            if logical_bytes == 0 || crosses_bytes || crosses_rows {
                groups = groups.saturating_add(1);
                logical_bytes = 0;
                rows = 0;
            }
            logical_bytes = logical_bytes.saturating_add(slice.logical_bytes);
            rows = rows.saturating_add(slice.len);
        }
        groups > MAX_FILE_ROW_GROUPS
    }

    /// Append one validated batch to the active bounded Parquet writer.
    ///
    /// The caller holds the fixed encoder and upload reservation before this
    /// method can create its scratch-backed writer. Encoding never grows that
    /// reservation after allocation. The writer flushes its current row group
    /// at the persisted encoder threshold so encoded buffering stays within
    /// the fixed envelope while completed bytes stream to attempt-owned scratch.
    ///
    /// # Errors
    ///
    /// Returns a Parquet or scratch-write failure, or
    /// [`ForgeError::Invariant`] when the batch row count cannot fit the
    /// rewrite's `u64` counters.
    fn write_batch(
        &mut self,
        schema: &SchemaRef,
        iceberg_schema: &IcebergSchemaRef,
        batch: &RecordBatch,
        row_groups: &[crate::parquet::memory::BoundedRowSlice],
        object_identity: &str,
        encoder_flush_bytes: usize,
    ) -> Result<(), ForgeError> {
        self.nan_value_counts
            .compute(Arc::clone(iceberg_schema), batch.clone())
            .map_err(|error| ForgeError::Invariant {
                detail: format!("Forge NaN metric derivation failed: {error}"),
            })?;
        if self.writer.is_none() {
            let scratch_path = self
                .scratch_dir
                .join(format!("rewrite-output-{:05}.parquet", self.files.len()));
            let file =
                std::fs::File::create(&scratch_path).map_err(|error| ForgeError::ScratchIo {
                    kind: error.kind(),
                    detail: format!("create {}: {error}", scratch_path.display()),
                })?;
            let metadata = BifrostParquetMemoryEnvelope::metadata_for_batch(batch, object_identity)
                .map_err(|detail| ForgeError::DataRefusal { detail })?;
            self.writer = Some(
                ArrowWriter::try_new(
                    ScratchBoundedWriter::new(
                        file,
                        self.output_scratch_limit_bytes
                            .min(crate::parquet::memory::MAX_FILE_BYTES),
                    ),
                    Arc::clone(schema),
                    Some(bifrost_writer_properties_with_metadata(
                        batch.num_rows(),
                        metadata,
                    )),
                )
                .map_err(|error| ForgeError::Parquet {
                    detail: error.to_string(),
                })?,
            );
            self.scratch_path = Some(scratch_path);
        }
        let writer = self.writer.as_mut().ok_or_else(|| ForgeError::Invariant {
            detail: "Forge rewrite failed to retain its active writer".to_owned(),
        })?;
        for slice in row_groups {
            let crosses_bytes = self
                .writer_row_group_logical_bytes
                .checked_add(slice.logical_bytes)
                .is_none_or(|bytes| bytes > MAX_LOGICAL_ROW_GROUP_BYTES);
            let crosses_rows = self
                .writer_row_group_rows
                .checked_add(slice.len)
                .is_none_or(|rows| rows > MAX_ROW_GROUP_ROWS);
            if self.writer_row_group_logical_bytes > 0 && (crosses_bytes || crosses_rows) {
                writer.flush().map_err(|error| ForgeError::ScratchIo {
                    kind: std::io::ErrorKind::Other,
                    detail: format!("flush bounded Parquet row group: {error}"),
                })?;
                self.writer_row_group_logical_bytes = 0;
                self.writer_row_group_rows = 0;
            }
            if self.writer_row_group_logical_bytes == 0 {
                self.writer_row_groups = self.writer_row_groups.saturating_add(1);
            }
            writer
                .write(&batch.slice(slice.offset, slice.len))
                .map_err(|error| ForgeError::ScratchIo {
                    kind: std::io::ErrorKind::StorageFull,
                    detail: format!("write bounded Parquet scratch output: {error}"),
                })?;
            self.writer_row_group_logical_bytes = self
                .writer_row_group_logical_bytes
                .saturating_add(slice.logical_bytes);
            self.writer_row_group_rows = self.writer_row_group_rows.saturating_add(slice.len);
        }
        if writer.in_progress_size() >= encoder_flush_bytes {
            writer.flush().map_err(|error| ForgeError::ScratchIo {
                kind: std::io::ErrorKind::Other,
                detail: format!("flush Parquet row group: {error}"),
            })?;
        }
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
    fn take_writer(
        &mut self,
    ) -> Result<
        (
            ArrowWriter<ScratchBoundedWriter>,
            PathBuf,
            ForgeFooterPhaseGuard,
        ),
        ForgeError,
    > {
        let writer = self.writer.take().ok_or_else(|| ForgeError::Invariant {
            detail: "Forge output rotation lost its active writer".to_owned(),
        })?;
        let path = self
            .scratch_path
            .take()
            .ok_or_else(|| ForgeError::Invariant {
                detail: "Forge output rotation lost its scratch path".to_owned(),
            })?;
        let footer_phase = self
            .footer_phase
            .take()
            .ok_or_else(|| ForgeError::Invariant {
                detail: "Forge output rotation lost its footer phase".to_owned(),
            })?;
        Ok((writer, path, footer_phase))
    }

    /// Record one finalized output and clear its rows from the active writer.
    /// Transfer the current output's NaN metrics and reset the visitor state.
    fn take_nan_value_counts(&mut self) -> HashMap<i32, u64> {
        std::mem::take(&mut self.nan_value_counts).nan_value_counts
    }

    /// Record one finalized output and clear its rows and per-file metrics.
    fn record_output(&mut self, file: DataFile, path: String) {
        self.output_rows = self.output_rows.saturating_add(self.writer_rows);
        self.writer_rows = 0;
        self.writer_row_groups = 0;
        self.writer_row_group_logical_bytes = 0;
        self.writer_row_group_rows = 0;
        self.output_paths.push(path);
        self.files.push(file);
        self.nan_value_counts = NanValueCountVisitor::new();
    }

    /// Clears non-durable counters after an encoded row group is rejected.
    fn discard_oversized_output(&mut self) {
        self.writer_rows = 0;
        self.writer_row_groups = 0;
        self.writer_row_group_logical_bytes = 0;
        self.writer_row_group_rows = 0;
        self.nan_value_counts = NanValueCountVisitor::new();
    }

    /// Retain the largest runtime spill observation seen between batches.
    fn observe_spill(&mut self, spill_bytes: u64) {
        self.peak_spill_bytes = self.peak_spill_bytes.max(spill_bytes);
        if let Some(peak) = &self.scratch_peak_bytes {
            peak.fetch_max(
                spill_bytes.saturating_add(self.output_scratch_limit_bytes),
                Ordering::AcqRel,
            );
        }
        #[cfg(feature = "test-support")]
        TEST_SCRATCH_PEAK_BYTES.fetch_max(
            spill_bytes.saturating_add(self.output_scratch_limit_bytes),
            Ordering::AcqRel,
        );
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
        self.footer_phase.take();
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
    /// Verified object paths retained for publication settlement or orphan GC.
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
struct RewriteFinalization {
    /// Terminal batch state, including all rewrite-owned output paths.
    batch: RewriteBatchResult,
}

/// Blocking inputs for deriving Iceberg metadata from one completed Parquet output.
struct OutputMetadataRequest {
    /// Open scratch-backed writer whose sealed file becomes the staged object.
    writer: ArrowWriter<ScratchBoundedWriter>,
    /// Execution-phase ownership transitioned to metadata after the file seals.
    footer_phase: ForgeFooterPhaseGuard,
    /// Attempt-owned scratch path for the writer.
    scratch_path: PathBuf,
    /// Per-field NaN counts required by Iceberg file metadata.
    nan_value_counts: HashMap<i32, u64>,
    /// Rows encoded into the completed Parquet output.
    rows: u64,
    /// Iceberg schema carrying field identifiers for file metadata.
    iceberg_schema: IcebergSchemaRef,
    /// Arrow schema whose fingerprint and leaf order were stamped before writing.
    expected_schema: SchemaRef,
    /// Catalog-visible Iceberg path for the output file.
    table_path: String,
    /// Object-store key stamped into the writer-v2 footer for later reads.
    object_identity: String,
    /// Day partition assigned to the entire output.
    partition_day: chrono::NaiveDate,
    /// Iceberg partition-spec identifier for the destination table.
    partition_spec_id: i32,
    /// Iceberg sort-order identifier for the destination table.
    sort_order_id: i32,
    /// Exact bounded read size used for the sealed-file checksum pass.
    checksum_chunk_bytes: usize,
}

/// Owned state detached from one rotated output before asynchronous publication.
struct FinishOutputParts {
    /// Completed bounded Parquet writer.
    writer: ArrowWriter<ScratchBoundedWriter>,
    /// Footer-phase guard retained through metadata validation.
    footer_phase: ForgeFooterPhaseGuard,
    /// Attempt-local scratch file containing the output.
    scratch_path: PathBuf,
    /// Per-column NaN counts accumulated during encoding.
    nan_value_counts: HashMap<i32, u64>,
    /// Exact encoded row count.
    rows: u64,
    /// Stable output ordinal within the attempt.
    ordinal: usize,
}

/// Bytes and Iceberg metadata derived together from one completed Parquet output.
struct FinalizedOutput {
    /// Sealed attempt-owned Parquet path to upload in bounded chunks.
    scratch_path: PathBuf,
    /// Exact sealed Parquet length.
    bytes: u64,
    /// SHA-256 computed while reading the sealed file in bounded chunks.
    sha256: [u8; 32],
    /// Iceberg data-file metadata derived from those bytes.
    file: DataFile,
}

/// Computes SHA-256 across the complete sealed file in bounded reads.
///
/// # Errors
///
/// Returns [`ForgeError::ScratchIo`] when rewind or read-back fails.
fn hash_sealed_file(
    source: &mut std::fs::File,
    path: &Path,
    chunk_bytes: usize,
) -> Result<[u8; 32], ForgeError> {
    source.rewind().map_err(|error| ForgeError::ScratchIo {
        kind: error.kind(),
        detail: format!("rewind {} for checksum: {error}", path.display()),
    })?;
    let mut reader = BufReader::new(source);
    let mut chunk = vec![0_u8; chunk_bytes];
    let mut hash = Sha256::new();
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|error| ForgeError::ScratchIo {
                kind: error.kind(),
                detail: format!("checksum {}: {error}", path.display()),
            })?;
        if read == 0 {
            return Ok(hash.finalize().into());
        }
        hash.update(&chunk[..read]);
    }
}

/// Confirms finalized Iceberg metadata maps to the prevalidated object key.
///
/// # Errors
///
/// Returns a path or invariant error when metadata escapes the exact binding.
fn validate_finalized_output_path(
    request: &RewriteRequest<'_>,
    staging: &opendal::Operator,
    file: &DataFile,
    object_path: &str,
) -> Result<(), ForgeError> {
    let mapped = catalog_path_to_object_key(
        request.table_location,
        request.binding,
        staging,
        file.file_path(),
    )?;
    if mapped != object_path {
        return Err(ForgeError::Invariant {
            detail: "Forge DataFile path does not map to its exact object key".to_owned(),
        });
    }
    Ok(())
}

/// Refuses an encoded footer above the pre-reserved 8 MiB allowance before decoding.
///
/// # Errors
/// Returns a Parquet data refusal when the trailer is malformed or its declared
/// footer length exceeds the fixed encoded-footer child.
fn validate_footer_length(file: &std::fs::File, file_bytes: u64) -> Result<(), ForgeError> {
    if file_bytes < 8 {
        return Err(ForgeError::DataRefusal {
            detail: "Parquet output is shorter than its trailer".to_owned(),
        });
    }
    let mut trailer = [0_u8; 8];
    let mut reader = file.try_clone().map_err(|error| ForgeError::ScratchIo {
        kind: error.kind(),
        detail: format!("clone Parquet output for trailer read: {error}"),
    })?;
    reader
        .seek(std::io::SeekFrom::Start(file_bytes - 8))
        .and_then(|_| reader.read_exact(&mut trailer))
        .map_err(|error| ForgeError::ScratchIo {
            kind: error.kind(),
            detail: format!("read Parquet output trailer: {error}"),
        })?;
    if &trailer[4..] != b"PAR1" {
        return Err(ForgeError::DataRefusal {
            detail: "Parquet output trailer magic is invalid".to_owned(),
        });
    }
    let encoded = u64::from(u32::from_le_bytes(
        trailer[..4]
            .try_into()
            .expect("four-byte footer length slice is exact"),
    ));
    crate::parquet::memory::validate_encoded_footer_bytes(encoded)
        .map_err(|detail| ForgeError::DataRefusal { detail })?;
    let footer_start = file_bytes
        .checked_sub(8)
        .and_then(|end| end.checked_sub(encoded))
        .ok_or_else(|| ForgeError::DataRefusal {
            detail: "Parquet encoded footer extends before the file start".to_owned(),
        })?;
    let mut footer = vec![
        0_u8;
        usize::try_from(encoded).map_err(|_| ForgeError::DataRefusal {
            detail: "Parquet encoded footer exceeds address space".to_owned(),
        })?
    ];
    reader
        .seek(std::io::SeekFrom::Start(footer_start))
        .and_then(|_| reader.read_exact(&mut footer))
        .map_err(|error| ForgeError::ScratchIo {
            kind: error.kind(),
            detail: format!("read Parquet output footer preflight bytes: {error}"),
        })?;
    crate::parquet::footer_preflight::preflight_compact_thrift(&footer)
        .map_err(|detail| ForgeError::DataRefusal { detail })?;
    Ok(())
}

/// Applies the conjunctive writer-v2 physical and structural footer ceilings.
///
/// # Errors
/// Returns a data refusal when row groups, leaf columns, column chunks, or the
/// conservative structural-element count exceed the closed contract.
fn validate_writer_v2_structure(
    metadata: &parquet::file::metadata::ParquetMetaData,
) -> Result<(), ForgeError> {
    crate::parquet::memory::validate_writer_v2_structure(metadata)
        .map_err(|detail| ForgeError::DataRefusal { detail })
}

/// Waits until one fixed decoded-batch slot can be reserved before polling IO.
///
/// The finite attempt pool is the wake condition: competing consumers release
/// reservations as sorted batches advance. Yielding keeps the cooperative
/// executor responsive without allocating the next batch speculatively.
async fn reserve_decoded_batch_slot(
    reservation: &datafusion::execution::memory_pool::MemoryReservation,
    decoded_batch_bytes: usize,
) {
    while reservation.try_grow(decoded_batch_bytes).is_err() {
        tokio::task::yield_now().await;
    }
}

/// Removes the next physical file's contiguous logical row groups.
///
/// The operator target is a preferred rotation boundary, while the writer-v2
/// row-group count and 128 MiB file ceiling are hard bounds. A first row group
/// larger than a small operator target is still admitted because it already
/// satisfies the independent 32 MiB row-group contract.
fn take_physical_file_group(
    pending: &mut VecDeque<BoundedRowSlice>,
    operator_target_bytes: u64,
) -> Vec<BoundedRowSlice> {
    let Some(first) = pending.pop_front() else {
        return Vec::new();
    };
    let target = operator_target_bytes.clamp(1, MAX_FILE_BYTES);
    let mut logical_bytes = first.logical_bytes;
    let mut group = vec![first];
    while group.len() < MAX_FILE_ROW_GROUPS {
        let Some(next) = pending.front() else {
            break;
        };
        let Some(candidate_bytes) = logical_bytes.checked_add(next.logical_bytes) else {
            break;
        };
        if candidate_bytes > target {
            break;
        }
        logical_bytes = candidate_bytes;
        group.push(
            pending
                .pop_front()
                .expect("front row group remains present"),
        );
    }
    group
}

/// Requeues one rejected physical file after bisecting its oversized row group.
///
/// # Errors
///
/// Returns a data-layout refusal when the reported row-group position is not
/// part of the attempted file or when the offending slice has only one row.
fn requeue_bisected_file(
    pending: &mut VecDeque<BoundedRowSlice>,
    batch: &RecordBatch,
    group: Vec<BoundedRowSlice>,
    row_group: usize,
) -> Result<(), ForgeError> {
    let offending = group
        .get(row_group)
        .copied()
        .ok_or_else(|| ForgeError::Invariant {
            detail: format!(
                "sealed output reported row group {row_group} outside attempted group count {}",
                group.len()
            ),
        })?;
    if offending.len == 1 {
        return Err(ForgeError::DataRefusal {
            detail: "one-row Forge output exceeds the encoded 32 MiB ceiling".to_owned(),
        });
    }
    let (left, right) = BifrostArrowLogicalSizer::bisect(batch, offending)
        .map_err(|detail| ForgeError::DataRefusal { detail })?;
    let mut replacement = Vec::with_capacity(group.len() + 1);
    for (index, slice) in group.into_iter().enumerate() {
        if index == row_group {
            replacement.push(left);
            replacement.push(right);
        } else {
            replacement.push(slice);
        }
    }
    for slice in replacement.into_iter().rev() {
        pending.push_front(slice);
    }
    Ok(())
}

/// Opens a sealed output and validates its footer memory contract.
///
/// # Errors
///
/// Returns scratch IO, Parquet, footer-length, or data-refusal errors.
fn read_finalized_output(
    scratch_path: &Path,
    expected_schema: &arrow::datatypes::Schema,
    object_identity: &str,
) -> Result<
    (
        std::fs::File,
        u64,
        Arc<parquet::file::metadata::ParquetMetaData>,
    ),
    ForgeError,
> {
    let source_file = std::fs::File::open(scratch_path).map_err(|error| ForgeError::ScratchIo {
        kind: error.kind(),
        detail: format!("open {}: {error}", scratch_path.display()),
    })?;
    let output_size = source_file
        .metadata()
        .map_err(|error| ForgeError::ScratchIo {
            kind: error.kind(),
            detail: format!("stat {}: {error}", scratch_path.display()),
        })?
        .len();
    validate_footer_length(&source_file, output_size)?;
    let metadata = Arc::new(
        parquet::file::metadata::ParquetMetaDataReader::new()
            .parse_and_finish(&source_file)
            .map_err(|error| ForgeError::Parquet {
                detail: format!("Forge output footer parsing failed: {error}"),
            })?,
    );
    crate::parquet::memory::BifrostParquetMemoryEnvelope::from_footer(
        metadata.file_metadata(),
        expected_schema,
        object_identity,
    )
    .map_err(|detail| ForgeError::DataRefusal { detail })?;
    Ok((source_file, output_size, metadata))
}

/// Projects one physical row-group selection into its owned output batch.
///
/// # Errors
///
/// Returns an invariant error when the group is empty or its row count overflows.
fn physical_output_batch(
    batch: &RecordBatch,
    group: &[BoundedRowSlice],
) -> Result<(RecordBatch, Vec<BoundedRowSlice>), ForgeError> {
    let first = group.first().ok_or_else(|| ForgeError::Invariant {
        detail: "physical output grouping returned no row groups".to_owned(),
    })?;
    let row_count = group.iter().try_fold(0_usize, |sum, slice| {
        sum.checked_add(slice.len)
            .ok_or_else(|| ForgeError::Invariant {
                detail: "physical output row count overflowed usize".to_owned(),
            })
    })?;
    let output_groups = group
        .iter()
        .map(|slice| BoundedRowSlice {
            offset: slice.offset - first.offset,
            len: slice.len,
            logical_bytes: slice.logical_bytes,
        })
        .collect();
    Ok((batch.slice(first.offset, row_count), output_groups))
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
        footer_phase,
        scratch_path,
        nan_value_counts,
        rows,
        iceberg_schema,
        expected_schema,
        table_path,
        object_identity,
        partition_day,
        partition_spec_id,
        sort_order_id,
        checksum_chunk_bytes,
    } = request;
    let mut sink = writer.into_inner().map_err(|error| ForgeError::Parquet {
        detail: error.to_string(),
    })?;
    sink.flush().map_err(|error| ForgeError::ScratchIo {
        kind: error.kind(),
        detail: format!("flush {}: {error}", scratch_path.display()),
    })?;
    sink.file()
        .sync_all()
        .map_err(|error| ForgeError::ScratchIo {
            kind: error.kind(),
            detail: format!("fsync {}: {error}", scratch_path.display()),
        })?;
    drop(sink);
    let _footer_phase = footer_phase.into_metadata();
    let (source_file, output_size, parquet_metadata) =
        read_finalized_output(&scratch_path, expected_schema.as_ref(), &object_identity)?;
    if let Some((row_group, group)) =
        parquet_metadata
            .row_groups()
            .iter()
            .enumerate()
            .find(|(_, group)| {
                u64::try_from(group.compressed_size()).unwrap_or(u64::MAX)
                    > crate::parquet::memory::MAX_LOGICAL_ROW_GROUP_BYTES
            })
    {
        let rows = usize::try_from(group.num_rows()).unwrap_or(usize::MAX);
        drop(parquet_metadata);
        drop(source_file);
        std::fs::remove_file(&scratch_path).map_err(|error| ForgeError::ScratchIo {
            kind: error.kind(),
            detail: format!("remove oversized {}: {error}", scratch_path.display()),
        })?;
        return Err(ForgeError::EncodedRowGroupOverflow { row_group, rows });
    }
    validate_writer_v2_structure(&parquet_metadata)?;
    let mut file = ParquetWriter::parquet_to_data_file_builder(
        iceberg_schema,
        Arc::clone(&parquet_metadata),
        usize::try_from(output_size).map_err(|error| ForgeError::Invariant {
            detail: format!("output size does not fit usize: {error}"),
        })?,
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
    let mut source_file = source_file;
    let sha256 = hash_sealed_file(&mut source_file, &scratch_path, checksum_chunk_bytes)?;
    Ok(FinalizedOutput {
        scratch_path,
        bytes: output_size,
        sha256,
        file,
    })
}

impl ForgeRewritePipeline {
    /// Compose a rewrite pipeline from host-provisioned runtime dependencies.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the concurrency limit is zero.
    pub(crate) fn new(
        staging: Arc<opendal::Operator>,
        object_store: Arc<dyn ForgeObjectStore>,
        max_concurrent_reads: usize,
    ) -> Result<Self, ForgeError> {
        if max_concurrent_reads == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "rewrite limits must be positive".to_owned(),
            });
        }
        Ok(Self {
            runtime: None,
            uploader: Arc::new(BifrostParquetUploader::new((*staging).clone())),
            staging,
            object_store,
            max_concurrent_reads,
            blocking_permits: Arc::new(tokio::sync::Semaphore::new(max_concurrent_reads)),
        })
    }

    /// Creates an attempt-local execution view over the retained dependencies.
    pub(crate) fn for_runtime(&self, runtime: Arc<ForgeRewriteRuntime>) -> Self {
        Self {
            runtime: Some(runtime),
            staging: Arc::clone(&self.staging),
            uploader: Arc::clone(&self.uploader),
            object_store: Arc::clone(&self.object_store),
            max_concurrent_reads: self.max_concurrent_reads,
            blocking_permits: Arc::clone(&self.blocking_permits),
        }
    }

    /// Returns the worker-installed runtime for this execution view.
    ///
    /// # Panics
    ///
    /// Panics when a caller bypasses [`Self::for_runtime`] and attempts IO on
    /// the dependency-only pipeline retained by [`Forge`](super::Forge).
    fn runtime(&self) -> &ForgeRewriteRuntime {
        self.runtime
            .as_ref()
            .expect("invariant: Forge rewrite execution installs its leased runtime")
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
    /// method returns success, paths already written remain rewrite-owned.
    /// Success transfers the complete set to compaction reconciliation;
    /// failed or pre-Prepared uploads remain remote until protected TTL orphan
    /// GC proves them unreferenced and deletes them.
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
        let initial_spill = self.runtime().runtime.spilling_progress();
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
        self.finalize_rewrite(RewriteFinalization { batch: result })
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
    /// finalized paths remain recorded for error settlement and protected orphan GC.
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
                .register(&self.runtime().runtime.memory_pool);
        if let Err(error) = reservation.try_grow(self.runtime().output_allowance_bytes) {
            return RewriteBatchState::with_reservation(
                Some(reservation),
                None,
                self.runtime().scratch_path().to_path_buf(),
            )
            .fail(ForgeError::ExecutionEnvelopeExceeded {
                resource: "memory",
                detail: format!("Forge fixed output allowance was refused: {error}"),
            });
        }
        let output_scratch = match self.runtime().reserve_output_scratch() {
            Ok(reservation) => reservation,
            Err(error) => {
                return RewriteBatchState::with_reservation(
                    Some(reservation),
                    None,
                    self.runtime().scratch_path().to_path_buf(),
                )
                .fail(error);
            }
        };
        let mut state = RewriteBatchState::with_reservation(
            Some(reservation),
            Some(output_scratch),
            self.runtime().scratch_path().to_path_buf(),
        );
        while let Some(batch) = tokio::select! {
            () = stop.cancelled() => return state.fail(ForgeError::Shutdown),
            batch = stream.next() => batch,
        } {
            let batch = match batch {
                Ok(batch) => batch,
                Err(error) => return state.fail(Self::map_datafusion_error(error)),
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
    /// final ownership transfer or protected orphan GC.
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
        let slices = BifrostArrowLogicalSizer::slice(batch)
            .map_err(|detail| ForgeError::DataRefusal { detail })?;
        let mut pending: VecDeque<_> = slices.into();
        while !pending.is_empty() {
            let group = take_physical_file_group(&mut pending, request.target_file_size_bytes);
            if state.should_rotate(request.target_file_size_bytes)
                || state.would_exceed_row_groups(&group)
            {
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
            let (output_batch, output_groups) = physical_output_batch(batch, &group)?;
            let output_rows = u64::try_from(output_batch.num_rows()).unwrap_or(u64::MAX);
            let footer_phase = if state.writer.is_none() {
                Some(tokio::select! {
                    () = stop.cancelled() => return Err(ForgeError::Shutdown),
                    phase = self.runtime().footer_phase.enter_execution() => phase?,
                })
            } else {
                None
            };
            // Detach the state owning the real reservation while the bounded
            // blocking encoder writes this physical output.
            let mut detached = std::mem::replace(
                state,
                RewriteBatchState::with_reservation(
                    None,
                    None,
                    self.runtime().scratch_path().to_path_buf(),
                ),
            );
            let schema = Arc::clone(&request.schema);
            let iceberg_schema = Arc::clone(&request.iceberg_schema);
            let permit = tokio::select! {
                () = stop.cancelled() => return Err(ForgeError::Shutdown),
                permit = self.blocking_permits.clone().acquire_owned() => permit.map_err(|error| ForgeError::Invariant {
                    detail: format!("Forge blocking permit closed: {error}"),
                })?,
            };
            let encoder_flush_bytes = self.runtime().encoder_flush_bytes;
            let object_identity = deterministic_output_path(
                &request.binding.object_prefix,
                request.attempt_generation,
                detached.files.len(),
            );
            if let Some(footer_phase) = footer_phase {
                detached.footer_phase = Some(footer_phase);
            }
            let join = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let result = detached.write_batch(
                    &schema,
                    &iceberg_schema,
                    &output_batch,
                    &output_groups,
                    &object_identity,
                    encoder_flush_bytes,
                );
                (detached, result)
            });
            let (returned, result) = join.await.map_err(|error| ForgeError::Invariant {
                detail: format!("Forge output encoding task failed: {error}"),
            })?;
            *state = returned;
            result?;
            state.observe_spill(self.runtime().runtime.spilling_progress().current_bytes);
            if state.should_rotate(request.target_file_size_bytes) {
                let rotation = self
                    .rotate_output(
                        state,
                        RewriteOutputContext {
                            request,
                            stop,
                            lease: &mut *lease,
                            operator_pool,
                        },
                    )
                    .await;
                match rotation {
                    Ok(()) => {}
                    Err(ForgeError::EncodedRowGroupOverflow { row_group, .. })
                        if state.writer_rows == output_rows =>
                    {
                        state.discard_oversized_output();
                        requeue_bisected_file(&mut pending, batch, group, row_group)?;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
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
    /// paths remain recorded for error settlement and protected orphan GC.
    async fn rotate_output(
        &self,
        state: &mut RewriteBatchState,
        context: RewriteOutputContext<'_, '_>,
    ) -> Result<(), ForgeError> {
        let (writer, scratch_path, footer_phase) = state.take_writer()?;
        let nan_value_counts = state.take_nan_value_counts();
        let (file, path) = self
            .finish_output(
                FinishOutputParts {
                    writer,
                    footer_phase,
                    scratch_path,
                    nan_value_counts,
                    rows: state.writer_rows,
                    ordinal: state.files.len(),
                },
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
    /// conservation invariant. Uploaded objects remain for fenced orphan GC.
    async fn finalize_rewrite(
        &self,
        finalization: RewriteFinalization,
    ) -> Result<RewriteOutput, ForgeError> {
        let RewriteFinalization { batch, .. } = finalization;
        let RewriteBatchResult {
            result,
            files,
            output_paths,
            input_rows,
            output_rows,
            peak_spill_bytes,
        } = batch;
        if let Err(error) = self.await_spill_cleanup().await {
            return Err(error);
        }
        if let Err(error) = result {
            return Err(error);
        }
        let final_spill = self.runtime().runtime.spilling_progress();
        if final_spill.active_files_count != 0 {
            return Err(ForgeError::Invariant {
                detail: "Forge rewrite ended with active spill files".to_owned(),
            });
        }
        if input_rows != output_rows {
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
            while self
                .runtime()
                .runtime
                .spilling_progress()
                .active_files_count
                != 0
            {
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
        let source = Arc::new(StagingParquetExec::new(StagingParquetExecConfig {
            files: request.source_files.to_vec(),
            binding: request.binding.clone(),
            schema: Arc::clone(&request.schema),
            object_store: Arc::clone(&self.object_store),
            max_concurrent_reads: self.max_concurrent_reads,
            memory_pool: Arc::clone(&self.runtime().runtime.memory_pool),
            decoded_batch_bytes: self.runtime().decoded_batch_bytes,
            footer_phase: Arc::clone(&self.runtime().footer_phase),
        }));
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
            datafusion::prelude::SessionConfig::new()
                .with_sort_spill_reservation_bytes(self.runtime().sort_merge_reservation_bytes),
            self.runtime().runtime(),
        );
        let sort_plan: Arc<dyn ExecutionPlan> = sort.clone();
        let stream =
            execute_stream(sort_plan, session.task_ctx()).map_err(ForgeError::DataFusion)?;
        // DataFusion populates the plan metrics during `execute_stream`; read
        // them only after execution has registered the SortExec metrics set.
        let metrics = sort.metrics().unwrap_or_default();
        Ok((stream, metrics))
    }

    /// Derives and validates both object-store and catalog output identities.
    ///
    /// # Errors
    ///
    /// Returns an invariant error when the table location or output prefix is invalid.
    fn validated_output_identity(
        &self,
        request: &RewriteRequest<'_>,
        ordinal: usize,
    ) -> Result<(String, String), ForgeError> {
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
        Ok((
            object_path,
            forge_table_output_path(request.table_location, request.attempt_generation, ordinal),
        ))
    }

    /// Close, PUT, and derive Iceberg metadata for one rotated output.
    ///
    /// # Errors
    ///
    /// Returns cancellation, lease, Parquet, object-store, path, scratch, or
    /// metadata-builder failures. The caller retains prior remote outputs for
    /// the fenced orphan-GC owner rather than deleting them here.
    ///
    /// # Cancellation
    ///
    /// Cancellation is checked before fence validation and again before PUT.
    /// Metadata construction finishes before PUT. After the shared uploader
    /// verifies the remote object, this method removes only the local sealed
    /// scratch file; any later remote reclamation belongs exclusively to
    /// orphan GC.
    async fn finish_output(
        &self,
        parts: FinishOutputParts,
        context: RewriteOutputContext<'_, '_>,
    ) -> Result<(DataFile, String), ForgeError> {
        let FinishOutputParts {
            writer,
            footer_phase,
            scratch_path,
            nan_value_counts,
            rows,
            ordinal,
        } = parts;
        let RewriteOutputContext {
            request,
            stop,
            lease,
            operator_pool,
        } = context;
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        let (object_path, table_path) = self.validated_output_identity(request, ordinal)?;
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
        let object_identity = object_path.clone();
        let partition_day = request.partition_day;
        let partition_spec_id = request.partition_spec_id;
        let sort_order_id = request.sort_order_id;
        let iceberg_schema = Arc::clone(&request.iceberg_schema);
        let expected_schema = Arc::clone(&request.schema);
        let checksum_chunk_bytes = self.runtime().upload_chunk_bytes;
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
                footer_phase,
                scratch_path,
                nan_value_counts,
                rows,
                iceberg_schema,
                expected_schema,
                table_path,
                object_identity,
                partition_day,
                partition_spec_id,
                sort_order_id,
                checksum_chunk_bytes,
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
        let FinalizedOutput {
            scratch_path,
            bytes,
            sha256,
            file,
        } = finalized.map_err(|error| ForgeError::Invariant {
            detail: format!("Forge output finalization task failed: {error}"),
        })??;
        validate_finalized_output_path(request, &self.staging, &file, &object_path)?;
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        self.upload_sealed_output(&scratch_path, &object_path, bytes, sha256)
            .await?;
        tracing::debug!(sha256 = ?sha256, bytes, "Forge verified sealed output");
        self.object_store.after_output_put(&object_path).await;
        tokio::fs::remove_file(&scratch_path)
            .await
            .map_err(|error| ForgeError::ScratchIo {
                kind: error.kind(),
                detail: format!("delete sealed output {}: {error}", scratch_path.display()),
            })?;
        Ok((file, object_path))
    }

    /// Delegates deterministic create and read-back verification to the shared uploader.
    ///
    /// # Errors
    ///
    /// Returns a typed Forge invariant when the shared uploader cannot converge
    /// on the exact sealed SHA-256 and length. Remote objects remain retained
    /// for fenced orphan GC after any failure or ambiguity.
    async fn upload_sealed_output(
        &self,
        scratch_path: &Path,
        object_path: &str,
        expected_bytes: u64,
        expected_sha256: [u8; 32],
    ) -> Result<(), ForgeError> {
        let identity =
            ParquetObjectIdentity::new(object_path).map_err(|error| ForgeError::Invariant {
                detail: format!("Forge output identity is invalid: {error}"),
            })?;
        let mut chunk = vec![0_u8; self.runtime().upload_chunk_bytes];
        let verified = self
            .uploader
            .upload_file_verified(
                &identity,
                scratch_path,
                expected_sha256,
                expected_bytes,
                &mut chunk,
                BifrostUploadRole::Forge,
            )
            .await
            .map_err(|error| ForgeError::Invariant {
                detail: format!("Forge verified upload failed: {error}"),
            })?;
        if verified.length != expected_bytes || verified.sha256 != expected_sha256 {
            return Err(ForgeError::Invariant {
                detail:
                    "Forge shared uploader returned evidence that diverges from the sealed output"
                        .to_owned(),
            });
        }
        Ok(())
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
    fn map_datafusion_error(error: datafusion::error::DataFusionError) -> ForgeError {
        if let datafusion::error::DataFusionError::External(source) = error {
            return match source.downcast::<ForgeError>() {
                Ok(error) => *error,
                Err(source) => {
                    ForgeError::DataFusion(datafusion::error::DataFusionError::External(source))
                }
            };
        }
        match error {
            datafusion::error::DataFusionError::ResourcesExhausted(detail) => {
                ForgeError::ExecutionEnvelopeExceeded {
                    resource: "memory",
                    detail,
                }
            }
            error => ForgeError::DataFusion(error),
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

/// Derives the Iceberg-visible writer-v2 identity stamped into one output footer.
#[must_use]
fn forge_table_output_path(
    table_location: &str,
    generation: ForgeAttemptGeneration,
    ordinal: usize,
) -> String {
    format!(
        "{}/data/forge/{BIFROST_WRITER_RECIPE_VERSION}/{}-{ordinal:05}.parquet",
        table_location.trim_end_matches('/'),
        generation.0
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
    /// Attempt-local memory pool charged before decoded batches are polled.
    memory_pool: Arc<dyn MemoryPool>,
    /// Total decoded-input allowance used for footer refusal.
    decoded_allowance_bytes: usize,
    /// Serial owner preventing source footer decode from overlapping encoding.
    footer_phase: Arc<ForgeFooterPhase>,
    /// `DataFusion` physical properties for one bounded source partition.
    properties: Arc<PlanProperties>,
}

/// Complete dependency bundle for one bounded staging source plan.
struct StagingParquetExecConfig {
    /// Deterministically ordered sources.
    files: Vec<RewriteSourceFile>,
    /// Tenant/table path authority.
    binding: TenantTableBinding,
    /// Projected physical schema.
    schema: SchemaRef,
    /// Object-store source seam.
    object_store: Arc<dyn ForgeObjectStore>,
    /// Maximum simultaneous source reads.
    max_concurrent_reads: usize,
    /// Attempt-local memory pool.
    memory_pool: Arc<dyn MemoryPool>,
    /// Per-reader decoded allowance.
    decoded_batch_bytes: usize,
    /// Serial footer phase owner.
    footer_phase: Arc<ForgeFooterPhase>,
}

impl StagingParquetExec {
    /// Build one bounded source plan for a deterministic candidate sequence.
    fn new(config: StagingParquetExecConfig) -> Self {
        let StagingParquetExecConfig {
            files,
            binding,
            schema,
            object_store,
            max_concurrent_reads,
            memory_pool,
            decoded_batch_bytes,
            footer_phase,
        } = config;
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
            memory_pool,
            decoded_allowance_bytes: decoded_batch_bytes,
            footer_phase,
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
        let footer_phase = self
            .footer_phase
            .enter_metadata()
            .await
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
        let size = self
            .object_store
            .stat(&path)
            .await
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?
            .content_length();
        preflight_remote_footer(self.object_store.as_ref(), &path, size)
            .await
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
        let reader = ForgeParquetReader {
            store: Arc::clone(&self.object_store),
            path,
            size,
            metadata_loading: false,
        };
        let builder = ParquetRecordBatchStreamBuilder::new(reader)
            .await
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
        let source_schema = builder.schema();
        BifrostParquetMemoryEnvelope::from_footer(
            builder.metadata().file_metadata(),
            source_schema.as_ref(),
            &file.object_path,
        )
        .map_err(|detail| {
            datafusion::error::DataFusionError::External(Box::new(ForgeError::DataRefusal {
                detail,
            }))
        })?;
        if !crate::catalog::schema_shape_matches(self.schema.as_ref(), source_schema.as_ref()) {
            return Err(datafusion::error::DataFusionError::External(Box::new(
                ForgeError::DataRefusal {
                    detail: "writer-v2 source schema does not match the catalog schema".to_owned(),
                },
            )));
        }
        validate_writer_v2_structure(builder.metadata())
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
        drop(footer_phase);
        let stream = builder
            .with_batch_size(REWRITE_BATCH_ROWS)
            .build()
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
        let tenant = self.binding.tenant;
        let schema = Arc::clone(&self.schema);
        let decoded =
            datafusion::execution::memory_pool::MemoryConsumer::new("forge-rewrite-decoded-batch")
                .register(&self.memory_pool);
        let decoded_batch_bytes = self.decoded_allowance_bytes;
        let projected = async_stream::try_stream! {
            futures_util::pin_mut!(stream);
            loop {
                decoded.free();
                reserve_decoded_batch_slot(&decoded, decoded_batch_bytes).await;
                let Some(batch) = stream.next().await else { break; };
                let batch = batch.map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
                validate_tenant_column(&batch, tenant)
                    .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
                crate::schema::managed_columns::row_ordinals(&batch)
                    .map_err(|error| datafusion::error::DataFusionError::Execution(
                        format!("Forge row identity invariant failed: {error}")
                    ))?;
                let projected = project_by_name(&batch, Arc::clone(&schema))
                    .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
                let decoded_bytes = projected.get_array_memory_size();
                if decoded_bytes > decoded_batch_bytes {
                    Err(datafusion::error::DataFusionError::External(Box::new(
                        ForgeError::DataRefusal {
                            detail: format!("decoded batch requires {decoded_bytes} bytes but slot target is {decoded_batch_bytes}"),
                        },
                    )))?;
                }
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
    /// True only while Parquet is fetching footer metadata ranges.
    metadata_loading: bool,
}

#[cfg(any(test, feature = "test-support"))]
static FORGE_DATA_PAGE_READS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Resets production-seam data-page read observations for one bounded test.
#[cfg(feature = "test-support")]
pub fn reset_data_page_reads_for_test() {
    FORGE_DATA_PAGE_READS.store(0, std::sync::atomic::Ordering::Release);
}

/// Returns data-page ranges requested after footer validation.
#[cfg(feature = "test-support")]
#[must_use]
pub fn data_page_reads_for_test() -> usize {
    FORGE_DATA_PAGE_READS.load(std::sync::atomic::Ordering::Acquire)
}

/// Reads only the remote trailer and refuses oversized footer declarations before decode.
///
/// # Errors
/// Returns a data refusal for malformed or oversized trailers and preserves
/// object-store transport failures through the existing Forge taxonomy.
async fn preflight_remote_footer(
    store: &dyn ForgeObjectStore,
    path: &str,
    size: u64,
) -> Result<(), ForgeError> {
    if size < 8 {
        return Err(ForgeError::DataRefusal {
            detail: "Parquet source is shorter than its trailer".to_owned(),
        });
    }
    let trailer = store
        .read_range(path, size - 8..size)
        .await
        .map_err(ForgeError::ObjectStore)?
        .to_bytes();
    if trailer.len() != 8 || &trailer[4..] != b"PAR1" {
        return Err(ForgeError::DataRefusal {
            detail: "Parquet source trailer is invalid".to_owned(),
        });
    }
    let encoded = u64::from(u32::from_le_bytes(
        trailer[..4]
            .try_into()
            .expect("four-byte remote footer length slice is exact"),
    ));
    crate::parquet::memory::validate_encoded_footer_bytes(encoded)
        .map_err(|detail| ForgeError::DataRefusal { detail })?;
    let footer_start = size
        .checked_sub(8)
        .and_then(|end| end.checked_sub(encoded))
        .ok_or_else(|| ForgeError::DataRefusal {
            detail: "Parquet source footer extends before the object start".to_owned(),
        })?;
    let footer = store
        .read_range(path, footer_start..size - 8)
        .await
        .map_err(ForgeError::ObjectStore)?
        .to_bytes();
    crate::parquet::footer_preflight::preflight_compact_thrift(&footer)
        .map_err(|detail| ForgeError::DataRefusal { detail })?;
    Ok(())
}

impl AsyncFileReader for ForgeParquetReader {
    /// Fetch exactly the requested Parquet byte range without materializing the file.
    ///
    /// # Errors
    ///
    /// Returns the object-store or Parquet external error for a failed range.
    fn get_bytes(&mut self, range: Range<u64>) -> BoxFuture<'_, parquet::errors::Result<Bytes>> {
        #[cfg(any(test, feature = "test-support"))]
        if !self.metadata_loading {
            FORGE_DATA_PAGE_READS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        }
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
            self.metadata_loading = true;
            let result = parquet::file::metadata::ParquetMetaDataReader::new()
                .load_and_finish(&mut *self, size)
                .await;
            self.metadata_loading = false;
            let metadata = result?;
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
        let decoded_batch_bytes = self.decoded_allowance_bytes;
        let memory_pool = Arc::clone(&self.memory_pool);
        let footer_phase = Arc::clone(&self.footer_phase);
        let stream = futures_util::stream::iter(files)
            .map(move |file| {
                let binding = binding.clone();
                let schema = Arc::clone(&schema);
                let store = Arc::clone(&store);
                let permits = Arc::clone(&permits);
                let memory_pool = Arc::clone(&memory_pool);
                let footer_phase = Arc::clone(&footer_phase);
                async move {
                    StagingParquetExec::new(StagingParquetExecConfig {
                        files: Vec::new(),
                        binding,
                        schema,
                        object_store: store,
                        max_concurrent_reads,
                        memory_pool,
                        decoded_batch_bytes,
                        footer_phase,
                    })
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

/// RAII ownership of the attempt's pending-output scratch allowance.
struct OutputScratchReservation {
    /// Shared sibling counter owned by the attempt runtime.
    used_bytes: Arc<AtomicU64>,
    /// Exact bytes returned when rewrite state terminates.
    bytes: u64,
    /// Conservative aggregate scratch peak shared with the attempt owner.
    peak_bytes: Arc<AtomicU64>,
}

impl Drop for OutputScratchReservation {
    /// Returns the pre-reserved output scratch to the attempt-local counter.
    fn drop(&mut self) {
        let previous = self.used_bytes.fetch_sub(self.bytes, Ordering::AcqRel);
        debug_assert!(previous >= self.bytes, "output scratch counter underflow");
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
    /// Scratch ceiling assigned only to pending encoded output.
    output_scratch_limit_bytes: u64,
    /// Attempt-local sibling counter for pending encoded output files.
    output_scratch_used_bytes: Arc<AtomicU64>,
    /// Largest bounded sort-spill plus pending-output ownership observation.
    scratch_peak_bytes: Arc<AtomicU64>,
    /// Fixed encoder/upload reservation derived from the acquired envelope.
    output_allowance_bytes: usize,
    /// Exact bytes reserved before polling each decoded stream.
    decoded_batch_bytes: usize,
    /// Explicit `DataFusion` sort merge reservation.
    sort_merge_reservation_bytes: usize,
    /// Exact row-group flush threshold derived from the encoder term.
    encoder_flush_bytes: usize,
    /// Exact bounded upload allocation and writer chunk.
    upload_chunk_bytes: usize,
    /// Serial phase owner for the one encoded-footer child and decoder workspace.
    footer_phase: Arc<ForgeFooterPhase>,
}

/// Exact runtime terms used to construct one attempt-owned execution environment.
#[derive(Clone, Copy)]
pub(crate) struct ForgeAttemptEnvelope {
    /// Aggregate scratch lease.
    spill_limit: u64,
    /// Scratch sibling assigned to `DataFusion` sort spill.
    sort_spill_limit_bytes: u64,
    /// Combined encoder and upload resident allowance.
    output_allowance_bytes: usize,
    /// Exact decoder reservation per active source.
    decoded_batch_bytes: usize,
    /// Explicit sort merge reservation.
    sort_merge_reservation_bytes: usize,
    /// Parquet row-group flush threshold.
    encoder_flush_bytes: usize,
    /// Upload allocation and backend writer chunk.
    upload_chunk_bytes: usize,
    /// Encoded footer child retained from writer creation through decode.
    footer_encoded_bytes: usize,
    /// Decoder workspace that joins the encoded footer only in metadata phase.
    footer_decode_workspace_bytes: usize,
}

/// Serial owner for non-overlapping Forge footer execution and metadata phases.
#[derive(Debug)]
struct ForgeFooterPhase {
    /// One permit prevents source metadata and output encoding from overlapping.
    gate: Arc<tokio::sync::Semaphore>,
    /// Encoded footer child present in both phases.
    encoded_bytes: usize,
    /// Decoder workspace present only after seal or during source metadata.
    decode_bytes: usize,
    /// Largest live footer-phase ownership observed by this attempt.
    peak_bytes: AtomicU64,
}

impl ForgeFooterPhase {
    /// Constructs the exact positive version-2 footer terms.
    ///
    /// # Errors
    /// Returns invalid configuration when either persisted term is zero.
    fn new(encoded_bytes: usize, decode_bytes: usize) -> Result<Self, ForgeError> {
        if encoded_bytes == 0 || decode_bytes == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge footer phase terms must be positive".to_owned(),
            });
        }
        Ok(Self {
            gate: Arc::new(tokio::sync::Semaphore::new(1)),
            encoded_bytes,
            decode_bytes,
            peak_bytes: AtomicU64::new(0),
        })
    }

    /// Acquires source/output metadata ownership before footer IO or decode.
    ///
    /// # Errors
    /// Returns an invariant error if the attempt phase owner has closed.
    async fn enter_metadata(self: &Arc<Self>) -> Result<ForgeFooterPhaseGuard, ForgeError> {
        let permit = Arc::clone(&self.gate)
            .acquire_owned()
            .await
            .map_err(|error| ForgeError::Invariant {
                detail: format!("Forge footer phase owner closed: {error}"),
            })?;
        self.observe(self.encoded_bytes.saturating_add(self.decode_bytes));
        Ok(ForgeFooterPhaseGuard {
            owner: Arc::clone(self),
            _permit: permit,
            metadata: true,
        })
    }

    /// Acquires the encoded-footer child before output writer creation.
    ///
    /// # Errors
    /// Returns an invariant error if the attempt phase owner has closed.
    async fn enter_execution(self: &Arc<Self>) -> Result<ForgeFooterPhaseGuard, ForgeError> {
        let permit = Arc::clone(&self.gate)
            .acquire_owned()
            .await
            .map_err(|error| ForgeError::Invariant {
                detail: format!("Forge footer phase owner closed: {error}"),
            })?;
        self.observe(self.encoded_bytes);
        Ok(ForgeFooterPhaseGuard {
            owner: Arc::clone(self),
            _permit: permit,
            metadata: false,
        })
    }

    /// Records one checked phase peak without acquiring another root lease.
    fn observe(&self, bytes: usize) {
        self.peak_bytes
            .fetch_max(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::AcqRel);
    }
}

/// Move-owned footer phase capability holding the serial phase permit.
#[derive(Debug)]
struct ForgeFooterPhaseGuard {
    /// Attempt phase owner whose exact terms this guard represents.
    owner: Arc<ForgeFooterPhase>,
    /// Permit retained across writer creation/seal or footer decode.
    _permit: tokio::sync::OwnedSemaphorePermit,
    /// Whether decoder workspace has joined the encoded child.
    metadata: bool,
}

impl ForgeFooterPhaseGuard {
    /// Transitions the same encoded child from execution into metadata.
    #[must_use]
    fn into_metadata(mut self) -> Self {
        if !self.metadata {
            self.metadata = true;
            self.owner.observe(
                self.owner
                    .encoded_bytes
                    .saturating_add(self.owner.decode_bytes),
            );
        }
        self
    }
}

impl ForgeRewriteRuntime {
    /// Creates the pod scratch root without inferring attempt ownership.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::ScratchIo`] when the root cannot be created.
    pub(crate) fn prepare_root(pod_spill_root: &Path) -> Result<(), ForgeError> {
        std::fs::create_dir_all(pod_spill_root).map_err(|error| ForgeError::ScratchIo {
            kind: error.kind(),
            detail: error.to_string(),
        })
    }

    /// Creates one attempt runtime without touching concurrent sibling directories.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] for a zero lease or a Parquet or
    /// `DataFusion` error when its unique child or runtime cannot be built.
    pub(crate) fn new_attempt(
        memory_pool: Arc<dyn MemoryPool>,
        pod_spill_root: &Path,
        task_id: Uuid,
        attempt_id: Uuid,
        envelope: ForgeAttemptEnvelope,
    ) -> Result<Self, ForgeError> {
        let ForgeAttemptEnvelope {
            spill_limit: spill_limit_bytes,
            sort_spill_limit_bytes,
            output_allowance_bytes,
            decoded_batch_bytes,
            sort_merge_reservation_bytes,
            encoder_flush_bytes,
            upload_chunk_bytes,
            footer_encoded_bytes,
            footer_decode_workspace_bytes,
        } = envelope;
        if spill_limit_bytes == 0
            || sort_spill_limit_bytes == 0
            || sort_spill_limit_bytes > spill_limit_bytes
            || decoded_batch_bytes == 0
            || sort_merge_reservation_bytes == 0
            || encoder_flush_bytes == 0
            || upload_chunk_bytes == 0
            || footer_encoded_bytes == 0
            || footer_decode_workspace_bytes == 0
        {
            return Err(ForgeError::InvalidConfig {
                detail:
                    "Forge scratch envelope must contain positive disjoint sort and output terms"
                        .to_owned(),
            });
        }
        std::fs::create_dir_all(pod_spill_root).map_err(|error| ForgeError::ScratchIo {
            kind: error.kind(),
            detail: error.to_string(),
        })?;
        let prefix = format!("forge-runtime-{task_id}-{attempt_id}-");
        let spill_dir = tempfile::Builder::new()
            .prefix(&prefix)
            .tempdir_in(pod_spill_root)
            .map_err(|error| ForgeError::ScratchIo {
                kind: error.kind(),
                detail: error.to_string(),
            })?;
        let runtime = RuntimeEnvBuilder::new()
            .with_memory_pool(memory_pool)
            .with_temp_file_path(spill_dir.path())
            .with_max_temp_directory_size(sort_spill_limit_bytes)
            .build()
            .map_err(ForgeError::DataFusion)?;
        Ok(Self {
            runtime: Arc::new(runtime),
            spill_dir,
            spill_limit_bytes,
            output_scratch_limit_bytes: spill_limit_bytes - sort_spill_limit_bytes,
            output_scratch_used_bytes: Arc::new(AtomicU64::new(0)),
            scratch_peak_bytes: Arc::new(AtomicU64::new(0)),
            output_allowance_bytes,
            decoded_batch_bytes,
            sort_merge_reservation_bytes,
            encoder_flush_bytes,
            upload_chunk_bytes,
            footer_phase: Arc::new(ForgeFooterPhase::new(
                footer_encoded_bytes,
                footer_decode_workspace_bytes,
            )?),
        })
    }

    /// Pre-reserves the complete pending-output scratch term before file creation.
    ///
    /// `DataFusion` receives only `sort_spill_limit_bytes`; this sibling counter
    /// owns the remainder, so the two concurrent consumers cannot exceed the
    /// aggregate attempt lease.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::ExecutionEnvelopeExceeded`] if another pending
    /// output already owns any portion of the attempt's fixed scratch term.
    fn reserve_output_scratch(&self) -> Result<OutputScratchReservation, ForgeError> {
        self.output_scratch_used_bytes
            .compare_exchange(
                0,
                self.output_scratch_limit_bytes,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|used| ForgeError::ExecutionEnvelopeExceeded {
                resource: "scratch",
                detail: format!(
                    "Forge output scratch is already reserved: used={used}, limit={}",
                    self.output_scratch_limit_bytes
                ),
            })?;
        self.scratch_peak_bytes
            .fetch_max(self.output_scratch_limit_bytes, Ordering::AcqRel);
        Ok(OutputScratchReservation {
            used_bytes: Arc::clone(&self.output_scratch_used_bytes),
            bytes: self.output_scratch_limit_bytes,
            peak_bytes: Arc::clone(&self.scratch_peak_bytes),
        })
    }

    /// Return the configured operation spill ceiling.
    #[must_use]
    pub fn spill_limit_bytes(&self) -> u64 {
        self.spill_limit_bytes
    }

    /// Returns the conservative attempt scratch peak used at final settlement.
    ///
    /// The runtime updates this value when it reserves pending output and after
    /// each bounded sort-spill observation, so it is the conservative checked
    /// sum of those concurrently possible siblings capped by the lease.
    #[must_use]
    fn scratch_peak_bytes(&self) -> u64 {
        self.scratch_peak_bytes
            .load(Ordering::Acquire)
            .min(self.spill_limit_bytes)
    }

    /// Returns the disjoint sort and output scratch ownership for diagnostics.
    #[cfg(test)]
    #[must_use]
    fn scratch_usage(&self) -> (u64, u64, u64) {
        (
            self.spill_limit_bytes - self.output_scratch_limit_bytes,
            self.output_scratch_used_bytes.load(Ordering::Acquire),
            self.spill_limit_bytes,
        )
    }

    /// Clone the runtime handle for one `DataFusion` task context.
    #[must_use]
    fn runtime(&self) -> Arc<RuntimeEnv> {
        debug_assert!(self.spill_dir.path().exists());
        Arc::clone(&self.runtime)
    }

    /// Reports whether this runtime retained the exact leased memory pool.
    #[must_use]
    pub(crate) fn uses_memory_pool(&self, pool: &Arc<dyn MemoryPool>) -> bool {
        Arc::ptr_eq(&self.runtime.memory_pool, pool)
    }

    /// Return the owned spill path for diagnostics and tests.
    #[must_use]
    pub(crate) fn scratch_path(&self) -> &Path {
        self.spill_dir.path()
    }

    /// Return the owned scratch path through the legacy test-only name.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn spill_path(&self) -> &Path {
        self.scratch_path()
    }
}

#[cfg(test)]
mod tests {
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::execution::memory_pool::GreedyMemoryPool;
    use wyrd_bench::BenchmarkRecorder;

    use super::*;

    /// Builds the tenant-qualified table binding retained by attempt tests.
    fn attempt_binding() -> crate::catalog::TenantTableBinding {
        crate::catalog::TenantTableBinding::resolve((
            wyrd_spec::DataTenantId::new_v7(),
            crate::catalog::TableRef::new(crate::namespaces::BifrostNamespace::Traces, "spans"),
        ))
        .expect("attempt binding")
    }

    /// A claim request retains every validated persisted term without reconstruction.
    #[test]
    fn rewrite_request_preserves_every_persisted_term() {
        let capacity = ForgeCapacity {
            max_files: 4,
            max_bytes: u64::MAX,
            max_parallelism: 4,
            max_memory_bytes: 256 * 1024 * 1024,
            max_spill_bytes: 1024 * 1024 * 1024,
            max_large_task_bytes: u64::MAX,
        };
        let envelope = crate::forge::ForgeEnvelopeSizer::size(1, 4, 4, capacity).expect("envelope");
        let estimates = ForgeTaskEstimates {
            files: 4,
            bytes: 1,
            parallelism: envelope.reader_permits,
            memory_bytes: envelope.memory_bytes().expect("resident total"),
            spill_bytes: envelope.scratch_bytes().expect("scratch total"),
            large_ceiling_bytes: i64::MAX as u64,
            envelope: Some(envelope),
        };

        let request = ForgeRewriteRequest::from_claim(&estimates, capacity).expect("request");

        assert_eq!(request.envelope, envelope);
        assert_eq!(
            request.memory_bytes,
            usize::try_from(envelope.memory_bytes().expect("resident total"))
                .expect("platform total")
        );
        assert_eq!(
            request.scratch_bytes,
            envelope.scratch_bytes().expect("scratch total")
        );
        assert_eq!(request.reader_permits, envelope.reader_permits);
    }

    /// Physical grouping honors the operator target without weakening hard caps.
    #[test]
    fn physical_file_grouping_caps_row_groups_and_treats_target_as_preference() {
        let mib = 1024 * 1024;
        let slices = (0..6)
            .map(|offset| BoundedRowSlice {
                offset,
                len: 1,
                logical_bytes: 16 * mib,
            })
            .collect::<VecDeque<_>>();

        let mut small_target = slices.clone();
        let first = take_physical_file_group(&mut small_target, 40 * mib);
        assert_eq!(
            first.len(),
            2,
            "logical target rotates before a third group"
        );

        let mut large_target = slices;
        let first = take_physical_file_group(&mut large_target, u64::MAX);
        assert_eq!(first.len(), MAX_FILE_ROW_GROUPS);
        assert_eq!(
            large_target.len(),
            2,
            "remaining groups retain source order"
        );

        let mut below_one_group = VecDeque::from([BoundedRowSlice {
            offset: 0,
            len: 1,
            logical_bytes: 16 * mib,
        }]);
        assert_eq!(
            take_physical_file_group(&mut below_one_group, 1).len(),
            1,
            "a soft target never refuses a safe row group"
        );
    }

    /// Encoded overflow bisects only the reported row group and preserves order.
    #[test]
    fn encoded_row_group_retry_preserves_unaffected_group_order() {
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "value",
                DataType::Int64,
                false,
            )])),
            vec![Arc::new(Int64Array::from_iter_values(0..8))],
        )
        .expect("batch");
        let group = vec![
            BoundedRowSlice {
                offset: 0,
                len: 2,
                logical_bytes: 16,
            },
            BoundedRowSlice {
                offset: 2,
                len: 4,
                logical_bytes: 32,
            },
            BoundedRowSlice {
                offset: 6,
                len: 2,
                logical_bytes: 16,
            },
        ];
        let mut pending = VecDeque::new();

        requeue_bisected_file(&mut pending, &batch, group, 1).expect("bisect retry");

        assert_eq!(
            pending
                .iter()
                .map(|slice| (slice.offset, slice.len))
                .collect::<Vec<_>>(),
            vec![(0, 2), (2, 2), (4, 2), (6, 2)]
        );
    }

    /// Proves an attempt executes on its leased pool and restores baselines.
    ///
    /// The attempt is acquired through the same production capability the Forge
    /// worker uses, so this pins lease-issued pool identity, spill-child
    /// ownership, and exact release of both counters on drop.
    #[test]
    fn forge_attempt_resources_use_lease_pool_and_restore_baselines_on_drop() {
        let roles = crate::resources::BifrostRuntimeResources::composed_for_test(
            2 * 1024 * 1024 * 1024,
            4 * 1024 * 1024,
            [crate::resources::BifrostRole::Forge],
        );
        let forge = roles
            .forge()
            .expect("composition must enable the Forge capability");
        let baseline = forge.snapshot().expect("live baseline");
        let root = tempfile::tempdir().expect("pod spill root");
        ForgeRewriteRuntime::prepare_root(root.path()).expect("prepared root");

        let envelope = crate::forge::ForgeEnvelopeSizer::size(
            1,
            1,
            1,
            crate::forge::ForgeCapacity {
                max_files: 1,
                max_bytes: u64::MAX,
                max_parallelism: 1,
                max_memory_bytes: 64 * 1024 * 1024,
                max_spill_bytes: 4 * 1024 * 1024,
                max_large_task_bytes: u64::MAX,
            },
        )
        .expect("test envelope");
        let mut attempt = ForgeAttemptResources::acquire(
            &forge,
            crate::resources::ForgeRewriteRequest {
                envelope,
                memory_bytes: usize::try_from(envelope.memory_bytes().expect("resident total"))
                    .expect("platform resident total"),
                scratch_bytes: envelope.scratch_bytes().expect("scratch total"),
                reader_permits: 1,
            },
            &attempt_binding(),
            root.path(),
            Uuid::nil(),
            Uuid::now_v7(),
            Arc::new(crate::forge::ForgeTelemetry::new()),
        )
        .expect("attempt resources");
        assert!(
            attempt.runtime().uses_memory_pool(&attempt.memory_pool()),
            "the attempt runtime must execute on the exact leased pool"
        );
        assert_eq!(
            attempt.runtime().output_allowance_bytes,
            usize::try_from(envelope.encoder_buffer_bytes + envelope.upload_chunk_bytes)
                .expect("platform output allowance")
        );
        let projection_reserved = attempt.memory_pool().reserved();
        assert!(
            projection_reserved > 0,
            "the physical projection must retain its admitted child"
        );
        let reservation =
            MemoryConsumer::new("forge-rewrite-output-peak-proof").register(&attempt.memory_pool());
        reservation
            .try_grow(4096)
            .expect("attempt-local output reservation");
        assert_eq!(
            attempt.peak_memory_bytes.load(Ordering::Acquire),
            u64::try_from(projection_reserved + 4096).expect("test peak fits u64"),
            "the production pool wrapper must retain the exact attempt peak"
        );
        reservation.free();
        assert_eq!(attempt.memory_pool().reserved(), projection_reserved);

        let held = forge.snapshot().expect("held snapshot");
        assert_eq!(
            held.forge_memory_used_bytes,
            baseline.forge_memory_used_bytes
                + usize::try_from(envelope.memory_bytes().expect("resident total"))
                    .expect("platform resident total")
        );
        assert_eq!(
            held.elastic_memory_used_bytes,
            baseline.elastic_memory_used_bytes
        );
        assert_eq!(
            held.scratch_used_bytes,
            baseline.scratch_used_bytes + envelope.scratch_bytes().expect("scratch total")
        );

        assert_eq!(
            attempt.finish(),
            crate::resources::ForgeResourceReleaseResult::Released
        );
        drop(attempt);
        let after = forge.snapshot().expect("restored snapshot");
        assert_eq!(
            after.elastic_memory_used_bytes,
            baseline.elastic_memory_used_bytes
        );
        assert_eq!(after.scratch_used_bytes, baseline.scratch_used_bytes);
        assert!(
            std::fs::read_dir(root.path())
                .expect("root readable")
                .next()
                .is_none(),
            "the attempt must remove its owned spill child"
        );
    }

    /// Runtime configuration consumes the exact persisted execution terms.
    #[test]
    fn runtime_uses_exact_sort_merge_encoder_upload_and_decoded_terms() {
        let root = tempfile::tempdir().expect("spill root");
        let runtime = ForgeRewriteRuntime::new_attempt(
            Arc::new(GreedyMemoryPool::new(4096)),
            root.path(),
            Uuid::nil(),
            Uuid::now_v7(),
            ForgeAttemptEnvelope {
                spill_limit: 1024,
                sort_spill_limit_bytes: 600,
                output_allowance_bytes: 300,
                decoded_batch_bytes: 101,
                sort_merge_reservation_bytes: 102,
                encoder_flush_bytes: 103,
                upload_chunk_bytes: 104,
                footer_encoded_bytes: 105,
                footer_decode_workspace_bytes: 106,
            },
        )
        .expect("runtime");

        assert_eq!(runtime.sort_merge_reservation_bytes, 102);
        assert_eq!(runtime.encoder_flush_bytes, 103);
        assert_eq!(runtime.upload_chunk_bytes, 104);
        assert_eq!(runtime.decoded_batch_bytes, 101);
        assert_eq!(
            runtime.spill_limit_bytes - runtime.output_scratch_limit_bytes,
            600
        );
        assert_eq!(runtime.output_scratch_limit_bytes, 424);
    }

    /// Attempt finalization is idempotent and poisons when a pipeline handle survives.
    #[test]
    fn panicking_attempt_finalizes_or_poisons_before_worker_exit() {
        let recorder = BenchmarkRecorder::new();
        metrics::with_local_recorder(&recorder, || {
            let roles = crate::resources::BifrostRuntimeResources::composed_for_test(
                2 * 1024 * 1024 * 1024,
                4 * 1024 * 1024,
                [crate::resources::BifrostRole::Forge],
            );
            let forge = roles.forge().expect("Forge capability");
            let root = tempfile::tempdir().expect("spill root");
            let envelope = crate::forge::ForgeEnvelopeSizer::size(
                1,
                1,
                1,
                ForgeCapacity {
                    max_files: 1,
                    max_bytes: u64::MAX,
                    max_parallelism: 1,
                    max_memory_bytes: 64 * 1024 * 1024,
                    max_spill_bytes: 4 * 1024 * 1024,
                    max_large_task_bytes: u64::MAX,
                },
            )
            .expect("envelope");
            let request = ForgeRewriteRequest {
                envelope,
                memory_bytes: usize::try_from(envelope.memory_bytes().expect("resident total"))
                    .expect("platform total"),
                scratch_bytes: envelope.scratch_bytes().expect("scratch total"),
                reader_permits: envelope.reader_permits,
            };
            let mut attempt = ForgeAttemptResources::acquire(
                &forge,
                request,
                &attempt_binding(),
                root.path(),
                Uuid::nil(),
                Uuid::now_v7(),
                Arc::new(crate::forge::ForgeTelemetry::new()),
            )
            .expect("attempt");
            let memory =
                MemoryConsumer::new("forge-panic-finalizer-peak").register(&attempt.memory_pool());
            memory.try_grow(1).expect("reserve one resident byte");
            memory.free();
            drop(
                attempt
                    .runtime()
                    .reserve_output_scratch()
                    .expect("reserve output scratch peak"),
            );
            let surviving = Arc::clone(
                attempt
                    .runtime
                    .as_ref()
                    .expect("active attempt retains runtime"),
            );

            assert_eq!(
                attempt.finish(),
                crate::resources::ForgeResourceReleaseResult::Poisoned
            );
            assert_eq!(
                attempt.finish(),
                crate::resources::ForgeResourceReleaseResult::Poisoned
            );
            drop(surviving);
            assert!(forge.snapshot().is_err(), "poisoning must fail closed");
        });
        let snapshot = recorder.snapshot();
        for resource in ["memory", "scratch"] {
            let poisoned = snapshot
                .counters
                .iter()
                .filter(|(name, _)| {
                    name.starts_with("bifrost_forge_attempt_resource_releases_total{")
                        && name.contains(&format!("resource=\"{resource}\""))
                        && name.contains("result=\"poisoned\"")
                })
                .map(|(_, count)| *count)
                .sum::<u64>();
            assert_eq!(poisoned, 1, "{resource} poison records exactly once");
        }
        assert_eq!(
            snapshot
                .histograms
                .iter()
                .filter(|(name, _)| { name.starts_with("bifrost_forge_attempt_resource_bytes{") })
                .map(|(_, histogram)| histogram.count)
                .sum::<u64>(),
            6,
            "one finalization records three observations for both resources"
        );
    }

    /// Returns ceilings wide enough to admit the parity fixture's exact demand.
    #[cfg(feature = "test-support")]
    fn parity_capacity() -> ForgeCapacity {
        ForgeCapacity {
            max_files: 8,
            max_bytes: 4_096,
            max_parallelism: 8,
            max_memory_bytes: 256 * 1024 * 1024,
            max_spill_bytes: 2 * 1024 * 1024,
            max_large_task_bytes: 8_192,
        }
    }

    /// Builds one deterministic candidate input of the requested size.
    #[cfg(feature = "test-support")]
    fn parity_file(path: &str, bytes: u64) -> super::super::right_size::IcebergCandidateFile {
        super::super::right_size::IcebergCandidateFile {
            catalog_path: path.to_owned(),
            object_path: path.to_owned(),
            file_size_bytes: bytes,
            record_count: 1,
            schema_id: 1,
            partition_spec_id: 1,
            partition_day: chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
                .expect("fixed parity day is valid"),
            sort_order_id: Some(1),
            writer_recipe_version: Some(BIFROST_WRITER_RECIPE_VERSION.to_owned()),
            min_event_time: chrono::DateTime::from_timestamp(1, 0)
                .expect("fixed parity timestamp is valid"),
            max_event_time: chrono::DateTime::from_timestamp(2, 0)
                .expect("fixed parity timestamp is valid"),
            source_snapshot_id: 1,
            data_sequence_number: Some(1),
            file_sequence_number: Some(1),
        }
    }

    /// The direct live-replacement request equals the request a persisted plan
    /// for the same group would produce.
    ///
    /// This is the sole guard that the `test-support` replacement path cannot
    /// drift into acquiring configuration ceilings instead of exact demand: the
    /// group's request is compared against the request derived from the durable
    /// estimates the production planner persists for the identical group.
    #[cfg(feature = "test-support")]
    #[test]
    fn live_group_request_matches_planned_candidate_estimates() {
        let capacity = parity_capacity();
        let group = super::super::right_size::IcebergRewriteGroup {
            files: vec![parity_file("a.parquet", 300), parity_file("b.parquet", 500)],
            reason: super::super::right_size::IcebergRewriteReason::Undersized,
        };

        let direct = ForgeRewriteRequest::from_live_group(&group, capacity)
            .expect("live group must form an exact request");

        let candidate = ForgePlanCandidate::from_live_group(
            &group,
            usize::from(capacity.max_parallelism),
            capacity,
        )
        .expect("group must map to a candidate");
        let planned = ForgePlanner::new(capacity)
            .plan_table(&super::super::planner::ForgeTableSnapshot {
                snapshot_id: 1,
                candidates: vec![candidate],
            })
            .expect("candidate must plan");
        let persisted = ForgeRewriteRequest::from_claim(&planned[0].estimates, capacity)
            .expect("persisted estimates must form an exact request");

        assert_eq!(direct, persisted);
        assert_eq!(
            u64::try_from(direct.memory_bytes).ok(),
            Some(planned[0].estimates.memory_bytes),
            "a small group leases its persisted streaming envelope"
        );
        assert_eq!(direct.scratch_bytes, planned[0].estimates.spill_bytes);
        assert_ne!(
            u64::try_from(direct.memory_bytes).ok(),
            Some(capacity.max_memory_bytes),
            "demand is never the configuration ceiling itself"
        );
    }

    /// A zero spill ceiling cannot create an unbounded rewrite runtime.
    #[test]
    fn rewrite_runtime_rejects_zero_spill_limit() {
        let root = tempfile::tempdir().expect("test spill root must be created");
        let result = ForgeRewriteRuntime::new_attempt(
            Arc::new(GreedyMemoryPool::new(1_024)),
            root.path(),
            Uuid::nil(),
            Uuid::nil(),
            ForgeAttemptEnvelope {
                spill_limit: 0,
                sort_spill_limit_bytes: 0,
                output_allowance_bytes: 2,
                decoded_batch_bytes: 1,
                sort_merge_reservation_bytes: 1,
                encoder_flush_bytes: 1,
                upload_chunk_bytes: 1,
                footer_encoded_bytes: 1,
                footer_decode_workspace_bytes: 1,
            },
        );
        assert!(matches!(result, Err(ForgeError::InvalidConfig { .. })));
    }

    /// Runtime startup removes only stale Forge-owned child directories.
    #[test]
    fn rewrite_runtime_preserves_unscoped_spill_children() {
        let root = tempfile::tempdir().expect("test spill root must be created");
        let stale = root.path().join("forge-runtime-stale");
        let unrelated = root.path().join("scribe-wal");
        std::fs::create_dir(&stale).expect("stale Forge child must be created");
        std::fs::create_dir(&unrelated).expect("unrelated child must be created");
        let runtime = ForgeRewriteRuntime::new_attempt(
            Arc::new(GreedyMemoryPool::new(1_024)),
            root.path(),
            Uuid::nil(),
            Uuid::nil(),
            ForgeAttemptEnvelope {
                spill_limit: 1_024,
                sort_spill_limit_bytes: 512,
                output_allowance_bytes: 2,
                decoded_batch_bytes: 1,
                sort_merge_reservation_bytes: 1,
                encoder_flush_bytes: 1,
                upload_chunk_bytes: 1,
                footer_encoded_bytes: 1,
                footer_decode_workspace_bytes: 1,
            },
        )
        .expect("bounded rewrite runtime must be created");
        let active = runtime.spill_path().to_path_buf();
        assert!(stale.exists());
        assert!(unrelated.exists());
        assert!(active.exists());
        drop(runtime);
        assert!(!active.exists());
        assert!(unrelated.exists());
    }

    /// Sort spill and pending output own disjoint terms within one scratch lease.
    #[test]
    fn rewrite_runtime_accounts_output_alongside_sort_spill_and_releases_it() {
        let root = tempfile::tempdir().expect("test spill root");
        let runtime = ForgeRewriteRuntime::new_attempt(
            Arc::new(GreedyMemoryPool::new(1024)),
            root.path(),
            Uuid::nil(),
            Uuid::now_v7(),
            ForgeAttemptEnvelope {
                spill_limit: 1024,
                sort_spill_limit_bytes: 640,
                output_allowance_bytes: 128,
                decoded_batch_bytes: 32,
                sort_merge_reservation_bytes: 32,
                encoder_flush_bytes: 32,
                upload_chunk_bytes: 32,
                footer_encoded_bytes: 8,
                footer_decode_workspace_bytes: 32,
            },
        )
        .expect("disjoint scratch envelope");

        let reservation = runtime.reserve_output_scratch().expect("output scratch");
        let (sort, output, aggregate) = runtime.scratch_usage();
        assert_eq!(sort, 640);
        assert_eq!(output, 384);
        assert_eq!(sort + output, aggregate);
        assert!(runtime.reserve_output_scratch().is_err());

        drop(reservation);
        assert_eq!(runtime.scratch_usage(), (640, 0, 1024));
        assert!(runtime.reserve_output_scratch().is_ok());
    }

    /// Physical output cannot cross its sibling ceiling and cleanup releases it.
    #[test]
    fn scratch_writer_refuses_one_byte_before_physical_growth() {
        let root = tempfile::tempdir().expect("test spill root");
        let runtime = ForgeRewriteRuntime::new_attempt(
            Arc::new(GreedyMemoryPool::new(1024)),
            root.path(),
            Uuid::nil(),
            Uuid::now_v7(),
            ForgeAttemptEnvelope {
                spill_limit: 1024,
                sort_spill_limit_bytes: 640,
                output_allowance_bytes: 128,
                decoded_batch_bytes: 32,
                sort_merge_reservation_bytes: 32,
                encoder_flush_bytes: 32,
                upload_chunk_bytes: 32,
                footer_encoded_bytes: 8,
                footer_decode_workspace_bytes: 32,
            },
        )
        .expect("disjoint scratch envelope");
        let reservation = runtime.reserve_output_scratch().expect("output scratch");
        let mut spill = runtime
            .runtime
            .disk_manager
            .create_tmp_file("aggregate-scratch")
            .expect("real DataFusion spill file");
        spill
            .inner()
            .as_file()
            .write_all(&vec![5_u8; 640])
            .expect("fill exact sort sibling");
        spill.update_disk_usage().expect("account real sort spill");
        let path = runtime.scratch_path().join("bounded-output.bin");
        let file = std::fs::File::create(&path).expect("bounded output file");
        let mut writer = ScratchBoundedWriter::new(file, reservation.bytes);
        writer
            .write_all(&vec![7_u8; 384])
            .expect("exact sibling ceiling");
        writer.flush().expect("flush exact sibling ceiling");
        assert_eq!(
            std::fs::metadata(&path).expect("bounded metadata").len(),
            384
        );
        let error = writer.write_all(&[8]).expect_err("one-byte excess refused");
        assert_eq!(error.kind(), std::io::ErrorKind::StorageFull);
        writer.flush().expect("flush after refusal");
        assert_eq!(
            std::fs::metadata(&path).expect("unchanged metadata").len(),
            384
        );
        let (sort, output, aggregate) = runtime.scratch_usage();
        assert_eq!((sort, output, aggregate), (640, 384, 1024));
        assert_eq!(runtime.runtime.spilling_progress().current_bytes, 640);
        drop(writer);
        std::fs::remove_file(path).expect("remove bounded output");
        drop(reservation);
        drop(spill);
        assert_eq!(runtime.scratch_usage(), (640, 0, 1024));
        assert_eq!(runtime.runtime.spilling_progress().current_bytes, 0);
    }

    /// A decoded slot waits for competing ownership and becomes reusable on release.
    #[tokio::test]
    async fn decoded_batch_slot_blocks_then_releases() {
        let pool: Arc<dyn MemoryPool> = Arc::new(GreedyMemoryPool::new(DECODED_BATCH_TARGET_BYTES));
        let holder =
            datafusion::execution::memory_pool::MemoryConsumer::new("holder").register(&pool);
        holder
            .try_grow(DECODED_BATCH_TARGET_BYTES)
            .expect("holder fills pool");
        let waiter =
            datafusion::execution::memory_pool::MemoryConsumer::new("waiter").register(&pool);
        let wait = tokio::spawn(async move {
            reserve_decoded_batch_slot(&waiter, DECODED_BATCH_TARGET_BYTES).await;
            waiter
        });
        tokio::task::yield_now().await;
        assert!(
            !wait.is_finished(),
            "the next decode must not poll while full"
        );

        holder.free();
        let waiter = tokio::time::timeout(std::time::Duration::from_secs(1), wait)
            .await
            .expect("released capacity wakes waiter")
            .expect("waiter task");
        assert_eq!(waiter.size(), DECODED_BATCH_TARGET_BYTES);
        waiter.free();
        assert_eq!(pool.reserved(), 0);
    }

    /// Sub-target batches share one writer until accumulated bytes reach the target.
    #[test]
    fn sub_target_batches_coalesce_before_rotation() {
        let root = tempfile::tempdir().expect("output scratch");
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, false).with_metadata(HashMap::from([(
                "PARQUET:field_id".to_owned(),
                "1".to_owned(),
            )])),
        ]));
        let iceberg_schema = Arc::new(
            iceberg::spec::Schema::builder()
                .with_fields(vec![Arc::new(iceberg::spec::NestedField::required(
                    1,
                    "value",
                    iceberg::spec::Type::Primitive(iceberg::spec::PrimitiveType::Long),
                ))])
                .build()
                .expect("Iceberg schema"),
        );
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from_iter_values(0..4096))],
        )
        .expect("batch");
        let mut state = RewriteBatchState::with_reservation(None, None, root.path().to_path_buf());
        let phase = Arc::new(ForgeFooterPhase::new(8, 32).expect("footer phase"));
        state.footer_phase = Some(ForgeFooterPhaseGuard {
            owner: Arc::clone(&phase),
            _permit: Arc::clone(&phase.gate)
                .try_acquire_owned()
                .expect("execution phase"),
            metadata: false,
        });
        state
            .write_batch(
                &schema,
                &iceberg_schema,
                &batch,
                &[BoundedRowSlice {
                    offset: 0,
                    len: batch.num_rows(),
                    logical_bytes: 32_768,
                }],
                "s3://bucket/table/data/forge/bifrost-writer-v2/test-00000.parquet",
                ROW_GROUP_FLUSH_BYTES,
            )
            .expect("scratch encode");
        let first_bytes = state
            .writer
            .as_ref()
            .map(|writer| writer.bytes_written() + writer.in_progress_size())
            .expect("active first-batch writer");
        let target = u64::try_from(first_bytes.saturating_add(1)).expect("bounded target");
        assert!(
            !state.should_rotate(target),
            "one sub-target batch retains the active writer"
        );
        state
            .write_batch(
                &schema,
                &iceberg_schema,
                &batch,
                &[BoundedRowSlice {
                    offset: 0,
                    len: batch.num_rows(),
                    logical_bytes: 32_768,
                }],
                "s3://bucket/table/data/forge/bifrost-writer-v2/test-00000.parquet",
                ROW_GROUP_FLUSH_BYTES,
            )
            .expect("second scratch encode");
        assert!(
            state.should_rotate(target),
            "the next boundary rotates after accumulated bytes cross target"
        );
        let (writer, path, _phase) = state.take_writer().expect("active writer");
        let mut sink = writer.into_inner().expect("close writer");
        sink.flush().expect("flush file");
        sink.file().sync_all().expect("fsync file");
        drop(sink);
        let file = std::fs::File::open(path).expect("sealed file");
        let metadata = parquet::file::metadata::ParquetMetaDataReader::new()
            .parse_and_finish(&file)
            .expect("valid Parquet footer");
        assert_eq!(metadata.file_metadata().num_rows(), 8192);

        validate_writer_v2_structure(&metadata).expect("writer-v2 structure is bounded");
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
