//! Bounded immutable-generation persistence for Scribe.

use std::path::Path;
#[cfg(any(test, feature = "test-support"))]
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(any(test, feature = "test-support"))]
use std::time::Duration;

use arrow::datatypes::SchemaRef;
use num_traits::ToPrimitive;
use opendal::Operator;
use sha2::{Digest, Sha256};
use tokio::runtime::Handle;
#[cfg(any(test, feature = "test-support"))]
use tokio::sync::watch;
use tokio::sync::{Notify, mpsc, oneshot};
use tracing::{Instrument, Span};
use vala_sql::{OperatorPool, ValaPostgres};

use crate::catalog::{TenantTableBinding, TenantTableKey};
use crate::contracts::ScribeError;
use crate::maintenance::StagingFilePublisher;
use crate::parquet::object_uploader::{
    BifrostParquetUploader, BifrostUploadRole, ParquetObjectIdentity, VerifiedParquetObject,
};
use crate::resources::{ScratchVolume, ScribeResources};
#[cfg(any(test, feature = "test-support"))]
use crate::scribe::assembly::StagingBacklog;
use crate::scribe::assembly::StagingClaim;
use crate::scribe::claim_assembly::{AssembledClaim, ClaimRuns};
use crate::scribe::execution_lanes::{
    ScribePersistenceCpuOp, ScribePersistenceCpuPool, ScribePersistenceCpuResult, ScribeWalIoOp,
    ScribeWalIoPool, ScribeWalIoResult,
};
use crate::scribe::file_list_writer;
use crate::scribe::memory::{MemoryCategory, PARQUET_TRANSFER_BUFFER_BYTES};
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::parquet_writer::BoundedParquetArtifactSet;
use crate::scribe::seal_key::SealKey;
use crate::scribe::staging::{
    RecoveredPublication, ScribeStaging, StageElection, StagedArtifactClaim,
};
#[cfg(any(test, feature = "test-support"))]
use crate::scribe::staging_runtime::PublishedClaimObservation;
use crate::scribe::staging_runtime::{ClaimTakeError, DrivenClaim, ScribeStagingRuntime};
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::{ScribeAppendMeta, WalLsn, WalSegmentRef, WalWriter};

/// Publishes the unlabeled persistence queue-depth gauge before workers accept jobs.
fn register_idle_persistence_queue() {
    metrics::gauge!("bifrost_scribe_persistence_queue_depth").set(0.0);
}

/// Removes one claim's scratch directory after a known-unpublished terminal.
///
/// [`ScribeClaimScratch`](crate::resources::ScribeClaimScratch) deletes on its
/// explicit blocking boundary and never in `Drop`, because a dropped owner may
/// sit on a Tokio worker. Dropping it therefore retains the charge and poisons
/// shared volume health, which the resource-health worker escalates into
/// terminating the pod. A publication that never committed is retryable, so its
/// scratch is cleaned here instead. A cleanup failure has already poisoned
/// health inside the resource layer; it is logged and swallowed so the caller
/// still reports the original publication error.
async fn discard_claim_scratch(scratch: crate::resources::ScribeClaimScratch) {
    let outcome = tokio::task::spawn_blocking(move || scratch.cleanup()).await;
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::error!(
            operation = "scribe_claim_scratch_discard",
            outcome = "cleanup_failed",
            error = %error,
            "Scribe claim scratch cleanup failed after a refused publication"
        ),
        Err(error) => tracing::error!(
            operation = "scribe_claim_scratch_discard",
            outcome = "task_failed",
            error = %error,
            "Scribe claim scratch cleanup task failed after a refused publication"
        ),
    }
}

/// Record the elapsed seconds for one persistence stage (D84).
///
/// The end-to-end `bifrost_scribe_persistence_publication_seconds` histogram
/// measures closed-to-published latency but cannot say which stage dominates.
/// `bifrost_scribe_persist_stage_seconds{stage}` splits it under the closed
/// `{stage_member, assemble_claim}` set: durably staging one generation's
/// member, and assembling a claim's members into publication output. Each is
/// recorded only after its stage completes successfully, so a stage that
/// errors and returns early contributes no sample. The label carries no
/// tenant, table, or object-path identity.
fn record_persist_stage(stage: &'static str, started: std::time::Instant) {
    metrics::histogram!("bifrost_scribe_persist_stage_seconds", "stage" => stage)
        .record(started.elapsed().as_secs_f64());
}

/// Counts one generation's Parquet-encoded bytes after a durable encode.
///
/// Increments `bifrost_scribe_persistence_encoded_bytes_total` by `file_size`.
fn record_encoded_bytes(file_size: usize) {
    metrics::counter!("bifrost_scribe_persistence_encoded_bytes_total")
        .increment(u64::try_from(file_size).unwrap_or(u64::MAX));
}

/// Test-tier one-shot failures for the concrete persistence seams.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Default)]
pub struct PersistenceFaults {
    /// One-shot failure of the next object-store write, before it mutates storage.
    object_write: Arc<AtomicBool>,
    /// Object-write attempts left until the armed one fails; zero arms nothing.
    object_write_failure_countdown: Arc<AtomicUsize>,
    /// One-shot failure of the next file-list SQL commit after it is staged.
    sql_commit: Arc<AtomicBool>,
    /// One-shot error returned after the next publication COMMIT succeeds.
    post_commit_client_error: Arc<AtomicBool>,
    /// One-shot failure of the next manifest publication after its SQL commit.
    manifest_publication: Arc<AtomicBool>,
    /// One-shot failure of a claim's retirement after its commit landed.
    claim_retirement: Arc<AtomicBool>,
    /// Delay, in milliseconds, applied to an object write once the per-write
    /// delays are used up.
    object_write_delay_ms: Arc<AtomicU64>,
    /// Per-write delays, in milliseconds, consumed in order before the fixed delay.
    object_write_delays_ms: Arc<Mutex<Vec<u64>>>,
    /// Object writes currently in flight.
    object_write_active: Arc<AtomicUsize>,
    /// Highest number of object writes observed in flight together.
    max_object_write_active: Arc<AtomicUsize>,
    /// Delay, in milliseconds, applied inside every encode interval.
    encode_delay_ms: Arc<AtomicU64>,
    /// Parquet encode intervals currently entered.
    encode_active: Arc<AtomicUsize>,
    /// Highest number of encode intervals observed entered together.
    max_encode_active: Arc<AtomicUsize>,
    /// Most recent persistence error observed by the fixture.
    last_error: Arc<Mutex<Option<String>>>,
    /// Next real writer-v2 publication paused on both sides of fenced SQL visibility.
    publication_barrier:
        Arc<Mutex<Option<crate::scribe::file_list_writer::PublicationFenceBarrier>>>,
    /// Claim-publication progress a test can await instead of polling.
    claim_probe: Arc<watch::Sender<ClaimPublicationProbe>>,
}

/// Claim-publication progress observed through [`PersistenceFaults`].
///
/// Every field only moves forward except `held`, which counts the claim
/// publications currently stopped at the object-write hold.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Copy, Default)]
struct ClaimPublicationProbe {
    /// Whether claim object writes stop at the hold until released.
    hold: bool,
    /// Claim publications currently waiting at the hold.
    held: usize,
    /// Times a publisher found every claim slot held by another publication
    /// and waited for one to be released.
    claim_slot_waits: usize,
    /// Claims whose publication committed and settled.
    published_claims: usize,
}

#[cfg(any(test, feature = "test-support"))]
impl PersistenceFaults {
    /// Stops every claim publication at its object write until
    /// [`Self::release_object_writes_for_test`].
    ///
    /// A held publication has already taken its claim slot and durably owns
    /// its members, so a test can arrange real contention for the slots.
    pub fn hold_object_writes_for_test(&self) {
        self.claim_probe.send_modify(|probe| probe.hold = true);
    }

    /// Lets every held claim publication, and every later one, continue.
    pub fn release_object_writes_for_test(&self) {
        self.claim_probe.send_modify(|probe| probe.hold = false);
    }

    /// Waits until at least `count` claim publications are stopped at the hold.
    pub async fn wait_for_held_object_writes_for_test(&self, count: usize) {
        let mut probe = self.claim_probe.subscribe();
        // The sender lives in `self`, so the channel cannot close while waiting.
        let _ = probe.wait_for(|probe| probe.held >= count).await;
    }

    /// Waits until a second publisher contends for the claim slots: either
    /// more than `budget` claim publications are held at once, or one waited
    /// because every slot was held.
    pub async fn wait_for_claim_contention_for_test(&self, budget: usize) {
        let mut probe = self.claim_probe.subscribe();
        let _ = probe
            .wait_for(|probe| probe.held > budget || probe.claim_slot_waits > 0)
            .await;
    }

    /// Waits until at least `count` claims have committed and settled.
    pub async fn wait_for_published_claims_for_test(&self, count: usize) {
        let mut probe = self.claim_probe.subscribe();
        let _ = probe
            .wait_for(|probe| probe.published_claims >= count)
            .await;
    }

    /// Stops one claim publication at the hold while it is set.
    async fn pass_object_write_hold(&self) {
        let mut probe = self.claim_probe.subscribe();
        if !probe.borrow_and_update().hold {
            return;
        }
        self.claim_probe.send_modify(|probe| probe.held += 1);
        let _ = probe.wait_for(|probe| !probe.hold).await;
        self.claim_probe.send_modify(|probe| probe.held -= 1);
    }

    /// Records that a publisher waited for another publication's claim slot.
    fn note_claim_slot_wait(&self) {
        self.claim_probe
            .send_modify(|probe| probe.claim_slot_waits += 1);
    }

    /// Records one committed and settled claim publication.
    fn note_claim_published(&self) {
        self.claim_probe
            .send_modify(|probe| probe.published_claims += 1);
    }

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

    /// Fail the next SQL commit after the file-list transaction is staged.
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

    /// Fails the next claim publication after its fenced commit landed and
    /// every member recorded it as `Published`, before any member moves to
    /// cleanup.
    pub fn fail_next_claim_retirement(&self) {
        self.claim_retirement.store(true, Ordering::Release);
    }

    /// Returns whether the claim-retirement failure is still armed, which
    /// means no publication has reached it yet.
    #[must_use]
    pub fn claim_retirement_failure_armed_for_test(&self) -> bool {
        self.claim_retirement.load(Ordering::Acquire)
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

    /// Consumes the armed claim-retirement failure.
    fn take_claim_retirement(&self) -> bool {
        self.claim_retirement.swap(false, Ordering::AcqRel)
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
    pub operator_pool: Option<OperatorPool>,
    /// Maximum queued immutable generations.
    pub queue_items: usize,
    /// Number of asynchronous persistence workers.
    pub workers: usize,
    /// Generation-scoped output scratch capability required before encoding.
    pub output_scratch: Option<Arc<ScratchVolume>>,
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
    pub fn with_operator_pool(mut self, operator_pool: OperatorPool) -> Self {
        self.operator_pool = Some(operator_pool);
        self
    }

    /// Installs the generation-owned output scratch authority.
    #[must_use]
    pub fn with_output_scratch(mut self, output_scratch: ScratchVolume) -> Self {
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

/// Shared dependencies every persistence worker needs to publish a claim.
///
/// Built once when the persistence runtime starts and handed to each worker,
/// so workers share one object-store operator, WAL writer, CPU and WAL-IO
/// lanes, fenced actor stream, memory budget, and hot-source registry rather
/// than constructing their own.
pub(crate) struct PersistenceRuntimeContext {
    /// Object-store operator shared by persistence workers.
    pub(crate) operator: Arc<Operator>,
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
    /// Pod-wide registry the staging runtime moves generation authority in.
    pub(crate) hot_sources: Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>,
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
    /// Operator-backed owner for caller-commit ambiguity reconciliation.
    reconciler: Option<ScribePublicationReconciler>,
    /// WAL-root stage owner used for pre-readiness publication recovery.
    recovery_wal: Option<Arc<WalWriter>>,
    /// Durable object store used for pre-readiness upload convergence.
    recovery_operator: Option<Operator>,
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
            reconciler: None,
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
            reconciler: None,
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
        operator_pool: Option<&OperatorPool>,
        workers: usize,
    ) -> Option<Arc<ScribeStagingRuntime>> {
        let stage_root = context.memory.stage_root()?.to_path_buf();
        let operator_pool = operator_pool?.clone();
        let config = crate::scribe::assembly::StagingAssemblerConfig::new(
            context.geometry.staging_target_file_size_bytes(),
            context.geometry.generation_max_age(),
            workers.max(1),
        )
        .ok()?;
        let stage = Arc::new(crate::scribe::hot_stage::ScribeHotStage::new(stage_root));
        let publisher = crate::scribe::claim_publication::ClaimPublisher::new(
            Arc::clone(&stage),
            ScribeStageMover::new(context.wal.base_dir(), (*context.operator).clone()),
            ScribePublicationReconciler::new(
                operator_pool,
                context.actor_stream,
                #[cfg(any(test, feature = "test-support"))]
                context.faults.clone(),
            ),
        );
        Some(Arc::new(
            ScribeStagingRuntime::new(stage, publisher, config)
                .with_hot_sources(Arc::clone(&context.hot_sources)),
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
            output_scratch,
            (staging, config.workers),
        ));
        let runtime_state = Arc::new(Self {
            sender: Arc::new(Mutex::new(Some(sender))),
            queued: Arc::new(AtomicUsize::new(0)),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            drained: Arc::new(Notify::new()),
            tasks: Arc::new(Mutex::new(Vec::new())),
            abort_handles: Arc::new(Mutex::new(Vec::new())),
            reconciler,
            recovery_wal: Some(recovery_wal),
            recovery_operator: Some(recovery_operator),
            recovery_memory: Some(recovery_memory),
            worker: Some(Arc::clone(&worker)),
        });
        let mut tasks = Vec::with_capacity(config.workers);
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

    /// Publishes every staged claim that target or dwell has made due.
    ///
    /// This is the only path that publishes target and dwell claims. The
    /// server's lifecycle scanner calls it on its tick, so target publication
    /// waits at most one tick and dwell is a wall-clock bound that holds
    /// without new data. A pod without a persistence worker or a
    /// staging volume publishes nothing and reports zero.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a due claim cannot be gathered, admitted,
    /// merged, or published. The claim stays outstanding and its members stay
    /// durable, so a later tick or startup recovery retries it.
    ///
    /// # Cancellation
    ///
    /// Dropping this future stops between claims or leaves one claim in
    /// flight; its uploaded objects and publication manifest converge on retry
    /// and its rows stay authoritative in the WAL.
    pub(crate) async fn publish_due(&self) -> Result<usize, ScribeError> {
        let Some(worker) = &self.worker else {
            return Ok(0);
        };
        if worker.staging.is_none() {
            return Ok(0);
        }
        worker.publish_due_claims().await
    }

    /// Publishes only the staged residue belonging to one physical partition.
    ///
    /// Returns zero when the pod has no persistence worker or no staging
    /// volume, which is the same condition under which it can publish nothing
    /// at all.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the selected key's residue claim or its
    /// fenced publication is refused; the key stays staged and its WAL stays
    /// authoritative.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn publish_partition_for_test(
        &self,
        partition: crate::catalog::layout::TimePartition,
    ) -> Result<usize, ScribeError> {
        let Some(worker) = &self.worker else {
            return Ok(0);
        };
        if worker.staging.is_none() {
            return Ok(0);
        }
        worker.publish_partition_residue(partition).await
    }

    /// Returns every claim this pod's staging runtime has published.
    ///
    /// Empty when the pod has no persistence worker or no staging volume, which
    /// is the same condition under which it can publish nothing at all.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn published_claims_for_test(&self) -> Vec<PublishedClaimObservation> {
        self.worker
            .as_ref()
            .and_then(|worker| worker.staging.as_ref())
            .map(|staging| staging.published_claims_for_test())
            .unwrap_or_default()
    }

    /// Returns this pod's staged backlog read from the staging owner.
    ///
    /// Default when the pod has no persistence worker or no staging volume.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the staging assembler lock is
    /// poisoned.
    #[cfg(any(test, feature = "test-support"))]
    pub fn staging_backlog_for_test(&self) -> Result<StagingBacklog, ScribeError> {
        self.worker
            .as_ref()
            .and_then(|worker| worker.staging.as_ref())
            .map_or(Ok(StagingBacklog::default()), |staging| staging.backlog())
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
        staging.restore(pool).await
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
        ScribeStageMover::new(wal.base_dir(), operator.clone())
            .recover_publications(reconciler, memory)
            .await
    }

    /// Resumes every durable claimed or publishing claim before readiness.
    ///
    /// Publication-manifest recovery runs first, so this driver either replays
    /// the exact already-committed file-list set or continues the same claim
    /// from its staged members, through the same resume the live tick uses
    /// (see [`PersistenceWorker::resume_claim`]). It never derives a
    /// replacement claim identity.
    ///
    /// # Errors
    ///
    /// Returns the first production publication failure. The durable claim and
    /// its staged members remain retained for the next startup attempt.
    pub(crate) async fn resume_staging_claims(&self) -> Result<usize, ScribeError> {
        let Some(worker) = &self.worker else {
            return Ok(0);
        };
        let Some(staging) = &worker.staging else {
            return Ok(0);
        };
        let mut resumed = 0_usize;
        for claim in staging.retryable_claims()? {
            worker.resume_claim(staging, &claim).await?;
            resumed = resumed
                .checked_add(1)
                .ok_or_else(|| ScribeError::Internal {
                    detail: "resumed Scribe claim count overflow".to_owned(),
                })?;
        }
        Ok(resumed)
    }
}

/// Sizes the claim-merge lane from the CPUs the persistence lane leaves free.
///
/// Member encoding on the persistence lane and claim merges run at the same
/// time, so giving merges every effective CPU would oversubscribe the pod by
/// the persistence lane's width. The lane keeps at least one thread so claims
/// always progress, and never more than `claim_budget`, since no more merges
/// can be outstanding at once.
fn merge_lane_threads(
    effective_cpu: usize,
    persistence_threads: usize,
    claim_budget: usize,
) -> usize {
    effective_cpu
        .saturating_sub(persistence_threads)
        .max(1)
        .min(claim_budget.max(1))
}

/// How a publisher reacts when every claim slot is held and it has no claim
/// of its own in flight.
#[derive(Debug, Clone, Copy)]
enum ClaimSlotWait {
    /// End the pass; a periodic caller takes the remaining work next time.
    Yield,
    /// Wait for another publisher's claim to settle or become retryable.
    Await,
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
    operator_pool: Option<OperatorPool>,
    /// Shared durable failure ledger consumed by startup recovery.
    failures: Arc<Mutex<Vec<String>>>,
    /// Replacement actor identity used only for publication authority.
    actor_stream: StreamIdentity,
    /// WAL writer retained for manifest location and generation recovery.
    wal: Arc<WalWriter>,
    /// Bounded CPU lane used for Parquet encoding.
    persistence_cpu: ScribePersistenceCpuPool,
    /// Dedicated lane that runs claim merges.
    ///
    /// A merge can take tens of seconds; on its own threads it never queues
    /// generation staging, so ingest and shutdown's final flush do not wait on
    /// it. It admits every claim the budget lets publish at once and merges as
    /// many together as the pod has effective CPUs.
    assembly_cpu: ScribePersistenceCpuPool,
    /// Claims this worker publishes at once: the assembler's claim budget, so
    /// every claim slot it hands out has a publication driving it.
    claim_budget: usize,
    /// Bounded filesystem lane used for manifest advancement.
    wal_io: ScribeWalIoPool,
    /// Scribe memory budget for persistence workspace reservations.
    memory: ScribeResources,
    /// Optional local wake-up publisher used after confirmed file-list commits.
    staging_file_publisher: Option<StagingFilePublisher>,
    /// Generation-owned scratch authority required before writer creation.
    output_scratch: Option<Arc<ScratchVolume>>,
    /// Test-only fault points for deterministic persistence-path coverage.
    #[cfg(any(test, feature = "test-support"))]
    faults: PersistenceFaults,
    /// Serializes manifest publication after SQL commit.
    manifest_guard: Arc<tokio::sync::Mutex<()>>,
    /// Pod-level owner of staged members and their assembly claims.
    ///
    /// `None` only when this pod was provisioned without a staging volume or
    /// without the operator capability publication requires; such a worker
    /// refuses durable work rather than persisting through a second path.
    staging: Option<Arc<ScribeStagingRuntime>>,
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
    pub fn new(wal_root: &Path, operator: Operator) -> Self {
        Self {
            staging: ScribeStaging::new(wal_root),
            uploader: BifrostParquetUploader::new(operator),
        }
    }

    /// Finalizes, elects, and verifies each artifact while retaining every local winner.
    ///
    /// Returns each elected local winner together with what the verifying
    /// read-back observed about the remote object, so publication can check its
    /// promotion evidence against the bytes that now exist rather than against
    /// the bytes it intended to write.
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
    ) -> Result<(Vec<StagedArtifactClaim>, Vec<VerifiedParquetObject>), ScribeError> {
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
        let mut verified = Vec::with_capacity(claims.len());
        for claim in &claims {
            let identity =
                ParquetObjectIdentity::new(claim.object_key.clone()).map_err(|error| {
                    ScribeError::Internal {
                        detail: format!("invalid recovered Scribe object identity: {error}"),
                    }
                })?;
            let object = self
                .uploader
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
            verified.push(object);
        }
        Ok((claims, verified))
    }

    /// Persists one complete member manifest after every candidate converges.
    ///
    /// # Errors
    ///
    /// Returns an internal error when the complete rows or durable stage
    /// claims contradict their deterministic generation identity.
    pub(crate) async fn persist_publication(
        &self,
        object_base: &str,
        actor_stream: StreamIdentity,
        rows: &[file_list_writer::FileListArtifactInsert],
        claims: &[StagedArtifactClaim],
    ) -> Result<(), ScribeError> {
        self.staging
            .persist_publication(
                &Self::publication_identity(object_base),
                actor_stream,
                rows,
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
    /// fenced SQL convergence, or local post-publication cleanup fails.
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
        let _transfer_owner = memory
            .try_reserve_maintenance(MemoryCategory::Persistence, PARQUET_TRANSFER_BUFFER_BYTES)?;
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
    /// when fenced SQL publication is not known committed, or when exact
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
        match reconciler.publish(&publication.rows).await {
            ScribePublicationOutcome::Committed(_) => {
                self.cleanup_published(&publication.claims).await
            }
            ScribePublicationOutcome::KnownNotCommitted(error)
            | ScribePublicationOutcome::UnknownCommitOutcome(error) => Err(error),
        }
    }
}

/// Move-owned terminal guard for one durable Scribe visibility publication.
#[derive(Debug)]
pub(crate) struct VisibilityPublishGuard {
    /// Production span retained until the durable lifecycle reaches one terminal.
    span: tracing::Span,
    /// Fallback terminal recorded when the owner is dropped before explicit completion.
    drop_outcome: &'static str,
    /// Whether an explicit terminal has already been recorded.
    terminal: bool,
}

impl VisibilityPublishGuard {
    /// Starts one production visibility publication with failure as the pre-commit fallback.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            span: tracing::info_span!(
                "bifrost.scribe.visibility.publish",
                tenant = tracing::field::Empty,
                table = tracing::field::Empty,
                shard_id = tracing::field::Empty,
                generation = tracing::field::Empty,
                outcome = tracing::field::Empty
            ),
            drop_outcome: "failed",
            terminal: false,
        }
    }

    /// Records the generation this publication persists on its span.
    ///
    /// The scrubbed tenant/table pair, shard, and member generation correlate
    /// the publication with the write trace and the durable staged member.
    fn correlate(&self, generation: &ImmutableGeneration) {
        self.span
            .record("tenant", tracing::field::display(&generation.table_key.0));
        self.span
            .record("table", tracing::field::display(&generation.table_key.1));
        self.span.record("shard_id", generation.shard_id);
        self.span.record("generation", generation.generation_id.0);
    }

    /// Returns the publication span so persistence work runs inside it.
    fn span(&self) -> &Span {
        &self.span
    }

    /// Changes abandonment after successful pre-commit into cancellation.
    pub(crate) fn arm_cancellation(&mut self) {
        self.drop_outcome = "cancelled";
    }

    /// Records the exact successful lifecycle terminal once.
    pub(crate) fn succeed(&mut self) {
        self.finish("success");
    }

    /// Records the exact failed lifecycle terminal once.
    pub(crate) fn fail(&mut self) {
        self.finish("failed");
    }

    /// Records the exact cancelled lifecycle terminal once.
    pub(crate) fn cancel(&mut self) {
        self.finish("cancelled");
    }

    /// Records one terminal outcome while preventing double terminalization.
    fn finish(&mut self, outcome: &'static str) {
        if !self.terminal {
            self.span.record("outcome", outcome);
            self.terminal = true;
        }
    }
}

impl Drop for VisibilityPublishGuard {
    /// Records abandonment or task cancellation before closing the production span.
    fn drop(&mut self) {
        if !self.terminal {
            self.span.record("outcome", self.drop_outcome);
            self.terminal = true;
        }
    }
}

/// Closed publication classification controlling cleanup and WAL authority.
#[derive(Debug)]
pub enum ScribePublicationOutcome {
    /// The complete artifact set committed or replayed exactly.
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
    operator_pool: OperatorPool,
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
        operator_pool: OperatorPool,
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

    /// Reports whether the test fault injector refuses the next claim
    /// retirement after its commit landed.
    ///
    /// Compiled only for tests and `test-support`; production has no injector.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn fail_claim_retirement(&self) -> bool {
        self.faults.take_claim_retirement()
    }

    /// Attempts or reconciles one exact full-set publication.
    ///
    /// Any error is conservatively unknown because the SQL owner crosses the
    /// COMMIT boundary internally; callers retain objects and WAL authority.
    pub(crate) async fn publish(
        &self,
        rows: &[file_list_writer::FileListArtifactInsert],
    ) -> ScribePublicationOutcome {
        #[cfg(any(test, feature = "test-support"))]
        let publication_barrier = self.faults.take_publication_barrier(rows);
        #[cfg(any(test, feature = "test-support"))]
        if let Some(barrier) = &publication_barrier {
            barrier.pause_before_publication().await;
        }
        match file_list_writer::insert_artifact_set_fenced(
            &self.operator_pool,
            self.actor_stream,
            rows,
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
    ///
    /// Compiled only for tests and `test-support`; production has no injector.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    fn fail_before_sql_commit(&self) -> bool {
        self.faults.take_sql_commit()
    }

    /// Builds a persistence worker from its complete durable dependencies.
    ///
    /// `staging` pairs the pod's staged lifecycle owner with its claim budget
    /// (the persistence worker count `build_staging` sizes the assembler
    /// with). Also starts the worker's own claim-merge lane: it queues up to
    /// the budget and runs merges on the effective CPUs the persistence lane
    /// does not already occupy (see [`merge_lane_threads`]).
    ///
    /// # Panics
    ///
    /// Panics if the claim-merge threads cannot be started.
    fn new(
        operator_pool: Option<OperatorPool>,
        failures: Arc<Mutex<Vec<String>>>,
        context: PersistenceRuntimeContext,
        output_scratch: Option<Arc<ScratchVolume>>,
        (staging, claim_budget): (Option<Arc<ScribeStagingRuntime>>, usize),
    ) -> Self {
        let claim_budget = claim_budget.max(1);
        let merge_threads = merge_lane_threads(
            context.memory.effective_cpu(),
            context.persistence_cpu.thread_count(),
            claim_budget,
        );
        Self {
            operator_pool,
            failures,
            actor_stream: context.actor_stream,
            wal: context.wal,
            persistence_cpu: context.persistence_cpu,
            assembly_cpu: ScribePersistenceCpuPool::new_with_capacity(merge_threads, claim_budget),
            claim_budget,
            wal_io: context.wal_io,
            memory: context.memory,
            staging_file_publisher: context.staging_file_publisher,
            output_scratch,
            #[cfg(any(test, feature = "test-support"))]
            faults: context.faults,
            manifest_guard: Arc::new(tokio::sync::Mutex::new(())),
            staging,
        }
    }

    /// Processes one queued generation and reports completion to its shard.
    ///
    /// The method performs the ordered persistence stages and always sends a
    /// completion outcome back to the owning shard.
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
        let mut visibility = VisibilityPublishGuard::new();
        visibility.arm_cancellation();
        let generation = Arc::clone(&job.generation);
        visibility.correlate(&generation);
        tracing::debug!(
            generation_id = generation.generation_id.0,
            seal_key = %generation.seal_key,
            wal_lsn_min = generation.wal_lsn_min.as_u64(),
            wal_lsn_max = generation.wal_lsn_max.as_u64(),
            "persisting immutable Scribe generation"
        );
        let result = self
            .persist_generation(&generation, &job)
            .instrument(visibility.span().clone())
            .await;
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
            visibility.span().in_scope(|| {
                tracing::warn!(
                    error = %error,
                    seal_key = %generation.seal_key,
                    "Scribe generation staging failed"
                );
            });
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
            Ok(member) => PersistenceCompletion {
                generation_id: generation.generation_id,
                staged: Some(member),
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

    /// Persists one immutable generation through the durable staging owner.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for any persistence-stage failure.
    async fn persist_generation(
        &self,
        generation: &Arc<ImmutableGeneration>,
        job: &PersistenceJob,
    ) -> Result<crate::scribe::assembly::StagedMemberId, ScribeError> {
        self.persist_once(generation, &job.binding, job.defer_manifest_advance)
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
        visibility: VisibilityPublishGuard,
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

    /// Stages one generation durably and returns the member it became.
    ///
    /// The ordering is the durable contract. The generation's rows are sorted,
    /// encoded, fsynced and preflighted onto the staging volume, the staged
    /// record that makes them a query source is published, and only then does
    /// the stream manifest advance — which is what allows the WAL behind those
    /// rows to be retired. Assembly and fenced publication are not part of this
    /// path: the server's publisher tick publishes due claims through
    /// [`PersistenceRuntime::publish_due`], so a claim merge never holds a
    /// generation, its shard, or shutdown's final flush.
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
    /// the recipe cannot be resolved, or when staging, record publication, or
    /// manifest advancement fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation before the staged record leaves a member directory that
    /// startup cleanup removes. Cancellation after it leaves a durable member
    /// that a later claim publishes.
    async fn persist_once(
        &self,
        generation: &Arc<ImmutableGeneration>,
        binding: &TenantTableBinding,
        defer_manifest_advance: bool,
    ) -> Result<crate::scribe::assembly::StagedMemberId, ScribeError> {
        let generation_id = generation.generation_id.0;
        let frozen = generation.frozen_snapshot();
        let staged = self.stage_member(generation, binding, &frozen).await;
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
        Ok(member)
    }

    /// Encodes and registers one frozen generation as a durable staged member.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the staging capability or the write recipe
    /// is unavailable, when the sorted candidate cannot be charged, when the lane returns the wrong result, or when the runs
    /// or their record cannot be made durable.
    async fn stage_member(
        &self,
        generation: &Arc<ImmutableGeneration>,
        binding: &TenantTableBinding,
        frozen: &FrozenMemtable,
    ) -> Result<crate::scribe::assembly::StagedMemberId, ScribeError> {
        let staging = self.staging()?;
        let layout = crate::scribe::write_recipe::resolve_write_recipe(
            self.operator_pool
                .as_ref()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "writer-v2 staging requires the operator control capability".to_owned(),
                })?,
            binding,
            &frozen.schema,
        )
        .await?;
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
                    memory: self.memory.clone(),
                    staging: Arc::clone(&staging),
                },
            )))
            .await?
        {
            ScribePersistenceCpuResult::MemberStaged(staged) => *staged,
            ScribePersistenceCpuResult::ClaimAssembled(_)
            | ScribePersistenceCpuResult::ReplayRestored(_) => {
                return Err(ScribeError::Internal {
                    detail: "persistence lane returned the wrong staging result".to_owned(),
                });
            }
        };
        record_persist_stage("stage_member", staged_started);
        record_encoded_bytes(usize::try_from(staged.staged_bytes()).unwrap_or(usize::MAX));
        let member = staged.member();
        staging.register_member(staged, chrono::Utc::now()).await?;
        Ok(member)
    }

    /// Publishes every claim that target or dwell has made due.
    ///
    /// Runs from [`PersistenceRuntime::publish_due`] on the server's lifecycle
    /// tick, so a key that reached target publishes within one tick and a key
    /// whose writes stopped still publishes once its dwell expires. Claims an
    /// earlier publication was refused on are retried first (see
    /// [`Self::publish_claims`]), so each tick retries them. Due claims publish
    /// together, up to the claim budget. Concurrent callers are safe: each
    /// claim is driven by exactly one caller at a time.
    ///
    /// A full claim budget ends the pass without error: the tick yields to the
    /// publications holding the slots and takes the remaining due claims on a
    /// later tick.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a due claim cannot be gathered, admitted,
    /// merged, or published. The claim stays outstanding and its members stay
    /// durable, so the next tick retries it rather than losing rows.
    async fn publish_due_claims(&self) -> Result<usize, ScribeError> {
        let staging = self.staging()?;
        self.publish_claims(&staging, ClaimSlotWait::Yield, || {
            staging.take_claim(chrono::Utc::now())
        })
        .await
    }

    /// Publishes every retryable claim and every claim `next` hands out, up to
    /// the claim budget at once.
    ///
    /// Claims are independent — each has its own key, members, scratch and
    /// fenced transaction — so they merge, upload and commit together instead
    /// of each waiting for the one before it. Each pass first resumes every
    /// outstanding claim an earlier publication was refused on (see
    /// [`Self::resume_claim`]), then asks `next` for new claims while fewer
    /// than `claim_budget` are in flight; a pass runs again after each claim
    /// settles, so work that becomes due meanwhile is picked up. Publication
    /// ends when there is nothing to take and nothing is in flight. Returns
    /// the number of claims settled.
    ///
    /// A full claim budget is backpressure, not a failure. This publisher
    /// stops taking claims and lets its own in-flight claims settle; when it
    /// has none, `slots` decides whether it ends the pass
    /// ([`ClaimSlotWait::Yield`]) or waits for another publisher's claim to
    /// end its drive ([`ClaimSlotWait::Await`]).
    ///
    /// # Errors
    ///
    /// Returns the first error from `next`, from reading retryable claims, or
    /// from a claim's publication. No further claim is taken after it, but
    /// claims already in flight run to their own settlement, so none is
    /// abandoned between its fenced commit and its retirement. A failed claim
    /// stays outstanding with durable members and retries. A waiting
    /// publisher fails when the budget is exhausted and no publisher in this
    /// process drives a claim, because no slot can then be released.
    ///
    /// # Cancellation
    ///
    /// Dropping the future drops every in-flight publication and releases its
    /// drive; each converges on retry like a single interrupted claim, and the
    /// WAL stays authoritative for its rows.
    async fn publish_claims(
        &self,
        staging: &Arc<ScribeStagingRuntime>,
        slots: ClaimSlotWait,
        mut next: impl FnMut() -> Result<Option<DrivenClaim>, ClaimTakeError>,
    ) -> Result<usize, ScribeError> {
        use futures_util::FutureExt as _;
        use futures_util::StreamExt as _;
        // A fresh claim was just taken from ready members, so only a retried
        // one can have committed already.
        let drive = |claim: DrivenClaim, retried: bool| async move {
            if retried {
                self.resume_claim(staging, &claim).await
            } else {
                self.publish_claim(staging, &claim).await
            }
        };
        let mut in_flight = futures_util::stream::FuturesUnordered::new();
        let mut published = 0_usize;
        let mut failure = None;
        loop {
            // Registered before any claim is asked for, so a drive that ends
            // while this pass runs still wakes a waiting publisher.
            let mut released = std::pin::pin!(staging.claim_released());
            let mut exhausted = None;
            if failure.is_none() {
                match staging.retryable_claims() {
                    Ok(claims) => {
                        in_flight.extend(claims.into_iter().map(|claim| drive(claim, true)));
                    }
                    Err(error) => failure = Some(error),
                }
            }
            while failure.is_none() && exhausted.is_none() && in_flight.len() < self.claim_budget {
                match next() {
                    Ok(Some(claim)) => in_flight.push(drive(claim, false)),
                    Ok(None) => break,
                    Err(ClaimTakeError::BudgetExhausted { budget }) => exhausted = Some(budget),
                    Err(ClaimTakeError::Failed(error)) => failure = Some(error),
                }
            }
            if in_flight.is_empty() {
                let (Some(budget), ClaimSlotWait::Await, None) = (exhausted, slots, &failure)
                else {
                    break;
                };
                if !staging.drives_no_claims() {
                    #[cfg(any(test, feature = "test-support"))]
                    self.faults.note_claim_slot_wait();
                    released.await;
                } else if released.as_mut().now_or_never().is_none() {
                    failure = Some(ClaimTakeError::BudgetExhausted { budget }.into());
                    break;
                }
                continue;
            }
            match in_flight.next().await {
                Some(Ok(())) => published += 1,
                Some(Err(error)) => {
                    failure.get_or_insert(error);
                }
                None => break,
            }
        }
        failure.map_or(Ok(published), Err)
    }

    /// Publishes every staged member that target and dwell would still hold.
    ///
    /// This is the explicit flush: it sweeps every ready key as residue rather
    /// than waiting for target or dwell. Shutdown does not call it; staged
    /// members survive a restart. Publication is still the same fenced
    /// transaction, so a member that cannot publish stays durable and staged
    /// rather than being dropped.
    ///
    /// Returns the number of claims published.
    ///
    /// An outstanding claim whose publication was refused is resumed before
    /// the ready sweep, because settlement is what returns members to the
    /// budget and a refused claim never settled. When every claim slot is held
    /// by another publisher, the flush waits for one rather than failing.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the staging capability is absent, when the
    /// assembler refuses a residue claim, when every claim slot is held by a
    /// claim no publisher can run again, or when a claim fails to publish. The
    /// remaining keys stay staged and the WAL stays authoritative for them.
    async fn publish_residue(
        &self,
        cause: crate::scribe::assembly::ClaimCause,
    ) -> Result<usize, ScribeError> {
        let staging = self.staging()?;
        // A claim whose publication was refused keeps its slot and its members;
        // nothing returns it to the ready index, so the sweep below cannot see
        // it. `publish_claims` drives those claims first, under their original
        // identity, and the sweep's claims then publish together with them, up
        // to the claim budget.
        let mut keys = staging.ready_keys()?.into_iter();
        let mut key = keys.next();
        self.publish_claims(&staging, ClaimSlotWait::Await, || {
            while let Some(current) = &key {
                if let Some(claim) = staging.take_residue(current, cause)? {
                    return Ok(Some(claim));
                }
                key = keys.next();
            }
            Ok(None)
        })
        .await
    }

    /// Publishes only the residue of the assembly keys owning one partition.
    ///
    /// Publication is otherwise pod-wide: every ready key settles together, so
    /// no caller can observe a pod in which one partition is served by a hot
    /// object while a neighbouring partition is still served by its live
    /// authority. That mixed state is a real production state — a claim becomes
    /// due when its own dwell or target says so, not when the pod is quiescent
    /// — and it is the state in which a cut must distinguish what it actually
    /// owns from what merely falls inside its bounds.
    ///
    /// This selects an already-existing ready key and drives the same residue
    /// claim and fenced publication owner `publish_residue` uses. It fabricates
    /// no member, object, row, or cut.
    ///
    /// Returns the number of claims published.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the staging capability is absent, when the
    /// assembler refuses a residue claim, or when a claim fails to publish. The
    /// remaining keys stay staged and the WAL stays authoritative for them.
    #[cfg(any(test, feature = "test-support"))]
    async fn publish_partition_residue(
        &self,
        partition: crate::catalog::layout::TimePartition,
    ) -> Result<usize, ScribeError> {
        let staging = self.staging()?;
        let mut published = 0;
        for key in staging.ready_keys()? {
            if key.partition() != partition {
                continue;
            }
            while let Some(claim) =
                staging.take_residue(&key, crate::scribe::assembly::ClaimCause::Drain)?
            {
                self.publish_claim(&staging, &claim).await?;
                published += 1;
            }
        }
        Ok(published)
    }

    /// Resumes one claim whose earlier publication was refused.
    ///
    /// A refusal after the fenced commit leaves only retirement to do, so the
    /// claim is first offered to
    /// [`ScribeStagingRuntime::finish_committed`](crate::scribe::staging_runtime::ScribeStagingRuntime::finish_committed);
    /// a claim it finishes is reported exactly like one this worker published
    /// — the Forge wake-up is sent and its slot is free — without merging or
    /// committing again. Every other claim publishes again under its own
    /// identity through [`Self::publish_claim`].
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the committed claim cannot be retired or
    /// settled, or for any refusal [`Self::publish_claim`] reports. The claim
    /// stays outstanding and the next tick resumes it again.
    async fn resume_claim(
        &self,
        staging: &Arc<ScribeStagingRuntime>,
        claim: &DrivenClaim,
    ) -> Result<(), ScribeError> {
        if !staging.finish_committed(claim).await? {
            return self.publish_claim(staging, claim).await;
        }
        tracing::info!(
            operation = "scribe_claim_publication",
            outcome = "retired_after_commit",
            claim = %claim.id(),
            tenant = %claim.key().tenant(),
            table = %claim.key().table(),
            members = claim.members().len(),
            "Scribe finished retiring a claim whose publication had already committed"
        );
        #[cfg(any(test, feature = "test-support"))]
        self.faults.note_claim_published();
        self.publish_staging_hint(claim.key());
        Ok(())
    }

    /// Merges and publishes exactly one outstanding claim.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the claim's runs no longer match their
    /// records, when output scratch cannot be created, when a merged batch or
    /// the upload chunk cannot be charged to the shared root, when the lane returns the wrong result, or when the fenced publication is
    /// refused or uncertain.
    async fn publish_claim(
        &self,
        staging: &Arc<ScribeStagingRuntime>,
        claim: &DrivenClaim,
    ) -> Result<(), ScribeError> {
        let runs = staging.gather(claim).await?;
        let scratch = self
            .output_scratch
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe output scratch is unavailable before claim assembly".to_owned(),
            })?
            .create_scribe_claim(&claim.key().node_id().to_string(), &claim.id().to_string())
            .map_err(|error| ScribeError::Internal {
                detail: format!("Scribe claim scratch creation failed: {error}"),
            })?;
        #[cfg(any(test, feature = "test-support"))]
        self.faults.pass_object_write_hold().await;
        #[cfg(any(test, feature = "test-support"))]
        let _object_write_guard = self.faults.begin_object_write().await;
        let mut assembled = match self
            .assemble_claim_output(staging, claim, &runs, scratch.path())
            .await
        {
            Ok(assembled) => assembled,
            Err(error) => {
                // Assembly never reached the fenced publication, so nothing this
                // claim produced can be authoritative. Remove the scratch
                // directory now, on a blocking boundary, rather than leaving it
                // for restart reconciliation.
                discard_claim_scratch(scratch).await;
                return Err(error);
            }
        };
        assembled.artifacts.attach_scratch(scratch)?;
        // The upload's one transfer chunk is the only buffer publication owns.
        let transfer = self
            .memory
            .try_reserve_maintenance(MemoryCategory::Persistence, PARQUET_TRANSFER_BUFFER_BYTES);
        let published = match transfer {
            Ok(_transfer) => {
                staging
                    .publish(claim, &runs, &assembled, self.actor_stream)
                    .await
            }
            Err(error) => Err(error),
        };
        if let Err(error) = &published {
            tracing::warn!(
                operation = "scribe_claim_publication",
                outcome = "refused",
                claim = %claim.id(),
                tenant = %claim.key().tenant(),
                table = %claim.key().table(),
                members = claim.members().len(),
                error = %error,
                "Scribe claim publication was refused; its members stay staged"
            );
        }
        // The scratch is disposable once the candidate has been uploaded: the
        // object and its publication manifest are the recovery evidence, and the
        // success path deletes the scratch immediately after the commit anyway.
        // Cleaning it on both terminals keeps a refused publication retryable
        // instead of poisoning volume health through the artifact set's drop.
        assembled.artifacts.cleanup().await?;
        let published = published?;
        #[cfg(any(test, feature = "test-support"))]
        self.faults.note_claim_published();
        self.publish_staging_hint(claim.key());
        if let Some(detail) = published.unacknowledged {
            let error = ScribeError::Internal { detail };
            // The publication committed and every member retired; the response
            // to it did not survive. Failing here keeps the flush honest about
            // what its caller knows without leaving the pod holding a claim
            // whose rows a durable object already serves.
            return Err(error);
        }
        Ok(())
    }

    /// Merges one claim's gathered runs into sealed artifacts under `scratch_dir`.
    ///
    /// Split out of [`Self::publish_claim`] so every pre-publication failure
    /// surfaces as one error value whose caller still owns the claim scratch and
    /// can clean it explicitly.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a test fault seam is armed, when
    /// the persistence lane rejects the assembly, or when the lane returns a
    /// result that does not belong to claim assembly.
    async fn assemble_claim_output(
        &self,
        staging: &Arc<ScribeStagingRuntime>,
        claim: &StagingClaim,
        runs: &ClaimRuns,
        scratch_dir: &Path,
    ) -> Result<AssembledClaim, ScribeError> {
        #[cfg(any(test, feature = "test-support"))]
        if self.fail_before_sql_commit() {
            return Err(ScribeError::Internal {
                detail: "test SQL failure before the first COMMIT attempt".to_owned(),
            });
        }
        #[cfg(any(test, feature = "test-support"))]
        if self.faults.take_object_write() {
            return Err(ScribeError::Internal {
                detail: "test object-store write failure".to_owned(),
            });
        }
        tracing::debug!(claim = %claim.id(), stage = "assemble_claim", "persist stage start");
        let assemble_started = std::time::Instant::now();
        let assembled = match self
            .assembly_cpu
            .submit(ScribePersistenceCpuOp::AssembleClaim(Box::new(
                crate::scribe::execution_lanes::AssembleClaimOp {
                    claim: Box::new(claim.clone()),
                    runs: Box::new(runs.clone()),
                    scratch_dir: scratch_dir.to_path_buf(),
                    memory: self.memory.clone(),
                    staging: Arc::clone(staging),
                },
            )))
            .await?
        {
            ScribePersistenceCpuResult::ClaimAssembled(assembled) => *assembled,
            ScribePersistenceCpuResult::MemberStaged(_)
            | ScribePersistenceCpuResult::ReplayRestored(_) => {
                return Err(ScribeError::Internal {
                    detail: "persistence lane returned the wrong assembly result".to_owned(),
                });
            }
        };
        record_persist_stage("assemble_claim", assemble_started);
        Ok(assembled)
    }

    /// Returns this worker's staged lifecycle owner.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the pod was provisioned without a
    /// staging volume or without the operator capability publication requires.
    /// Refusing is the conservative outcome: the WAL stays authoritative.
    fn staging(&self) -> Result<Arc<ScribeStagingRuntime>, ScribeError> {
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
    mut visibility: VisibilityPublishGuard,
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
    /// The claim-merge lane takes the CPUs the persistence lane leaves free.
    ///
    /// # Panics
    ///
    /// Panics when the merge lane oversubscribes the pod, drops below one
    /// thread, or exceeds the claim budget.
    #[test]
    fn merge_lane_threads_leave_the_persistence_lane_its_cpus() {
        assert_eq!(
            super::merge_lane_threads(8, 2, 4),
            4,
            "capped by the budget"
        );
        assert_eq!(super::merge_lane_threads(8, 6, 4), 2, "only the free CPUs");
        assert_eq!(
            super::merge_lane_threads(2, 2, 4),
            1,
            "never below one thread"
        );
        assert_eq!(
            super::merge_lane_threads(4, 0, 0),
            1,
            "an empty budget still merges"
        );
    }

    use super::*;
    use std::future::Future;
    use std::panic::AssertUnwindSafe;
    #[cfg(feature = "test-support")]
    use std::path::PathBuf;
    use std::task::{Context, Poll};

    #[cfg(feature = "test-support")]
    use crate::resources::BifrostRuntimeResources;
    use crate::test_support::{SpanCaptureSubscriber, has_span_outcome};

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
            reconciler: None,
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
                super::VisibilityPublishGuard::new(),
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
                super::VisibilityPublishGuard::new(),
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
                super::VisibilityPublishGuard::new(),
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
                super::VisibilityPublishGuard::new(),
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
            let mut visibility = super::VisibilityPublishGuard::new();
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
        /// Authority registry the fixture registers its generation in.
        ///
        /// Production registers at memtable freeze; this fixture has no
        /// memtable, so it performs the same registration itself rather than
        /// letting staging advance an authority nothing ever held.
        hot_sources: Arc<crate::scribe::hot_source::ScribeHotSourceRegistry>,
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
            scratch_root: &Path,
            wal_root: &Path,
            scribe_stage: PathBuf,
            scribe_output: PathBuf,
        ) -> BifrostRuntimeResources {
            BifrostRuntimeResources::from_snapshot(
                crate::resources::SystemResourceSnapshot {
                    memory_limit_bytes: 2 * 1024 * 1024 * 1024,
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
                    server_memory_min_bytes: None,
                    bifrost_memory_limit_bytes: None,
                    scratch_limit_bytes: None,
                    effective_cpu: None,
                    oracle_query_slot_limit: None,
                    scratch_root: Some(scratch_root.to_owned()),
                    volume_roots: Some(crate::resources::BifrostVolumeRoots {
                        wal: wal_root.to_owned(),
                        scribe_stage,
                        scribe_output_scratch: scribe_output,
                    }),
                },
            )
            .expect("test Bifrost resources")
        }

        /// Starts one real persistence runtime before it accepts work.
        ///
        /// # Panics
        ///
        /// Panics when the Postgres fixture, its superuser pool, the temporary
        /// WAL or scratch roots, the cluster-node seed row, or the persistence
        /// runtime itself cannot be set up.
        async fn start() -> Self {
            let database = wyrd_dev_fixtures::pg::PgFixture::start()
                .await
                .expect("Postgres fixture");
            let tenant = database.data_tenant_id();
            let wal_root = tempfile::tempdir().expect("WAL directory");
            let scratch_root = tempfile::tempdir().expect("scratch directory");
            let scribe_stage = wal_root.path().join("scribe-stage");
            let scribe_output = scratch_root.path().join("scribe-output");
            for root in [&scribe_stage, &scribe_output] {
                std::fs::create_dir(root).expect("test volume root");
            }
            let node_id = crate::scribe::stream_identity::NodeId::generate();
            let stream =
                StreamIdentity::new(node_id, crate::scribe::stream_identity::WriterEpoch::new(1));
            let superuser = database.superuser_pool().expect("superuser pool");
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
            );
            let roles = runtime_resources
                .compose_roles()
                .expect("test role resources");
            let memory = roles.scribe().expect("test Scribe resources");
            let output_scratch = memory.output_scratch().expect("test Scribe output scratch");
            let hot_sources = Arc::new(crate::scribe::hot_source::ScribeHotSourceRegistry::new());
            let runtime = PersistenceRuntime::start(
                ScribePersistenceConfig::new(Arc::new(database.vala_postgres().clone()), 1, 1)
                    .with_operator_pool(database.operator_pool().clone())
                    .with_output_scratch(output_scratch),
                PersistenceRuntimeContext {
                    operator: Arc::new(
                        Operator::new(opendal::services::Memory::default())
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
                    hot_sources: Arc::clone(&hot_sources),
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
                hot_sources,
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
            self.hot_sources
                .register_memtable(
                    &seal_key,
                    crate::scribe::hot_source::GenerationOrdinal::new(0, 1),
                )
                .expect("the fixture generation registers once");
            let batch_id = uuid::Uuid::now_v7();
            self.register_control_row(&table).await;
            let schema = Self::fixture_schema();
            let rows = vec![
                arrow::record_batch::RecordBatch::try_new(
                    Arc::clone(&schema),
                    vec![
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
            1
        );
        assert_eq!(
            snapshot
                .gauges
                .get("bifrost_scribe_persistence_queue_depth"),
            Some(&0.0)
        );

        fixture.submit_and_drain().await;

        let drained = recorder.snapshot();
        assert_eq!(
            drained.gauges.get("bifrost_scribe_persistence_queue_depth"),
            Some(&0.0)
        );
    }

    /// Per-generation encoded bytes are counted after a durable persist.
    ///
    /// Drives one complete `persist_once` through the real persistence runtime
    /// using the [`IdlePersistenceFixture`] and asserts that
    /// `bifrost_scribe_persistence_encoded_bytes_total` is incremented by a
    /// positive amount (the Parquet-encoded size).
    #[tokio::test(flavor = "current_thread")]
    #[cfg(feature = "test-support")]
    async fn persist_once_counts_encoded_bytes() {
        let recorder = wyrd_bench::BenchmarkRecorder::new();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let fixture = IdlePersistenceFixture::start().await;
        fixture.submit_and_drain().await;

        let encoded_bytes = recorder
            .snapshot()
            .counters
            .get("bifrost_scribe_persistence_encoded_bytes_total")
            .copied();
        assert!(
            encoded_bytes.is_some_and(|bytes| bytes > 0),
            "encoded bytes must be counted after persist_once"
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
    /// `{stage_member, assemble_claim}` set and carries no identity labels, so the
    /// split extends the end-to-end publication histogram without leaking tenant
    /// or table dimensions.
    #[test]
    fn persist_stage_histograms_split_by_stage() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            record_persist_stage("stage_member", std::time::Instant::now());
            record_persist_stage("assemble_claim", std::time::Instant::now());
        });
        let snapshot = recorder.snapshot();
        for stage in ["stage_member", "assemble_claim"] {
            let key = format!("bifrost_scribe_persist_stage_seconds{{stage=\"{stage}\"}}");
            assert!(
                snapshot.histograms.contains_key(&key),
                "missing stage histogram {key}"
            );
        }
        // Match forbidden label *keys* (`name="`), not value substrings.
        assert!(!snapshot.histograms.keys().any(|key| {
            ["tenant", "table", "path", "request", "node", "error", "sql"]
                .iter()
                .any(|forbidden| key.contains(&format!("{forbidden}=\"")))
        }));
    }
}
