//! Bounded immutable-generation persistence for Scribe.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(any(test, feature = "test-support"))]
use std::time::Duration;

use arrow::datatypes::SchemaRef;
use num_traits::ToPrimitive;
use sha2::{Digest, Sha256};
use tokio::runtime::Handle;
use tokio::sync::{Notify, mpsc, oneshot};
use vala_sql::ValaPostgres;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::{TenantTableBinding, TenantTableKey};
use crate::contracts::ScribeError;
use crate::maintenance::StagingFilePublisher;
use crate::parquet::object_uploader::{
    BifrostParquetUploader, BifrostUploadRole, ParquetObjectIdentity,
};
use crate::resources::{ScribeMemoryLease, ScribeResources};
use crate::scribe::execution_lanes::{
    ScribePersistenceCpuOp, ScribePersistenceCpuPool, ScribePersistenceCpuResult, ScribeWalIoOp,
    ScribeWalIoPool, ScribeWalIoResult,
};
use crate::scribe::file_list_writer::{self, FileListCommitKey};
use crate::scribe::memory::{
    MemoryCategory, PARQUET_TRANSFER_BUFFER_BYTES, parquet_candidate_incremental_bytes,
};
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::BoundedParquetArtifactSet;
use crate::scribe::seal_key::SealKey;
use crate::scribe::staging::{
    RecoveredPublication, ScribeStaging, StageElection, StagedArtifactClaim,
};
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::telemetry::{
    ProducerLifecycleEvent, ProducerLifecycleIdentity, record_producer_lifecycle,
};
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
    object_write_failure_countdown: Arc<AtomicUsize>,
    sql_commit: Arc<std::sync::atomic::AtomicBool>,
    post_commit_client_error: Arc<std::sync::atomic::AtomicBool>,
    manifest_publication: Arc<std::sync::atomic::AtomicBool>,
    object_write_delay_ms: Arc<AtomicU64>,
    object_write_delays_ms: Arc<Mutex<Vec<u64>>>,
    object_write_active: Arc<AtomicUsize>,
    max_object_write_active: Arc<AtomicUsize>,
    encode_delay_ms: Arc<AtomicU64>,
    encode_active: Arc<AtomicUsize>,
    max_encode_active: Arc<AtomicUsize>,
    last_error: Arc<Mutex<Option<String>>>,
    /// Next real writer-v2 publication paused on both sides of fenced SQL visibility.
    publication_barrier:
        Arc<Mutex<Option<crate::scribe::file_list_writer::PublicationFenceBarrier>>>,
}

#[cfg(any(test, feature = "test-support"))]
impl PersistenceFaults {
    /// Installs a two-phase barrier for the next actual writer-v2 publication.
    ///
    /// The selected publication pauses only after its durable local manifest
    /// exists, then again after its file-list transaction commits but before
    /// any local immutable cleanup or retirement can run.
    pub fn pause_next_publication(
        &self,
        barrier: crate::scribe::file_list_writer::PublicationFenceBarrier,
    ) {
        if let Ok(mut selected) = self.publication_barrier.lock() {
            *selected = Some(barrier);
        }
    }

    /// Fail the next object-store write before it mutates storage.
    pub fn fail_next_object_write(&self) {
        self.object_write.store(true, Ordering::Release);
    }

    /// Fails the one-based candidate object-write attempt selected by a test.
    pub fn fail_object_write_attempt_for_test(&self, attempt: usize) {
        self.object_write_failure_countdown
            .store(attempt, Ordering::Release);
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

    /// Delays entered encode intervals so tests can prove real overlap.
    pub fn set_encode_delay_for_test(&self, delay: Duration) {
        self.encode_delay_ms.store(
            delay.as_millis().try_into().unwrap_or(u64::MAX),
            Ordering::Release,
        );
    }

    /// Returns the maximum number of Parquet encode intervals active together.
    #[must_use]
    pub fn max_concurrent_encodes_for_test(&self) -> usize {
        self.max_encode_active.load(Ordering::Acquire)
    }

    /// Enters the instrumented interval that owns an actual encode submission.
    async fn begin_encode(&self) -> EncodeGuard {
        let active = self.encode_active.fetch_add(1, Ordering::AcqRel) + 1;
        self.max_encode_active.fetch_max(active, Ordering::AcqRel);
        let delay = self.encode_delay_ms.load(Ordering::Acquire);
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        EncodeGuard {
            active: Arc::clone(&self.encode_active),
        }
    }

    /// Return the most recent persistence error observed by a test fixture.
    #[must_use]
    pub fn last_error_for_test(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|value| value.clone())
    }

    /// Takes the one publication barrier assigned to the next reconciler call.
    fn take_publication_barrier(
        &self,
        rows: &[file_list_writer::FileListArtifactInsert],
    ) -> Option<crate::scribe::file_list_writer::PublicationFenceBarrier> {
        let mut selected = self.publication_barrier.lock().ok()?;
        selected
            .as_ref()
            .is_some_and(|barrier| barrier.matches(rows))
            .then(|| selected.take())
            .flatten()
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
        if self.object_write.swap(false, Ordering::AcqRel) {
            return true;
        }
        self.object_write_failure_countdown
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                if remaining > 0 {
                    Some(remaining - 1)
                } else {
                    None
                }
            })
            .is_ok_and(|remaining| remaining == 1)
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

/// Test observation guard spanning one actual Parquet encode submission.
#[cfg(any(test, feature = "test-support"))]
struct EncodeGuard {
    /// Shared live encode count decremented at interval exit.
    active: Arc<AtomicUsize>,
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for EncodeGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
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
    /// Durable staged member identity when staging succeeded.
    ///
    /// This, not a file-list key, is what the owning shard retires against: the
    /// member's rows survive and are servable without the WAL behind them long
    /// before the claim that publishes them is due.
    pub(crate) staged: Option<crate::scribe::assembly::StagedMemberId>,
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
    /// Validated geometry fixing the hot-object target and member dwell.
    pub(crate) geometry: crate::scribe::geometry::ScribeGeometry,
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
    /// Generation-owned output scratch authority shared by direct and queued producers.
    output_scratch: Option<Arc<crate::resources::ScratchVolume>>,
    /// Operator-backed owner for caller-commit ambiguity reconciliation.
    reconciler: Option<ScribePublicationReconciler>,
    /// Bounded ownership-transfer seam for already-admitted ambiguous attempts.
    reconciliation_sender: Option<mpsc::Sender<ScribeReconciliationJob>>,
    /// Arrival-ordered exact-byte producer admission shared by every worker.
    producer_admission: Option<ProducerAdmission>,
    /// WAL-root stage owner used for pre-readiness publication recovery.
    recovery_wal: Option<Arc<WalWriter>>,
    /// Durable object store used for pre-readiness upload convergence.
    recovery_operator: Option<opendal::Operator>,
    /// Root owner charged before allocating recovery transfer buffers.
    recovery_memory: Option<ScribeResources>,
    /// The shared worker retained so drain can publish residue after the queue empties.
    ///
    /// `None` only for the workerless test constructors, which have no staged
    /// members to settle.
    worker: Option<Arc<PersistenceWorker>>,
}

/// Reason a non-blocking persistence submission retained ownership of its job.
#[derive(Debug)]
pub(crate) enum PersistenceSubmitError {
    /// The bounded queue is temporarily full and worker progress can make room.
    Full(Box<PersistenceJob>),
    /// The persistence runtime no longer accepts jobs.
    Closed(Box<PersistenceJob>),
}

/// Runtime-owned ambiguous caller publication and its completion waiter.
struct ScribeReconciliationJob {
    /// Complete move-only attempt retained across SQL retries.
    attempt: Box<super::seal::ScribeCommitAttempt>,
    /// Caller notification; dropping the receiver does not cancel reconciliation.
    completion: oneshot::Sender<Result<(), String>>,
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
            tasks: Arc::new(Mutex::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(Vec::new())),
            output_scratch: None,
            reconciler: None,
            reconciliation_sender: None,
            producer_admission: None,
            recovery_wal: None,
            recovery_operator: None,
            recovery_memory: None,
            worker: None,
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
            tasks: Arc::new(Mutex::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(Vec::new())),
            output_scratch: None,
            reconciler: None,
            reconciliation_sender: None,
            producer_admission: None,
            recovery_wal: None,
            recovery_operator: None,
            recovery_memory: None,
            worker: None,
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
                let ScribeReconciliationJob {
                    attempt,
                    completion,
                } = job;
                let result = attempt
                    .reconcile(&reconciler)
                    .await
                    .map_err(|error| error.to_string());
                let _ = completion.send(result);
            }
        })
    }

    /// Builds the pod's staged-member lifecycle owner, when it can exist.
    ///
    /// Returns `None` when this pod was provisioned without a durable staging
    /// volume or without the operator capability the fenced publication
    /// transaction requires. Both are startup provisioning facts, not runtime
    /// conditions, so refusing here keeps a worker from discovering halfway
    /// through a generation that it cannot make its rows durable.
    ///
    /// The claim budget is the worker count because a worker drives at most one
    /// claim at a time: a budget above that would hand out claims nothing is
    /// free to merge, and a budget below it would idle a worker that has work.
    fn build_staging(
        context: &PersistenceRuntimeContext,
        operator_pool: Option<&vala_sql::OperatorPool>,
        workers: usize,
    ) -> Option<Arc<crate::scribe::staging_runtime::ScribeStagingRuntime>> {
        let volume = context.memory.stage_volume()?;
        let operator_pool = operator_pool?.clone();
        let config = crate::scribe::assembly::StagingAssemblerConfig::new(
            context.geometry.staging_target_file_size_bytes(),
            context.geometry.generation_max_age(),
            workers.max(1),
        )
        .ok()?;
        let stage = Arc::new(crate::scribe::hot_stage::ScribeHotStage::new(
            volume.root().ok()?.to_path_buf(),
        ));
        let publisher = crate::scribe::claim_publication::ClaimPublisher::new(
            Arc::clone(&stage),
            ScribeStageMover::new(Arc::clone(&context.wal), (*context.operator).clone()),
            ScribePublicationReconciler::new(
                operator_pool,
                context.actor_stream,
                #[cfg(any(test, feature = "test-support"))]
                context.faults.clone(),
            ),
        );
        Some(Arc::new(
            crate::scribe::staging_runtime::ScribeStagingRuntime::new(
                stage, volume, publisher, config,
            ),
        ))
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
        let producer_admission = ProducerAdmission::new(context.memory.clone());
        let recovery_wal = Arc::clone(&context.wal);
        let recovery_operator = (*context.operator).clone();
        let recovery_memory = context.memory.clone();
        let failures = Arc::new(Mutex::new(Vec::new()));
        let receiver = Arc::new(tokio::sync::Mutex::new(receiver));
        let staging = Self::build_staging(&context, config.operator_pool.as_ref(), config.workers);
        let worker = Arc::new(PersistenceWorker::new(
            config.operator_pool,
            Arc::clone(&failures),
            context,
            output_scratch.clone(),
            producer_admission.clone(),
            staging,
        ));
        let runtime_state = Arc::new(Self {
            sender: Arc::new(Mutex::new(Some(sender))),
            queued: Arc::new(AtomicUsize::new(0)),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            drained: Arc::new(Notify::new()),
            tasks: Arc::new(Mutex::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(Vec::new())),
            output_scratch,
            reconciler,
            reconciliation_sender: Some(reconciliation_sender),
            producer_admission: Some(producer_admission),
            recovery_wal: Some(recovery_wal),
            recovery_operator: Some(recovery_operator),
            recovery_memory: Some(recovery_memory),
            worker: Some(Arc::clone(&worker)),
        });
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
        if let Some(admission) = &self.producer_admission {
            admission.close();
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

    /// Settles every staged member that no future arrival will complete.
    ///
    /// Call this after [`Self::drain`], when every accepted generation is
    /// already durable on the staging volume: the sweep publishes the ready
    /// members that target and dwell would otherwise keep waiting for. A pod
    /// without staged members, or one provisioned without staging at all,
    /// publishes nothing and reports zero.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a residue claim cannot be taken, merged, or
    /// published. Members that did not publish stay durable and staged, and the
    /// WAL stays authoritative for their rows.
    ///
    /// # Cancellation
    ///
    /// Dropping this future stops the sweep between claims. Published claims
    /// stay published and the rest stay staged for the next sweep or restart.
    pub(crate) async fn publish_residue(
        &self,
        cause: crate::scribe::assembly::ClaimCause,
    ) -> Result<usize, ScribeError> {
        let Some(worker) = &self.worker else {
            return Ok(0);
        };
        if worker.staging.is_none() {
            return Ok(0);
        }
        worker.publish_residue(cause).await
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

    /// Rebuilds the staged ready and claim indexes from the durable volume.
    ///
    /// Runs before admission opens, so the members that survived the previous
    /// process are owned again before any new one is staged over them. A pod
    /// without staging restores nothing and reports zero.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the pod has staging but no
    /// control pool to re-resolve write recipes with, and propagates the
    /// staged namespace's fail-closed recovery refusals unchanged. Startup
    /// stops rather than opening admission over evidence it cannot vouch for.
    pub(crate) async fn restore_staging(&self) -> Result<usize, ScribeError> {
        let Some(worker) = &self.worker else {
            return Ok(0);
        };
        let Some(staging) = &worker.staging else {
            return Ok(0);
        };
        let pool = worker
            .operator_pool
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe staged recovery has no control pool for write recipes".to_owned(),
            })?;
        staging.restore(pool.pool()).await
    }

    /// Reconciles durable staged publications before WAL replay opens readiness.
    ///
    /// # Errors
    ///
    /// Returns an internal error when runtime recovery dependencies are absent
    /// or any manifest/upload/fenced publication/cleanup step fails closed.
    pub(crate) async fn recover_staged_publications(&self) -> Result<usize, ScribeError> {
        let wal = self
            .recovery_wal
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe staged recovery has no WAL owner".to_owned(),
            })?;
        let operator = self
            .recovery_operator
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe staged recovery has no object store".to_owned(),
            })?;
        let reconciler = self
            .reconciler
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe staged recovery has no fenced reconciler".to_owned(),
            })?;
        let memory = self
            .recovery_memory
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe staged recovery has no root memory owner".to_owned(),
            })?;
        ScribeStageMover::new(Arc::clone(wal), operator.clone())
            .recover_publications(reconciler, memory)
            .await
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
            .try_send(ScribeReconciliationJob {
                attempt: Box::new(attempt),
                completion,
            })
            .map_err(|error| error.into_inner().attempt)?;
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
    /// Test-only fault points for deterministic persistence-path coverage.
    #[cfg(any(test, feature = "test-support"))]
    faults: PersistenceFaults,
    /// Serializes manifest publication after SQL commit.
    manifest_guard: Arc<tokio::sync::Mutex<()>>,
    /// Fair exact-byte admission owner shared by live and replay persistence.
    producer_admission: ProducerAdmission,
    /// Pod-level owner of staged members and their assembly claims.
    ///
    /// `None` only when this pod was provisioned without a staging volume or
    /// without the operator capability publication requires; such a worker
    /// refuses durable work rather than persisting through a second path.
    staging: Option<Arc<crate::scribe::staging_runtime::ScribeStagingRuntime>>,
}

/// What one settled generation left behind on the staging volume.
///
/// A generation completes at the staged boundary, not at publication: its
/// member is durable and servable, and the claims listed here are the ones that
/// happened to become due while it settled. An empty list is the normal steady
/// state for a key that is still filling toward its object target.
#[derive(Debug)]
pub(crate) struct StagedGenerationOutcome {
    /// Identity of the durable member this generation became.
    pub(crate) member: crate::scribe::assembly::StagedMemberId,
    /// Commit keys of every claim published while this generation settled.
    pub(crate) published: Vec<FileListCommitKey>,
}

/// Estimates the Arrow bytes one claim's bounded merge holds at once.
///
/// The merge decodes one bounded batch per member rather than a whole member,
/// so the footprint is derived from what the members measured: their encoded
/// bytes per row, scaled by the bounded row window each member contributes.
/// [`parquet_candidate_incremental_bytes`] then adds the writer's own footer
/// and transfer children on top of it.
fn claim_merge_bytes(claim: &crate::scribe::assembly::StagingClaim) -> usize {
    let rows = claim.rows().max(1);
    let bytes_per_row = claim.encoded_bytes().div_ceil(rows);
    let window = crate::scribe::claim_assembly::MERGE_BATCH_ROWS as u64;
    let members = claim.members().len() as u64;
    usize::try_from(bytes_per_row.saturating_mul(window).saturating_mul(members))
        .unwrap_or(usize::MAX)
}

/// Separate Scribe mover that uploads finalized stages but owns no catalog decision.
pub struct ScribeStageMover {
    /// Durable local namespace that owns election and crash-recovery manifests.
    staging: ScribeStaging,
    /// Shared bounded uploader that converges deterministic remote content.
    uploader: BifrostParquetUploader,
}

impl ScribeStageMover {
    /// Builds the mover over the WAL-root stage namespace and durable object store.
    #[must_use]
    pub fn new(wal: Arc<WalWriter>, operator: opendal::Operator) -> Self {
        Self {
            staging: ScribeStaging::new(wal),
            uploader: BifrostParquetUploader::new(operator),
        }
    }

    /// Finalizes, elects, and verifies each artifact while retaining every local winner.
    ///
    /// # Errors
    ///
    /// Returns an internal error for staging, manifest, identity, or upload failure.
    ///
    /// # Cancellation
    ///
    /// Cancellation may leave elected local stages, a publication manifest, or
    /// verified remote objects. Those artifacts are intentionally retained so
    /// startup recovery can converge the same deterministic publication.
    pub(crate) async fn stage_and_upload_candidate(
        &self,
        object_base: &str,
        artifacts: &BoundedParquetArtifactSet,
        chunk: &mut [u8],
    ) -> Result<Vec<StagedArtifactClaim>, ScribeError> {
        let mut claims = Vec::with_capacity(artifacts.len());
        let publication_identity = Self::publication_identity(object_base);
        for artifact in artifacts {
            let identity =
                ParquetObjectIdentity::new(artifact.object_identity.clone()).map_err(|error| {
                    ScribeError::Internal {
                        detail: format!("invalid deterministic Scribe object identity: {error}"),
                    }
                })?;
            let mut sha256 = [0_u8; 32];
            hex::decode_to_slice(&artifact.checksum, &mut sha256).map_err(|error| {
                ScribeError::Internal {
                    detail: format!("invalid staged Parquet SHA-256: {error}"),
                }
            })?;
            let logical_identity = format!("{publication_identity}-artifact-{}", artifact.ordinal);
            let election = self
                .staging
                .stage_and_elect(
                    &logical_identity,
                    &artifact.scratch_path,
                    &identity,
                    sha256,
                    artifact.file_size,
                    chunk,
                )
                .await?;
            let claim = match election {
                StageElection::Winner(claim) | StageElection::ExistingWinner(claim) => claim,
            };
            claims.push(claim);
        }
        for claim in &claims {
            let identity =
                ParquetObjectIdentity::new(claim.object_key.clone()).map_err(|error| {
                    ScribeError::Internal {
                        detail: format!("invalid recovered Scribe object identity: {error}"),
                    }
                })?;
            self.uploader
                .upload_file_verified(
                    &identity,
                    &claim.staged_path,
                    claim.sha256,
                    claim.length,
                    chunk,
                    BifrostUploadRole::Scribe,
                )
                .await
                .map_err(|error| ScribeError::Internal {
                    detail: format!("verified staged upload failed: {error}"),
                })?;
        }
        Ok(claims)
    }

    /// Persists one complete member manifest after every candidate converges.
    ///
    /// # Errors
    ///
    /// Returns an internal error when the complete rows, audit transition, or
    /// durable stage claims contradict their deterministic generation identity.
    pub(crate) async fn persist_publication(
        &self,
        object_base: &str,
        actor_stream: StreamIdentity,
        rows: &[file_list_writer::FileListArtifactInsert],
        audit_events: &[AuditEvent],
        claims: &[StagedArtifactClaim],
    ) -> Result<(), ScribeError> {
        self.staging
            .persist_publication(
                &Self::publication_identity(object_base),
                actor_stream,
                rows,
                audit_events,
                claims,
            )
            .await
    }

    /// Derives the stable local publication-manifest identity for one member.
    fn publication_identity(object_base: &str) -> String {
        format!(
            "publication-{}",
            hex::encode(Sha256::digest(object_base.as_bytes()))
        )
    }

    /// Removes finalized stages only after committed or already-identical publication.
    ///
    /// # Errors
    ///
    /// Returns an internal error when exact local cleanup cannot be fsynced.
    ///
    /// # Cancellation
    ///
    /// Cancellation may clean only a prefix of the claims. Exact cleanup is
    /// idempotent, and the publication manifest remains until every claim has
    /// been processed.
    pub(crate) async fn cleanup_published(
        &self,
        claims: &[StagedArtifactClaim],
    ) -> Result<(), ScribeError> {
        let publication_identity = claims
            .first()
            .and_then(|claim| {
                claim
                    .logical_identity
                    .rsplit_once("-artifact-")
                    .map(|value| value.0)
            })
            .ok_or_else(|| ScribeError::Internal {
                detail: "published Scribe claims lack generation identity".to_owned(),
            })?;
        for claim in claims {
            self.staging.cleanup_published(claim).await?;
        }
        self.staging.cleanup_publication(publication_identity).await
    }

    /// Reconciles all durable publication manifests before WAL replay/readiness.
    ///
    /// # Errors
    ///
    /// Returns an internal error when manifest validation, remote verification,
    /// fenced SQL/audit convergence, or local post-publication cleanup fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation stops at the current publication. Already committed
    /// publications remain valid and uncleaned manifests are retried on startup.
    async fn recover_publications(
        &self,
        reconciler: &ScribePublicationReconciler,
        memory: &ScribeResources,
    ) -> Result<usize, ScribeError> {
        // Namespace-wide orphan classification and winner completion belong
        // only to startup recovery. Live workers perform per-identity election
        // so they cannot race recovery against another active attempt.
        let transfer_bytes = PARQUET_TRANSFER_BUFFER_BYTES
            .checked_mul(2)
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe recovery transfer reservation overflowed".to_owned(),
            })?;
        let _transfer_owner =
            memory.try_reserve_maintenance(MemoryCategory::Persistence, transfer_bytes)?;
        let mut recovered = 0_usize;
        let mut chunk = vec![0_u8; PARQUET_TRANSFER_BUFFER_BYTES];
        let _recovered_stages = self.staging.recover(&mut chunk).await?;
        let publications = self.staging.recover_publications(&mut chunk).await?;
        for publication in publications {
            self.recover_publication(reconciler, publication, &mut chunk)
                .await?;
            recovered = recovered
                .checked_add(1)
                .ok_or_else(|| ScribeError::Internal {
                    detail: "recovered Scribe publication count overflow".to_owned(),
                })?;
        }
        Ok(recovered)
    }

    /// Replays one validated durable publication in upload/catalog/cleanup order.
    ///
    /// # Errors
    ///
    /// Returns an internal error when object identity or verified upload fails,
    /// when fenced SQL/audit publication is not known committed, or when exact
    /// post-commit cleanup cannot complete.
    ///
    /// # Cancellation
    ///
    /// Cancellation can occur after remote convergence or an ambiguous commit.
    /// The durable publication manifest and staged files remain authoritative for
    /// a later retry; cleanup begins only after a confirmed commit.
    async fn recover_publication(
        &self,
        reconciler: &ScribePublicationReconciler,
        publication: RecoveredPublication,
        chunk: &mut [u8],
    ) -> Result<(), ScribeError> {
        tracing::debug!(
            source_stream = %publication.actor_stream,
            recovery_actor = %reconciler.actor_stream,
            publication = %publication.logical_identity,
            "reconciling staged publication under the active replacement fence"
        );
        for claim in &publication.claims {
            let identity =
                ParquetObjectIdentity::new(claim.object_key.clone()).map_err(|error| {
                    ScribeError::Internal {
                        detail: format!("invalid recovered Scribe object identity: {error}"),
                    }
                })?;
            self.uploader
                .upload_file_verified(
                    &identity,
                    &claim.staged_path,
                    claim.sha256,
                    claim.length,
                    chunk,
                    BifrostUploadRole::Scribe,
                )
                .await
                .map_err(|error| ScribeError::Internal {
                    detail: format!("recovered staged upload failed: {error}"),
                })?;
        }
        match reconciler
            .publish(&publication.rows, &publication.audit_events)
            .await
        {
            ScribePublicationOutcome::Committed(_) => {
                self.cleanup_published(&publication.claims).await
            }
            ScribePublicationOutcome::KnownNotCommitted(error)
            | ScribePublicationOutcome::UnknownCommitOutcome(error) => Err(error),
        }
    }
}

/// Arrival-ordered waiter metadata; root leases remain the only byte ledger.
#[derive(Debug, Clone, Copy)]
struct ProducerWaiter {
    /// Monotonic identity used for cancellation-safe removal.
    id: u64,
    /// Exact workspace requested from the root when this waiter reaches head.
    workspace_bytes: usize,
}

/// Mutable queue state protected independently from root resource accounting.
#[derive(Debug, Default)]
struct ProducerAdmissionState {
    /// FIFO of accepted waiters.
    waiters: VecDeque<ProducerWaiter>,
}

/// Dependency-owning FIFO admission for concurrent byte-weighted producers.
#[derive(Debug, Clone)]
pub struct ProducerAdmission {
    /// Sole authority for exact persistence byte ownership.
    resources: ScribeResources,
    /// Arrival-ordered waiter queue containing no aggregate byte state.
    state: Arc<Mutex<ProducerAdmissionState>>,
    /// Wakes the new head after grants, cancellation, or shutdown.
    changed: Arc<Notify>,
    /// Rejects new waiters and releases queued waiters during shutdown.
    closed: Arc<AtomicBool>,
    /// Process-local monotonic waiter identity.
    next_id: Arc<AtomicU64>,
}

impl ProducerAdmission {
    /// Creates an open admission owner over the authoritative root capability.
    #[must_use]
    pub(crate) fn new(resources: ScribeResources) -> Self {
        Self {
            resources,
            state: Arc::new(Mutex::new(ProducerAdmissionState::default())),
            changed: Arc::new(Notify::new()),
            closed: Arc::new(AtomicBool::new(false)),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Acquires the exact root workspace once this waiter reaches queue head.
    ///
    /// Only the head attempts root admission. Temporary occupation waits on the
    /// root epoch; later arrivals therefore cannot starve an older large request.
    /// Dropping this future removes its waiter through `ProducerWaiterGuard`.
    ///
    /// # Errors
    ///
    /// Returns an internal error after shutdown or poisoned queue state, and
    /// propagates non-capacity root failures unchanged.
    pub async fn acquire(&self, workspace_bytes: usize) -> Result<ScribeMemoryLease, ScribeError> {
        self.acquire_inner(workspace_bytes, false, None).await
    }

    /// Acquires for a job accepted before runtime shutdown began.
    ///
    /// Graceful drain closes admission before its residue claims publish, but
    /// every member in those claims was admitted while the runtime was open and
    /// is already durable. Refusing their merge workspace would strand durable
    /// rows on the staging volume, so this admission ignores the closed flag
    /// while still queueing behind the same fair waiter order.
    ///
    /// # Errors
    /// Returns root admission failures or poisoned queue state.
    pub(crate) async fn acquire_accepted(
        &self,
        workspace_bytes: usize,
    ) -> Result<ScribeMemoryLease, ScribeError> {
        self.acquire_inner(workspace_bytes, true, None).await
    }

    /// Acquires production work while retaining its immutable producer identity.
    ///
    /// # Errors
    ///
    /// Returns root admission failures or poisoned queue state. Every emitted
    /// lifecycle event carries `identity` as tracing fields, never metric labels.
    pub(crate) async fn acquire_with_identity(
        &self,
        workspace_bytes: usize,
        identity: ProducerLifecycleIdentity,
    ) -> Result<ScribeMemoryLease, ScribeError> {
        self.acquire_inner(workspace_bytes, false, Some(identity))
            .await
    }

    /// Implements FIFO admission for new or already-accepted work.
    ///
    /// # Errors
    ///
    /// Returns an internal error when admission is closed, queue state is
    /// poisoned, or memory-change notification fails, and propagates non-capacity
    /// root reservation failures.
    ///
    /// # Cancellation
    ///
    /// Dropping a waiting future removes its waiter through the guard without
    /// acquiring memory. Once a lease is returned, its owner controls release.
    async fn acquire_inner(
        &self,
        workspace_bytes: usize,
        accepted: bool,
        identity: Option<ProducerLifecycleIdentity>,
    ) -> Result<ScribeMemoryLease, ScribeError> {
        self.ensure_open(accepted, workspace_bytes, identity.as_ref())?;
        let (id, queue_position) = match self.enqueue(workspace_bytes, accepted) {
            Ok(waiter) => waiter,
            Err(error) => {
                self.record_refusal(
                    workspace_bytes,
                    self.queue_len(),
                    identity.as_ref(),
                    "closed",
                    "runtime_shutdown",
                );
                return Err(error);
            }
        };
        let mut guard =
            ProducerWaiterGuard::new(self, id, workspace_bytes, queue_position, identity);
        let mut waited = false;
        loop {
            if !accepted && self.closed.load(Ordering::Acquire) {
                guard.refuse("closed", "runtime_shutdown");
                return Err(Self::closed_error());
            }
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let is_head = match self.waiter_is_head(id, workspace_bytes) {
                Ok(is_head) => is_head,
                Err(error) => {
                    guard.refuse("refused", "queue_state_error");
                    return Err(error);
                }
            };
            if !is_head {
                waited = true;
                notified.await;
                continue;
            }
            let epoch = self.resources.memory_epoch();
            match self.resources.try_reserve_maintenance(
                crate::scribe::memory::MemoryCategory::Persistence,
                workspace_bytes,
            ) {
                Ok(lease) => {
                    record_producer_lifecycle(
                        ProducerLifecycleEvent {
                            workspace_bytes,
                            queue_position,
                            queue_count: self.queue_len(),
                            resource_epoch: epoch,
                            operation: "admission",
                            stage: "grant",
                            outcome: "granted",
                            reason: if waited { "waited" } else { "immediate" },
                        },
                        guard.identity(),
                    );
                    guard.settle();
                    return Ok(lease);
                }
                Err(ScribeError::IngestBusy { .. }) => {
                    waited = true;
                    record_producer_lifecycle(
                        ProducerLifecycleEvent {
                            workspace_bytes,
                            queue_position,
                            queue_count: self.queue_len(),
                            resource_epoch: epoch,
                            operation: "admission",
                            stage: "wait",
                            outcome: "pending",
                            reason: "occupied",
                        },
                        guard.identity(),
                    );
                    tokio::select! {
                        result = self.resources.wait_for_memory_change(epoch) => {
                            if let Err(error) = result {
                                guard.refuse("refused", "memory_change_error");
                                return Err(ScribeError::Internal { detail: error.to_string() });
                            }
                        }
                        () = notified => {}
                    }
                }
                Err(error) => {
                    guard.refuse("refused", "root_error");
                    return Err(error);
                }
            }
        }
    }

    /// Rejects new work that arrives after producer admission closes.
    ///
    /// # Errors
    ///
    /// Returns the stable runtime-shutdown error after emitting an
    /// identity-bearing refusal; already-accepted work remains eligible.
    fn ensure_open(
        &self,
        accepted: bool,
        workspace_bytes: usize,
        identity: Option<&ProducerLifecycleIdentity>,
    ) -> Result<(), ScribeError> {
        if accepted || !self.closed.load(Ordering::Acquire) {
            return Ok(());
        }
        self.record_refusal(
            workspace_bytes,
            self.queue_len(),
            identity,
            "closed",
            "runtime_shutdown",
        );
        Err(Self::closed_error())
    }

    /// Reports whether one exact waiter currently owns the FIFO queue head.
    ///
    /// # Errors
    ///
    /// Returns an internal Scribe error when the admission queue is poisoned.
    fn waiter_is_head(&self, id: u64, workspace_bytes: usize) -> Result<bool, ScribeError> {
        let state = self.state.lock().map_err(|_| ScribeError::Internal {
            detail: "Scribe producer admission queue is poisoned".to_owned(),
        })?;
        Ok(state
            .waiters
            .front()
            .is_some_and(|waiter| waiter.id == id && waiter.workspace_bytes == workspace_bytes))
    }

    /// Appends one waiter after rechecking shutdown under the queue lock.
    ///
    /// # Errors
    ///
    /// Returns an internal error when the queue lock is poisoned or new work
    /// races with admission closure.
    fn enqueue(&self, workspace_bytes: usize, accepted: bool) -> Result<(u64, usize), ScribeError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut state = self.state.lock().map_err(|_| ScribeError::Internal {
            detail: "Scribe producer admission queue is poisoned".to_owned(),
        })?;
        if !accepted && self.closed.load(Ordering::Acquire) {
            return Err(Self::closed_error());
        }
        let position = state.waiters.len();
        state.waiters.push_back(ProducerWaiter {
            id,
            workspace_bytes,
        });
        Ok((id, position))
    }

    /// Closes admission and wakes every queued future to observe shutdown.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }

    /// Returns the current waiter count for bounded telemetry and tests.
    pub(crate) fn queue_len(&self) -> usize {
        self.state.lock().map_or(0, |state| state.waiters.len())
    }

    /// Builds the stable internal shutdown projection.
    fn closed_error() -> ScribeError {
        ScribeError::Internal {
            detail: "Scribe producer admission closed".to_owned(),
        }
    }

    /// Emits one refusal before this producer can acquire a queue waiter.
    fn record_refusal(
        &self,
        workspace_bytes: usize,
        queue_position: usize,
        identity: Option<&ProducerLifecycleIdentity>,
        outcome: &'static str,
        reason: &'static str,
    ) {
        record_producer_lifecycle(
            ProducerLifecycleEvent {
                workspace_bytes,
                queue_position,
                queue_count: self.queue_len(),
                resource_epoch: self.resources.memory_epoch(),
                operation: "admission",
                stage: "refusal",
                outcome,
                reason,
            },
            identity,
        );
    }
}

/// Cancellation guard that removes exactly one queued waiter without bytes.
struct ProducerWaiterGuard {
    /// Shared queue state containing this guard's waiter while armed.
    state: Arc<Mutex<ProducerAdmissionState>>,
    /// Queue notification used after head removal.
    changed: Arc<Notify>,
    /// Root capability retained only to read the cancellation epoch.
    resources: ScribeResources,
    /// Trace-only immutable identity emitted if this waiter is cancelled.
    identity: Option<ProducerLifecycleIdentity>,
    /// Exact producer workspace requested by this waiter.
    workspace_bytes: usize,
    /// Arrival position observed when this waiter entered the queue.
    queue_position: usize,
    /// Waiter identity, cleared after a successful explicit removal.
    id: Option<u64>,
}

impl ProducerWaiterGuard {
    /// Arms cancellation cleanup for one enqueued waiter.
    fn new(
        admission: &ProducerAdmission,
        id: u64,
        workspace_bytes: usize,
        queue_position: usize,
        identity: Option<ProducerLifecycleIdentity>,
    ) -> Self {
        Self {
            state: Arc::clone(&admission.state),
            changed: Arc::clone(&admission.changed),
            resources: admission.resources.clone(),
            identity,
            workspace_bytes,
            queue_position,
            id: Some(id),
        }
    }

    /// Returns the trace identity retained by this queued waiter.
    fn identity(&self) -> Option<&ProducerLifecycleIdentity> {
        self.identity.as_ref()
    }

    /// Removes a waiter after a non-cancellation terminal transition.
    fn settle(&mut self) {
        self.identity = None;
        self.remove();
    }

    /// Emits one terminal admission refusal before removing this waiter.
    ///
    /// A refusal is a production result, unlike a future drop, so this clears
    /// the guard after emitting the refusal and cannot subsequently report a
    /// cancellation for the same waiter.
    fn refuse(&mut self, outcome: &'static str, reason: &'static str) {
        record_producer_lifecycle(
            ProducerLifecycleEvent {
                workspace_bytes: self.workspace_bytes,
                queue_position: self.queue_position,
                queue_count: self.state.lock().map_or(0, |state| state.waiters.len()),
                resource_epoch: self.resources.memory_epoch(),
                operation: "admission",
                stage: "refusal",
                outcome,
                reason,
            },
            self.identity(),
        );
        self.settle();
    }

    /// Removes this waiter after grant or terminal refusal.
    fn remove(&mut self) {
        let Some(id) = self.id.take() else { return };
        if let Ok(mut state) = self.state.lock()
            && let Some(index) = state.waiters.iter().position(|waiter| waiter.id == id)
        {
            state.waiters.remove(index);
        }
        self.changed.notify_waiters();
    }
}

impl Drop for ProducerWaiterGuard {
    fn drop(&mut self) {
        let identity = self.identity.take();
        self.remove();
        if let Some(identity) = identity {
            record_producer_lifecycle(
                ProducerLifecycleEvent {
                    workspace_bytes: self.workspace_bytes,
                    queue_position: self.queue_position,
                    queue_count: self.state.lock().map_or(0, |state| state.waiters.len()),
                    resource_epoch: self.resources.memory_epoch(),
                    operation: "admission",
                    stage: "cancel",
                    outcome: "cancelled",
                    reason: "future_dropped",
                },
                Some(&identity),
            );
        }
    }
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
    pub(crate) fn new(
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
        #[cfg(any(test, feature = "test-support"))]
        let publication_barrier = self.faults.take_publication_barrier(rows);
        #[cfg(any(test, feature = "test-support"))]
        if let Some(barrier) = &publication_barrier {
            barrier.pause_before_publication().await;
        }
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
                if let Some(barrier) = &publication_barrier {
                    barrier.pause_after_publication().await;
                }
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
    /// Reports whether the test fault injector refuses the next SQL commit.
    #[must_use]
    fn fail_before_sql_commit(&self) -> bool {
        #[cfg(any(test, feature = "test-support"))]
        {
            self.faults.take_sql_commit()
        }
        #[cfg(not(any(test, feature = "test-support")))]
        {
            false
        }
    }

    /// Builds a persistence worker from its complete durable dependencies.
    fn new(
        operator_pool: Option<vala_sql::OperatorPool>,
        failures: Arc<Mutex<Vec<String>>>,
        context: PersistenceRuntimeContext,
        output_scratch: Option<Arc<crate::resources::ScratchVolume>>,
        producer_admission: ProducerAdmission,
        staging: Option<Arc<crate::scribe::staging_runtime::ScribeStagingRuntime>>,
    ) -> Self {
        Self {
            operator_pool,
            failures,
            actor_stream: context.actor_stream,
            wal: context.wal,
            persistence_cpu: context.persistence_cpu,
            wal_io: context.wal_io,
            memory: context.memory,
            staging_file_publisher: context.staging_file_publisher,
            output_scratch,
            #[cfg(any(test, feature = "test-support"))]
            faults: context.faults,
            manifest_guard: Arc::new(tokio::sync::Mutex::new(())),
            producer_admission,
            staging,
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
        tracing::info!(
            tenant = %generation.table_key.0,
            table = %generation.table_key.1,
            shard_id = generation.shard_id,
            writer_epoch = generation.stream.writer_epoch.as_i64(),
            shard_generation = generation.wal_lsn_min.as_u64(),
            member_generation = generation.generation_id.0,
            terminal = false,
            "Scribe immutable generation persistence started"
        );
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
        tracing::info!(
            tenant = %generation.table_key.0,
            table = %generation.table_key.1,
            shard_id = generation.shard_id,
            writer_epoch = generation.stream.writer_epoch.as_i64(),
            shard_generation = generation.wal_lsn_min.as_u64(),
            member_generation = generation.generation_id.0,
            terminal = true,
            outcome = if publication_succeeded { "committed" } else { "failed" },
            "Scribe immutable generation persistence settled"
        );
        let replay_identity = generation
            .replay_identity
            .lock()
            .ok()
            .and_then(|mut identity| identity.take());
        let completion = match result {
            Ok(outcome) => PersistenceCompletion {
                generation_id: generation.generation_id,
                staged: Some(Self::record_published_claims(&generation, &outcome)),
                wal_segments: generation.wal_segments.clone(),
                wal: generation.wal.clone(),
                arrow_bytes: generation.arrow_bytes,
                replay_identity,
                error: None,
            },
            Err(error) => PersistenceCompletion {
                generation_id: generation.generation_id,
                staged: None,
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
    ) -> Result<StagedGenerationOutcome, ScribeError> {
        let producer_identity = ProducerLifecycleIdentity {
            seal_key: generation.seal_key.clone(),
            shard_id: generation.shard_id,
            writer_epoch: generation.stream.writer_epoch.as_i64(),
            shard_generation: generation.wal_lsn_min.as_u64(),
            cohort_id: generation
                .wal_segments
                .first()
                .map(|segment| segment.path.clone()),
            member_generation: generation.generation_id.0,
        };
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
        self.persist_once(
            generation,
            &job.binding,
            job.defer_manifest_advance,
            owned_bytes,
            &producer_identity,
        )
        .await
    }

    /// Records the claims one settled generation published and returns its member.
    ///
    /// Publishing is decoupled from staging, so the count belongs to the
    /// generation that happened to make those claims due rather than to any one
    /// of their members. Zero is the normal steady state for a key still
    /// filling toward its object target.
    fn record_published_claims(
        generation: &ImmutableGeneration,
        outcome: &StagedGenerationOutcome,
    ) -> crate::scribe::assembly::StagedMemberId {
        metrics::counter!("bifrost_scribe_staging_claims_published_total")
            .increment(outcome.published.len() as u64);
        if !outcome.published.is_empty() {
            tracing::info!(
                shard_id = generation.shard_id,
                member_generation = generation.generation_id.0,
                claims = outcome.published.len(),
                "Scribe staged members published as hot objects"
            );
        }
        outcome.member
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

    /// Emits one candidate terminal or release event under its typed identity.
    fn record_candidate_settlement(
        &self,
        identity: &ProducerLifecycleIdentity,
        workspace_bytes: usize,
        stage: &'static str,
        succeeded: bool,
    ) {
        record_producer_lifecycle(
            ProducerLifecycleEvent {
                workspace_bytes,
                queue_position: 0,
                queue_count: self.producer_admission.queue_len(),
                resource_epoch: self.memory.memory_epoch(),
                operation: "persistence",
                stage,
                outcome: if stage == "release" {
                    "released"
                } else if succeeded {
                    "candidate_ready"
                } else {
                    "failed"
                },
                reason: if stage == "release" {
                    if succeeded {
                        "candidate_ready"
                    } else {
                        "failed"
                    }
                } else if succeeded {
                    "durable_candidate_released"
                } else {
                    "stage_error"
                },
            },
            Some(identity),
        );
    }

    /// Stages one generation durably, then publishes whatever claim it completes.
    ///
    /// The ordering is the durable contract. The generation's rows are sorted,
    /// encoded, fsynced and preflighted onto the staging volume, the staged
    /// record that makes them a query source is published, and only then does
    /// the stream manifest advance — which is what allows the WAL behind those
    /// rows to be retired. Assembly and fenced publication happen after that
    /// boundary and may span several generations, so a member whose claim has
    /// not yet filled is durable, servable, and WAL-free while it waits.
    ///
    /// Each costed stage records its elapsed time into
    /// `bifrost_scribe_persist_stage_seconds{stage}` via [`record_persist_stage`]
    /// and emits a `persist stage start` debug event before it begins, so a
    /// worker parked mid-stage is identifiable from logs: the last emitted stage
    /// for a generation is the stage that never completed.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when this pod owns no staging capability, when
    /// the recipe cannot be resolved, when staging, record publication, or
    /// manifest advancement fails, or when a due claim cannot be merged or
    /// published.
    ///
    /// # Cancellation
    ///
    /// Cancellation before the staged record leaves a member directory that
    /// startup cleanup removes. Cancellation after it leaves a durable member
    /// that a later claim publishes. Cancellation during publication leaves
    /// uploaded objects and a publication manifest that a retry converges on.
    async fn persist_once(
        &self,
        generation: &Arc<ImmutableGeneration>,
        binding: &TenantTableBinding,
        defer_manifest_advance: bool,
        generation_owned_bytes: usize,
        producer_identity: &ProducerLifecycleIdentity,
    ) -> Result<StagedGenerationOutcome, ScribeError> {
        let generation_id = generation.generation_id.0;
        let frozen = generation.frozen_snapshot();
        let workspace_bytes = parquet_candidate_incremental_bytes(frozen.arrow_bytes)?;
        let mut reservation = self
            .producer_admission
            .acquire_with_identity(workspace_bytes, producer_identity.clone())
            .await?;
        reservation
            .attach_shard(generation.shard_id)
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        let mut owner = Some(reservation);
        let staged = self
            .stage_member(
                generation,
                binding,
                &frozen,
                &mut owner,
                generation_owned_bytes,
            )
            .await;
        self.record_candidate_settlement(
            producer_identity,
            workspace_bytes,
            "terminal",
            staged.is_ok(),
        );
        drop(owner.take());
        self.record_candidate_settlement(
            producer_identity,
            workspace_bytes,
            "release",
            staged.is_ok(),
        );
        let member = staged?;
        if defer_manifest_advance {
            tracing::debug!(
                generation_id,
                "replay defers manifest advance until reader exit"
            );
        } else {
            self.advance_manifest(generation).await?;
        }
        tracing::debug!(generation_id, "generation is durable on the staging volume");
        let published = self.publish_due_claims(producer_identity).await?;
        Ok(StagedGenerationOutcome { member, published })
    }

    /// Encodes and registers one frozen generation as a durable staged member.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the staging capability or the write recipe
    /// is unavailable, when the footer child cannot be split from the admitted
    /// producer owner, when the lane returns the wrong result, or when the runs
    /// or their record cannot be made durable.
    async fn stage_member(
        &self,
        generation: &Arc<ImmutableGeneration>,
        binding: &TenantTableBinding,
        frozen: &FrozenMemtable,
        owner: &mut Option<ScribeMemoryLease>,
        generation_owned_bytes: usize,
    ) -> Result<crate::scribe::assembly::StagedMemberId, ScribeError> {
        let staging = self.staging()?;
        let layout = crate::scribe::write_recipe::resolve_write_recipe(
            self.operator_pool
                .as_ref()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "writer-v2 staging requires the operator control capability".to_owned(),
                })?
                .pool(),
            binding,
            &frozen.schema,
        )
        .await?;
        let footer_reservation = crate::scribe::memory::EncodedFooterReservation::transfer_from(
            owner.as_mut().ok_or_else(|| ScribeError::Internal {
                detail: "producer reservation was transferred before staging".to_owned(),
            })?,
            generation_owned_bytes,
        )?;
        tracing::debug!(
            generation_id = generation.generation_id.0,
            stage = "stage_member",
            "persist stage start"
        );
        let staged_started = std::time::Instant::now();
        #[cfg(any(test, feature = "test-support"))]
        let _encode_guard = self.faults.begin_encode().await;
        let staged = match self
            .persistence_cpu
            .submit(ScribePersistenceCpuOp::StageMember(Box::new(
                crate::scribe::execution_lanes::StageMemberOp {
                    frozen: Box::new(frozen.clone()),
                    binding: binding.clone(),
                    layout,
                    origin: crate::scribe::member_stager::StagedMemberOrigin {
                        node_id: generation.stream.node_id,
                        writer_epoch: generation.stream.writer_epoch,
                        shard: u16::try_from(generation.shard_id).map_err(|_| {
                            ScribeError::Internal {
                                detail: "shard lane identity exceeds the staged member width"
                                    .to_owned(),
                            }
                        })?,
                        generation: generation.generation_id.0,
                        wal: crate::scribe::hot_stage::StagedLsnRange {
                            min: generation.wal_lsn_min.as_u64(),
                            max: generation.wal_lsn_max.as_u64(),
                        },
                    },
                    footer_reservation,
                    staging: Arc::clone(&staging),
                },
            )))
            .await?
        {
            ScribePersistenceCpuResult::MemberStaged(staged) => *staged,
            ScribePersistenceCpuResult::Prepared(_)
            | ScribePersistenceCpuResult::ParquetEncoded(_)
            | ScribePersistenceCpuResult::ClaimAssembled(_)
            | ScribePersistenceCpuResult::ReplayRestored(_) => {
                return Err(ScribeError::Internal {
                    detail: "persistence lane returned the wrong staging result".to_owned(),
                });
            }
        };
        record_persist_stage("stage_member", staged_started);
        emit_compression_telemetry(
            frozen.arrow_bytes,
            usize::try_from(staged.staged_bytes()).unwrap_or(usize::MAX),
        );
        let member = staged.member();
        staging.register_member(staged, chrono::Utc::now()).await?;
        Ok(member)
    }

    /// Publishes every claim that is due after this generation became durable.
    ///
    /// Draining the assembler here rather than on a separate timer keeps the
    /// pod's publication rate tied to its ingest rate: a key that has reached
    /// target publishes as soon as the member that completed it is durable, and
    /// a key that has not stays staged at no cost to this worker.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a due claim cannot be gathered, admitted,
    /// merged, or published. The claim stays outstanding and its members stay
    /// durable, so the failure retries rather than losing rows.
    async fn publish_due_claims(
        &self,
        producer_identity: &ProducerLifecycleIdentity,
    ) -> Result<Vec<FileListCommitKey>, ScribeError> {
        let staging = self.staging()?;
        let mut published = Vec::new();
        while let Some(claim) = staging.take_claim(chrono::Utc::now())? {
            published.push(
                self.publish_claim(&staging, &claim, Some(producer_identity))
                    .await?,
            );
        }
        Ok(published)
    }

    /// Publishes every staged member that target and dwell would still hold.
    ///
    /// Drain is the one point where waiting costs more than publishing: no
    /// further member will arrive to complete a partial key, and its dwell
    /// would expire against a runtime that is gone. Sweeping every ready key as
    /// residue therefore settles the staging volume instead of leaving durable
    /// members for the next process to rediscover. Publication is still the
    /// same fenced transaction, so a member that cannot publish stays durable
    /// and staged rather than being dropped.
    ///
    /// Returns the number of claims published.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the staging capability is absent, when the
    /// assembler refuses a residue claim, or when a claim fails to publish. The
    /// remaining keys stay staged and the WAL stays authoritative for them.
    async fn publish_residue(
        &self,
        cause: crate::scribe::assembly::ClaimCause,
    ) -> Result<usize, ScribeError> {
        let staging = self.staging()?;
        let mut published = 0;
        for key in staging.ready_keys()? {
            while let Some(claim) = staging.take_residue(&key, cause)? {
                self.publish_claim(&staging, &claim, None).await?;
                published += 1;
            }
        }
        Ok(published)
    }

    /// Merges and publishes exactly one outstanding claim.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the claim's runs no longer match their
    /// records, when merge workspace or output scratch cannot be admitted, when
    /// the lane returns the wrong result, or when the fenced publication is
    /// refused or uncertain.
    async fn publish_claim(
        &self,
        staging: &Arc<crate::scribe::staging_runtime::ScribeStagingRuntime>,
        claim: &crate::scribe::assembly::StagingClaim,
        producer_identity: Option<&ProducerLifecycleIdentity>,
    ) -> Result<FileListCommitKey, ScribeError> {
        let runs = staging.gather(claim).await?;
        let workspace_bytes = parquet_candidate_incremental_bytes(claim_merge_bytes(claim))?;
        let mut reservation = match producer_identity {
            Some(identity) => {
                self.producer_admission
                    .acquire_with_identity(workspace_bytes, identity.clone())
                    .await?
            }
            None => {
                self.producer_admission
                    .acquire_accepted(workspace_bytes)
                    .await?
            }
        };
        let footer_reservation =
            crate::scribe::memory::EncodedFooterReservation::split_from(&mut reservation)?;
        let scratch = self
            .output_scratch
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe output scratch is unavailable before claim assembly".to_owned(),
            })?
            .create_scribe_claim(
                &claim.key().node_id().to_string(),
                &claim.id().to_string(),
                claim.encoded_bytes(),
            )
            .map_err(|error| ScribeError::Internal {
                detail: format!("Scribe claim scratch admission failed: {error}"),
            })?;
        if self.fail_before_sql_commit() {
            return Err(ScribeError::Internal {
                detail: "test SQL failure before the first COMMIT attempt".to_owned(),
            });
        }
        #[cfg(any(test, feature = "test-support"))]
        let _object_write_guard = self.faults.begin_object_write().await;
        #[cfg(any(test, feature = "test-support"))]
        if self.faults.take_object_write() {
            return Err(ScribeError::Internal {
                detail: "test object-store write failure".to_owned(),
            });
        }
        tracing::debug!(claim = %claim.id(), stage = "assemble_claim", "persist stage start");
        let assemble_started = std::time::Instant::now();
        let mut assembled = match self
            .persistence_cpu
            .submit(ScribePersistenceCpuOp::AssembleClaim(Box::new(
                crate::scribe::execution_lanes::AssembleClaimOp {
                    claim: Box::new(claim.clone()),
                    runs: Box::new(runs.clone()),
                    scratch_dir: scratch.path().to_path_buf(),
                    footer_reservation,
                    staging: Arc::clone(staging),
                },
            )))
            .await?
        {
            ScribePersistenceCpuResult::ClaimAssembled(assembled) => *assembled,
            ScribePersistenceCpuResult::Prepared(_)
            | ScribePersistenceCpuResult::ParquetEncoded(_)
            | ScribePersistenceCpuResult::MemberStaged(_)
            | ScribePersistenceCpuResult::ReplayRestored(_) => {
                return Err(ScribeError::Internal {
                    detail: "persistence lane returned the wrong assembly result".to_owned(),
                });
            }
        };
        record_persist_stage("assemble_claim", assemble_started);
        assembled.artifacts.attach_scratch(scratch)?;
        let published = staging
            .publish(claim, &runs, &assembled, self.actor_stream)
            .await?;
        assembled.artifacts.cleanup()?;
        drop(reservation);
        self.publish_staging_hint(claim.key());
        Ok(published.commit_key)
    }

    /// Returns this worker's staged lifecycle owner.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the pod was provisioned without a
    /// staging volume or without the operator capability publication requires.
    /// Refusing is the conservative outcome: the WAL stays authoritative.
    fn staging(
        &self,
    ) -> Result<Arc<crate::scribe::staging_runtime::ScribeStagingRuntime>, ScribeError> {
        self.staging.clone().ok_or_else(|| ScribeError::Internal {
            detail: "Scribe staging requires a staging volume and the operator capability"
                .to_owned(),
        })
    }

    /// Publishes the best-effort local wake-up after the durable SQL commit.
    fn publish_staging_hint(&self, key: &crate::scribe::assembly::ScribeAssemblyKey) {
        let Some(publisher) = &self.staging_file_publisher else {
            return;
        };
        let Ok(binding) = TenantTableBinding::resolve((key.tenant(), key.table().clone())) else {
            return;
        };
        let event = crate::maintenance::StagingFileCommitted::new(binding, key.partition());
        let _ = publisher.try_publish(event);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::panic::AssertUnwindSafe;
    use std::task::{Context, Poll};

    use crate::catalog::TableRef;

    use crate::namespaces::BifrostNamespace;
    use crate::scribe::telemetry::producer_lifecycle_tests::{EventCaptureSubscriber, field};
    use crate::test_support::{SpanCaptureSubscriber, has_span_outcome};

    /// Builds one immutable producer identity for admission lifecycle proof.
    fn producer_identity_for_test() -> ProducerLifecycleIdentity {
        ProducerLifecycleIdentity {
            seal_key: SealKey::new(
                crate::test_support::tenant(),
                TableRef::new(BifrostNamespace::Bifrost, "producer_lifecycle"),
                crate::test_support::day_partition(2026, 8, 19),
            ),
            shard_id: 3,
            writer_epoch: 41,
            shard_generation: 9001,
            cohort_id: Some(std::path::PathBuf::from("wal/shard-3/segment-9001")),
            member_generation: 77,
        }
    }

    /// A production producer refusal retains identity and cannot fall through as cancellation.
    ///
    /// # Panics
    ///
    /// Panics when a closed production admission does not produce one
    /// identity-bearing refusal.
    #[tokio::test]
    async fn production_producer_refusal_retains_identity_without_cancellation() {
        const MIB: usize = 1024 * 1024;
        let roles = crate::resources::BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [crate::resources::BifrostRole::Scribe],
        );
        let resources = roles.scribe().expect("Scribe capability");
        let admission = ProducerAdmission::new(resources.clone());
        let subscriber = EventCaptureSubscriber::default();
        let events = Arc::clone(&subscriber.events);
        admission.close();
        let _subscriber = tracing::subscriber::set_default(subscriber);
        admission
            .acquire_with_identity(MIB, producer_identity_for_test())
            .await
            .expect_err("closed admission refuses production producer");
        let events = events.lock().expect("event capture");
        let refusal = events
            .iter()
            .find(|event| field(event, "stage") == "refusal")
            .expect("identity-bearing producer refusal");
        assert_eq!(field(refusal, "outcome"), "closed");
        assert_eq!(field(refusal, "reason"), "runtime_shutdown");
        assert_eq!(field(refusal, "shard_id"), "3");
        assert_eq!(field(refusal, "writer_epoch"), "41");
        assert_eq!(field(refusal, "shard_generation"), "9001");
        assert_eq!(field(refusal, "member_generation"), "77");
        assert!(
            events.iter().all(|event| field(event, "stage") != "cancel"),
            "a terminal refusal must not also emit cancellation"
        );
    }

    /// FIFO producer admission removes cancellation and never mirrors byte totals.
    ///
    /// # Panics
    ///
    /// Panics if a second producer enters concurrently or cancellation/drop of
    /// either the waiting future or acquired permit prevents later progress.
    #[tokio::test]
    async fn producer_admission_is_fifo_cancellation_safe_and_starvation_free() {
        const MIB: usize = 1024 * 1024;
        let roles = crate::resources::BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [crate::resources::BifrostRole::Scribe],
        );
        let resources = roles.scribe().expect("Scribe capability");
        let admission = ProducerAdmission::new(resources.clone());
        let blocker = admission.acquire(500 * MIB).await.expect("initial owner");
        let large = tokio::spawn({
            let admission = admission.clone();
            async move { admission.acquire(32 * MIB).await }
        });
        tokio::task::yield_now().await;
        let cancelled = tokio::spawn({
            let admission = admission.clone();
            async move { admission.acquire(MIB).await }
        });
        tokio::task::yield_now().await;
        cancelled.abort();
        let small = (0..16)
            .map(|_| {
                tokio::spawn({
                    let admission = admission.clone();
                    async move { admission.acquire(MIB).await }
                })
            })
            .collect::<Vec<_>>();
        tokio::task::yield_now().await;
        assert_eq!(admission.queue_len(), 17);
        drop(blocker);
        let large_lease = tokio::time::timeout(Duration::from_secs(1), large)
            .await
            .expect("large head progresses")
            .expect("large task")
            .expect("large lease");
        let mut small_leases = Vec::new();
        for waiter in small {
            small_leases.push(
                tokio::time::timeout(Duration::from_secs(1), waiter)
                    .await
                    .expect("continuous small follower progresses")
                    .expect("small task")
                    .expect("small lease"),
            );
        }
        assert_eq!(admission.queue_len(), 0);
        drop((large_lease, small_leases));
        admission.close();
        assert!(admission.acquire(1).await.is_err());
        let accepted = admission
            .acquire_accepted(1)
            .await
            .expect("accepted work drains after close");
        drop(accepted);
    }

    /// Two fitting producer workspaces coexist while a larger FIFO head waits.
    ///
    /// # Panics
    ///
    /// Panics if exact root leases serialize fitting work or admit an oversized
    /// follower before earlier ownership releases.
    #[tokio::test]
    async fn two_candidate_encodes_overlap_and_large_candidate_waits() {
        const MIB: usize = 1024 * 1024;
        let roles = crate::resources::BifrostRuntimeResources::composed_for_test(
            768 * MIB,
            512 * MIB as u64,
            [crate::resources::BifrostRole::Scribe],
        );
        let admission = ProducerAdmission::new(roles.scribe().expect("Scribe capability"));
        let first = admission.acquire(64 * MIB).await.expect("first workspace");
        let second = admission.acquire(64 * MIB).await.expect("second workspace");
        let large = tokio::spawn({
            let admission = admission.clone();
            async move { admission.acquire(448 * MIB).await }
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!large.is_finished());
        drop(first);
        assert!(!large.is_finished());
        drop(second);
        let lease = tokio::time::timeout(Duration::from_secs(1), large)
            .await
            .expect("large workspace wakes")
            .expect("large task")
            .expect("large lease");
        drop(lease);
    }

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
            output_scratch: None,
            reconciler: None,
            reconciliation_sender: None,
            producer_admission: None,
            recovery_wal: None,
            recovery_operator: None,
            recovery_memory: None,
            worker: None,
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
        /// Database retained until worker shutdown completes, and the source of
        /// the tenant connection the fixture registers its control row through.
        database: wyrd_dev_fixtures::pg::PgFixture,
        /// WAL directory retained until worker shutdown completes.
        _wal_root: tempfile::TempDir,
        /// Scratch namespace roots retained until worker shutdown completes.
        _scratch_root: tempfile::TempDir,
    }

    #[cfg(feature = "test-support")]
    impl IdlePersistenceFixture {
        /// Builds Scribe-only runtime resources over the fixture's real volume roots.
        ///
        /// The snapshot is injected rather than probed so the fixture's capacity
        /// is deterministic across machines, and only the Scribe role is
        /// composed so the test exercises a single role's floor. The volume
        /// roots are the fixture's real directories, so the WAL, output, and
        /// scratch accounting under test is measured against actual files.
        ///
        /// # Panics
        ///
        /// Panics if the injected snapshot and policy cannot produce valid
        /// Bifrost runtime resources.
        fn scribe_only_runtime_resources(
            scratch_root: &std::path::Path,
            wal_root: &std::path::Path,
            scribe_stage: std::path::PathBuf,
            scribe_output: std::path::PathBuf,
            forge_scratch: std::path::PathBuf,
            oracle_scratch: std::path::PathBuf,
        ) -> crate::resources::BifrostRuntimeResources {
            crate::resources::BifrostRuntimeResources::from_snapshot(
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
                    oracle_query_slot_limit: None,
                    scratch_root: scratch_root.to_owned(),
                    volume_roots: Some(crate::resources::BifrostVolumeRoots {
                        wal: wal_root.to_owned(),
                        scribe_stage,
                        scribe_output_scratch: scribe_output,
                        forge_scratch,
                        oracle_scratch,
                    }),
                },
            )
            .expect("test Bifrost resources")
        }

        /// Starts one real persistence runtime before it accepts work.
        async fn start() -> Self {
            let database = wyrd_dev_fixtures::pg::PgFixture::start()
                .await
                .expect("Postgres fixture");
            let tenant = database.data_tenant_id();
            let wal_root = tempfile::tempdir().expect("WAL directory");
            let scratch_root = tempfile::tempdir().expect("scratch directory");
            let scribe_stage = wal_root.path().join("scribe-stage");
            let scribe_output = scratch_root.path().join("scribe-output");
            let forge_scratch = scratch_root.path().join("forge");
            let oracle_scratch = scratch_root.path().join("oracle");
            for root in [
                &scribe_stage,
                &scribe_output,
                &forge_scratch,
                &oracle_scratch,
            ] {
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
            let runtime_resources = Self::scribe_only_runtime_resources(
                scratch_root.path(),
                wal_root.path(),
                scribe_stage,
                scribe_output,
                forge_scratch,
                oracle_scratch,
            );
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
                    geometry: crate::scribe::geometry::ScribeGeometry::default(),
                    faults: PersistenceFaults::default(),
                },
                &Handle::current(),
            );
            Self {
                runtime,
                wal,
                stream,
                tenant,
                database,
                _wal_root: wal_root,
                _scratch_root: scratch_root,
            }
        }

        /// Builds the one bounded generation this fixture submits.
        ///
        /// Every field is fixed — one row, one audit event, one append meta,
        /// LSN 1, shard 0 — so the queue gauges the caller asserts on move by a
        /// known amount rather than by whatever a randomized fixture produced.
        /// The WAL handle is taken from the fixture's real writer so the
        /// persistence worker exercises a genuine durable path.
        ///
        /// # Panics
        ///
        /// Panics if the fixture's WAL writer has no shard-0 handle.
        fn fixture_generation(
            &self,
            table: crate::catalog::TableRef,
            seal_key: &SealKey,
            rows: Vec<arrow::record_batch::RecordBatch>,
            schema: Arc<arrow::datatypes::Schema>,
            batch_id: uuid::Uuid,
        ) -> Arc<ImmutableGeneration> {
            Arc::new(ImmutableGeneration {
                table_key: (self.tenant, table),
                seal_key: seal_key.clone(),
                generation_id: GenerationId(1),
                stream: self.stream,
                wal_lsn_min: WalLsn::new(1),
                wal_lsn_max: WalLsn::new(1),
                wal_segments: Vec::new(),
                wal: self.wal.handle_for_shard(0).expect("WAL handle"),
                rows,
                schema,
                audit_events: vec![wyrd_spec::vala::api::AuditEvent {
                    request_id: wyrd_spec::request_id::RequestId::now_v7(),
                    trace_id: None,
                    operation: "test".to_owned(),
                    resource: "bifrost.idle_queue".to_owned(),
                    card_ref: None,
                    principal_id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::new_v4()),
                    principal_kind: wyrd_spec::auth::PrincipalKindTag::User,
                    auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
                    permission: "bifrost.write".to_owned(),
                    decision: wyrd_spec::vala::api::AuditDecision::Allow,
                    result: wyrd_spec::vala::api::AuditResult::Success,
                    payload_summary: "idle persistence fixture".to_owned(),
                    detail: None,
                }],
                append_metas: vec![crate::scribe::wal::ScribeAppendMeta {
                    batch_id: *batch_id.as_bytes(),
                    schema_fingerprint: [0; 32],
                    data_digest: [0; 32],
                    data_len: 0,
                    payload_digest: [0; 32],
                    payload_len: 0,
                    slice_index: 0,
                    slice_count: 1,
                    rows_accepted: 1,
                    wal_lsn_min: WalLsn::new(1),
                    wal_lsn_max: WalLsn::new(1),
                    seal_key: seal_key.to_string(),
                }],
                row_count: 1,
                arrow_bytes: 1,
                replay_identity: Mutex::new(None),
                opened_at: std::time::Instant::now(),
                closed_at: std::time::Instant::now(),
                shard_id: 0,
            })
        }

        /// Returns the physical Arrow schema every fixture generation writes.
        ///
        /// Registration and submission must agree exactly: the write recipe is
        /// re-canonicalized against the sealed schema, so a column present in
        /// one and absent from the other fails the seal closed.
        fn fixture_schema() -> Arc<arrow::datatypes::Schema> {
            Arc::new(arrow::datatypes::Schema::new(vec![
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
            ]))
        }

        /// Registers the control row the seal path resolves its write recipe from.
        ///
        /// The fixture drives the persistence worker directly rather than through
        /// the catalog, so it owns the one registration the worker requires. The
        /// layout is the canonical omission default, matching a table registered
        /// without a declaration.
        async fn register_control_row(&self, table: &crate::catalog::TableRef) {
            let fqn = table.fqn();
            let layout = crate::catalog::layout::PhysicalLayout::resolve(
                &fqn,
                &Self::fixture_schema(),
                None,
            )
            .expect("the fixture schema resolves");
            let mut conn = self
                .database
                .vala_postgres()
                .tenant_conn(self.tenant)
                .await
                .expect("fixture tenant connection");
            vala_sql::queries::olap_catalog::upsert_table(
                &mut conn,
                uuid::Uuid::now_v7().as_bytes(),
                &fqn,
                &[0_u8; 32],
                &serde_json::to_value(layout.to_wire()).expect("canonical layout encodes"),
            )
            .await
            .expect("register the fixture control row");
            conn.commit().await.expect("commit the fixture control row");
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
                crate::test_support::day_partition(2026, 1, 1),
            );
            let binding =
                TenantTableBinding::resolve((self.tenant, table.clone())).expect("binding");
            let batch_id = uuid::Uuid::now_v7();
            self.register_control_row(&table).await;
            let schema = Self::fixture_schema();
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
                    generation: self.fixture_generation(table, &seal_key, rows, schema, batch_id),
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
