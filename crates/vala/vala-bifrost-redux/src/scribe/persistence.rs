//! Bounded immutable-generation persistence for Scribe.

#[cfg(any(test, feature = "test-support"))]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arrow::datatypes::SchemaRef;
use bytes::Bytes;
use num_traits::ToPrimitive;
use tokio::runtime::Handle;
use tokio::sync::{Notify, mpsc, oneshot};
use uuid::Uuid;
use vala_sql::ValaPostgres;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::{TenantTableBinding, TenantTableKey};
use crate::contracts::ScribeError;
use crate::maintenance::StagingFilePublisher;
use crate::scribe::execution_lanes::{
    ScribePersistenceCpuOp, ScribePersistenceCpuPool, ScribePersistenceCpuResult, ScribeWalIoOp,
    ScribeWalIoPool, ScribeWalIoResult,
};
use crate::scribe::file_list_writer::{self, FileListCommitKey};
use crate::scribe::memory::{MemoryCategory, ScribeMemoryBudget};
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::seal_key::SealKey;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::{ScribeAppendMeta, WalLsn, WalSegmentRef, WalWriter};

/// Test-tier one-shot failures for the concrete persistence seams.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Default)]
pub struct PersistenceFaults {
    object_write: Arc<std::sync::atomic::AtomicBool>,
    sql_commit: Arc<std::sync::atomic::AtomicBool>,
    manifest_publication: Arc<std::sync::atomic::AtomicBool>,
    object_write_delay_ms: Arc<AtomicU64>,
    object_write_delays_ms: Arc<Mutex<Vec<u64>>>,
    object_write_active: Arc<AtomicUsize>,
    max_object_write_active: Arc<AtomicUsize>,
    last_error: Arc<Mutex<Option<String>>>,
}

#[cfg(any(test, feature = "test-support"))]
impl PersistenceFaults {
    /// Fail the next object-store write before it mutates storage.
    pub fn fail_next_object_write(&self) {
        self.object_write.store(true, Ordering::Release);
    }

    /// Fail the next SQL commit after the file-list/audit transaction is staged.
    pub fn fail_next_sql_commit(&self) {
        self.sql_commit.store(true, Ordering::Release);
    }

    /// Fail the next manifest publication after the SQL transaction commits.
    pub fn fail_next_manifest_publication(&self) {
        self.manifest_publication.store(true, Ordering::Release);
    }

    /// Delay object writes and expose their maximum overlap for concurrency tests.
    pub fn set_object_write_delay_for_test(&self, delay: Duration) {
        self.object_write_delay_ms.store(
            delay.as_millis().try_into().unwrap_or(u64::MAX),
            Ordering::Release,
        );
    }

    /// Set deterministic per-write delays consumed in order by object writes.
    pub fn set_object_write_delays_for_test(&self, delays: &[Duration]) {
        if let Ok(mut configured) = self.object_write_delays_ms.lock() {
            *configured = delays
                .iter()
                .map(|delay| delay.as_millis().try_into().unwrap_or(u64::MAX))
                .collect();
        }
    }

    /// Return the maximum number of object writes active at once.
    #[must_use]
    pub fn max_concurrent_object_writes_for_test(&self) -> usize {
        self.max_object_write_active.load(Ordering::Acquire)
    }

    /// Return the most recent persistence error observed by a test fixture.
    #[must_use]
    pub fn last_error_for_test(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|value| value.clone())
    }

    async fn begin_object_write(&self) -> ObjectWriteGuard {
        let active = self.object_write_active.fetch_add(1, Ordering::AcqRel) + 1;
        let mut observed = self.max_object_write_active.load(Ordering::Acquire);
        while active > observed {
            match self.max_object_write_active.compare_exchange(
                observed,
                active,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => observed = current,
            }
        }
        let delay_ms = self
            .object_write_delays_ms
            .lock()
            .ok()
            .and_then(|mut delays| {
                let delay = delays.first().copied()?;
                delays.remove(0);
                Some(delay)
            })
            .unwrap_or_else(|| self.object_write_delay_ms.load(Ordering::Acquire));
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        ObjectWriteGuard {
            active: Arc::clone(&self.object_write_active),
        }
    }

    fn take_object_write(&self) -> bool {
        self.object_write.swap(false, Ordering::AcqRel)
    }

    fn take_sql_commit(&self) -> bool {
        self.sql_commit.swap(false, Ordering::AcqRel)
    }

    fn take_manifest_publication(&self) -> bool {
        self.manifest_publication.swap(false, Ordering::AcqRel)
    }
}

#[cfg(any(test, feature = "test-support"))]
struct ObjectWriteGuard {
    active: Arc<AtomicUsize>,
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for ObjectWriteGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Stable identity for one detached generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenerationId(pub u64);

/// Immutable state transferred from a shard owner to persistence.
#[derive(Debug, Clone)]
pub struct ImmutableGeneration {
    /// Logical tenant/table owning the generation.
    pub table_key: TenantTableKey,
    /// Event-day partition represented by the generation.
    pub seal_key: SealKey,
    /// Local generation identity.
    pub generation_id: GenerationId,
    /// WAL stream identity used for file-list and manifest publication.
    pub stream: StreamIdentity,
    /// Inclusive minimum WAL LSN.
    pub wal_lsn_min: WalLsn,
    /// Inclusive maximum WAL LSN.
    pub wal_lsn_max: WalLsn,
    /// WAL segments retained until publication and grace expiry.
    pub wal_segments: Vec<WalSegmentRef>,
    /// WAL handle retained for grace-ordered segment retirement.
    pub wal: crate::scribe::wal::WalHandle,
    /// Original Arrow batches, shared by tail and persistence readers.
    pub rows: Vec<arrow::record_batch::RecordBatch>,
    /// Arrow schema shared by the original append batches.
    pub schema: SchemaRef,
    /// Canonical audit events paired with the rows.
    pub audit_events: Vec<AuditEvent>,
    /// WAL-derived append metadata paired with the batches.
    pub append_metas: Vec<ScribeAppendMeta>,
    /// Number of rows in the generation.
    pub row_count: usize,
    /// Estimated Arrow bytes retained by the generation.
    pub arrow_bytes: usize,
    /// Monotonic active-generation open time.
    pub opened_at: std::time::Instant,
    /// Monotonic detach time.
    pub closed_at: std::time::Instant,
}

impl ImmutableGeneration {
    /// Detach a frozen memtable snapshot into a persistence payload.
    #[must_use]
    pub fn from_frozen(
        frozen: &FrozenMemtable,
        table_key: TenantTableKey,
        stream: StreamIdentity,
        wal_segments: Vec<WalSegmentRef>,
        wal: crate::scribe::wal::WalHandle,
    ) -> Self {
        let wal_lsn_min = frozen
            .metas
            .iter()
            .map(|meta| meta.wal_lsn_min)
            .min()
            .unwrap_or(WalLsn::ZERO);
        let wal_lsn_max = frozen
            .metas
            .iter()
            .map(|meta| meta.wal_lsn_max)
            .max()
            .unwrap_or(WalLsn::ZERO);
        Self {
            table_key,
            seal_key: frozen.seal_key.clone(),
            generation_id: GenerationId(frozen.seal_id),
            stream,
            wal_lsn_min,
            wal_lsn_max,
            wal_segments,
            wal,
            rows: frozen.batches.clone(),
            schema: frozen.schema.clone(),
            audit_events: frozen.events.clone(),
            append_metas: frozen.metas.clone(),
            row_count: frozen.row_count(),
            arrow_bytes: frozen.arrow_bytes,
            opened_at: frozen.opened_at,
            closed_at: frozen.closed_at,
        }
    }

    fn frozen_snapshot(&self) -> FrozenMemtable {
        FrozenMemtable {
            seal_id: self.generation_id.0,
            seal_key: self.seal_key.clone(),
            schema: self.schema.clone(),
            batches: self.rows.clone(),
            events: self.audit_events.clone(),
            metas: self.append_metas.clone(),
            opened_at: self.opened_at,
            closed_at: self.closed_at,
            arrow_bytes: self.arrow_bytes,
        }
    }
}

/// Completion sent through the owning shard command queue.
#[derive(Debug, Clone)]
pub(crate) struct PersistenceCompletion {
    /// Generation identity reconciled by the owning shard.
    pub(crate) generation_id: GenerationId,
    /// Durable file-list key when publication succeeded.
    pub(crate) file_list_key: Option<FileListCommitKey>,
    /// WAL segments retained until shard retirement.
    pub(crate) wal_segments: Vec<WalSegmentRef>,
    /// WAL handle required for grace-ordered retirement.
    pub(crate) wal: crate::scribe::wal::WalHandle,
    /// Arrow bytes released after the generation becomes retireable.
    pub(crate) arrow_bytes: usize,
    /// Persistence detail when one durable stage failed.
    pub(crate) error: Option<String>,
}

/// One immutable generation submitted to the bounded persistence queue.
#[derive(Debug)]
pub(crate) struct PersistenceJob {
    /// Immutable generation sent to a persistence worker.
    pub(crate) generation: Arc<ImmutableGeneration>,
    /// Tenant/table binding used for object and SQL publication.
    pub(crate) binding: TenantTableBinding,
    /// Owning shard mailbox for completion reconciliation.
    pub(crate) completion_tx: mpsc::Sender<crate::scribe::shards::ShardCommand>,
    /// Optional caller waiter for explicit post-commit completion.
    pub(crate) completion_waiter: Option<oneshot::Sender<Result<(), String>>>,
}

/// Server-provisioned persistence dependencies.
pub struct ScribePersistenceConfig {
    /// Tenant-scoped Vala Postgres pool owner.
    pub postgres: Arc<ValaPostgres>,
    /// Maximum queued immutable generations.
    pub queue_items: usize,
    /// Number of asynchronous persistence workers.
    pub workers: usize,
    /// Concrete test-tier fault points; production uses the default no-fault value.
    #[cfg(any(test, feature = "test-support"))]
    pub faults: PersistenceFaults,
}

impl std::fmt::Debug for ScribePersistenceConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScribePersistenceConfig")
            .field("queue_items", &self.queue_items)
            .field("workers", &self.workers)
            .finish_non_exhaustive()
    }
}

impl ScribePersistenceConfig {
    /// Construct bounded persistence configuration.
    #[must_use]
    pub fn new(postgres: Arc<ValaPostgres>, queue_items: usize, workers: usize) -> Self {
        Self {
            postgres,
            queue_items: queue_items.max(1),
            workers: workers.max(1),
            #[cfg(any(test, feature = "test-support"))]
            faults: PersistenceFaults::default(),
        }
    }

    /// Install concrete test-tier fault points for this persistence runtime.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_test_faults(mut self, faults: PersistenceFaults) -> Self {
        self.faults = faults;
        self
    }
}

pub(crate) struct PersistenceRuntimeContext {
    /// Object-store operator shared by persistence workers.
    pub(crate) operator: Arc<opendal::Operator>,
    /// WAL writer used for manifest location and recovery identity.
    pub(crate) wal: Arc<WalWriter>,
    /// Bounded CPU lane used to encode immutable generations.
    pub(crate) persistence_cpu: ScribePersistenceCpuPool,
    /// Bounded WAL lane used to advance manifests.
    pub(crate) wal_io: ScribeWalIoPool,
    /// Pod identity used in staged object and file-list keys.
    pub(crate) node_id: String,
    /// Writer epoch used to distinguish concurrent pod writers.
    pub(crate) writer_epoch: i64,
    /// Scribe child budget used for per-job workspace reservations.
    pub(crate) memory: ScribeMemoryBudget,
    /// Optional local wake-up publisher used after confirmed file-list commits.
    pub(crate) staging_file_publisher: Option<StagingFilePublisher>,
    /// Deterministic fault points used only by test-tier persistence paths.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) faults: PersistenceFaults,
}

/// Bounded persistence queue and worker set.
#[derive(Clone)]
pub struct PersistenceRuntime {
    sender: Arc<Mutex<Option<mpsc::Sender<Box<PersistenceJob>>>>>,
    queued: Arc<AtomicUsize>,
    queued_bytes: Arc<AtomicUsize>,
    drained: Arc<Notify>,
    /// Retained worker handles aborted when the process deadline ends graceful persistence.
    tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    /// Lock-independent abort handles for every spawned persistence worker.
    abort_handles: Arc<Mutex<Vec<tokio::task::AbortHandle>>>,
}

impl std::fmt::Debug for PersistenceRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PersistenceRuntime")
            .field("queued", &self.queued.load(Ordering::Acquire))
            .field("queued_bytes", &self.queued_bytes.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl PersistenceRuntime {
    /// Start bounded persistence workers on the Scribe coordination runtime.
    #[must_use]
    pub(crate) fn start(
        config: ScribePersistenceConfig,
        context: PersistenceRuntimeContext,
        runtime: &Handle,
    ) -> Arc<Self> {
        let (sender, receiver) = mpsc::channel::<Box<PersistenceJob>>(config.queue_items);
        let runtime_state = Arc::new(Self {
            sender: Arc::new(Mutex::new(Some(sender))),
            queued: Arc::new(AtomicUsize::new(0)),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            drained: Arc::new(Notify::new()),
            tasks: Arc::new(Mutex::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(Vec::new())),
        });
        let receiver = Arc::new(tokio::sync::Mutex::new(receiver));
        let worker = Arc::new(PersistenceWorker::new(config.postgres, context));
        let mut tasks = Vec::with_capacity(config.workers);
        for _ in 0..config.workers {
            let receiver = Arc::clone(&receiver);
            let worker = Arc::clone(&worker);
            let state = Arc::clone(&runtime_state);
            let task = runtime.spawn(async move {
                loop {
                    let job = receiver.lock().await.recv().await;
                    let Some(job) = job else { break };
                    let queued_bytes = job.generation.arrow_bytes;
                    worker.process_job(*job).await;
                    state.queued.fetch_sub(1, Ordering::AcqRel);
                    state.queued_bytes.fetch_sub(queued_bytes, Ordering::AcqRel);
                    metrics::gauge!("bifrost_scribe_persistence_queue_depth").set(
                        state
                            .queued
                            .load(Ordering::Acquire)
                            .to_f64()
                            .unwrap_or(f64::MAX),
                    );
                    metrics::gauge!("bifrost_scribe_persistence_queue_bytes").set(
                        state
                            .queued_bytes
                            .load(Ordering::Acquire)
                            .to_f64()
                            .unwrap_or(f64::MAX),
                    );
                    state.drained.notify_waiters();
                }
            });
            let abort_handle = task.abort_handle();
            match runtime_state.abort_handles.lock() {
                Ok(mut handles) => handles.push(abort_handle),
                Err(poisoned) => {
                    tracing::error!(
                        "Scribe persistence abort-handle registry poisoned during startup"
                    );
                    poisoned.into_inner().push(abort_handle);
                }
            }
            tasks.push(task);
        }
        match runtime_state.tasks.lock() {
            Ok(mut retained) => *retained = tasks,
            Err(poisoned) => {
                tracing::error!("Scribe persistence join registry poisoned during startup");
                *poisoned.into_inner() = tasks;
            }
        }
        runtime_state
    }

    /// Try to enqueue a job without waiting in the shard owner.
    pub(crate) fn try_submit(&self, job: PersistenceJob) -> Result<(), Box<PersistenceJob>> {
        let sender = match self.sender.lock() {
            Ok(sender) => sender.clone(),
            Err(_) => return Err(Box::new(job)),
        };
        let Some(sender) = sender else {
            return Err(Box::new(job));
        };
        let queued_bytes = job.generation.arrow_bytes;
        self.queued.fetch_add(1, Ordering::AcqRel);
        self.queued_bytes.fetch_add(queued_bytes, Ordering::AcqRel);
        match sender.try_send(Box::new(job)) {
            Ok(()) => {
                metrics::gauge!("bifrost_scribe_persistence_queue_depth").set(
                    self.queued
                        .load(Ordering::Acquire)
                        .to_f64()
                        .unwrap_or(f64::MAX),
                );
                metrics::gauge!("bifrost_scribe_persistence_queue_bytes").set(
                    self.queued_bytes
                        .load(Ordering::Acquire)
                        .to_f64()
                        .unwrap_or(f64::MAX),
                );
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(job) | mpsc::error::TrySendError::Closed(job)) => {
                self.queued.fetch_sub(1, Ordering::AcqRel);
                self.queued_bytes.fetch_sub(queued_bytes, Ordering::AcqRel);
                metrics::counter!("bifrost_scribe_persistence_queue_rejections_total").increment(1);
                Err(job)
            }
        }
    }

    /// Stops accepting persistence jobs without awaiting worker progress.
    ///
    /// Already accepted jobs remain owned by retained worker handles until
    /// graceful drain completes or [`Self::abort_retained`] cancels them.
    pub(crate) fn close(&self) {
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
    }

    /// Waits for queued persistence jobs after [`Self::close`] has run.
    ///
    /// Dropping this future leaves worker ownership in the runtime so the
    /// caller can still invoke [`Self::abort_retained`] at its deadline.
    pub(crate) async fn drain(&self) {
        loop {
            let notified = self.drained.notified();
            if self.queued.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }

    /// Aborts every retained persistence worker without touching the async join registry.
    ///
    /// The abort-handle registry is drained before cancellation so repeated
    /// finalizer calls are idempotent and cannot double-count a worker.
    /// Poisoned registry state is recovered because shutdown must remain
    /// fail-closed after a panic in an unrelated join-registry owner.
    pub(crate) fn abort_retained(&self) -> usize {
        let handles = match self.abort_handles.lock() {
            Ok(mut handles) => std::mem::take(&mut *handles),
            Err(poisoned) => {
                tracing::error!("Scribe persistence abort-handle registry is poisoned");
                std::mem::take(&mut *poisoned.into_inner())
            }
        };
        let mut aborted = 0;
        for handle in handles {
            if !handle.is_finished() {
                handle.abort();
                aborted += 1;
            }
        }
        metrics::counter!("bifrost_scribe_persistence_worker_abort_total")
            .increment(aborted as u64);
        aborted
    }

    /// Drops retained join handles when the synchronous registry is available.
    ///
    /// This bookkeeping never controls cancellation; a contended join registry
    /// is left for its current owner while [`Self::abort_retained`] remains
    /// lock-independent.
    pub(crate) fn clear_retained_join_handles(&self) {
        match self.tasks.try_lock() {
            Ok(mut tasks) => tasks.clear(),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                tracing::error!("Scribe persistence join registry is poisoned");
                poisoned.into_inner().clear();
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                tracing::debug!("Scribe persistence join registry remained contended");
            }
        }
    }

    /// Current queued/in-flight persistence jobs.
    #[must_use]
    pub fn queue_depth(&self) -> usize {
        self.queued.load(Ordering::Acquire)
    }

    /// Current Arrow bytes retained by queued or executing persistence jobs.
    #[must_use]
    pub fn queue_bytes(&self) -> usize {
        self.queued_bytes.load(Ordering::Acquire)
    }
}

/// Owns the dependencies and durable workflow for one persistence worker.
///
/// Workers share the bounded queue but keep all object-store, SQL, WAL, memory,
/// retry-fault, and manifest-ordering state behind this concrete owner. The
/// manifest mutex serializes publication while independent workers may encode
/// and upload different generations.
struct PersistenceWorker {
    /// Vala Postgres handle used for tenant-scoped file-list transactions.
    postgres: Arc<ValaPostgres>,
    /// Object-store operator used for staged Parquet writes.
    operator: Arc<opendal::Operator>,
    /// WAL writer retained for manifest location and generation recovery.
    wal: Arc<WalWriter>,
    /// Bounded CPU lane used for Parquet encoding.
    persistence_cpu: ScribePersistenceCpuPool,
    /// Bounded filesystem lane used for manifest advancement.
    wal_io: ScribeWalIoPool,
    /// Pod identity embedded in object paths and file-list rows.
    node_id: String,
    /// Writer epoch embedded in file-list rows and manifests.
    writer_epoch: i64,
    /// Scribe memory budget for persistence workspace reservations.
    memory: ScribeMemoryBudget,
    /// Optional local wake-up publisher used after confirmed file-list commits.
    staging_file_publisher: Option<StagingFilePublisher>,
    /// Test-only fault points for deterministic persistence-path coverage.
    #[cfg(any(test, feature = "test-support"))]
    faults: PersistenceFaults,
    /// Serializes manifest publication after SQL commit.
    manifest_guard: tokio::sync::Mutex<()>,
}

impl PersistenceWorker {
    /// Builds a persistence worker from its complete durable dependencies.
    fn new(postgres: Arc<ValaPostgres>, context: PersistenceRuntimeContext) -> Self {
        Self {
            postgres,
            operator: context.operator,
            wal: context.wal,
            persistence_cpu: context.persistence_cpu,
            wal_io: context.wal_io,
            node_id: context.node_id,
            writer_epoch: context.writer_epoch,
            memory: context.memory,
            staging_file_publisher: context.staging_file_publisher,
            #[cfg(any(test, feature = "test-support"))]
            faults: context.faults,
            manifest_guard: tokio::sync::Mutex::new(()),
        }
    }

    /// Processes one queued generation and reports completion to its shard.
    ///
    /// The method reserves bounded workspace, performs the ordered persistence
    /// stages, and always sends a completion outcome back to the owning shard.
    /// A failed stage leaves the immutable generation retained for retry; an
    /// interrupted task may have already written an idempotent object or SQL
    /// row and is reconciled by replay.
    ///
    /// # Errors
    ///
    /// Persistence errors are captured in the completion sent to the shard;
    /// channel delivery failure is recorded as a dropped completion metric.
    ///
    /// # Cancellation
    ///
    /// Cancellation can occur after object or SQL side effects. The shard keeps
    /// the generation pending and WAL replay reconciles any partial progress.
    async fn process_job(&self, job: PersistenceJob) {
        let generation = Arc::clone(&job.generation);
        tracing::debug!(
            generation_id = generation.generation_id.0,
            seal_key = %generation.seal_key,
            wal_lsn_min = generation.wal_lsn_min.as_u64(),
            wal_lsn_max = generation.wal_lsn_max.as_u64(),
            "persisting immutable Scribe generation"
        );
        let workspace_bytes = generation
            .arrow_bytes
            .saturating_mul(2)
            .saturating_add(8 * 1024 * 1024);
        let result = match self
            .memory
            .try_reserve_maintenance(MemoryCategory::Persistence, workspace_bytes)
        {
            Ok(mut reservation) => {
                reservation.attach_shard(
                    self.memory.shard_accounting(),
                    crate::scribe::routing::shard_for(
                        generation.seal_key.tenant,
                        &generation.seal_key.table,
                    ),
                );
                let result = self.persist_once(&generation, &job.binding).await;
                drop(reservation);
                result
            }
            Err(error) => Err(error),
        };
        let status = if result.is_ok() {
            "published"
        } else {
            #[cfg(any(test, feature = "test-support"))]
            if let Err(error) = &result
                && let Ok(mut last_error) = self.faults.last_error.lock()
            {
                *last_error = Some(error.to_string());
            }
            "failed"
        };
        metrics::counter!("bifrost_scribe_persistence_jobs_total", "status" => status).increment(1);
        let published_at = std::time::Instant::now();
        metrics::histogram!("bifrost_scribe_persistence_publication_seconds").record(
            published_at
                .duration_since(generation.closed_at)
                .as_secs_f64(),
        );
        let completion = match result {
            Ok(file_list_key) => PersistenceCompletion {
                generation_id: generation.generation_id,
                file_list_key: Some(file_list_key),
                wal_segments: generation.wal_segments.clone(),
                wal: generation.wal.clone(),
                arrow_bytes: generation.arrow_bytes,
                error: None,
            },
            Err(error) => PersistenceCompletion {
                generation_id: generation.generation_id,
                file_list_key: None,
                wal_segments: generation.wal_segments.clone(),
                wal: generation.wal.clone(),
                arrow_bytes: generation.arrow_bytes,
                error: Some(error.to_string()),
            },
        };
        if job
            .completion_tx
            .send(crate::scribe::shards::ShardCommand::PersistenceComplete {
                completion: Box::new(completion),
                waiter: job.completion_waiter,
            })
            .await
            .is_err()
        {
            metrics::counter!("bifrost_scribe_persistence_completion_dropped_total").increment(1);
        }
    }

    /// Persists one generation through encode, object store, SQL, and manifest stages.
    ///
    /// SQL commits the file-list and audit rows before the manifest advances. A
    /// later manifest failure is intentionally recoverable through WAL replay;
    /// object writes use the deterministic generation path and may already exist
    /// when a retry begins.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when encoding, object storage, tenant SQL, SQL
    /// commit, or manifest advancement fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation may leave an object or committed file-list row behind. The
    /// immutable generation remains retained by its shard for reconciliation.
    async fn persist_once(
        &self,
        generation: &ImmutableGeneration,
        binding: &TenantTableBinding,
    ) -> Result<FileListCommitKey, ScribeError> {
        let frozen = generation.frozen_snapshot();
        let mut encoded = match self
            .persistence_cpu
            .submit(ScribePersistenceCpuOp::EncodeParquet {
                frozen: Box::new(frozen.clone()),
                binding: binding.clone(),
                tenant: binding.tenant,
            })
            .await?
        {
            ScribePersistenceCpuResult::ParquetEncoded(encoded) => encoded,
            ScribePersistenceCpuResult::Prepared(_)
            | ScribePersistenceCpuResult::ReplayRestored(_) => {
                return Err(ScribeError::Internal {
                    detail: "persistence lane returned the wrong persistence result".to_owned(),
                });
            }
        };
        let path = object_path(binding, &generation.seal_key, &self.node_id)?;
        #[cfg(any(test, feature = "test-support"))]
        let _object_write_guard = self.faults.begin_object_write().await;
        #[cfg(any(test, feature = "test-support"))]
        if self.faults.take_object_write() {
            return Err(ScribeError::Internal {
                detail: "test object-store write failure".to_owned(),
            });
        }
        let path = self
            .put_object(&path, std::mem::take(&mut encoded.bytes))
            .await?;
        let row = file_list_writer::build_insert(
            &frozen,
            &encoded,
            binding,
            &self.node_id,
            self.writer_epoch,
            &path,
        )?;
        let mut conn = self
            .postgres
            .tenant_conn(binding.tenant)
            .await
            .map_err(ScribeError::from)?;
        let outcome = file_list_writer::insert_and_audit(&mut conn, &row, &encoded.audit_events)
            .await
            .map_err(ScribeError::from)?;
        #[cfg(any(test, feature = "test-support"))]
        if self.faults.take_sql_commit() {
            return Err(ScribeError::Internal {
                detail: "test SQL commit failure".to_owned(),
            });
        }
        conn.commit().await.map_err(ScribeError::from)?;
        if let Some(publisher) = &self.staging_file_publisher {
            let event = crate::maintenance::StagingFileCommitted::new(
                binding.clone(),
                generation.seal_key.day.as_naive_date(),
            );
            let _ = publisher.try_publish(event);
        }

        let manifest_path = self.wal.base_dir().join("manifest");
        let lsn = generation.wal_lsn_max;
        #[cfg(any(test, feature = "test-support"))]
        if self.faults.take_manifest_publication() {
            return Err(ScribeError::Internal {
                detail: "test manifest publication failure".to_owned(),
            });
        }
        let _manifest_guard = self.manifest_guard.lock().await;
        let result = self
            .wal_io
            .submit(ScribeWalIoOp::AdvanceManifest {
                path: manifest_path,
                stream: generation.stream,
                seal_key: generation.seal_key.clone(),
                sealed_lsn: lsn,
            })
            .await?;
        if !matches!(result, ScribeWalIoResult::Completed) {
            return Err(ScribeError::Internal {
                detail: "WAL IO lane returned the wrong manifest result".to_owned(),
            });
        }
        Ok(outcome.commit_key)
    }
}

/// Builds the deterministic staged object path for one generation.
///
/// This helper remains free because it only validates and formats its inputs;
/// it owns no persistence state or external dependency.
///
/// # Errors
///
/// Returns [`ScribeError`] when the node identity or derived pod identifier is invalid.
fn object_path(
    binding: &TenantTableBinding,
    seal_key: &SealKey,
    node_id: &str,
) -> Result<String, ScribeError> {
    let node_uuid = Uuid::parse_str(node_id).map_err(|error| ScribeError::Internal {
        detail: format!("node_id is not a valid UUID: {error}"),
    })?;
    let pod_id =
        wyrd_spec::ids::PodId::new(format!("pod-{}", node_uuid.simple())).map_err(|error| {
            ScribeError::Internal {
                detail: format!("derived node PodId is invalid: {error}"),
            }
        })?;
    Ok(format!(
        "{}/day={}/{}",
        binding.object_prefix,
        seal_key.day,
        crate::scribe::filename::seal_filename(&pod_id)
    ))
}

impl PersistenceWorker {
    /// Writes one Parquet object with bounded timeout and retry backoff.
    ///
    /// The object-store operation is isolated from SQL and manifest publication
    /// so partial writes remain recoverable by the generation's deterministic path.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when all object-store attempts fail or time out.
    ///
    /// # Cancellation
    ///
    /// Cancellation stops the current attempt; a later retry may safely reuse
    /// the same object path.
    async fn put_object(&self, path: &str, bytes: Vec<u8>) -> Result<String, ScribeError> {
        let bytes = Bytes::from(bytes);
        let mut last_error = None;
        for attempt in 0..5_u32 {
            match tokio::time::timeout(
                Duration::from_secs(30),
                self.operator.write(path, bytes.clone()),
            )
            .await
            {
                Ok(Ok(_)) => return Ok(path.to_owned()),
                Ok(Err(error)) => {
                    let retryable = matches!(error.kind(), opendal::ErrorKind::RateLimited)
                        || (matches!(error.kind(), opendal::ErrorKind::Unexpected)
                            && error.is_temporary());
                    last_error = Some(ScribeError::ObjectStorePutFailed(error));
                    if !retryable || attempt == 4 {
                        break;
                    }
                }
                Err(_) => {
                    last_error = Some(ScribeError::Internal {
                        detail: "object-store PUT timed out".to_owned(),
                    });
                    if attempt == 4 {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100 * 2_u64.pow(attempt))).await;
        }
        Err(last_error.unwrap_or_else(|| ScribeError::Internal {
            detail: "object-store PUT failed without an error".to_owned(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::AssertUnwindSafe;

    /// Builds a persistence owner with empty queues for finalizer-only tests.
    fn test_runtime() -> PersistenceRuntime {
        let (sender, _receiver) = mpsc::channel(1);
        PersistenceRuntime {
            sender: Arc::new(Mutex::new(Some(sender))),
            queued: Arc::new(AtomicUsize::new(0)),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            drained: Arc::new(Notify::new()),
            tasks: Arc::new(Mutex::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Poisons a registry to prove shutdown recovers its retained state.
    fn poison_registry<T>(registry: &Mutex<T>) {
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = registry
                .lock()
                .expect("registry is healthy before poisoning");
            panic!("intentional registry poison");
        }));
    }

    /// Proves persistence cancellation works while the join registry is held.
    #[tokio::test]
    async fn abort_retained_cancels_worker_when_join_registry_is_contended() {
        let runtime = test_runtime();
        let task = tokio::spawn(std::future::pending::<()>());
        let abort = task.abort_handle();
        runtime.abort_handles.lock().unwrap().push(abort.clone());
        runtime.tasks.lock().unwrap().push(task);
        let tasks_guard = runtime.tasks.lock().unwrap();

        let recorder = wyrd_bench::BenchmarkRecorder::new();
        let aborted = metrics::with_local_recorder(&recorder, || runtime.abort_retained());
        assert_eq!(aborted, 1);
        assert_eq!(runtime.abort_retained(), 0);
        assert!(runtime.abort_handles.lock().unwrap().is_empty());
        drop(tasks_guard);
        tokio::task::yield_now().await;
        assert!(abort.is_finished());
        assert_eq!(
            recorder
                .snapshot()
                .counters
                .get("bifrost_scribe_persistence_worker_abort_total"),
            Some(&1)
        );
        runtime.clear_retained_join_handles();
        assert!(runtime.tasks.lock().unwrap().is_empty());
    }

    /// Proves poisoned persistence registries still cancel and clean owners.
    #[tokio::test]
    async fn abort_retained_recovers_poisoned_registries() {
        let runtime = test_runtime();
        let task = tokio::spawn(std::future::pending::<()>());
        let abort = task.abort_handle();
        runtime.abort_handles.lock().unwrap().push(abort.clone());
        runtime.tasks.lock().unwrap().push(task);
        poison_registry(&runtime.tasks);
        poison_registry(&runtime.abort_handles);

        assert_eq!(runtime.abort_retained(), 1);
        tokio::task::yield_now().await;
        assert!(abort.is_finished());
        let handles = match runtime.abort_handles.lock() {
            Ok(handles) => handles,
            Err(poisoned) => poisoned.into_inner(),
        };
        assert!(handles.is_empty());
        runtime.clear_retained_join_handles();
        let tasks = match runtime.tasks.lock() {
            Ok(tasks) => tasks,
            Err(poisoned) => poisoned.into_inner(),
        };
        assert!(tasks.is_empty());
    }
}
