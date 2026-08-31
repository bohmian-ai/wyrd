//! Attempt-scoped Forge rewrite resource and scratch ownership.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use datafusion::execution::memory_pool::{
    MemoryConsumer, MemoryLimit, MemoryPool, MemoryReservation,
};
use datafusion::execution::runtime_env::{RuntimeEnv, RuntimeEnvBuilder};
use uuid::Uuid;

use super::error::ForgeError;

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

impl std::fmt::Display for ForgeAttemptMemoryPool {
    /// Renders the pool name `DataFusion` reports in resource-exhaustion errors.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("forge_attempt")
    }
}

impl MemoryPool for ForgeAttemptMemoryPool {
    /// Returns the stable pool name `DataFusion` attributes reservations to.
    fn name(&self) -> &'static str {
        "forge_attempt"
    }

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
        let peak_memory_bytes = Arc::new(AtomicU64::new(0));
        let pool: Arc<dyn MemoryPool> = Arc::new(ForgeAttemptMemoryPool::new(
            lease.memory_pool(),
            Arc::clone(&peak_memory_bytes),
        ));
        let runtime = ForgeRewriteRuntime::new_attempt(
            Arc::clone(&pool),
            pod_spill_root,
            task_id,
            attempt_id,
            ForgeAttemptEnvelope {
                spill_limit: lease.scratch_bytes(),
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

/// `DataFusion` runtime with an owned, bounded spill directory.
pub struct ForgeRewriteRuntime {
    /// Runtime environment sharing the host-provisioned memory pool.
    runtime: Arc<RuntimeEnv>,
    /// Current Forge-owned child removed automatically on clean shutdown.
    ///
    /// Held, never read: dropping the runtime is what removes the directory,
    /// so the field is the lifetime and nothing else consults it.
    _spill_dir: tempfile::TempDir,
    /// Runtime-wide spill ceiling, equivalent to one operation under `tick`.
    ///
    /// Sort spill is the only consumer: rewrite output streams straight to the
    /// object store, so it owns none of the attempt's disk lease.
    spill_limit_bytes: u64,
    /// Largest bounded sort-spill ownership observed by this attempt.
    scratch_peak_bytes: Arc<AtomicU64>,
}

/// Exact runtime terms used to construct one attempt-owned execution environment.
#[derive(Clone, Copy)]
pub(crate) struct ForgeAttemptEnvelope {
    /// Sole scratch lease, owned entirely by `DataFusion` sort spill.
    spill_limit: u64,
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
    /// Returns [`ForgeError::InvalidConfig`] for a zero scratch lease,
    /// [`ForgeError::ScratchIo`] when the attempt's unique spill child cannot
    /// be created beneath the pod root, and [`ForgeError::Invariant`] when the
    /// execution runtime cannot be built from the granted envelope.
    pub(crate) fn new_attempt(
        memory_pool: Arc<dyn MemoryPool>,
        pod_spill_root: &Path,
        task_id: Uuid,
        attempt_id: Uuid,
        envelope: ForgeAttemptEnvelope,
    ) -> Result<Self, ForgeError> {
        let ForgeAttemptEnvelope {
            spill_limit: spill_limit_bytes,
        } = envelope;
        if spill_limit_bytes == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge scratch envelope must contain a positive sort-spill term".to_owned(),
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
                .map_err(|error| ForgeError::Invariant {
                    detail: format!("Forge attempt runtime construction failed: {error}"),
                })?,
        );
        Ok(Self {
            runtime,
            _spill_dir: spill_dir,
            spill_limit_bytes,
            scratch_peak_bytes: Arc::new(AtomicU64::new(0)),
        })
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

    /// Reports whether this runtime retained the exact leased memory pool.
    #[must_use]
    pub(crate) fn uses_memory_pool(&self, pool: &Arc<dyn MemoryPool>) -> bool {
        Arc::ptr_eq(&self.runtime.memory_pool, pool)
    }
}
