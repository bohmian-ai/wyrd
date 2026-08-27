//! Bounded, spillable staging-to-Iceberg rewrite execution.

use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
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

/// Recovers the table's canonical Bloom column union from its Iceberg
/// properties.
///
/// Forge rewrites must reproduce the exact `bifrost-writer-v2` footer recipe
/// that Scribe wrote, so the union is read back from the registered table
/// rather than reconstructed from a fixed list.
///
/// # Errors
/// Returns [`ForgeError::InvalidConfig`] when the property is absent or is not
/// the canonical JSON string array; Forge fails closed rather than writing a
/// file whose footer misrepresents the recipe.
pub(crate) fn table_bloom_columns(
    metadata: &iceberg::spec::TableMetadata,
) -> Result<Arc<[String]>, super::ForgeError> {
    crate::catalog::layout::PhysicalLayout::bloom_columns_from_property(
        metadata
            .properties()
            .get(crate::catalog::layout::BLOOM_COLUMNS_PROPERTY),
    )
    .map_err(|detail| super::ForgeError::InvalidConfig { detail })
}

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
        let runtime =
            ForgeRewriteRuntime::new_attempt(
                Arc::clone(&pool),
                pod_spill_root,
                task_id,
                attempt_id,
                ForgeAttemptEnvelope {
                    spill_limit: lease.scratch_bytes(),
                    output_allowance_bytes: output_allowance,
                    decoded_batch_bytes,
                    sort_working_bytes: usize::try_from(request.envelope.sort_working_bytes)
                        .map_err(|_| ForgeError::Capacity {
                            detail: "sort working term exceeds this platform".to_owned(),
                        })?,
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
                    upload_chunk_bytes: usize::try_from(request.envelope.upload_chunk_bytes)
                        .map_err(|_| ForgeError::Capacity {
                            detail: "upload chunk exceeds this platform".to_owned(),
                        })?,
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
use iceberg::spec::{DataFile, SchemaRef as IcebergSchemaRef, Struct};
use iceberg::writer::file_writer::ParquetWriter;
use parquet::arrow::AsyncArrowWriter;
use parquet::arrow::arrow_reader::ArrowReaderOptions;
use parquet::arrow::async_reader::{AsyncFileReader, ParquetRecordBatchStreamBuilder};
use parquet::arrow::async_writer::AsyncFileWriter;
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
use crate::parquet::writer_properties::bifrost_writer_properties_with_metadata;
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
    /// Testable object-store seam used for source reads and failure cleanup.
    object_store: Arc<dyn ForgeObjectStore>,
    /// Upper bound for concurrently open source readers.
    max_concurrent_reads: usize,
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
    /// Exact physical time partition shared by every source file.
    pub(crate) partition: crate::catalog::layout::TimePartition,
    /// Canonical Bloom column union registered for the destination table.
    ///
    /// Forge reproduces the same footer recipe Scribe wrote, so a rewritten
    /// file is byte-comparable to its inputs under one `bifrost-writer-v2`
    /// marker rather than silently losing or gaining Bloom filters.
    pub(crate) bloom_columns: Arc<[String]>,
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

/// Exact length of the Parquet trailer: four footer-length bytes plus `PAR1`.
const PARQUET_TRAILER_BYTES: usize = 8;

/// Streaming Parquet sink that hands encoded output bytes to the object store.
///
/// This is the only sink Forge's encoder writes into: bytes produced by
/// [`AsyncArrowWriter`] are split into writes no larger than the persisted
/// upload term and handed to [`ForgeObjectStore::write_output_chunk`] as they
/// are produced, so an output never exists as a local file. The sink also
/// accumulates the streamed digest, the streamed length, and the eight-byte
/// Parquet trailer, which together are the evidence the publication path uses
/// to accept a write without reading the object back.
struct ForgeOutputSink {
    /// Object-store seam that owns the real writer and its chunk observation.
    store: Arc<dyn ForgeObjectStore>,
    /// Staging operator the seam reopens the deterministic key against.
    staging: Arc<opendal::Operator>,
    /// Open backend writer, taken by [`Self::complete_upload`].
    writer: Option<opendal::Writer>,
    /// Deterministic object key this sink streams into.
    path: String,
    /// Exact per-write ceiling taken from the persisted upload term.
    chunk_bytes: usize,
    /// Bytes handed to the seam and accepted by it.
    streamed_bytes: u64,
    /// Rolling CRC32C over every accepted byte, in stream order.
    digest: u32,
    /// Last eight streamed bytes, which become the Parquet trailer at close.
    trailer: [u8; PARQUET_TRAILER_BYTES],
    /// Bytes of `trailer` that are populated, saturating at eight.
    trailer_len: usize,
    /// Whether the one first-chunk reopen has already been consumed.
    reopened: bool,
    /// Remote length reported by the backend when the upload completed.
    remote_bytes: Option<u64>,
    /// Typed Forge failure retained across the Parquet error boundary.
    failure: Option<ForgeError>,
}

impl ForgeOutputSink {
    /// Opens the backend writer for one deterministic output key.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::ObjectStore`] when the backend cannot begin a
    /// streaming or multipart upload for `path`.
    async fn open(
        store: Arc<dyn ForgeObjectStore>,
        staging: Arc<opendal::Operator>,
        path: String,
        chunk_bytes: usize,
    ) -> Result<Self, ForgeError> {
        let writer = store
            .output_writer(&staging, &path, chunk_bytes)
            .await
            .map_err(ForgeError::ObjectStore)?;
        Ok(Self {
            store,
            staging,
            writer: Some(writer),
            path,
            chunk_bytes,
            streamed_bytes: 0,
            digest: 0,
            trailer: [0_u8; PARQUET_TRAILER_BYTES],
            trailer_len: 0,
            reopened: false,
            remote_bytes: None,
            failure: None,
        })
    }

    /// Records one accepted chunk in the streamed length, digest, and trailer.
    fn observe(&mut self, chunk: &[u8]) {
        self.digest = crc32c::crc32c_append(self.digest, chunk);
        self.streamed_bytes = self
            .streamed_bytes
            .saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        if chunk.len() >= PARQUET_TRAILER_BYTES {
            self.trailer
                .copy_from_slice(&chunk[chunk.len() - PARQUET_TRAILER_BYTES..]);
            self.trailer_len = PARQUET_TRAILER_BYTES;
        } else if !chunk.is_empty() {
            self.trailer.rotate_left(chunk.len());
            self.trailer[PARQUET_TRAILER_BYTES - chunk.len()..].copy_from_slice(chunk);
            self.trailer_len = PARQUET_TRAILER_BYTES.min(self.trailer_len + chunk.len());
        }
    }

    /// Streams one encoder buffer as writes bounded by the upload term.
    ///
    /// A backend failure before the object has accepted any byte aborts the
    /// incomplete upload and reopens the same deterministic key exactly once,
    /// which is the only point at which a retry can still reproduce the whole
    /// object. Once any byte has been accepted the failure is terminal for the
    /// output, because the aborted upload cannot be replayed without the source
    /// batches the encoder has already consumed.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::ObjectStore`] when a chunk is rejected and the
    /// bounded reopen is unavailable or also fails, and [`ForgeError::Invariant`]
    /// when the sink has already completed its upload.
    async fn stream(&mut self, buffer: &Bytes) -> Result<(), ForgeError> {
        let mut offset = 0;
        while offset < buffer.len() {
            let end = buffer.len().min(offset + self.chunk_bytes);
            let chunk = buffer.slice(offset..end);
            match self.write_chunk(chunk.clone()).await {
                Ok(()) => {
                    self.observe(&chunk);
                    offset = end;
                }
                Err(error) => {
                    let accepted = self.streamed_bytes;
                    self.abort().await;
                    if accepted != 0 || self.reopened {
                        return Err(error);
                    }
                    self.reopened = true;
                    self.writer = Some(
                        self.store
                            .output_writer(&self.staging, &self.path, self.chunk_bytes)
                            .await
                            .map_err(ForgeError::ObjectStore)?,
                    );
                    offset = 0;
                }
            }
        }
        Ok(())
    }

    /// Hands one already-bounded chunk to the object-store seam.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::ObjectStore`] when the seam rejects the chunk and
    /// [`ForgeError::Invariant`] when no writer is open.
    async fn write_chunk(&mut self, chunk: Bytes) -> Result<(), ForgeError> {
        let writer = self.writer.as_mut().ok_or_else(|| ForgeError::Invariant {
            detail: "Forge output sink wrote after its upload completed".to_owned(),
        })?;
        self.store
            .write_output_chunk(writer, chunk)
            .await
            .map_err(ForgeError::ObjectStore)
    }

    /// Aborts the incomplete upload, discarding any partially accepted bytes.
    ///
    /// Abort is best effort: the deterministic key is generation scoped, so an
    /// upload the backend fails to reclaim is an orphan no successor attempt
    /// reuses and protected orphan GC deletes it.
    async fn abort(&mut self) {
        if let Some(mut writer) = self.writer.take()
            && let Err(error) = self.store.abort_output_writer(&mut writer).await
        {
            tracing::warn!(path = %self.path, %error, "Forge output abort was not confirmed");
        }
        self.streamed_bytes = 0;
        self.digest = 0;
        self.trailer = [0_u8; PARQUET_TRAILER_BYTES];
        self.trailer_len = 0;
    }

    /// Closes the upload and records the length the backend acknowledged.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::ObjectStore`] when the backend refuses to complete
    /// the upload, [`ForgeError::Invariant`] when the sink has already
    /// completed, and [`ForgeError::Invariant`] when the acknowledged length
    /// disagrees with the bytes this sink streamed.
    async fn complete_upload(&mut self) -> Result<(), ForgeError> {
        let mut writer = self.writer.take().ok_or_else(|| ForgeError::Invariant {
            detail: "Forge output sink completed twice".to_owned(),
        })?;
        let metadata = writer.close().await.map_err(ForgeError::ObjectStore)?;
        let remote = metadata.content_length();
        if remote != self.streamed_bytes {
            return Err(ForgeError::Invariant {
                detail: format!(
                    "Forge upload length mismatch: streamed={}, remote={remote}",
                    self.streamed_bytes
                ),
            });
        }
        self.remote_bytes = Some(remote);
        Ok(())
    }

    /// Returns the eight-byte Parquet trailer observed at the end of the stream.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::DataRefusal`] when fewer than eight bytes reached
    /// the object, which cannot be a Parquet file.
    fn trailer(&self) -> Result<[u8; PARQUET_TRAILER_BYTES], ForgeError> {
        if self.trailer_len < PARQUET_TRAILER_BYTES {
            return Err(ForgeError::DataRefusal {
                detail: "Parquet output is shorter than its trailer".to_owned(),
            });
        }
        Ok(self.trailer)
    }

    /// Takes the typed Forge failure retained across the Parquet error boundary.
    fn take_failure(&mut self) -> Option<ForgeError> {
        self.failure.take()
    }
}

impl AsyncFileWriter for ForgeOutputSink {
    /// Streams one encoder buffer, retaining the typed failure for the caller.
    ///
    /// The [`AsyncFileWriter`] contract only carries [`parquet::errors::ParquetError`],
    /// so the typed Forge cause is retained on the sink and recovered by the
    /// publication path through [`AsyncArrowWriter::into_inner`].
    fn write(&mut self, bs: Bytes) -> BoxFuture<'_, parquet::errors::Result<()>> {
        Box::pin(async move {
            self.stream(&bs).await.map_err(|error| {
                let detail = error.to_string();
                self.failure = Some(error);
                parquet::errors::ParquetError::General(detail)
            })
        })
    }

    /// Completes the upload so the object is durable before metadata is derived.
    ///
    /// The typed Forge cause is retained on the sink exactly as in
    /// [`Self::write`].
    fn complete(&mut self) -> BoxFuture<'_, parquet::errors::Result<()>> {
        Box::pin(async move {
            self.complete_upload().await.map_err(|error| {
                let detail = error.to_string();
                self.failure = Some(error);
                parquet::errors::ParquetError::General(detail)
            })
        })
    }
}

/// Mutable writer and accounting state for one streamed rewrite operation.
///
/// The state owns every output path as soon as its PUT succeeds and retains
/// that ownership across later batch failures so rewrite finalization can
/// delete the complete accumulated set.
struct RewriteBatchState {
    /// Currently open streaming Parquet writer, if any batch has been accepted.
    writer: Option<AsyncArrowWriter<ForgeOutputSink>>,
    /// Footer-phase capability retained for the lifetime of the active writer.
    footer_phase: Option<ForgeFooterPhaseGuard>,
    /// Object key the active writer streams into, present exactly with `writer`.
    output_object_path: Option<String>,
    /// Catalog-visible path stamped into the active output's Iceberg metadata.
    output_table_path: Option<String>,
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
    /// Attempt-local conservative scratch peak shared with the resource owner.
    scratch_peak_bytes: Option<Arc<AtomicU64>>,
    /// Per-output NaN counts keyed by Iceberg field ID.
    nan_value_counts: NanValueCountVisitor,
    /// Canonical Bloom column union applied to every output this state writes.
    bloom_columns: Arc<[String]>,
}

impl RewriteBatchState {
    /// Create empty state for isolated batch-state tests and callers without
    /// a shared `DataFusion` reservation.
    #[cfg(test)]
    fn new() -> Self {
        Self::with_reservation(None, None, Arc::from([]))
    }

    /// Create state with an optional shared-pool output reservation.
    ///
    /// `scratch_peak_bytes` is the attempt runtime's conservative sort-spill
    /// high-water counter; it is absent only for the isolated batch-state tests
    /// that never observe spill.
    fn with_reservation(
        reservation: Option<datafusion::execution::memory_pool::MemoryReservation>,
        scratch_peak_bytes: Option<Arc<AtomicU64>>,
        bloom_columns: Arc<[String]>,
    ) -> Self {
        Self {
            writer: None,
            footer_phase: None,
            bloom_columns,
            output_object_path: None,
            output_table_path: None,
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

    /// Opens the streaming writer for the next output object.
    ///
    /// The caller has already validated the output identity, admitted the
    /// footer-phase child, and proven the lease fence, so this method is the
    /// exact point at which the first encoded byte becomes eligible to leave
    /// the process.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::DataRefusal`] when the batch cannot produce the
    /// writer-v2 footer metadata and [`ForgeError::Parquet`] when the writer
    /// cannot be constructed.
    fn open_writer(
        &mut self,
        schema: &SchemaRef,
        batch: &RecordBatch,
        sink: ForgeOutputSink,
        object_path: String,
        table_path: String,
        footer_phase: ForgeFooterPhaseGuard,
    ) -> Result<(), ForgeError> {
        let metadata = BifrostParquetMemoryEnvelope::metadata_for_batch(batch, &object_path)
            .map_err(|detail| ForgeError::DataRefusal { detail })?;
        self.writer = Some(
            AsyncArrowWriter::try_new(
                sink,
                Arc::clone(schema),
                Some(bifrost_writer_properties_with_metadata(
                    batch.num_rows(),
                    metadata,
                    &self.bloom_columns,
                )),
            )
            .map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?,
        );
        self.output_object_path = Some(object_path);
        self.output_table_path = Some(table_path);
        self.footer_phase = Some(footer_phase);
        Ok(())
    }

    /// Append one validated batch to the active streaming Parquet writer.
    ///
    /// The caller holds the fixed encoder and upload reservation before this
    /// method runs. Encoding never grows that reservation after allocation. The
    /// writer flushes its current row group at the persisted encoder threshold
    /// so encoded buffering stays within the fixed envelope while completed
    /// bytes stream straight to the object store.
    ///
    /// # Errors
    ///
    /// Returns the typed Forge failure the sink retained across the Parquet
    /// error boundary, a Parquet encoding failure, or [`ForgeError::Invariant`]
    /// when no writer is open or the batch row count cannot fit the rewrite's
    /// `u64` counters.
    async fn write_batch(
        &mut self,
        iceberg_schema: &IcebergSchemaRef,
        batch: &RecordBatch,
        row_groups: &[crate::parquet::memory::BoundedRowSlice],
        encoder_flush_bytes: usize,
    ) -> Result<(), ForgeError> {
        self.nan_value_counts
            .compute(Arc::clone(iceberg_schema), batch.clone())
            .map_err(|error| ForgeError::Invariant {
                detail: format!("Forge NaN metric derivation failed: {error}"),
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
                self.flush_active().await?;
                self.writer_row_group_logical_bytes = 0;
                self.writer_row_group_rows = 0;
            }
            if self.writer_row_group_logical_bytes == 0 {
                self.writer_row_groups = self.writer_row_groups.saturating_add(1);
            }
            self.write_slice(batch, *slice).await?;
            self.writer_row_group_logical_bytes = self
                .writer_row_group_logical_bytes
                .saturating_add(slice.logical_bytes);
            self.writer_row_group_rows = self.writer_row_group_rows.saturating_add(slice.len);
        }
        let buffered = self
            .writer
            .as_ref()
            .map_or(0, AsyncArrowWriter::in_progress_size);
        if buffered >= encoder_flush_bytes {
            self.flush_active().await?;
        }
        let rows = u64::try_from(batch.num_rows()).map_err(|error| ForgeError::Invariant {
            detail: format!("rewrite row count does not fit u64: {error}"),
        })?;
        self.writer_rows = self.writer_rows.saturating_add(rows);
        Ok(())
    }

    /// Encodes one bounded row-group slice into the active writer.
    ///
    /// # Errors
    ///
    /// Returns the sink's retained typed failure, [`ForgeError::Parquet`] when
    /// the encoder refuses the slice, or [`ForgeError::Invariant`] when no
    /// writer is open.
    async fn write_slice(
        &mut self,
        batch: &RecordBatch,
        slice: crate::parquet::memory::BoundedRowSlice,
    ) -> Result<(), ForgeError> {
        let writer = self.writer.as_mut().ok_or_else(|| ForgeError::Invariant {
            detail: "Forge rewrite failed to retain its active writer".to_owned(),
        })?;
        let result = writer.write(&batch.slice(slice.offset, slice.len)).await;
        self.settle_writer_result(result).await
    }

    /// Closes the current row group and streams it to the object store.
    ///
    /// # Errors
    ///
    /// Returns the sink's retained typed failure, [`ForgeError::Parquet`] when
    /// the encoder refuses to flush, or [`ForgeError::Invariant`] when no
    /// writer is open.
    async fn flush_active(&mut self) -> Result<(), ForgeError> {
        let writer = self.writer.as_mut().ok_or_else(|| ForgeError::Invariant {
            detail: "Forge rewrite failed to retain its active writer".to_owned(),
        })?;
        let result = writer.flush().await;
        self.settle_writer_result(result).await
    }

    /// Discards a poisoned writer and prefers the sink's typed failure.
    ///
    /// A sink failure reaches the encoder only as
    /// [`parquet::errors::ParquetError`], so the object-store or invariant
    /// cause is recovered from the sink. The incomplete upload is aborted here
    /// because a failed encoder step can never produce a publishable object.
    ///
    /// # Errors
    ///
    /// Returns the recovered typed failure, or [`ForgeError::Parquet`] carrying
    /// the encoder's own message when the sink retained none.
    async fn settle_writer_result(
        &mut self,
        result: parquet::errors::Result<()>,
    ) -> Result<(), ForgeError> {
        let Err(error) = result else {
            return Ok(());
        };
        let mut failure = None;
        if let Some(writer) = self.writer.take() {
            let mut sink = writer.into_inner();
            failure = sink.take_failure();
            sink.abort().await;
        }
        Err(failure.unwrap_or(ForgeError::Parquet {
            detail: error.to_string(),
        }))
    }

    /// Take the active writer and its identity for close and metadata derivation.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when rotation is requested without an
    /// active writer, object identity, or footer phase.
    fn take_writer(&mut self) -> Result<ActiveOutput, ForgeError> {
        let writer = self.writer.take().ok_or_else(|| ForgeError::Invariant {
            detail: "Forge output rotation lost its active writer".to_owned(),
        })?;
        let object_path = self
            .output_object_path
            .take()
            .ok_or_else(|| ForgeError::Invariant {
                detail: "Forge output rotation lost its object identity".to_owned(),
            })?;
        let table_path = self
            .output_table_path
            .take()
            .ok_or_else(|| ForgeError::Invariant {
                detail: "Forge output rotation lost its catalog identity".to_owned(),
            })?;
        let footer_phase = self
            .footer_phase
            .take()
            .ok_or_else(|| ForgeError::Invariant {
                detail: "Forge output rotation lost its footer phase".to_owned(),
            })?;
        Ok(ActiveOutput {
            writer,
            object_path,
            table_path,
            footer_phase,
        })
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
    ///
    /// Sort spill is the only scratch a rewrite owns: output bytes stream
    /// straight to the object store and never reach the attempt's disk lease.
    fn observe_spill(&mut self, spill_bytes: u64) {
        self.peak_spill_bytes = self.peak_spill_bytes.max(spill_bytes);
        if let Some(peak) = &self.scratch_peak_bytes {
            peak.fetch_max(spill_bytes, Ordering::AcqRel);
        }
        #[cfg(feature = "test-support")]
        TEST_SCRATCH_PEAK_BYTES.fetch_max(spill_bytes, Ordering::AcqRel);
    }

    /// Transfer accumulated output ownership alongside a successful terminal result.
    async fn complete(self) -> RewriteBatchResult {
        self.into_result(Ok(())).await
    }

    /// Transfer accumulated output ownership alongside the terminal failure.
    async fn fail(self, error: ForgeError) -> RewriteBatchResult {
        self.into_result(Err(error)).await
    }

    /// Abort any unfinished upload and move finalized state into the result.
    ///
    /// An unfinished writer can only belong to an output that will never be
    /// published, so its upload is aborted rather than dropped, which keeps a
    /// backend from retaining an unreferenced incomplete multipart.
    async fn into_result(mut self, result: Result<(), ForgeError>) -> RewriteBatchResult {
        if let Some(writer) = self.writer.take() {
            let mut sink = writer.into_inner();
            sink.abort().await;
        }
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

/// One open output object detached from batch state for close and publication.
struct ActiveOutput {
    /// Streaming Parquet writer whose sink is already bound to `object_path`.
    writer: AsyncArrowWriter<ForgeOutputSink>,
    /// Deterministic object key the writer has been streaming into.
    object_path: String,
    /// Catalog-visible path stamped into this output's Iceberg metadata.
    table_path: String,
    /// Footer-phase capability retained until metadata validation finishes.
    footer_phase: ForgeFooterPhaseGuard,
}

/// Owned state detached from one rotated output before its close and publication.
struct FinishOutputParts {
    /// Open output whose upload completes when its writer closes.
    active: ActiveOutput,
    /// Per-column NaN counts accumulated during encoding.
    nan_value_counts: HashMap<i32, u64>,
    /// Exact encoded row count.
    rows: u64,
}

/// Streamed evidence and Iceberg metadata for one published Parquet output.
struct FinalizedOutput {
    /// Exact length the object store acknowledged for the completed upload.
    bytes: u64,
    /// CRC32C accumulated over the encoded bytes as they streamed.
    digest: u32,
    /// Iceberg data-file metadata derived from the encoder's own footer.
    file: DataFile,
}

/// Confirms the catalog-visible output path maps to its exact object key.
///
/// This runs before the writer is opened, so no byte can leave the process for
/// a path that would not project back onto the deterministic object key the
/// publication settlement and orphan-GC owners expect.
///
/// # Errors
///
/// Returns a path or invariant error when the catalog path escapes the exact
/// binding or does not map to `object_path`.
fn validate_output_path_binding(
    request: &RewriteRequest<'_>,
    staging: &opendal::Operator,
    table_path: &str,
    object_path: &str,
) -> Result<(), ForgeError> {
    let mapped =
        catalog_path_to_object_key(request.table_location, request.binding, staging, table_path)?;
    if mapped != object_path {
        return Err(ForgeError::Invariant {
            detail: "Forge DataFile path does not map to its exact object key".to_owned(),
        });
    }
    Ok(())
}

/// Refuses an encoded footer above the pre-reserved 8 MiB allowance.
///
/// Forge no longer decodes the footer it just wrote — the encoder returns the
/// metadata directly — so the only remaining hazard is publishing an object
/// whose footer a Bifrost reader would refuse. The eight-byte trailer observed
/// at the end of the stream carries the declared encoded length and the format
/// magic, which is exactly what a later reader checks first.
///
/// # Errors
/// Returns a Parquet data refusal when the trailer magic is invalid or the
/// declared footer length exceeds the fixed encoded-footer child.
fn validate_streamed_trailer(trailer: [u8; PARQUET_TRAILER_BYTES]) -> Result<(), ForgeError> {
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
        .map_err(|detail| ForgeError::DataRefusal { detail })
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

/// Inputs needed to close one streamed output and derive its Iceberg metadata.
struct OutputMetadataRequest {
    /// Open output whose upload completes when its writer closes.
    active: ActiveOutput,
    /// Per-field NaN counts required by Iceberg file metadata.
    nan_value_counts: HashMap<i32, u64>,
    /// Rows encoded into the completed Parquet output.
    rows: u64,
    /// Iceberg schema carrying field identifiers for file metadata.
    iceberg_schema: IcebergSchemaRef,
    /// Arrow schema whose fingerprint and leaf order were stamped before writing.
    expected_schema: SchemaRef,
    /// Exact time partition assigned to the entire output.
    partition: crate::catalog::layout::TimePartition,
    /// Iceberg partition-spec identifier for the destination table.
    partition_spec_id: i32,
    /// Iceberg sort-order identifier for the destination table.
    sort_order_id: i32,
}

/// Close one streamed output and derive the matching Iceberg data-file metadata.
///
/// Closing the writer flushes the final row group, encodes the footer, and
/// completes the upload, after which the encoder hands back the exact
/// [`parquet::file::metadata::ParquetMetaData`] it wrote. Every admission the
/// sealed-file path used to prove by reading the object back is therefore
/// proven here from in-process state plus the length the backend acknowledged;
/// nothing is re-downloaded.
///
/// # Errors
///
/// Returns the sink's typed object-store failure when the upload cannot
/// complete, [`ForgeError::Parquet`] when the encoder cannot close,
/// [`ForgeError::DataRefusal`] for a refused trailer, footer envelope, or
/// oversized encoded row group, and [`ForgeError::Invariant`] when the derived
/// Iceberg file disagrees with the bytes that were streamed.
async fn finalize_output_metadata(
    request: OutputMetadataRequest,
) -> Result<FinalizedOutput, ForgeError> {
    let OutputMetadataRequest {
        active,
        nan_value_counts,
        rows,
        iceberg_schema,
        expected_schema,
        partition,
        partition_spec_id,
        sort_order_id,
    } = request;
    let ActiveOutput {
        mut writer,
        object_path,
        table_path,
        footer_phase,
    } = active;
    let finished = writer.finish().await;
    let mut sink = writer.into_inner();
    let parquet_metadata = match finished {
        Ok(metadata) => Arc::new(metadata),
        Err(error) => {
            let failure = sink.take_failure();
            sink.abort().await;
            return Err(failure.unwrap_or(ForgeError::Parquet {
                detail: error.to_string(),
            }));
        }
    };
    // The upload is durable from here: the footer child now also owns the
    // metadata workspace the derived Iceberg file is built in.
    let _footer_phase = footer_phase.into_metadata();
    let output_size = sink.streamed_bytes;
    validate_streamed_trailer(sink.trailer()?)?;
    crate::parquet::memory::BifrostParquetMemoryEnvelope::from_footer(
        parquet_metadata.file_metadata(),
        expected_schema.as_ref(),
        &object_path,
    )
    .map_err(|detail| ForgeError::DataRefusal { detail })?;
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
        // The object is already durable at its generation-scoped key. The
        // caller bisects and re-encodes the same ordinal, which overwrites it;
        // an abandoned attempt leaves it for protected orphan GC.
        let rows = usize::try_from(group.num_rows()).unwrap_or(usize::MAX);
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
    let partition_value = Struct::from_iter([Some(partition.iceberg_partition_literal())]);
    file.partition(partition_value)
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
    Ok(FinalizedOutput {
        bytes: output_size,
        digest: sink.digest,
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
            staging,
            object_store,
            max_concurrent_reads,
        })
    }

    /// Creates an attempt-local execution view over the retained dependencies.
    pub(crate) fn for_runtime(&self, runtime: Arc<ForgeRewriteRuntime>) -> Self {
        Self {
            runtime: Some(runtime),
            staging: Arc::clone(&self.staging),
            object_store: Arc::clone(&self.object_store),
            max_concurrent_reads: self.max_concurrent_reads,
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
                Arc::clone(&request.bloom_columns),
            )
            .fail(ForgeError::ExecutionEnvelopeExceeded {
                resource: "memory",
                detail: format!("Forge fixed output allowance was refused: {error}"),
            })
            .await;
        }
        let mut state = RewriteBatchState::with_reservation(
            Some(reservation),
            Some(self.runtime().scratch_peak_handle()),
            Arc::clone(&request.bloom_columns),
        );
        while let Some(batch) = tokio::select! {
            () = stop.cancelled() => return state.fail(ForgeError::Shutdown).await,
            batch = stream.next() => batch,
        } {
            let batch = match batch {
                Ok(batch) => batch,
                Err(error) => return state.fail(Self::map_datafusion_error(error)).await,
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
                return state.fail(error).await;
            }
        }
        let context = RewriteOutputContext {
            request,
            stop,
            lease,
            operator_pool,
        };
        if let Err(error) = self.finish_pending_output(&mut state, context).await {
            return state.fail(error).await;
        }
        if state.files.is_empty() {
            return state
                .fail(ForgeError::Invariant {
                    detail: "Forge rewrite produced no non-empty output".to_owned(),
                })
                .await;
        }
        state.complete().await
    }

    /// Encodes one physical output file for the in-flight rewrite.
    ///
    /// When no writer is open this first admits the footer-phase child,
    /// validates the output identity, crosses the object-store boundary seam,
    /// and proves the lease fence — every one of them strictly before the
    /// encoder can emit a byte, because with a streaming sink the first
    /// encoder buffer is already a durable-side effect. Encoding then runs
    /// inline on the async task: the Parquet encoder drives the object-store
    /// writer directly, so there is no blocking file step left to isolate.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Shutdown`] when cancellation wins any admission
    /// race, lease, path, object-store, or Parquet failures from opening the
    /// output, and propagates the encoder's own error.
    async fn encode_physical_output(
        &self,
        state: &mut RewriteBatchState,
        output_batch: &RecordBatch,
        output_groups: &[crate::parquet::memory::BoundedRowSlice],
        context: RewriteOutputContext<'_, '_>,
    ) -> Result<(), ForgeError> {
        let RewriteOutputContext {
            request,
            stop,
            lease,
            operator_pool,
        } = context;
        if state.writer.is_none() {
            self.open_output(
                state,
                output_batch,
                RewriteOutputContext {
                    request,
                    stop,
                    lease,
                    operator_pool,
                },
            )
            .await?;
        }
        state
            .write_batch(
                &request.iceberg_schema,
                output_batch,
                output_groups,
                self.runtime().encoder_flush_bytes,
            )
            .await
    }

    /// Admits, fences, and opens the streaming writer for the next output.
    ///
    /// Ordering is the contract: the footer child is admitted, the output
    /// identity and its catalog projection are validated, the object-store
    /// boundary seam runs, and the lease fence is proven — all before the sink
    /// opens and therefore before the first encoded byte can leave. Every
    /// cancellation check the sealed-file path performed before its PUT is
    /// preserved at the same relative position.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Shutdown`] on cancellation, the object-store error
    /// from the boundary seam or the writer open, the lease fence error,
    /// [`ForgeError::Invariant`] when the output identity escapes its binding,
    /// and [`ForgeError::Parquet`] when the encoder cannot be constructed.
    async fn open_output(
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
        let footer_phase = tokio::select! {
            () = stop.cancelled() => return Err(ForgeError::Shutdown),
            phase = self.runtime().footer_phase.enter_execution() => phase?,
        };
        let (object_path, table_path) =
            self.validated_output_identity(request, state.files.len())?;
        validate_output_path_binding(request, &self.staging, &table_path, &object_path)?;
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
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
        let sink = ForgeOutputSink::open(
            Arc::clone(&self.object_store),
            Arc::clone(&self.staging),
            object_path.clone(),
            self.runtime().upload_chunk_bytes,
        )
        .await?;
        state.open_writer(
            &request.schema,
            batch,
            sink,
            object_path,
            table_path,
            footer_phase,
        )
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
            self.encode_physical_output(
                state,
                &output_batch,
                &output_groups,
                RewriteOutputContext {
                    request,
                    stop,
                    lease: &mut *lease,
                    operator_pool,
                },
            )
            .await?;
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
        let active = state.take_writer()?;
        let nan_value_counts = state.take_nan_value_counts();
        let (file, path) = self
            .finish_output(
                FinishOutputParts {
                    active,
                    nan_value_counts,
                    rows: state.writer_rows,
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
        self.await_spill_cleanup().await?;
        result?;
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
            self.runtime().sort_runtime(),
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

    /// Close one streamed output, complete its upload, and derive its metadata.
    ///
    /// The output identity, the object-store boundary seam, and the lease fence
    /// were all satisfied before this output's first byte left, so the work
    /// remaining here is the encoder's own close: the final row group and the
    /// footer stream out, the backend acknowledges the object, and the Iceberg
    /// file is derived from the metadata the encoder returns. Forge performs no
    /// read-back of a successful write.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Shutdown`] when cancellation is observed before
    /// the close, the sink's object-store failure when the upload cannot
    /// complete, [`ForgeError::DataRefusal`] for a refused footer or oversized
    /// encoded row group, and [`ForgeError::Invariant`] when the derived
    /// metadata disagrees with the streamed bytes.
    ///
    /// # Cancellation
    ///
    /// Cancellation before the close returns [`ForgeError::Shutdown`] and the
    /// incomplete upload is aborted; earlier finalized paths remain recorded
    /// for error settlement and protected orphan GC. A cancellation that
    /// arrives after the backend acknowledges the object cannot report partial
    /// success, because the object is only recorded once metadata validation
    /// succeeds.
    async fn finish_output(
        &self,
        parts: FinishOutputParts,
        context: RewriteOutputContext<'_, '_>,
    ) -> Result<(DataFile, String), ForgeError> {
        let FinishOutputParts {
            active,
            nan_value_counts,
            rows,
        } = parts;
        let RewriteOutputContext { request, stop, .. } = context;
        if stop.is_cancelled() {
            let mut sink = active.writer.into_inner();
            sink.abort().await;
            return Err(ForgeError::Shutdown);
        }
        let object_path = active.object_path.clone();
        let finalized = finalize_output_metadata(OutputMetadataRequest {
            active,
            nan_value_counts,
            rows,
            iceberg_schema: Arc::clone(&request.iceberg_schema),
            expected_schema: Arc::clone(&request.schema),
            partition: request.partition,
            partition_spec_id: request.partition_spec_id,
            sort_order_id: request.sort_order_id,
        })
        .await?;
        let FinalizedOutput {
            bytes,
            digest,
            file,
        } = finalized;
        tracing::debug!(
            path = %object_path,
            crc32c = digest,
            bytes,
            "Forge streamed one output directly to the object store"
        );
        self.object_store.after_output_put(&object_path).await;
        Ok((file, object_path))
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
        "{}/data/forge/{generation}-{ordinal:05}.parquet",
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
        "{}/data/forge/{}-{ordinal:05}.parquet",
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

/// `DataFusion` runtime with an owned, bounded spill directory.
pub struct ForgeRewriteRuntime {
    /// Runtime environment sharing the host-provisioned memory pool.
    runtime: Arc<RuntimeEnv>,
    /// Current Forge-owned child removed automatically on clean shutdown.
    spill_dir: tempfile::TempDir,
    /// Runtime-wide spill ceiling, equivalent to one operation under `tick`.
    ///
    /// Sort spill is the only consumer: rewrite output streams straight to the
    /// object store, so it owns none of the attempt's disk lease.
    spill_limit_bytes: u64,
    /// Largest bounded sort-spill ownership observed by this attempt.
    scratch_peak_bytes: Arc<AtomicU64>,
    /// Fixed encoder/upload reservation derived from the acquired envelope.
    output_allowance_bytes: usize,
    /// Exact bytes reserved before polling each decoded stream.
    decoded_batch_bytes: usize,
    /// Explicit `DataFusion` sort merge reservation.
    sort_merge_reservation_bytes: usize,
    /// Runtime environment whose memory pool caps the sort at its persisted term.
    ///
    /// Shares this attempt's disk manager, so sort spill still accrues against
    /// the one scratch lease; only the memory ceiling differs.
    sort_runtime: Arc<RuntimeEnv>,
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
    /// Sole scratch lease, owned entirely by `DataFusion` sort spill.
    spill_limit: u64,
    /// Combined encoder and upload resident allowance.
    output_allowance_bytes: usize,
    /// Exact decoder reservation per active source.
    decoded_batch_bytes: usize,
    /// Explicit sort merge reservation.
    sort_merge_reservation_bytes: usize,
    /// Exact persisted ceiling the `DataFusion` sort may hold resident.
    sort_working_bytes: usize,
    /// Parquet row-group flush threshold.
    encoder_flush_bytes: usize,
    /// Upload allocation and backend writer chunk.
    upload_chunk_bytes: usize,
    /// Encoded footer child retained from writer creation through decode.
    footer_encoded_bytes: usize,
    /// Decoder workspace that joins the encoded footer only in metadata phase.
    footer_decode_workspace_bytes: usize,
}

/// Attempt pool that additionally caps one `DataFusion` sort at its persisted term.
///
/// The aggregate attempt pool deliberately keeps no child ledgers, so nothing
/// stops a `SortExec` from buffering its whole input up to the attempt's total
/// resident lease and starving the decoder and encoder terms that share it. The
/// execution envelope already publishes `sort_working_bytes` as the exact
/// resident ceiling for sort, and `sort_spill_bytes` exists so the sort can
/// spill past it, so this pool enforces that split: growth beyond the local
/// ceiling is refused, which is the signal `SortExec` uses to spill, while every
/// accepted byte is still charged to the aggregate attempt pool so total
/// admission and peak accounting stay unchanged.
#[derive(Debug)]
struct ForgeSortMemoryPool {
    /// Aggregate attempt pool that owns admission and peak observation.
    inner: Arc<dyn MemoryPool>,
    /// Exact persisted resident ceiling this sort may hold.
    limit: usize,
    /// Live bytes this sort currently holds against `limit`.
    used: AtomicUsize,
}

impl ForgeSortMemoryPool {
    /// Wraps the aggregate attempt pool with the sort's exact resident ceiling.
    fn new(inner: Arc<dyn MemoryPool>, limit: usize) -> Self {
        Self {
            inner,
            limit,
            used: AtomicUsize::new(0),
        }
    }
}

impl MemoryPool for ForgeSortMemoryPool {
    /// Delegates registration to the aggregate attempt pool.
    fn register(&self, consumer: &MemoryConsumer) {
        self.inner.register(consumer);
    }

    /// Delegates removal to the aggregate attempt pool.
    fn unregister(&self, consumer: &MemoryConsumer) {
        self.inner.unregister(consumer);
    }

    /// Charges unconditional growth to both the local ceiling and the aggregate.
    ///
    /// `grow` has no failure path, so the local counter records the debt even
    /// when it passes `limit`; the next `try_grow` then refuses and the sort
    /// spills.
    fn grow(&self, reservation: &MemoryReservation, additional: usize) {
        self.used.fetch_add(additional, Ordering::AcqRel);
        self.inner.grow(reservation, additional);
    }

    /// Releases bytes from both the local ceiling and the aggregate.
    fn shrink(&self, reservation: &MemoryReservation, shrink: usize) {
        let mut current = self.used.load(Ordering::Acquire);
        loop {
            let next = current.saturating_sub(shrink);
            match self.used.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
        self.inner.shrink(reservation, shrink);
    }

    /// Refuses growth past the persisted sort term before consulting the aggregate.
    ///
    /// # Errors
    ///
    /// Returns [`datafusion::error::DataFusionError::ResourcesExhausted`] when
    /// the request would exceed `limit`, and the aggregate pool's own error when
    /// the attempt lease is exhausted. Both are the spill signal `SortExec`
    /// expects, not a terminal failure.
    fn try_grow(
        &self,
        reservation: &MemoryReservation,
        additional: usize,
    ) -> datafusion::error::Result<()> {
        let mut current = self.used.load(Ordering::Acquire);
        loop {
            let next = current.checked_add(additional).ok_or_else(|| {
                datafusion::error::DataFusionError::ResourcesExhausted(
                    "Forge sort reservation overflows".to_owned(),
                )
            })?;
            if next > self.limit {
                return Err(datafusion::error::DataFusionError::ResourcesExhausted(
                    format!(
                        "Forge sort reservation {next} exceeds its persisted term {}",
                        self.limit
                    ),
                ));
            }
            match self.used.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
        self.inner
            .try_grow(reservation, additional)
            .inspect_err(|_| {
                self.used.fetch_sub(additional, Ordering::AcqRel);
            })
    }

    /// Returns aggregate live ownership, which admission is still measured against.
    fn reserved(&self) -> usize {
        self.inner.reserved()
    }
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
            output_allowance_bytes,
            decoded_batch_bytes,
            sort_working_bytes,
            sort_merge_reservation_bytes,
            encoder_flush_bytes,
            upload_chunk_bytes,
            footer_encoded_bytes,
            footer_decode_workspace_bytes,
        } = envelope;
        if spill_limit_bytes == 0
            || decoded_batch_bytes == 0
            || sort_working_bytes <= sort_merge_reservation_bytes
            || sort_merge_reservation_bytes == 0
            || encoder_flush_bytes == 0
            || upload_chunk_bytes == 0
            || footer_encoded_bytes == 0
            || footer_decode_workspace_bytes == 0
        {
            return Err(ForgeError::InvalidConfig {
                detail:
                    "Forge scratch envelope must contain positive resident and sort-spill terms"
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
        let runtime = Arc::new(
            RuntimeEnvBuilder::new()
                .with_memory_pool(memory_pool)
                .with_temp_file_path(spill_dir.path())
                .with_max_temp_directory_size(spill_limit_bytes)
                .build()
                .map_err(ForgeError::DataFusion)?,
        );
        let sort_runtime = Arc::new(RuntimeEnv {
            memory_pool: Arc::new(ForgeSortMemoryPool::new(
                Arc::clone(&runtime.memory_pool),
                sort_working_bytes,
            )),
            disk_manager: Arc::clone(&runtime.disk_manager),
            cache_manager: Arc::clone(&runtime.cache_manager),
            object_store_registry: Arc::clone(&runtime.object_store_registry),
        });
        Ok(Self {
            runtime,
            sort_runtime,
            spill_dir,
            spill_limit_bytes,
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

    /// Shares the attempt's conservative sort-spill high-water counter.
    ///
    /// Batch state observes `DataFusion` spill progress between batches and
    /// records it here, so final settlement reports the largest scratch the
    /// attempt actually owned rather than its lease.
    fn scratch_peak_handle(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.scratch_peak_bytes)
    }

    /// Return the configured operation spill ceiling.
    #[must_use]
    pub fn spill_limit_bytes(&self) -> u64 {
        self.spill_limit_bytes
    }

    /// Returns the conservative attempt scratch peak used at final settlement.
    ///
    /// Batch state updates this value after each bounded sort-spill
    /// observation, and sort spill is the attempt's only scratch consumer, so
    /// the reported peak is capped by the lease.
    #[must_use]
    fn scratch_peak_bytes(&self) -> u64 {
        self.scratch_peak_bytes
            .load(Ordering::Acquire)
            .min(self.spill_limit_bytes)
    }

    /// Clone the sort-bounded runtime handle for one `DataFusion` sort context.
    ///
    /// Shares this attempt's disk manager with [`Self::runtime`] so sort spill
    /// still accrues against the single scratch lease; only the memory pool
    /// differs, capping the sort at its persisted `sort_working_bytes`.
    #[must_use]
    fn sort_runtime(&self) -> Arc<RuntimeEnv> {
        debug_assert!(self.spill_dir.path().exists());
        Arc::clone(&self.sort_runtime)
    }

    /// Reports whether this runtime retained the exact leased memory pool.
    #[must_use]
    pub(crate) fn uses_memory_pool(&self, pool: &Arc<dyn MemoryPool>) -> bool {
        Arc::ptr_eq(&self.runtime.memory_pool, pool)
    }

    /// Return the owned `DataFusion` spill path for diagnostics and tests.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn spill_path(&self) -> &Path {
        self.spill_dir.path()
    }
}

#[cfg(test)]
mod tests {
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::execution::memory_pool::GreedyMemoryPool;
    use wyrd_bench::BenchmarkRecorder;

    use super::*;

    /// The one exact-partition contract Forge's rewrite pipeline depends on.
    ///
    /// A rewrite must reproduce the destination table's registered physical
    /// identity rather than a fixed recipe, so this proves the three seams
    /// that carry it: the Bloom union is recovered from the table property and
    /// fails closed when it is absent or malformed; `Hour` and `Day` are
    /// distinct identities that survive the Iceberg transform round trip and
    /// never alias one another; and the object-path projection separates them.
    #[test]
    fn time_partition_rewrite_contract() {
        use crate::catalog::layout::{PhysicalLayout, TimeGranularity, TimePartition};

        // The Bloom union is table state, not a recipe: a rewrite reads it back
        // from the property the catalog persisted.
        let stored = vec![
            "data_tenant_id".to_owned(),
            "run_id".to_owned(),
            "customer".to_owned(),
        ];
        let property = serde_json::to_string(&stored).expect("fixture serializes");
        let recovered = PhysicalLayout::bloom_columns_from_property(Some(&property))
            .expect("a well-formed property is recovered verbatim");
        assert_eq!(recovered.as_ref(), stored.as_slice());

        // Absent or malformed table state fails closed rather than silently
        // rewriting with no Bloom filters at all.
        assert!(PhysicalLayout::bloom_columns_from_property(None).is_err());
        let malformed = "not-a-json-array".to_owned();
        assert!(PhysicalLayout::bloom_columns_from_property(Some(&malformed)).is_err());

        // Hour and Day are distinct identities. The same instant is a legal
        // boundary for both, and the two must never compare or hash equal.
        let midnight = chrono::DateTime::from_timestamp(1_787_443_200, 0)
            .expect("fixture instant is representable");
        let hour = TimePartition::new(TimeGranularity::Hour, midnight)
            .expect("midnight is an exact hour boundary");
        let day = TimePartition::new(TimeGranularity::Day, midnight)
            .expect("midnight is an exact day boundary");
        assert_ne!(hour, day);
        assert_eq!(hour.start_utc(), day.start_utc());
        assert_ne!(hour.granularity_tag(), day.granularity_tag());

        // Each survives the Iceberg transform round trip Forge uses to stamp
        // and re-read a rewritten file's partition, and the transform values
        // are not interchangeable between granularities.
        for partition in [hour, day] {
            let value = partition.iceberg_transform_value();
            let round_tripped =
                TimePartition::from_iceberg_transform_value(partition.granularity(), value)
                    .expect("a stamped transform value is recoverable");
            assert_eq!(round_tripped, partition);
        }
        assert_ne!(
            hour.iceberg_transform_value(),
            day.iceberg_transform_value()
        );
        assert_ne!(
            hour.iceberg_partition_literal(),
            day.iceberg_partition_literal(),
            "hour stamps an int literal and day a date literal"
        );

        // Non-boundary instants are unrepresentable, so a rewrite cannot invent
        // a partition that no writer could have produced.
        let mid_hour = chrono::DateTime::from_timestamp(1_787_443_200 + 1_800, 0)
            .expect("fixture instant is representable");
        assert!(TimePartition::new(TimeGranularity::Hour, mid_hour).is_err());
        assert!(TimePartition::new(TimeGranularity::Day, mid_hour).is_err());

        // The object-path projection separates the two, so an hourly and a
        // daily rewrite of the same instant cannot collide on one key.
        assert_ne!(hour.as_path_components(), day.as_path_components());
        assert!(
            hour.as_path_components()
                .contains("partition_granularity=hour")
        );
        assert!(
            day.as_path_components()
                .contains("partition_granularity=day")
        );

        // Durable file-list columns rebuild the same identity a rewrite grouped
        // on, and an unknown token is refused rather than defaulted.
        for partition in [hour, day] {
            let rebuilt = TimePartition::from_durable_columns(
                partition.granularity_str(),
                partition.start_utc(),
            )
            .expect("durable columns rebuild the exact partition");
            assert_eq!(rebuilt, partition);
        }
        assert!(TimePartition::from_durable_columns("month", midnight).is_err());
    }

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
                output_allowance_bytes: 300,
                decoded_batch_bytes: 101,
                sort_merge_reservation_bytes: 102,
                sort_working_bytes: 304,
                encoder_flush_bytes: 103,
                upload_chunk_bytes: 104,
                footer_encoded_bytes: 105,
                footer_decode_workspace_bytes: 106,
            },
        )
        .expect("runtime");

        assert_eq!(runtime.sort_merge_reservation_bytes, 102);
        assert_eq!(
            runtime.sort_runtime().memory_pool.reserved(),
            0,
            "a fresh sort pool holds nothing before execution"
        );
        assert_eq!(runtime.encoder_flush_bytes, 103);
        assert_eq!(runtime.upload_chunk_bytes, 104);
        assert_eq!(runtime.decoded_batch_bytes, 101);
        assert_eq!(runtime.spill_limit_bytes, 1024);
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
            let mut spilling = RewriteBatchState::with_reservation(
                None,
                Some(attempt.runtime().scratch_peak_handle()),
                Arc::from(["data_tenant_id".to_owned()]),
            );
            spilling.observe_spill(1);
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
            partition: crate::catalog::layout::TimePartition::new(
                crate::catalog::TimeGranularity::Hour,
                chrono::DateTime::from_timestamp(1_767_312_000, 0)
                    .expect("fixed parity instant is representable"),
            )
            .expect("fixed parity instant is an exact hour boundary"),
            sort_order_id: Some(1),
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
                output_allowance_bytes: 2,
                decoded_batch_bytes: 1,
                sort_merge_reservation_bytes: 1,
                sort_working_bytes: 3,
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
                output_allowance_bytes: 2,
                decoded_batch_bytes: 1,
                sort_merge_reservation_bytes: 1,
                sort_working_bytes: 3,
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

    /// Object store that streams every rewrite output into one in-memory operator.
    ///
    /// The streaming seam only needs `output_writer`, `write_output_chunk`, and
    /// `abort_output_writer`, all of which the trait already implements against
    /// the supplied operator, so the remaining read surface stays unreachable.
    #[derive(Debug)]
    struct MemoryOutputStore;

    #[async_trait::async_trait]
    impl ForgeObjectStore for MemoryOutputStore {
        /// Reject reads because a streamed output is never read back.
        async fn read(&self, _path: &str) -> opendal::Result<opendal::Buffer> {
            unreachable!("streaming output unit does not read objects")
        }

        /// Reject ranged reads because a streamed output is never read back.
        async fn read_range(
            &self,
            _path: &str,
            _range: Range<u64>,
        ) -> opendal::Result<opendal::Buffer> {
            unreachable!("streaming output unit does not read object ranges")
        }

        /// Reject listings because this unit addresses exactly one known key.
        async fn list(&self, _prefix: &str) -> opendal::Result<Vec<opendal::Entry>> {
            unreachable!("streaming output unit does not list objects")
        }

        /// Reject metadata reads because the writer close already reports length.
        async fn stat(&self, _path: &str) -> opendal::Result<opendal::Metadata> {
            unreachable!("streaming output unit does not stat objects")
        }

        /// Reject deletes because this unit publishes exactly one object.
        async fn delete(&self, _path: &str) -> opendal::Result<()> {
            unreachable!("streaming output unit does not delete objects")
        }
    }

    /// Builds the one Arrow/Iceberg schema pair and 4096-row batch the rotation
    /// unit encodes twice.
    ///
    /// The Arrow field carries the `PARQUET:field_id` metadata the writer-v2
    /// recipe requires, so the returned batch is admissible without further
    /// preparation.
    fn coalesce_fixture() -> (SchemaRef, IcebergSchemaRef, RecordBatch) {
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
        (schema, iceberg_schema, batch)
    }

    /// Sub-target batches share one streaming writer until accumulated bytes
    /// reach the rotation target, and the single published object carries every
    /// row from both batches.
    #[tokio::test]
    async fn sub_target_batches_coalesce_before_rotation() {
        let (schema, iceberg_schema, batch) = coalesce_fixture();
        let staging = Arc::new(
            opendal::Operator::new(opendal::services::Memory::default())
                .expect("in-memory staging operator")
                .finish(),
        );
        let object_path =
            "s3://bucket/table/data/forge/test-00000.parquet".to_owned();
        let sink = ForgeOutputSink::open(
            Arc::new(MemoryOutputStore),
            Arc::clone(&staging),
            object_path.clone(),
            8 * 1024 * 1024,
        )
        .await
        .expect("streaming output sink");
        let mut state = RewriteBatchState::with_reservation(
            None,
            None,
            Arc::from(["data_tenant_id".to_owned()]),
        );
        let phase = Arc::new(ForgeFooterPhase::new(8, 32).expect("footer phase"));
        let guard = ForgeFooterPhaseGuard {
            owner: Arc::clone(&phase),
            _permit: Arc::clone(&phase.gate)
                .try_acquire_owned()
                .expect("execution phase"),
            metadata: false,
        };
        state
            .open_writer(
                &schema,
                &batch,
                sink,
                object_path.clone(),
                object_path.clone(),
                guard,
            )
            .expect("streaming writer opens before the first byte");
        let slices = [BoundedRowSlice {
            offset: 0,
            len: batch.num_rows(),
            logical_bytes: 32_768,
        }];
        state
            .write_batch(&iceberg_schema, &batch, &slices, ROW_GROUP_FLUSH_BYTES)
            .await
            .expect("streamed encode");
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
            .write_batch(&iceberg_schema, &batch, &slices, ROW_GROUP_FLUSH_BYTES)
            .await
            .expect("second streamed encode");
        assert!(
            state.should_rotate(target),
            "the next boundary rotates after accumulated bytes cross target"
        );

        let mut active = state.take_writer().expect("active writer");
        let metadata = active.writer.finish().await.expect("streamed footer");
        let sink = active.writer.into_inner();
        assert_eq!(
            sink.streamed_bytes,
            staging
                .stat(&object_path)
                .await
                .expect("published object metadata")
                .content_length(),
            "the acknowledged object length must equal every streamed byte"
        );
        assert_eq!(metadata.file_metadata().num_rows(), 8192);
        validate_writer_v2_structure(&metadata).expect("writer-v2 structure is bounded");

        let published = staging
            .read(&object_path)
            .await
            .expect("published object")
            .to_bytes();
        let durable = parquet::file::metadata::ParquetMetaDataReader::new()
            .parse_and_finish(&published)
            .expect("valid Parquet footer");
        assert_eq!(durable.file_metadata().num_rows(), 8192);
    }

    /// A file a real Forge rewrite wrote proves the same footer recipe Scribe
    /// seals under: a low-cardinality string column encodes through a
    /// dictionary, `wyrd_event_time` encodes as `DELTA_BINARY_PACKED` with no
    /// dictionary page, and every decoded row equals the row written. The
    /// assertion reads the published object's own footer, so a second property
    /// builder anywhere in the rewrite path would fail it.
    #[tokio::test]
    async fn rewritten_file_footer_encoding_contract() {
        const ROWS: i64 = 4_096;

        let field_id = |id: &str| HashMap::from([("PARQUET:field_id".to_owned(), id.to_owned())]);
        let schema = Arc::new(Schema::new(vec![
            Field::new("service_name", DataType::Utf8, false).with_metadata(field_id("1")),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None),
                false,
            )
            .with_metadata(field_id("2")),
        ]));
        let iceberg_schema = Arc::new(
            iceberg::spec::Schema::builder()
                .with_fields(vec![
                    Arc::new(iceberg::spec::NestedField::required(
                        1,
                        "service_name",
                        iceberg::spec::Type::Primitive(iceberg::spec::PrimitiveType::String),
                    )),
                    Arc::new(iceberg::spec::NestedField::required(
                        2,
                        "wyrd_event_time",
                        iceberg::spec::Type::Primitive(iceberg::spec::PrimitiveType::Timestamp),
                    )),
                ])
                .build()
                .expect("Iceberg schema"),
        );
        let times: Vec<i64> = (0..ROWS).map(|row| 1_767_312_000_000_000 + row).collect();
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(arrow::array::StringArray::from_iter_values(
                    (0..ROWS).map(|row| format!("service-{}", row % 4)),
                )),
                Arc::new(
                    arrow::array::TimestampMicrosecondArray::from_iter_values(times.iter().copied()),
                ),
            ],
        )
        .expect("rewrite fixture batch");

        let staging = Arc::new(
            opendal::Operator::new(opendal::services::Memory::default())
                .expect("in-memory staging operator")
                .finish(),
        );
        let object_path = "s3://bucket/table/data/forge/footer-00000.parquet".to_owned();
        let sink = ForgeOutputSink::open(
            Arc::new(MemoryOutputStore),
            Arc::clone(&staging),
            object_path.clone(),
            8 * 1024 * 1024,
        )
        .await
        .expect("streaming output sink");
        let mut state = RewriteBatchState::with_reservation(
            None,
            None,
            Arc::from(["service_name".to_owned()]),
        );
        let phase = Arc::new(ForgeFooterPhase::new(8, 32).expect("footer phase"));
        let guard = ForgeFooterPhaseGuard {
            owner: Arc::clone(&phase),
            _permit: Arc::clone(&phase.gate)
                .try_acquire_owned()
                .expect("execution phase"),
            metadata: false,
        };
        state
            .open_writer(
                &schema,
                &batch,
                sink,
                object_path.clone(),
                object_path.clone(),
                guard,
            )
            .expect("streaming writer opens before the first byte");
        let slices = [BoundedRowSlice {
            offset: 0,
            len: batch.num_rows(),
            logical_bytes: 32_768,
        }];
        state
            .write_batch(&iceberg_schema, &batch, &slices, ROW_GROUP_FLUSH_BYTES)
            .await
            .expect("streamed encode");
        let mut active = state.take_writer().expect("active writer");
        active.writer.finish().await.expect("streamed footer");

        let published = staging
            .read(&object_path)
            .await
            .expect("published object")
            .to_bytes();
        let durable = parquet::file::metadata::ParquetMetaDataReader::new()
            .parse_and_finish(&published)
            .expect("valid Parquet footer");

        let mut saw_dictionary = false;
        let mut saw_event_time = false;
        for group in durable.row_groups() {
            for column in group.columns() {
                let name = column.column_path().string();
                let encodings = column.encodings().collect::<Vec<_>>();
                if name == "service_name" {
                    saw_dictionary = true;
                    assert!(
                        encodings.contains(&parquet::basic::Encoding::RLE_DICTIONARY)
                            || encodings.contains(&parquet::basic::Encoding::PLAIN_DICTIONARY),
                        "low-cardinality {name} must be dictionary-encoded, saw {encodings:?}"
                    );
                }
                if name == "wyrd_event_time" {
                    saw_event_time = true;
                    assert!(
                        encodings.contains(&parquet::basic::Encoding::DELTA_BINARY_PACKED),
                        "wyrd_event_time must be DELTA_BINARY_PACKED, saw {encodings:?}"
                    );
                    assert!(
                        !encodings.contains(&parquet::basic::Encoding::RLE_DICTIONARY)
                            && !encodings.contains(&parquet::basic::Encoding::PLAIN_DICTIONARY),
                        "wyrd_event_time must not carry a dictionary page, saw {encodings:?}"
                    );
                }
            }
        }
        assert!(saw_dictionary && saw_event_time);

        let decoded = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
            published.clone(),
        )
        .expect("decode builder")
        .build()
        .expect("decode reader")
        .collect::<Result<Vec<_>, _>>()
        .expect("decode rows");
        let mut decoded_times = Vec::with_capacity(times.len());
        let mut decoded_services = Vec::with_capacity(times.len());
        for decoded_batch in &decoded {
            decoded_times.extend(
                decoded_batch
                    .column_by_name("wyrd_event_time")
                    .expect("decoded event-time column")
                    .as_any()
                    .downcast_ref::<arrow::array::TimestampMicrosecondArray>()
                    .expect("decoded event-time is microsecond timestamps")
                    .values()
                    .iter()
                    .copied(),
            );
            let services = decoded_batch
                .column_by_name("service_name")
                .expect("decoded service column")
                .as_any()
                .downcast_ref::<arrow::array::StringArray>()
                .expect("decoded service column is Utf8");
            for row in 0..decoded_batch.num_rows() {
                decoded_services.push(services.value(row).to_owned());
            }
        }
        assert_eq!(decoded_times, times);
        assert_eq!(
            decoded_services,
            (0..ROWS)
                .map(|row| format!("service-{}", row % 4))
                .collect::<Vec<_>>()
        );
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

    /// The rewrite output path carries generation and ordinal alone, with no
    /// recipe segment that a value change could strand the orphan collector on.
    #[test]
    fn deterministic_path_carries_no_recipe_segment() {
        let operation_id = Uuid::from_u128(42);
        let generation = ForgeAttemptGeneration::for_test(operation_id);
        let path = deterministic_output_path("tenants/a/table", generation, 0);
        assert_eq!(
            path,
            format!("tenants/a/table/data/forge/{operation_id}-00000.parquet")
        );
    }

    /// A later batch failure retains every finalized file and path for cleanup.
    #[tokio::test]
    async fn batch_failure_preserves_complete_output_ownership() {
        let file = DataFileBuilder::default()
            .content(DataContentType::Data)
            .file_path("table/data/forge.parquet".to_owned())
            .file_format(DataFileFormat::Parquet)
            .partition(Struct::from_iter([Some(iceberg::spec::Literal::date(0))]))
            .partition_spec_id(0)
            .record_count(7)
            .file_size_in_bytes(128)
            .build()
            .expect("test output metadata must be complete");
        let mut state = RewriteBatchState::new();
        state.writer_rows = 7;
        state.record_output(file, "tenant/table/data/forge.parquet".to_owned());

        let result = state.fail(ForgeError::Shutdown).await;

        assert!(matches!(result.result, Err(ForgeError::Shutdown)));
        assert_eq!(result.files.len(), 1);
        assert_eq!(
            result.output_paths,
            ["tenant/table/data/forge.parquet".to_owned()]
        );
        assert_eq!(result.output_rows, 7);
    }
}
