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
use vala_sql::ValaPostgres;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::{TenantTableBinding, TenantTableKey};
use crate::contracts::ScribeError;
use crate::maintenance::StagingFilePublisher;
use crate::resources::{ScribeMemoryLease, ScribeResources};
use crate::scribe::execution_lanes::{
    ScribePersistenceCpuOp, ScribePersistenceCpuPool, ScribePersistenceCpuResult, ScribeWalIoOp,
    ScribeWalIoPool, ScribeWalIoResult,
};
use crate::scribe::file_list_writer::{self, FileListCommitKey};
use crate::scribe::memory::{MemoryCategory, parquet_producer_delta};
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::{BoundedParquetArtifactSet, ParquetEncoded};
use crate::scribe::seal_key::{ScribeArtifactIdentity, SealKey};
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::{ScribeAppendMeta, WalLsn, WalSegmentRef, WalWriter};

/// Publishes the unlabeled persistence queue gauges before workers accept jobs.
fn register_idle_persistence_queue() {
    metrics::gauge!("bifrost_scribe_persistence_queue_depth").set(0.0);
    metrics::gauge!("bifrost_scribe_persistence_queue_bytes").set(0.0);
}

/// Record the elapsed seconds for one persistence stage (D84).
///
/// The end-to-end `bifrost_scribe_persistence_publication_seconds` histogram
/// measures closed-to-published latency but cannot say which stage dominates.
/// This extends — never replaces — that histogram with a per-stage split under
/// `bifrost_scribe_persist_stage_seconds{stage}`, where `stage` is closed to
/// `{parquet, put, sql_commit}`: the Parquet encode, the object-store put, and
/// the SQL file-list/audit commit. Each is recorded only after its stage
/// completes successfully, so a stage that errors and returns early contributes
/// no sample. The label carries no tenant, table, or object-path identity.
fn record_persist_stage(stage: &'static str, started: std::time::Instant) {
    metrics::histogram!("bifrost_scribe_persist_stage_seconds", "stage" => stage)
        .record(started.elapsed().as_secs_f64());
}

/// Emits the per-generation compression telemetry pair after a Parquet encode.
///
/// Sets `bifrost_scribe_persistence_compression_ratio` (raw Arrow bytes /
/// encoded Parquet bytes; skipped when `file_size` is zero to avoid division
/// by zero) and increments
/// `bifrost_scribe_persistence_encoded_bytes_total` by `file_size`.
fn emit_compression_telemetry(arrow_bytes: usize, file_size: usize) {
    if file_size > 0 {
        let arrow = arrow_bytes.to_f64().unwrap_or(f64::MAX);
        let encoded = file_size.to_f64().unwrap_or(f64::MAX);
        metrics::gauge!("bifrost_scribe_persistence_compression_ratio").set(arrow / encoded);
    }
    metrics::counter!("bifrost_scribe_persistence_encoded_bytes_total")
        .increment(u64::try_from(file_size).unwrap_or(u64::MAX));
}

/// Test-tier one-shot failures for the concrete persistence seams.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Default)]
pub struct PersistenceFaults {
    object_write: Arc<std::sync::atomic::AtomicBool>,
    sql_commit: Arc<std::sync::atomic::AtomicBool>,
    post_commit_client_error: Arc<std::sync::atomic::AtomicBool>,
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

    /// Return an error after the next publication COMMIT actually succeeds.
    pub fn fail_next_post_commit_response(&self) {
        self.post_commit_client_error.store(true, Ordering::Release);
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

    fn take_post_commit_client_error(&self) -> bool {
        self.post_commit_client_error.swap(false, Ordering::AcqRel)
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
#[derive(Debug)]
pub struct ImmutableGeneration {
    /// Logical tenant/table owning the generation.
    pub table_key: TenantTableKey,
    /// Event-day partition represented by the generation.
    pub seal_key: SealKey,
    /// Local generation identity.
    pub generation_id: GenerationId,
    /// WAL stream identity used for file-list and manifest publication.
    pub stream: StreamIdentity,
    /// Pod-local shard lane that owns this generation.
    ///
    /// Used for approximate memory-accounting attribution under batch-spread
    /// routing. The originating shard is the lane that froze the writable bucket.
    pub shard_id: usize,
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
    /// Replay-only committed identity retained through publication.
    pub(crate) replay_identity: Mutex<Option<crate::scribe::memory::ReplayIdentityOwnership>>,
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
            shard_id: frozen.shard_id,
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
            replay_identity: Mutex::new(None),
            opened_at: frozen.opened_at,
            closed_at: frozen.closed_at,
        }
    }

    fn frozen_snapshot(&self) -> FrozenMemtable {
        FrozenMemtable {
            seal_id: self.generation_id.0,
            shard_id: self.shard_id,
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
#[derive(Debug)]
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
    /// Replay identity returned after the persistence attempt settles.
    pub(crate) replay_identity: Option<crate::scribe::memory::ReplayIdentityOwnership>,
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
    /// Whether recovery must defer WAL-lane work until its reader releases the worker.
    pub(crate) defer_manifest_advance: bool,
}

/// Server-provisioned persistence dependencies.
pub struct ScribePersistenceConfig {
    /// Tenant-scoped Vala Postgres pool owner.
    pub postgres: Arc<ValaPostgres>,
    /// Operator capability used for membership-fenced durable publication.
    pub operator_pool: Option<vala_sql::OperatorPool>,
    /// Maximum queued immutable generations.
    pub queue_items: usize,
    /// Number of asynchronous persistence workers.
    pub workers: usize,
    /// Generation-scoped output scratch capability required before encoding.
    pub output_scratch: Option<Arc<crate::resources::ScratchVolume>>,
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
            operator_pool: None,
            queue_items: queue_items.max(1),
            workers: workers.max(1),
            output_scratch: None,
            #[cfg(any(test, feature = "test-support"))]
            faults: PersistenceFaults::default(),
        }
    }

    /// Installs the audited operator capability required by production publication.
    #[must_use]
    pub fn with_operator_pool(mut self, operator_pool: vala_sql::OperatorPool) -> Self {
        self.operator_pool = Some(operator_pool);
        self
    }

    /// Installs the generation-owned output scratch authority.
    #[must_use]
    pub fn with_output_scratch(mut self, output_scratch: crate::resources::ScratchVolume) -> Self {
        self.output_scratch = Some(Arc::new(output_scratch));
        self
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
    /// Replacement actor stream whose current membership fence authorizes publication.
    pub(crate) actor_stream: StreamIdentity,
    /// Scribe child budget used for per-job workspace reservations.
    pub(crate) memory: ScribeResources,
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
    /// Persistence failures retained for startup recovery readiness checks.
    failures: Arc<Mutex<Vec<String>>>,
    /// Retained worker handles aborted when the process deadline ends graceful persistence.
    tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    /// Lock-independent abort handles for every spawned persistence worker.
    abort_handles: Arc<Mutex<Vec<tokio::task::AbortHandle>>>,
    /// Generation-owned output scratch authority shared by direct and queued producers.
    output_scratch: Option<Arc<crate::resources::ScratchVolume>>,
    /// Operator-backed owner for caller-commit ambiguity reconciliation.
    reconciler: Option<ScribePublicationReconciler>,
    /// Bounded ownership-transfer seam for already-admitted ambiguous attempts.
    reconciliation_sender: Option<mpsc::Sender<ScribeReconciliationJob>>,
}

/// Reason a non-blocking persistence submission retained ownership of its job.
#[derive(Debug)]
pub(crate) enum PersistenceSubmitError {
    /// The bounded queue is temporarily full and worker progress can make room.
    Full(Box<PersistenceJob>),
    /// The persistence runtime no longer accepts jobs.
    Closed(Box<PersistenceJob>),
}

/// Runtime-owned ambiguous caller publication and optional completion waiter.
enum ScribeReconciliationJob {
    /// Caller-owned seal attempt transferred after an ambiguous COMMIT result.
    Caller {
        /// Complete move-only attempt retained across SQL retries.
        attempt: super::seal::ScribeCommitAttempt,
        /// Caller notification; dropping the receiver does not cancel reconciliation.
        completion: oneshot::Sender<Result<(), String>>,
    },
    /// Automatic persistence publication retained independently of its worker future.
    Persistence {
        /// Complete worker dependencies needed for manifest completion.
        worker: PersistenceWorker,
        /// Immutable generation whose WAL authority remains retained.
        generation: Arc<ImmutableGeneration>,
        /// Tenant/table identity used for the post-commit wake-up.
        binding: TenantTableBinding,
        /// Whether replay must defer manifest advancement.
        defer_manifest_advance: bool,
        /// Exact deterministic rows retried without reconstruction.
        rows: Vec<file_list_writer::FileListArtifactInsert>,
        /// Exact audit transition paired with the artifact set.
        events: Vec<AuditEvent>,
        /// Complete local scratch and remote artifact identity owner.
        artifacts: BoundedParquetArtifactSet,
        /// Checked producer workspace retained through reconciliation.
        parquet_owner: ScribeMemoryLease,
        /// Worker notification; dropping the receiver does not cancel reconciliation.
        completion: oneshot::Sender<Result<FileListCommitKey, ScribeError>>,
    },
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
    /// Build a persistence runtime with empty queues and no workers for tests.
    ///
    /// Mirrors the finalizer-only `tests::test_runtime` but is reachable from
    /// sibling scribe modules, so a seal-path test can construct a shard owner
    /// with `persistence: Some(..)` — required to exercise the binding-resolve
    /// and WAL retention prep steps — without a Postgres or object-store
    /// dependency. The receiver is dropped immediately, so `try_submit` finds a
    /// closed queue and `submit_front` leaves the generation queued under the
    /// existing full-queue retry semantics, which is exactly the state-B
    /// condition these tests assert.
    #[cfg(test)]
    pub(crate) fn empty_for_test() -> Self {
        let (sender, _receiver) = mpsc::channel(1);
        Self {
            sender: Arc::new(Mutex::new(Some(sender))),
            queued: Arc::new(AtomicUsize::new(0)),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            drained: Arc::new(Notify::new()),
            failures: Arc::new(Mutex::new(Vec::new())),
            tasks: Arc::new(Mutex::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(Vec::new())),
            output_scratch: None,
            reconciler: None,
            reconciliation_sender: None,
        }
    }

    /// Builds a one-slot workerless queue so shard tests can force backpressure.
    #[cfg(test)]
    pub(crate) fn bounded_for_test() -> (Self, mpsc::Receiver<Box<PersistenceJob>>) {
        let (sender, receiver) = mpsc::channel(1);
        let runtime = Self {
            sender: Arc::new(Mutex::new(Some(sender))),
            queued: Arc::new(AtomicUsize::new(0)),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            drained: Arc::new(Notify::new()),
            failures: Arc::new(Mutex::new(Vec::new())),
            tasks: Arc::new(Mutex::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(Vec::new())),
            output_scratch: None,
            reconciler: None,
            reconciliation_sender: None,
        };
        (runtime, receiver)
    }

    /// Accounts one workerless test job as completed and wakes capacity waiters.
    #[cfg(test)]
    pub(crate) fn complete_for_test(&self, arrow_bytes: usize) {
        self.queued.fetch_sub(1, Ordering::AcqRel);
        self.queued_bytes.fetch_sub(arrow_bytes, Ordering::AcqRel);
        self.drained.notify_waiters();
    }

    /// Spawns the cancellation-independent reconciliation queue owner.
    fn spawn_reconciliation_worker(
        runtime: &Handle,
        mut receiver: mpsc::Receiver<ScribeReconciliationJob>,
        reconciler: ScribePublicationReconciler,
    ) -> tokio::task::JoinHandle<()> {
        runtime.spawn(async move {
            while let Some(job) = receiver.recv().await {
                match job {
                    ScribeReconciliationJob::Caller {
                        attempt,
                        completion,
                    } => {
                        let result = attempt
                            .reconcile(&reconciler)
                            .await
                            .map_err(|error| error.to_string());
                        let _ = completion.send(result);
                    }
                    ScribeReconciliationJob::Persistence {
                        worker,
                        generation,
                        binding,
                        defer_manifest_advance,
                        rows,
                        events,
                        artifacts,
                        parquet_owner,
                        completion,
                    } => {
                        let result = worker
                            .reconcile_persistence(
                                &reconciler,
                                PersistenceReconciliation {
                                    generation: &generation,
                                    binding: &binding,
                                    defer_manifest_advance,
                                    rows: &rows,
                                    events: &events,
                                    artifacts,
                                    parquet_owner,
                                },
                            )
                            .await;
                        let _ = completion.send(result);
                    }
                }
            }
        })
    }

    /// Spawns one bounded persistence queue consumer.
    fn spawn_persistence_worker(
        runtime: &Handle,
        receiver: Arc<tokio::sync::Mutex<mpsc::Receiver<Box<PersistenceJob>>>>,
        worker: Arc<PersistenceWorker>,
        state: Arc<Self>,
    ) -> tokio::task::JoinHandle<()> {
        runtime.spawn(async move {
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
        })
    }

    /// Start bounded persistence workers on the Scribe coordination runtime.
    #[must_use]
    pub(crate) fn start(
        config: ScribePersistenceConfig,
        context: PersistenceRuntimeContext,
        runtime: &Handle,
    ) -> Arc<Self> {
        let (sender, receiver) = mpsc::channel::<Box<PersistenceJob>>(config.queue_items);
        register_idle_persistence_queue();
        let output_scratch = config.output_scratch.clone();
        let reconciler = config.operator_pool.as_ref().map(|pool| {
            ScribePublicationReconciler::new(
                pool.clone(),
                context.actor_stream,
                #[cfg(any(test, feature = "test-support"))]
                config.faults.clone(),
            )
        });
        let (reconciliation_sender, reconciliation_receiver) = mpsc::channel(config.queue_items);
        let runtime_state = Arc::new(Self {
            sender: Arc::new(Mutex::new(Some(sender))),
            queued: Arc::new(AtomicUsize::new(0)),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            drained: Arc::new(Notify::new()),
            failures: Arc::new(Mutex::new(Vec::new())),
            tasks: Arc::new(Mutex::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(Vec::new())),
            output_scratch: output_scratch.clone(),
            reconciler,
            reconciliation_sender: Some(reconciliation_sender),
        });
        let receiver = Arc::new(tokio::sync::Mutex::new(receiver));
        let worker = Arc::new(PersistenceWorker::new(
            config.operator_pool,
            Arc::clone(&runtime_state.failures),
            context,
            output_scratch,
            runtime_state
                .reconciliation_sender
                .as_ref()
                .expect("started persistence runtime owns reconciliation sender")
                .clone(),
        ));
        let mut tasks = Vec::with_capacity(config.workers);
        if let Some(reconciler) = runtime_state.reconciler.clone() {
            let task =
                Self::spawn_reconciliation_worker(runtime, reconciliation_receiver, reconciler);
            let abort_handle = task.abort_handle();
            match runtime_state.abort_handles.lock() {
                Ok(mut handles) => handles.push(abort_handle),
                Err(poisoned) => poisoned.into_inner().push(abort_handle),
            }
            tasks.push(task);
        }
        for _ in 0..config.workers {
            let receiver = Arc::clone(&receiver);
            let worker = Arc::clone(&worker);
            let state = Arc::clone(&runtime_state);
            let task = Self::spawn_persistence_worker(runtime, receiver, worker, state);
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
    pub(crate) fn try_submit(&self, job: PersistenceJob) -> Result<(), PersistenceSubmitError> {
        let sender = match self.sender.lock() {
            Ok(sender) => sender.clone(),
            Err(_) => return Err(PersistenceSubmitError::Closed(Box::new(job))),
        };
        let Some(sender) = sender else {
            return Err(PersistenceSubmitError::Closed(Box::new(job)));
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
            Err(error) => {
                self.queued.fetch_sub(1, Ordering::AcqRel);
                self.queued_bytes.fetch_sub(queued_bytes, Ordering::AcqRel);
                metrics::counter!("bifrost_scribe_persistence_queue_rejections_total").increment(1);
                super::record_scribe_rejection("queue");
                Err(match error {
                    mpsc::error::TrySendError::Full(job) => PersistenceSubmitError::Full(job),
                    mpsc::error::TrySendError::Closed(job) => PersistenceSubmitError::Closed(job),
                })
            }
        }
    }

    /// Waits until a persistence worker completes queued work or the runtime closes.
    ///
    /// Registering the notification before inspecting queue state prevents a
    /// completion between those operations from stranding a replay retry.
    /// Cancellation drops only this waiter; queued jobs and worker ownership
    /// remain in the runtime.
    ///
    /// # Errors
    ///
    /// Returns an internal Scribe error when the persistence queue is closed.
    pub(crate) async fn wait_for_submission_progress(&self) -> Result<(), ScribeError> {
        let notified = self.drained.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let queue_open = self
            .sender
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "Scribe persistence sender lock poisoned".to_owned(),
            })?
            .as_ref()
            .is_some_and(|sender| !sender.is_closed());
        if !queue_open {
            return Err(ScribeError::Internal {
                detail: "Scribe persistence queue closed during replay".to_owned(),
            });
        }
        if self.queued.load(Ordering::Acquire) != 0 {
            notified.await;
        }
        let queue_open = self
            .sender
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "Scribe persistence sender lock poisoned".to_owned(),
            })?
            .as_ref()
            .is_some_and(|sender| !sender.is_closed());
        if !queue_open {
            return Err(ScribeError::Internal {
                detail: "Scribe persistence queue closed during replay".to_owned(),
            });
        }
        Ok(())
    }

    /// Stops accepting persistence jobs without awaiting worker progress.
    ///
    /// Already accepted jobs remain owned by retained worker handles until
    /// graceful drain completes or [`Self::abort_retained`] cancels them. All
    /// capacity waiters are awakened so they observe the closed queue.
    pub(crate) fn close(&self) {
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        self.drained.notify_waiters();
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
    /// Closing submissions first releases every capacity waiter even when an
    /// aborted worker cannot decrement the retained queue counters.
    /// Poisoned registry state is recovered because shutdown must remain
    /// fail-closed after a panic in an unrelated join-registry owner.
    pub(crate) fn abort_retained(&self) -> usize {
        self.close();
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

    /// Returns the generation scratch authority used by caller-owned seals.
    #[must_use]
    pub(crate) fn output_scratch(&self) -> Option<Arc<crate::resources::ScratchVolume>> {
        self.output_scratch.clone()
    }

    /// Returns the concrete operator-backed publication reconciler.
    #[must_use]
    pub(crate) fn publication_reconciler(&self) -> Option<ScribePublicationReconciler> {
        self.reconciler.clone()
    }

    /// Transfers one ambiguous caller attempt into the independent retry owner.
    ///
    /// # Errors
    /// Returns the attempt when shutdown has removed the reconciliation owner.
    pub(crate) fn submit_reconciliation(
        &self,
        attempt: super::seal::ScribeCommitAttempt,
    ) -> Result<oneshot::Receiver<Result<(), String>>, Box<super::seal::ScribeCommitAttempt>> {
        let Some(sender) = &self.reconciliation_sender else {
            return Err(Box::new(attempt));
        };
        let (completion, receiver) = oneshot::channel();
        sender
            .try_send(ScribeReconciliationJob::Caller {
                attempt,
                completion,
            })
            .map_err(|error| match error.into_inner() {
                ScribeReconciliationJob::Caller { attempt, .. } => Box::new(attempt),
                ScribeReconciliationJob::Persistence { .. } => {
                    unreachable!("caller submission cannot return a persistence job")
                }
            })?;
        Ok(receiver)
    }
}

/// Owns the dependencies and durable workflow for one persistence worker.
///
/// Workers share the bounded queue but keep all object-store, SQL, WAL, memory,
/// retry-fault, and manifest-ordering state behind this concrete owner. The
/// manifest mutex serializes publication while independent workers may encode
/// and upload different generations.
#[derive(Clone)]
struct PersistenceWorker {
    /// Audited operator pool for atomically fenced publication.
    operator_pool: Option<vala_sql::OperatorPool>,
    /// Shared durable failure ledger consumed by startup recovery.
    failures: Arc<Mutex<Vec<String>>>,
    /// Replacement actor identity used only for publication authority.
    actor_stream: StreamIdentity,
    /// Object-store operator used for staged Parquet writes.
    operator: Arc<opendal::Operator>,
    /// WAL writer retained for manifest location and generation recovery.
    wal: Arc<WalWriter>,
    /// Bounded CPU lane used for Parquet encoding.
    persistence_cpu: ScribePersistenceCpuPool,
    /// Bounded filesystem lane used for manifest advancement.
    wal_io: ScribeWalIoPool,
    /// Scribe memory budget for persistence workspace reservations.
    memory: ScribeResources,
    /// Optional local wake-up publisher used after confirmed file-list commits.
    staging_file_publisher: Option<StagingFilePublisher>,
    /// Generation-owned scratch authority required before writer creation.
    output_scratch: Option<Arc<crate::resources::ScratchVolume>>,
    /// Runtime-owned queue shielding ambiguous publication from worker cancellation.
    reconciliation_sender: mpsc::Sender<ScribeReconciliationJob>,
    /// Test-only fault points for deterministic persistence-path coverage.
    #[cfg(any(test, feature = "test-support"))]
    faults: PersistenceFaults,
    /// Serializes manifest publication after SQL commit.
    manifest_guard: Arc<tokio::sync::Mutex<()>>,
}

/// Complete retained state for one automatic publication reconciliation.
struct PersistenceReconciliation<'a> {
    /// Immutable generation whose manifest may advance after publication.
    generation: &'a ImmutableGeneration,
    /// Tenant-table binding used for the local visibility wake-up.
    binding: &'a TenantTableBinding,
    /// Whether replay retains manifest advancement for its reader owner.
    defer_manifest_advance: bool,
    /// Exact ordered SQL rows retried without mutation.
    rows: &'a [file_list_writer::FileListArtifactInsert],
    /// Exact audit events retried with the publication transaction.
    events: &'a [AuditEvent],
    /// Scratch-backed artifacts retained until outcome resolution.
    artifacts: BoundedParquetArtifactSet,
    /// Producer memory retained until outcome resolution.
    parquet_owner: ScribeMemoryLease,
}

/// Closed publication classification controlling cleanup and WAL authority.
#[derive(Debug)]
pub enum ScribePublicationOutcome {
    /// The complete artifact set and audit transition committed or replayed exactly.
    Committed(file_list_writer::FileListArtifactSetOutcome),
    /// Failure occurred before the first COMMIT attempt and exact uploads may be removed.
    KnownNotCommitted(ScribeError),
    /// A COMMIT-capable transaction returned an error and all authority must be retained.
    UnknownCommitOutcome(ScribeError),
}

/// Concrete owner that retries the identical fenced full-set transaction.
#[derive(Clone)]
pub struct ScribePublicationReconciler {
    /// Existing audited operator capability used for every retry.
    operator_pool: vala_sql::OperatorPool,
    /// Replacement actor whose membership fence authorizes publication.
    actor_stream: StreamIdentity,
    /// Deterministic post-COMMIT response fault used by integration proofs.
    #[cfg(any(test, feature = "test-support"))]
    faults: PersistenceFaults,
}

impl ScribePublicationReconciler {
    /// Constructs the sole publication reconciler for one persistence worker.
    #[must_use]
    fn new(
        operator_pool: vala_sql::OperatorPool,
        actor_stream: StreamIdentity,
        #[cfg(any(test, feature = "test-support"))] faults: PersistenceFaults,
    ) -> Self {
        Self {
            operator_pool,
            actor_stream,
            #[cfg(any(test, feature = "test-support"))]
            faults,
        }
    }

    /// Attempts or reconciles one exact full-set publication.
    ///
    /// Any error is conservatively unknown because the SQL owner crosses the
    /// COMMIT boundary internally; callers retain objects and WAL authority.
    pub(crate) async fn publish(
        &self,
        rows: &[file_list_writer::FileListArtifactInsert],
        events: &[AuditEvent],
    ) -> ScribePublicationOutcome {
        match file_list_writer::insert_artifact_set_and_audit_fenced(
            &self.operator_pool,
            self.actor_stream,
            rows,
            events,
        )
        .await
        {
            Ok(outcome) => {
                #[cfg(any(test, feature = "test-support"))]
                if self.faults.take_post_commit_client_error() {
                    return ScribePublicationOutcome::UnknownCommitOutcome(ScribeError::Internal {
                        detail: "test client lost the committed publication response".to_owned(),
                    });
                }
                ScribePublicationOutcome::Committed(outcome)
            }
            Err(error) => ScribePublicationOutcome::UnknownCommitOutcome(ScribeError::from(error)),
        }
    }
}

impl PersistenceWorker {
    /// Builds a persistence worker from its complete durable dependencies.
    fn new(
        operator_pool: Option<vala_sql::OperatorPool>,
        failures: Arc<Mutex<Vec<String>>>,
        context: PersistenceRuntimeContext,
        output_scratch: Option<Arc<crate::resources::ScratchVolume>>,
        reconciliation_sender: mpsc::Sender<ScribeReconciliationJob>,
    ) -> Self {
        Self {
            operator_pool,
            failures,
            actor_stream: context.actor_stream,
            operator: context.operator,
            wal: context.wal,
            persistence_cpu: context.persistence_cpu,
            wal_io: context.wal_io,
            memory: context.memory,
            staging_file_publisher: context.staging_file_publisher,
            output_scratch,
            reconciliation_sender,
            #[cfg(any(test, feature = "test-support"))]
            faults: context.faults,
            manifest_guard: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Processes one queued generation and reports completion to its shard.
    ///
    /// The method reserves the shared generation-derived workspace, performs
    /// the ordered persistence stages, and always sends a completion outcome
    /// back to the owning shard.
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
        let mut visibility = super::seal::VisibilityPublishGuard::new();
        visibility.arm_cancellation();
        let generation = Arc::clone(&job.generation);
        tracing::debug!(
            generation_id = generation.generation_id.0,
            seal_key = %generation.seal_key,
            wal_lsn_min = generation.wal_lsn_min.as_u64(),
            wal_lsn_max = generation.wal_lsn_max.as_u64(),
            "persisting immutable Scribe generation"
        );
        let result = self.persist_generation(&generation, &job).await;
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
        if let Err(error) = &result {
            tracing::warn!(
                error = %error,
                generation_id = generation.generation_id.0,
                seal_key = %generation.seal_key,
                "persistence job failed"
            );
            if let Ok(mut failures) = self.failures.lock() {
                failures.push(error.to_string());
            }
        }
        metrics::counter!("bifrost_scribe_persistence_jobs_total", "status" => status).increment(1);
        let published_at = std::time::Instant::now();
        metrics::histogram!("bifrost_scribe_persistence_publication_seconds").record(
            published_at
                .duration_since(generation.closed_at)
                .as_secs_f64(),
        );
        let publication_succeeded = result.is_ok();
        let replay_identity = generation
            .replay_identity
            .lock()
            .ok()
            .and_then(|mut identity| identity.take());
        let completion = match result {
            Ok(file_list_key) => PersistenceCompletion {
                generation_id: generation.generation_id,
                file_list_key: Some(file_list_key),
                wal_segments: generation.wal_segments.clone(),
                wal: generation.wal.clone(),
                arrow_bytes: generation.arrow_bytes,
                replay_identity,
                error: None,
            },
            Err(error) => PersistenceCompletion {
                generation_id: generation.generation_id,
                file_list_key: None,
                wal_segments: generation.wal_segments.clone(),
                wal: generation.wal.clone(),
                arrow_bytes: generation.arrow_bytes,
                replay_identity,
                error: Some(error.to_string()),
            },
        };
        self.deliver_completion(job, completion, visibility, publication_succeeded)
            .await;
    }

    /// Reserves the exact producer delta and persists one immutable generation.
    ///
    /// Replay identity bytes participate in the producer tuple while the
    /// existing footer transfer consumes that same complete owned-byte value.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for ownership overflow, producer admission, or
    /// any persistence-stage failure.
    async fn persist_generation(
        &self,
        generation: &Arc<ImmutableGeneration>,
        job: &PersistenceJob,
    ) -> Result<FileListCommitKey, ScribeError> {
        let identity_bytes = generation
            .replay_identity
            .lock()
            .map(|identity| {
                identity
                    .as_ref()
                    .map_or(0, super::memory::ReplayIdentityOwnership::bytes)
            })
            .unwrap_or_default();
        let owned_bytes = generation
            .arrow_bytes
            .checked_add(identity_bytes)
            .ok_or_else(|| ScribeError::Internal {
                detail: "replay producer ownership overflowed".to_owned(),
            })?;
        let workspace_bytes = parquet_producer_delta(owned_bytes)?;
        let mut reservation = self
            .memory
            .try_reserve_maintenance(MemoryCategory::Persistence, workspace_bytes)?;
        reservation
            .attach_shard(generation.shard_id)
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        let mut parquet_owner = Some(reservation);
        self.persist_once(
            generation,
            &job.binding,
            job.defer_manifest_advance,
            &mut parquet_owner,
            owned_bytes,
        )
        .await
    }

    /// Delivers one completion to the owning shard and resolves its visibility span.
    ///
    /// Emits `completion_send` and `visibility_ack` stage events around the two
    /// awaits so a worker parked in mailbox delivery or in the shard
    /// acknowledgment is identifiable from logs. A failed delivery increments
    /// the dropped-completion counter and warns, because the owning shard can no
    /// longer retire the generation; the visibility span is then failed or
    /// cancelled by [`finish_visibility_publication`].
    async fn deliver_completion(
        &self,
        job: PersistenceJob,
        completion: PersistenceCompletion,
        visibility: super::seal::VisibilityPublishGuard,
        publication_succeeded: bool,
    ) {
        let generation_id = job.generation.generation_id.0;
        let (visibility_result_tx, visibility_result_rx) = oneshot::channel();
        tracing::debug!(
            generation_id,
            stage = "completion_send",
            "persist stage start"
        );
        let completion_delivered = job
            .completion_tx
            .send(crate::scribe::shards::ShardCommand::PersistenceComplete {
                completion: Box::new(completion),
                waiter: job.completion_waiter,
                visibility_result: visibility_result_tx,
            })
            .await
            .is_ok();
        if !completion_delivered {
            metrics::counter!("bifrost_scribe_persistence_completion_dropped_total").increment(1);
            tracing::warn!(
                generation_id,
                seal_key = %job.generation.seal_key,
                "persistence completion dropped: shard owner is gone"
            );
        }
        tracing::debug!(
            generation_id,
            stage = "visibility_ack",
            "persist stage start"
        );
        finish_visibility_publication(
            visibility,
            publication_succeeded,
            completion_delivered,
            visibility_result_rx,
        )
        .await;
        tracing::debug!(generation_id, "persistence job finished");
    }

    /// Encode one frozen generation to Parquet and time the encode stage (D84).
    ///
    /// Submits the encode to the persistence CPU lane, asserts the lane returned
    /// a Parquet result, and records the elapsed time into the `parquet` stage of
    /// `bifrost_scribe_persist_stage_seconds` on success. Extracted from
    /// [`Self::persist_once`] so that method stays within its length budget while
    /// the stage timing lives next to the work it measures.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the persistence lane returns a
    /// non-Parquet result, and propagates any encode failure from the lane.
    async fn encode_parquet_stage(
        &self,
        frozen: &FrozenMemtable,
        binding: &TenantTableBinding,
        scratch_dir: &std::path::Path,
        object_base: &str,
        footer_reservation: crate::scribe::memory::EncodedFooterReservation,
    ) -> Result<ParquetEncoded, ScribeError> {
        let parquet_started = std::time::Instant::now();
        let encoded = match self
            .persistence_cpu
            .submit(ScribePersistenceCpuOp::EncodeParquet(Box::new(
                crate::scribe::execution_lanes::EncodeParquetOp {
                    frozen: Box::new(frozen.clone()),
                    binding: binding.clone(),
                    tenant: binding.tenant,
                    scratch_dir: scratch_dir.to_path_buf(),
                    object_base: object_base.to_owned(),
                    footer_reservation,
                },
            )))
            .await?
        {
            ScribePersistenceCpuResult::ParquetEncoded(encoded) => encoded,
            ScribePersistenceCpuResult::Prepared(_)
            | ScribePersistenceCpuResult::NativeSliceProduced { .. }
            | ScribePersistenceCpuResult::OtlpSliceProduced { .. }
            | ScribePersistenceCpuResult::ReplayRestored(_) => {
                return Err(ScribeError::Internal {
                    detail: "persistence lane returned the wrong persistence result".to_owned(),
                });
            }
        };
        record_persist_stage("parquet", parquet_started);
        Ok(encoded)
    }

    /// Prepares generation scratch, object identity, and footer ownership.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when scratch admission, artifact identity, or
    /// producer-child transfer fails.
    fn prepare_encoding(
        &self,
        generation: &ImmutableGeneration,
        binding: &TenantTableBinding,
        parquet_owner: &mut Option<ScribeMemoryLease>,
        generation_owned_bytes: usize,
    ) -> Result<
        (
            crate::resources::ScribeGenerationScratch,
            String,
            crate::scribe::memory::EncodedFooterReservation,
        ),
        ScribeError,
    > {
        let generation_id = generation.generation_id.0;
        let scratch_capability =
            self.output_scratch
                .as_ref()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "Scribe output scratch is unavailable before encoding".to_owned(),
                })?;
        let scratch = scratch_capability
            .create_scribe_generation(
                &generation.stream.node_id.to_string(),
                generation_id,
                256 * 1024 * 1024,
            )
            .map_err(|error| ScribeError::Internal {
                detail: format!("Scribe output scratch admission failed: {error}"),
            })?;
        let object_base = ScribeArtifactIdentity::new(
            binding,
            generation.seal_key.day,
            &generation.stream.node_id.to_string(),
            generation.stream.writer_epoch.as_i64(),
            generation.shard_id,
            generation.wal_lsn_min.as_u64(),
            generation.wal_lsn_max.as_u64(),
        )?
        .object_base();
        let footer_reservation = crate::scribe::memory::EncodedFooterReservation::transfer_from(
            parquet_owner
                .as_mut()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "automatic producer reservation was transferred before encoding"
                        .to_owned(),
                })?,
            generation_owned_bytes,
        )?;
        Ok((scratch, object_base, footer_reservation))
    }

    /// Uploads every encoded artifact and removes prior uploads on failure.
    ///
    /// # Errors
    ///
    /// Returns the injected or object-store write failure after best-effort
    /// cleanup of exact identities uploaded by this attempt.
    async fn upload_artifacts(
        &self,
        generation_id: u64,
        artifacts: &BoundedParquetArtifactSet,
    ) -> Result<Vec<String>, ScribeError> {
        #[cfg(any(test, feature = "test-support"))]
        let _object_write_guard = self.faults.begin_object_write().await;
        #[cfg(any(test, feature = "test-support"))]
        if self.faults.take_object_write() {
            return Err(ScribeError::Internal {
                detail: "test object-store write failure".to_owned(),
            });
        }
        tracing::debug!(generation_id, stage = "object_put", "persist stage start");
        let put_started = std::time::Instant::now();
        let mut uploaded = Vec::with_capacity(artifacts.len());
        for artifact in artifacts {
            match self.put_artifact(artifact).await {
                Ok(()) => uploaded.push(artifact.object_identity.clone()),
                Err(error) => {
                    self.cleanup_uploaded(&uploaded).await;
                    return Err(error);
                }
            }
        }
        record_persist_stage("put", put_started);
        Ok(uploaded)
    }

    /// Transfers an ambiguous publication into the runtime reconciliation owner.
    ///
    /// # Errors
    ///
    /// Returns an internal error when the reconciliation queue or completion
    /// channel is unavailable. Queue loss poisons and retains material owners.
    async fn await_ambiguous_publication(
        &self,
        generation: &Arc<ImmutableGeneration>,
        binding: &TenantTableBinding,
        defer_manifest_advance: bool,
        rows: Vec<file_list_writer::FileListArtifactInsert>,
        encoded: ParquetEncoded,
        parquet_owner: ScribeMemoryLease,
    ) -> Result<FileListCommitKey, ScribeError> {
        let (completion, receiver) = oneshot::channel();
        if let Err(error) =
            self.reconciliation_sender
                .try_send(ScribeReconciliationJob::Persistence {
                    worker: self.clone(),
                    generation: Arc::clone(generation),
                    binding: binding.clone(),
                    defer_manifest_advance,
                    rows,
                    events: encoded.audit_events,
                    artifacts: encoded.artifacts,
                    parquet_owner,
                    completion,
                })
        {
            if let ScribeReconciliationJob::Persistence {
                artifacts,
                parquet_owner,
                ..
            } = error.into_inner()
            {
                artifacts.retain_for_reconciliation();
                std::mem::forget(parquet_owner);
            }
            self.memory.poison();
            return Err(ScribeError::Internal {
                detail: "automatic reconciliation owner is unavailable after COMMIT".to_owned(),
            });
        }
        receiver.await.map_err(|_| ScribeError::Internal {
            detail: "automatic reconciliation owner exited without a result".to_owned(),
        })?
    }

    /// Persists one generation through encode, object store, SQL, and manifest stages.
    ///
    /// SQL commits the file-list and audit rows before the manifest advances. A
    /// later manifest failure is intentionally recoverable through WAL replay;
    /// object writes use the deterministic generation path and may already exist
    /// when a retry begins.
    ///
    /// Each of the three costed stages — Parquet encode, object-store put, and
    /// the SQL commit — records its elapsed time into
    /// `bifrost_scribe_persist_stage_seconds{stage}` via [`record_persist_stage`]
    /// (D84), splitting the end-to-end publication histogram without changing the
    /// persistence work or its ordering. Every stage additionally emits a
    /// `persist stage start` debug event before it begins so a worker parked
    /// mid-stage is identifiable from logs: the last emitted stage for a
    /// generation is the stage that never completed.
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
        generation: &Arc<ImmutableGeneration>,
        binding: &TenantTableBinding,
        defer_manifest_advance: bool,
        parquet_owner: &mut Option<ScribeMemoryLease>,
        generation_owned_bytes: usize,
    ) -> Result<FileListCommitKey, ScribeError> {
        let generation_id = generation.generation_id.0;
        let frozen = generation.frozen_snapshot();
        let reconciler = ScribePublicationReconciler::new(
            self.operator_pool
                .as_ref()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "writer-v2 publication requires the operator reconciler capability"
                        .to_owned(),
                })?
                .clone(),
            self.actor_stream,
            #[cfg(any(test, feature = "test-support"))]
            self.faults.clone(),
        );
        let (scratch, object_base, footer_reservation) =
            self.prepare_encoding(generation, binding, parquet_owner, generation_owned_bytes)?;
        tracing::debug!(generation_id, stage = "encode", "persist stage start");
        let mut encoded = self
            .encode_parquet_stage(
                &frozen,
                binding,
                scratch.path(),
                &object_base,
                footer_reservation,
            )
            .await?;
        encoded.artifacts.attach_scratch(scratch)?;
        encoded.audit_events =
            crate::scribe::audit_envelope::publication_audit_events(&encoded.audit_events);
        let source_node_id = generation.stream.node_id.to_string();
        let file_size = encoded
            .artifacts
            .iter()
            .try_fold(0_usize, |sum, artifact| {
                sum.checked_add(usize::try_from(artifact.file_size).ok()?)
            })
            .ok_or_else(|| ScribeError::Internal {
                detail: "writer-v2 artifact-set size overflows".to_owned(),
            })?;
        emit_compression_telemetry(generation.arrow_bytes, file_size);
        let uploaded = self
            .upload_artifacts(generation_id, &encoded.artifacts)
            .await?;
        let rows = file_list_writer::build_artifact_inserts(
            &frozen,
            &encoded,
            binding,
            &source_node_id,
            generation.stream.writer_epoch.as_i64(),
        )?;
        #[cfg(any(test, feature = "test-support"))]
        let fail_before_commit = self.faults.take_sql_commit();
        #[cfg(not(any(test, feature = "test-support")))]
        let fail_before_commit = false;
        tracing::debug!(generation_id, stage = "sql_commit", "persist stage start");
        let commit_started = std::time::Instant::now();
        if fail_before_commit {
            self.cleanup_uploaded(&uploaded).await;
            return Err(ScribeError::Internal {
                detail: "test SQL failure before the first COMMIT attempt".to_owned(),
            });
        }
        let outcome = match reconciler.publish(&rows, &encoded.audit_events).await {
            ScribePublicationOutcome::Committed(outcome) => outcome,
            ScribePublicationOutcome::UnknownCommitOutcome(_error) => {
                let retained_owner = parquet_owner.take().ok_or_else(|| ScribeError::Internal {
                    detail: "automatic producer reservation is unavailable for reconciliation"
                        .to_owned(),
                })?;
                return self
                    .await_ambiguous_publication(
                        generation,
                        binding,
                        defer_manifest_advance,
                        rows,
                        encoded,
                        retained_owner,
                    )
                    .await;
            }
            ScribePublicationOutcome::KnownNotCommitted(error) => return Err(error),
        };
        record_persist_stage("sql_commit", commit_started);
        self.publish_staging_hint(generation, binding);

        if defer_manifest_advance {
            tracing::debug!(
                generation_id,
                "replay defers manifest advance until reader exit"
            );
            encoded.artifacts.cleanup()?;
            return Ok(outcome.commit_key);
        }
        self.advance_manifest(generation).await?;
        encoded.artifacts.cleanup()?;
        tracing::debug!(generation_id, "persist stages complete");
        Ok(outcome.commit_key)
    }

    /// Publishes the best-effort local wake-up after the durable SQL commit.
    fn publish_staging_hint(&self, generation: &ImmutableGeneration, binding: &TenantTableBinding) {
        if let Some(publisher) = &self.staging_file_publisher {
            let event = crate::maintenance::StagingFileCommitted::new(
                binding.clone(),
                generation.seal_key.day.as_naive_date(),
            );
            let _ = publisher.try_publish(event);
        }
    }

    /// Reconciles one automatic publication under runtime ownership.
    ///
    /// The job retains exact rows, audit events, artifacts, immutable/WAL
    /// identity, and producer memory while retrying the fenced transaction.
    /// Caller or worker cancellation cannot interrupt this owner. Once the
    /// complete set is inserted or replay-validated, it advances the manifest
    /// exactly once unless replay explicitly deferred that step, publishes the
    /// local wake-up, and releases scratch and producer memory.
    ///
    /// # Errors
    /// Returns when manifest advancement or exact scratch cleanup fails after
    /// durable publication. SQL errors remain inside the bounded retry loop.
    async fn reconcile_persistence(
        &self,
        reconciler: &ScribePublicationReconciler,
        reconciliation: PersistenceReconciliation<'_>,
    ) -> Result<FileListCommitKey, ScribeError> {
        let PersistenceReconciliation {
            generation,
            binding,
            defer_manifest_advance,
            rows,
            events,
            artifacts,
            parquet_owner,
        } = reconciliation;
        let outcome = loop {
            if let ScribePublicationOutcome::Committed(outcome) =
                reconciler.publish(rows, events).await
            {
                break outcome;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        };
        if let Some(publisher) = &self.staging_file_publisher {
            let _ = publisher.try_publish(crate::maintenance::StagingFileCommitted::new(
                binding.clone(),
                generation.seal_key.day.as_naive_date(),
            ));
        }
        if !defer_manifest_advance {
            self.advance_manifest(generation).await?;
        }
        artifacts.cleanup()?;
        drop(parquet_owner);
        Ok(outcome.commit_key)
    }

    /// Advances the durable replay watermark for one published generation.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the injected publication fault fires, WAL
    /// IO submission fails, or the lane returns a result of the wrong kind.
    async fn advance_manifest(&self, generation: &ImmutableGeneration) -> Result<(), ScribeError> {
        let generation_id = generation.generation_id.0;
        let manifest_path =
            crate::scribe::replay::stream_directory(self.wal.base_dir(), generation.stream)
                .join("manifest");
        #[cfg(any(test, feature = "test-support"))]
        if self.faults.take_manifest_publication() {
            return Err(ScribeError::Internal {
                detail: "test manifest publication failure".to_owned(),
            });
        }
        tracing::debug!(
            generation_id,
            stage = "manifest_lock",
            "persist stage start"
        );
        let _manifest_guard = self.manifest_guard.lock().await;
        tracing::debug!(
            generation_id,
            stage = "manifest_advance",
            "persist stage start"
        );
        let result = self
            .wal_io
            .submit(ScribeWalIoOp::AdvanceManifest {
                path: manifest_path,
                stream: generation.stream,
                seal_key: generation.seal_key.clone(),
                sealed_lsn: generation.wal_lsn_max,
            })
            .await?;
        if !matches!(result, ScribeWalIoResult::Completed) {
            return Err(ScribeError::Internal {
                detail: "WAL IO lane returned the wrong manifest result".to_owned(),
            });
        }
        Ok(())
    }
}

/// Terminalizes one automatic visibility span from persistence and shard outcomes.
///
/// This narrow deterministic join keeps the production worker's outcome mapping
/// directly testable without reproducing persistence or shard state machines.
/// Success requires both durable persistence, mailbox delivery, and a successful
/// shard acknowledgment. A dropped acknowledgment is cancellation because the
/// shard outcome is unknown.
async fn finish_visibility_publication(
    mut visibility: super::seal::VisibilityPublishGuard,
    publication_succeeded: bool,
    completion_delivered: bool,
    visibility_result: oneshot::Receiver<Result<(), String>>,
) {
    if !publication_succeeded || !completion_delivered {
        visibility.fail();
        return;
    }
    match visibility_result.await {
        Ok(Ok(())) => visibility.succeed(),
        Ok(Err(_)) => visibility.fail(),
        Err(_) => visibility.cancel(),
    }
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
    async fn put_artifact(
        &self,
        artifact: &crate::scribe::parquet_writer::BoundedParquetArtifact,
    ) -> Result<(), ScribeError> {
        let mut last_error = None;
        for attempt in 0..5_u32 {
            let upload = async {
                use tokio::io::AsyncReadExt;

                let mut source = tokio::fs::File::open(&artifact.scratch_path)
                    .await
                    .map_err(|error| {
                        opendal::Error::new(
                            opendal::ErrorKind::Unexpected,
                            "open writer-v2 scratch artifact",
                        )
                        .set_source(error)
                    })?;
                let mut writer = self
                    .operator
                    .writer_with(&artifact.object_identity)
                    .chunk(8 * 1024 * 1024)
                    .await?;
                let mut chunk = vec![0_u8; 8 * 1024 * 1024];
                let mut uploaded = 0_u64;
                loop {
                    let read = source.read(&mut chunk).await.map_err(|error| {
                        opendal::Error::new(
                            opendal::ErrorKind::Unexpected,
                            "read writer-v2 scratch artifact",
                        )
                        .set_source(error)
                    })?;
                    if read == 0 {
                        break;
                    }
                    writer.write(Bytes::copy_from_slice(&chunk[..read])).await?;
                    uploaded = uploaded.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
                }
                let metadata = writer.close().await?;
                if uploaded != artifact.file_size || metadata.content_length() != artifact.file_size
                {
                    return Err(opendal::Error::new(
                        opendal::ErrorKind::Unexpected,
                        "writer-v2 upload length mismatch",
                    ));
                }
                Ok(())
            };
            match tokio::time::timeout(Duration::from_secs(30), upload).await {
                Ok(Ok(())) => return Ok(()),
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

    /// Deletes only positively known uploaded identities before any COMMIT attempt.
    async fn cleanup_uploaded(&self, paths: &[String]) {
        for path in paths {
            if let Err(error) = self.operator.delete(path).await {
                tracing::warn!(%error, path, "writer-v2 exact uploaded cleanup failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::panic::AssertUnwindSafe;
    use std::task::{Context, Poll};

    use crate::test_support::{SpanCaptureSubscriber, has_span_outcome};

    /// Builds a persistence owner with empty queues for finalizer-only tests.
    fn test_runtime() -> PersistenceRuntime {
        let (sender, _receiver) = mpsc::channel(1);
        PersistenceRuntime {
            sender: Arc::new(Mutex::new(Some(sender))),
            queued: Arc::new(AtomicUsize::new(0)),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            drained: Arc::new(Notify::new()),
            failures: Arc::new(Mutex::new(Vec::new())),
            tasks: Arc::new(Mutex::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(Vec::new())),
            output_scratch: None,
            reconciler: None,
            reconciliation_sender: None,
        }
    }

    /// Polls one visibility join without timing or runtime scheduling.
    fn poll_visibility(future: std::pin::Pin<&mut impl Future<Output = ()>>) -> Poll<()> {
        let waker = futures_util::task::noop_waker();
        future.poll(&mut Context::from_waker(&waker))
    }

    /// Automatic visibility remains open until the shard acknowledgment succeeds.
    #[test]
    fn automatic_visibility_waits_for_successful_shard_acknowledgment() {
        let subscriber = SpanCaptureSubscriber::default();
        let records = Arc::clone(&subscriber.records);
        let closed = Arc::clone(&subscriber.closed);
        tracing::subscriber::with_default(subscriber, || {
            let (sender, receiver) = oneshot::channel();
            let mut future = Box::pin(finish_visibility_publication(
                super::super::seal::VisibilityPublishGuard::new(),
                true,
                true,
                receiver,
            ));
            assert!(poll_visibility(future.as_mut()).is_pending());
            assert!(closed.lock().expect("closed spans").is_empty());
            sender.send(Ok(())).expect("shard success");
            assert!(poll_visibility(future.as_mut()).is_ready());
            drop(future);
        });
        assert!(has_span_outcome(
            &records,
            "bifrost.scribe.visibility.publish",
            "success"
        ));
    }

    /// Shard failure closes automatic visibility as failed.
    #[test]
    fn automatic_visibility_maps_shard_failure_to_failed() {
        let subscriber = SpanCaptureSubscriber::default();
        let records = Arc::clone(&subscriber.records);
        tracing::subscriber::with_default(subscriber, || {
            let (sender, receiver) = oneshot::channel();
            sender
                .send(Err("publication failed".to_owned()))
                .expect("shard failure");
            let mut future = Box::pin(finish_visibility_publication(
                super::super::seal::VisibilityPublishGuard::new(),
                true,
                true,
                receiver,
            ));
            assert!(poll_visibility(future.as_mut()).is_ready());
        });
        assert!(has_span_outcome(
            &records,
            "bifrost.scribe.visibility.publish",
            "failed"
        ));
    }

    /// Mailbox delivery failure closes automatic visibility as failed.
    #[test]
    fn automatic_visibility_maps_mailbox_failure_to_failed() {
        let subscriber = SpanCaptureSubscriber::default();
        let records = Arc::clone(&subscriber.records);
        tracing::subscriber::with_default(subscriber, || {
            let (_sender, receiver) = oneshot::channel();
            let mut future = Box::pin(finish_visibility_publication(
                super::super::seal::VisibilityPublishGuard::new(),
                true,
                false,
                receiver,
            ));
            assert!(poll_visibility(future.as_mut()).is_ready());
        });
        assert!(has_span_outcome(
            &records,
            "bifrost.scribe.visibility.publish",
            "failed"
        ));
    }

    /// A dropped shard acknowledgment closes automatic visibility as cancelled.
    #[test]
    fn automatic_visibility_maps_dropped_acknowledgment_to_cancelled() {
        let subscriber = SpanCaptureSubscriber::default();
        let records = Arc::clone(&subscriber.records);
        tracing::subscriber::with_default(subscriber, || {
            let (sender, receiver) = oneshot::channel::<Result<(), String>>();
            drop(sender);
            let mut future = Box::pin(finish_visibility_publication(
                super::super::seal::VisibilityPublishGuard::new(),
                true,
                true,
                receiver,
            ));
            assert!(poll_visibility(future.as_mut()).is_ready());
        });
        assert!(has_span_outcome(
            &records,
            "bifrost.scribe.visibility.publish",
            "cancelled"
        ));
    }

    /// Cancelling the pending join drops its armed guard as cancelled.
    #[test]
    fn automatic_visibility_maps_task_cancellation_to_cancelled() {
        let subscriber = SpanCaptureSubscriber::default();
        let records = Arc::clone(&subscriber.records);
        tracing::subscriber::with_default(subscriber, || {
            let (_sender, receiver) = oneshot::channel::<Result<(), String>>();
            let mut visibility = super::super::seal::VisibilityPublishGuard::new();
            visibility.arm_cancellation();
            let mut future = Box::pin(finish_visibility_publication(
                visibility, true, true, receiver,
            ));
            assert!(poll_visibility(future.as_mut()).is_pending());
            drop(future);
        });
        assert!(has_span_outcome(
            &records,
            "bifrost.scribe.visibility.publish",
            "cancelled"
        ));
    }

    /// Real minimal persistence owner and dependencies retained for one queue transition.
    #[cfg(feature = "test-support")]
    struct IdlePersistenceFixture {
        /// Runtime under causal telemetry test.
        runtime: Arc<PersistenceRuntime>,
        /// WAL handle used by the representative generation.
        wal: Arc<WalWriter>,
        /// Stream identity shared by runtime and generation.
        stream: StreamIdentity,
        /// Tenant that owns the representative generation.
        tenant: wyrd_spec::DataTenantId,
        /// Database retained until worker shutdown completes.
        _database: wyrd_dev_fixtures::pg::PgFixture,
        /// WAL directory retained until worker shutdown completes.
        _wal_root: tempfile::TempDir,
        /// Scratch namespace roots retained until worker shutdown completes.
        _scratch_root: tempfile::TempDir,
    }

    #[cfg(feature = "test-support")]
    impl IdlePersistenceFixture {
        /// Starts one real persistence runtime before it accepts work.
        async fn start() -> Self {
            let database = wyrd_dev_fixtures::pg::PgFixture::start()
                .await
                .expect("Postgres fixture");
            let tenant = database.data_tenant_id();
            let wal_root = tempfile::tempdir().expect("WAL directory");
            let scratch_root = tempfile::tempdir().expect("scratch directory");
            let scribe_output = scratch_root.path().join("scribe-output");
            let forge_scratch = scratch_root.path().join("forge");
            let oracle_scratch = scratch_root.path().join("oracle");
            for root in [&scribe_output, &forge_scratch, &oracle_scratch] {
                std::fs::create_dir(root).expect("test volume root");
            }
            let node_id = crate::scribe::stream_identity::NodeId::generate();
            let stream =
                StreamIdentity::new(node_id, crate::scribe::stream_identity::WriterEpoch::new(1));
            let superuser = database.superuser_pool().await.expect("superuser pool");
            sqlx::query(
                "INSERT INTO vala.cluster_nodes (data_tenant_id,node_id,role,advertise_addr,fencing_token,started_at,heartbeat_at) VALUES ($1,$2,'scribe','127.0.0.1:1',$3,now(),now()) ON CONFLICT (data_tenant_id,node_id,role) DO UPDATE SET fencing_token=EXCLUDED.fencing_token,heartbeat_at=now()",
            )
            .bind(wyrd_spec::DataTenantId::SYSTEM_OWNER.as_uuid())
            .bind(node_id.as_uuid())
            .bind(1_i64)
            .execute(&superuser)
            .await
            .expect("register Scribe publication fence");
            let wal = Arc::new(
                WalWriter::new(
                    wal_root.path(),
                    *node_id.as_bytes(),
                    1,
                    crate::scribe::wal::WalConfig::default(),
                )
                .expect("WAL writer"),
            );
            std::fs::create_dir_all(crate::scribe::replay::stream_directory(
                wal_root.path(),
                stream,
            ))
            .expect("WAL stream directory");
            let runtime_resources = crate::resources::BifrostRuntimeResources::from_snapshot(
                crate::resources::SystemResourceSnapshot {
                    memory_limit_bytes: 1024 * 1024 * 1024,
                    effective_cpu: 2,
                    scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
                    scratch_available_bytes: 2 * 1024 * 1024 * 1024,
                    memory_source: crate::resources::ResourceSource::Injected,
                    cpu_source: crate::resources::ResourceSource::Injected,
                },
                crate::resources::BifrostResourcePolicy {
                    roles: std::collections::BTreeSet::from([
                        crate::resources::BifrostRole::Scribe,
                    ]),
                    memory_limit_bytes: None,
                    unmanaged_reserve_bytes: None,
                    scratch_limit_bytes: None,
                    effective_cpu: None,
                    scratch_root: scratch_root.path().to_owned(),
                    volume_roots: Some(crate::resources::BifrostVolumeRoots {
                        wal: wal_root.path().to_owned(),
                        scribe_output_scratch: scribe_output,
                        forge_scratch,
                        oracle_scratch,
                    }),
                },
            )
            .expect("test Bifrost resources");
            let roles = runtime_resources
                .compose_roles()
                .expect("test role resources");
            let memory = roles.scribe().expect("test Scribe resources");
            let (_, output_scratch) = memory.volume_capabilities().expect("test Scribe volumes");
            let runtime = PersistenceRuntime::start(
                ScribePersistenceConfig::new(Arc::new(database.vala_postgres().clone()), 1, 1)
                    .with_operator_pool(database.operator_pool().clone())
                    .with_output_scratch(output_scratch),
                PersistenceRuntimeContext {
                    operator: Arc::new(
                        opendal::Operator::new(opendal::services::Memory::default())
                            .expect("memory operator")
                            .finish(),
                    ),
                    wal: Arc::clone(&wal),
                    persistence_cpu: ScribePersistenceCpuPool::new(1),
                    wal_io: ScribeWalIoPool::new(1),
                    actor_stream: stream,
                    memory,
                    staging_file_publisher: None,
                    faults: PersistenceFaults::default(),
                },
                &Handle::current(),
            );
            Self {
                runtime,
                wal,
                stream,
                tenant,
                _database: database,
                _wal_root: wal_root,
                _scratch_root: scratch_root,
            }
        }

        /// Submits one bounded generation and waits for its real worker completion.
        async fn submit_and_drain(&self) {
            let table = crate::catalog::TableRef::new(
                crate::namespaces::BifrostNamespace::Bifrost,
                "idle_queue",
            );
            let seal_key = SealKey::new(
                self.tenant,
                table.clone(),
                crate::scribe::seal_key::EventDay::new(
                    chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid test day"),
                ),
            );
            let binding =
                TenantTableBinding::resolve((self.tenant, table.clone())).expect("binding");
            let schema = Arc::new(arrow::datatypes::Schema::new(vec![
                arrow::datatypes::Field::new(
                    wyrd_spec::vala::managed_columns::DATA_TENANT_ID,
                    arrow::datatypes::DataType::Utf8,
                    false,
                ),
                arrow::datatypes::Field::new(
                    wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME,
                    arrow::datatypes::DataType::Timestamp(
                        arrow::datatypes::TimeUnit::Microsecond,
                        None,
                    ),
                    false,
                ),
                arrow::datatypes::Field::new("value", arrow::datatypes::DataType::Int64, false),
            ]));
            let rows = vec![
                arrow::record_batch::RecordBatch::try_new(
                    Arc::clone(&schema),
                    vec![
                        Arc::new(arrow::array::StringArray::from(vec![
                            self.tenant.to_string(),
                        ])),
                        Arc::new(arrow::array::TimestampMicrosecondArray::from(vec![
                            1_767_225_600_000_000_i64,
                        ])),
                        Arc::new(arrow::array::Int64Array::from(vec![1_i64])),
                    ],
                )
                .expect("valid persistence fixture batch"),
            ];
            let (completion_tx, mut completion_rx) = mpsc::channel(1);
            self.runtime
                .try_submit(PersistenceJob {
                    generation: Arc::new(ImmutableGeneration {
                        table_key: (self.tenant, table),
                        seal_key,
                        generation_id: GenerationId(1),
                        stream: self.stream,
                        wal_lsn_min: WalLsn::ZERO,
                        wal_lsn_max: WalLsn::ZERO,
                        wal_segments: Vec::new(),
                        wal: self.wal.handle_for_shard(0).expect("WAL handle"),
                        rows,
                        schema,
                        audit_events: Vec::new(),
                        append_metas: Vec::new(),
                        row_count: 1,
                        arrow_bytes: 1,
                        replay_identity: Mutex::new(None),
                        opened_at: std::time::Instant::now(),
                        closed_at: std::time::Instant::now(),
                        shard_id: 0,
                    }),
                    binding,
                    completion_tx,
                    completion_waiter: None,
                    defer_manifest_advance: false,
                })
                .expect("bounded persistence submission");
            let completion = completion_rx.recv().await.expect("persistence completion");
            let crate::scribe::shards::ShardCommand::PersistenceComplete {
                completion,
                visibility_result,
                ..
            } = completion
            else {
                panic!("worker must return one persistence completion");
            };
            assert!(
                completion.error.is_none(),
                "persistence completion failed before visibility acknowledgment: {:?}",
                completion.error
            );
            visibility_result
                .send(Ok(()))
                .expect("visibility acknowledgment");
            self.runtime.close();
            self.runtime.drain().await;
            self.runtime.abort_retained();
            self.runtime.clear_retained_join_handles();
        }
    }

    /// Persistence queue state is observable as zero before the runtime accepts work.
    #[tokio::test(flavor = "current_thread")]
    #[cfg(feature = "test-support")]
    async fn persistence_runtime_registers_idle_queue_gauges() {
        let recorder = wyrd_bench::BenchmarkRecorder::new();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let fixture = IdlePersistenceFixture::start().await;

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .gauges
                .keys()
                .filter(|series| series.starts_with("bifrost_scribe_persistence_queue_"))
                .count(),
            2
        );
        assert_eq!(
            snapshot
                .gauges
                .get("bifrost_scribe_persistence_queue_depth"),
            Some(&0.0)
        );
        assert_eq!(
            snapshot
                .gauges
                .get("bifrost_scribe_persistence_queue_bytes"),
            Some(&0.0)
        );

        fixture.submit_and_drain().await;

        let drained = recorder.snapshot();
        assert_eq!(
            drained.gauges.get("bifrost_scribe_persistence_queue_depth"),
            Some(&0.0)
        );
        assert_eq!(
            drained.gauges.get("bifrost_scribe_persistence_queue_bytes"),
            Some(&0.0)
        );
    }

    /// Per-generation compression telemetry is recorded after a durable persist.
    ///
    /// Drives one complete `persist_once` through the real persistence runtime
    /// using the [`IdlePersistenceFixture`] and asserts that:
    /// - `bifrost_scribe_persistence_compression_ratio` gauge is set and
    ///   positive (arrow bytes / encoded bytes > 0);
    /// - `bifrost_scribe_persistence_encoded_bytes_total` counter is
    ///   incremented by a positive amount (the Parquet-encoded size).
    #[tokio::test(flavor = "current_thread")]
    #[cfg(feature = "test-support")]
    async fn persist_once_emits_compression_telemetry() {
        let recorder = wyrd_bench::BenchmarkRecorder::new();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let fixture = IdlePersistenceFixture::start().await;
        fixture.submit_and_drain().await;

        let snapshot = recorder.snapshot();
        let ratio = snapshot
            .gauges
            .get("bifrost_scribe_persistence_compression_ratio");
        assert!(
            ratio.is_some(),
            "compression ratio gauge must be set after persist_once"
        );
        assert!(
            *ratio.expect("ratio is present") > 0.0,
            "compression ratio must be positive"
        );
        let encoded_bytes = snapshot
            .counters
            .get("bifrost_scribe_persistence_encoded_bytes_total");
        assert!(
            encoded_bytes.is_some(),
            "encoded bytes counter must be set after persist_once"
        );
        assert!(
            *encoded_bytes.expect("counter is present") > 0_u64,
            "encoded bytes must be positive"
        );
    }

    /// Compression telemetry pins the exact ratio orientation, the exact
    /// counter increment, and the zero-byte gauge skip in isolation.
    ///
    /// [`persist_once_emits_compression_telemetry`] proves the pair is wired
    /// into a real `persist_once`, but a fixed `arrow_bytes: 1` there cannot
    /// distinguish a correct `arrow / encoded` orientation from an inverted
    /// `encoded / arrow` one, nor a `file_size`-valued counter increment from
    /// a constant one. This test calls [`emit_compression_telemetry`]
    /// directly with values that make each of those regressions observable.
    #[test]
    fn emit_compression_telemetry_pins_formula_and_zero_edge() {
        let recorder = wyrd_bench::BenchmarkRecorder::new();
        let _guard = metrics::set_default_local_recorder(&recorder);

        emit_compression_telemetry(400, 100);
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .gauges
                .get("bifrost_scribe_persistence_compression_ratio"),
            Some(&4.0),
            "ratio must be arrow_bytes / file_size, not the inverse"
        );
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_persistence_encoded_bytes_total"),
            Some(&100_u64),
            "counter must increment by file_size, not a constant amount"
        );

        emit_compression_telemetry(400, 0);
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .gauges
                .get("bifrost_scribe_persistence_compression_ratio"),
            Some(&4.0),
            "gauge must be skipped (left unchanged) when file_size is zero"
        );
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_persistence_encoded_bytes_total"),
            Some(&100_u64),
            "counter must still increment by zero, leaving the total unchanged"
        );
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

    /// The persistence stage split emits one histogram per closed stage label.
    ///
    /// Proves [`record_persist_stage`] records into
    /// `bifrost_scribe_persist_stage_seconds{stage}` under the closed
    /// `{parquet, put, sql_commit}` set and carries no identity labels, so the
    /// split extends the end-to-end publication histogram without leaking tenant
    /// or table dimensions.
    #[test]
    fn persist_stage_histograms_split_by_stage() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            record_persist_stage("parquet", std::time::Instant::now());
            record_persist_stage("put", std::time::Instant::now());
            record_persist_stage("sql_commit", std::time::Instant::now());
        });
        let snapshot = recorder.snapshot();
        for stage in ["parquet", "put", "sql_commit"] {
            let key = format!("bifrost_scribe_persist_stage_seconds{{stage=\"{stage}\"}}");
            assert!(
                snapshot.histograms.contains_key(&key),
                "missing stage histogram {key}"
            );
        }
        // Match forbidden label *keys* (`name="`), not value substrings: the
        // legitimate `stage="sql_commit"` value contains "sql" but carries no
        // sql identity label.
        assert!(!snapshot.histograms.keys().any(|key| {
            ["tenant", "table", "path", "request", "node", "error", "sql"]
                .iter()
                .any(|forbidden| key.contains(&format!("{forbidden}=\"")))
        }));
    }
}
