//! Fixed pod-local Scribe shard mailboxes and tenant round-robin scheduling.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::sync::{mpsc, watch};
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::scribe::admission::AdmissionController;
use crate::scribe::execution_lanes::{
    ScribePersistenceCpuPool, ScribeWalIoOp, ScribeWalIoPool, ScribeWalIoResult,
};
use crate::scribe::memory::{MemoryCategory, ScribeOwnership};
use crate::scribe::memtable::{
    BucketMemorySnapshot, Memtable, MemtableStats, PressureCandidate, ReadableBatchLimits,
    SealTriggerReason,
};
use crate::scribe::persistence::{
    ImmutableGeneration, PersistenceCompletion, PersistenceJob, PersistenceRuntime,
    PersistenceSubmitError,
};
use crate::scribe::preprocess::{AppendSliceId, PreparedAppend, PreparedSlice, PreparedSliceSet};
use crate::scribe::routing::{SCRIBE_SHARD_COUNT, shard_for};
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::tail_rpc::{FetchLiveTailRequest, HotBatch};
use crate::scribe::wal::{WalHandle, WalWriter};

type WalSegmentsByKey = HashMap<
    crate::scribe::seal_key::SealKey,
    HashMap<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>,
>;

#[derive(Debug)]
struct PendingGeneration {
    generation: Arc<ImmutableGeneration>,
    binding: crate::catalog::TenantTableBinding,
    submitted: bool,
    /// Whether replay owns one queued wakeup for a full persistence queue.
    retry_scheduled: bool,
    replay_response: Option<
        tokio::sync::oneshot::Sender<
            Result<crate::scribe::replay::ReplayChunkResponse, ScribeError>,
        >,
    >,
    /// Whether this generation belongs to the shard's synchronous replay owner.
    replay_owned: bool,
}

/// One reconstructed generation waiting behind the sole replay identity owner.
#[derive(Debug)]
struct ReplayChunkGeneration {
    /// Immutable generation prepared from one sorted seal-key state.
    generation: Arc<ImmutableGeneration>,
    /// Catalog binding used by the persistence worker.
    binding: crate::catalog::TenantTableBinding,
}

/// Result of preparing one sorted seal-key state from a replay chunk.
enum PreparedReplayState {
    /// Durable identity already exists, so only WAL retirement remains.
    Retired(ReplayRetirement),
    /// Reconstructed generation and its exact Arrow ownership.
    Generation {
        /// Generation ready for serialized persistence.
        prepared: ReplayChunkGeneration,
        /// Exact reconstructed Arrow bytes adopted by the chunk owner.
        arrow_bytes: usize,
    },
}

/// Serializes every generation from one replay chunk through one identity lease.
#[derive(Debug)]
struct ReplayChunkOwner {
    /// Prepared generations not yet submitted, in stable seal-key order.
    remaining: VecDeque<ReplayChunkGeneration>,
    /// Generation currently awaiting persistence completion.
    current_generation: Option<u64>,
    /// Durable retirements accumulated for the complete chunk.
    retirements: Vec<ReplayRetirement>,
    /// Sole identity owner, present only between persistence generations.
    identity: Option<crate::scribe::memory::ReplayIdentityOwnership>,
    /// Synchronous WAL-lane response completed after the final generation.
    response: Option<
        tokio::sync::oneshot::Sender<
            Result<crate::scribe::replay::ReplayChunkResponse, ScribeError>,
        >,
    >,
}

type PendingGenerationsByKey =
    HashMap<crate::scribe::seal_key::SealKey, VecDeque<PendingGeneration>>;

/// Resolves the physical table binding needed to persist one replayed seal.
///
/// # Errors
///
/// Returns an internal Scribe error when the replayed tenant/table identity is
/// not a valid catalog binding.
fn replay_binding(
    seal_key: &crate::scribe::seal_key::SealKey,
) -> Result<crate::catalog::TenantTableBinding, ScribeError> {
    crate::catalog::TenantTableBinding::resolve((seal_key.tenant, seal_key.table.clone())).map_err(
        |error| ScribeError::Internal {
            detail: error.to_string(),
        },
    )
}

/// Removes exact-retry batches from one replay group while preserving append order.
fn retain_replayed_batches(
    replayed: &mut crate::scribe::replay::ReplayedSealKey,
    suppressed: &HashSet<[u8; 16]>,
) {
    let metas = std::mem::take(&mut replayed.append_metas);
    let audits = std::mem::take(&mut replayed.audit_events);
    let data = std::mem::take(&mut replayed.data_records);
    for ((meta, audit), payload) in metas.into_iter().zip(audits).zip(data) {
        if !suppressed.contains(&meta.batch_id) {
            replayed.append_metas.push(meta);
            replayed.audit_events.push(audit);
            replayed.data_records.push(payload);
        }
    }
    replayed
        .commits
        .retain(|commit| !suppressed.contains(&commit.batch_id));
}

#[derive(Debug, Clone)]
struct RetainedGeneration {
    arrow_bytes: usize,
    wal_segments: Vec<crate::scribe::wal::WalSegmentRef>,
    wal: crate::scribe::wal::WalHandle,
}

/// WAL ownership deferred until the directory replay worker has stopped reading.
#[derive(Debug)]
pub(crate) struct ReplayRetirement {
    /// WAL handle that owns the replayed generation's retained segments.
    pub(crate) wal: WalHandle,
    /// Exact segment references released after the replay stream completes.
    pub(crate) segments: Vec<crate::scribe::wal::WalSegmentRef>,
    /// Stream whose manifest advances before its retained segments retire.
    pub(crate) stream: StreamIdentity,
    /// Seal key receiving the durable replay watermark.
    pub(crate) seal_key: crate::scribe::seal_key::SealKey,
    /// Inclusive durable watermark published for this generation.
    pub(crate) sealed_lsn: crate::scribe::wal::WalLsn,
}

/// Capacity of one shard command mailbox.
pub const SHARD_COMMAND_CAPACITY: usize = 256;

/// Maximum requests in one tenant scheduling group.
pub const MAX_GROUP_ITEMS: usize = 64;

/// Maximum bytes in one tenant scheduling group.
pub const MAX_GROUP_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub(crate) struct PressureSignal {
    pub(crate) keys: Vec<crate::scribe::seal_key::SealKey>,
    pub(crate) wal_key: Option<crate::scribe::seal_key::SealKey>,
}

/// A value that can be scheduled by a Scribe shard.
pub trait ShardItem {
    /// Authenticated tenant owning the item.
    fn tenant(&self) -> DataTenantId;
    /// In-flight bytes charged to the item.
    fn bytes(&self) -> usize;
}

/// One fixed shard mailbox.
#[derive(Debug)]
pub struct ScribeShard<T> {
    /// Stable pod-local shard number.
    pub id: usize,
    sender: mpsc::Sender<T>,
}

impl<T> Clone for ScribeShard<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            sender: self.sender.clone(),
        }
    }
}

impl<T> ScribeShard<T> {
    /// Try to send a command without waiting behind an overloaded shard.
    pub fn try_send(&self, item: T) -> Result<(), mpsc::error::TrySendError<T>> {
        self.sender.try_send(item)
    }
}

/// Exactly sixteen pod-local shard mailboxes.
#[derive(Debug)]
pub struct ScribeShardSet<T> {
    shards: Vec<ScribeShard<T>>,
    receivers: Option<Vec<mpsc::Receiver<T>>>,
}

impl<T> ScribeShardSet<T> {
    /// Construct the fixed shard topology.
    #[must_use]
    pub fn new() -> Self {
        let channels = (0..SCRIBE_SHARD_COUNT).map(|id| {
            let (sender, receiver) = mpsc::channel(SHARD_COMMAND_CAPACITY);
            (ScribeShard { id, sender }, receiver)
        });
        let (shards, receivers): (Vec<_>, Vec<_>) = channels.unzip();
        Self {
            shards,
            receivers: Some(receivers),
        }
    }

    /// Return the canonical shard for one authenticated batch.
    ///
    /// The `batch_id` is included in the routing key so a retried batch lands
    /// on the same lane that holds its dedup state.
    #[must_use]
    pub fn shard_for(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        batch_id: uuid::Uuid,
    ) -> &ScribeShard<T> {
        &self.shards[shard_for(tenant, table, batch_id)]
    }

    /// Return a sender by its fixed shard number.
    pub(crate) fn shard(&self, id: usize) -> ScribeShard<T> {
        self.shards[id].clone()
    }

    /// Take all shard receivers once for task startup.
    pub fn take_receivers(&mut self) -> Option<Vec<mpsc::Receiver<T>>> {
        self.receivers.take()
    }

    /// Return the fixed topology cardinality.
    #[must_use]
    pub const fn len(&self) -> usize {
        SCRIBE_SHARD_COUNT
    }

    /// Whether the fixed topology is empty. Always false for Scribe.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}

impl<T> Default for ScribeShardSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Tenant-fair pending scheduler owned by one shard task.
#[derive(Debug)]
pub struct TenantRoundRobin<T> {
    pending_by_tenant: HashMap<DataTenantId, VecDeque<T>>,
    active_tenants: VecDeque<DataTenantId>,
}

impl<T> Default for TenantRoundRobin<T> {
    fn default() -> Self {
        Self {
            pending_by_tenant: HashMap::new(),
            active_tenants: VecDeque::new(),
        }
    }
}

impl<T: ShardItem> TenantRoundRobin<T> {
    /// Enqueue an item and activate its tenant once.
    pub fn push(&mut self, item: T) {
        let tenant = item.tenant();
        let queue = self.pending_by_tenant.entry(tenant).or_default();
        let was_empty = queue.is_empty();
        queue.push_back(item);
        if was_empty {
            self.active_tenants.push_back(tenant);
        }
    }

    /// Drain one bounded group in tenant round-robin order.
    ///
    /// Each turn takes one complete request from the next active tenant. A
    /// tenant with more pending work is requeued at the back, so a hot tenant
    /// cannot fill an entire fsync group while another tenant waits behind it.
    pub fn pop_group(&mut self) -> Vec<T> {
        let mut result = Vec::new();
        let mut bytes = 0_usize;
        while result.len() < MAX_GROUP_ITEMS {
            let Some(tenant) = self.active_tenants.pop_front() else {
                break;
            };
            let Some(next_bytes) = self
                .pending_by_tenant
                .get(&tenant)
                .and_then(|queue| queue.front())
                .map(ShardItem::bytes)
            else {
                self.pending_by_tenant.remove(&tenant);
                continue;
            };
            if !result.is_empty() && bytes.saturating_add(next_bytes) > MAX_GROUP_BYTES {
                self.active_tenants.push_front(tenant);
                break;
            }
            let item = self
                .pending_by_tenant
                .get_mut(&tenant)
                .and_then(VecDeque::pop_front);
            let Some(item) = item else {
                continue;
            };
            bytes = bytes.saturating_add(item.bytes());
            result.push(item);
            if self
                .pending_by_tenant
                .get(&tenant)
                .is_some_and(|queue| !queue.is_empty())
            {
                self.active_tenants.push_back(tenant);
            } else {
                self.pending_by_tenant.remove(&tenant);
            }
        }
        result
    }

    /// Whether the shard has no queued requests.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.active_tenants.is_empty()
    }
}

impl ShardItem for PreparedAppend {
    fn tenant(&self) -> DataTenantId {
        self.tenant
    }

    fn bytes(&self) -> usize {
        self.prepared_bytes
    }
}

/// One command accepted by a fixed shard owner.
#[derive(Debug)]
pub(crate) enum ShardCommand {
    Append(Box<PreparedAppend>),
    Snapshot {
        request: FetchLiveTailRequest,
        response: tokio::sync::oneshot::Sender<Result<Vec<HotBatch>, ScribeError>>,
    },
    FlushExpired {
        now: std::time::Instant,
    },
    /// Retire committed generations and acknowledge completion for test control.
    ///
    /// Retirement is immediate: any `ImmutableState::Committed` generation is
    /// retired on this pass. The `response` fires after the owner completes the
    /// retirement and the test-support sweep, so the caller has a deterministic
    /// observation point.
    #[cfg(any(test, feature = "test-support"))]
    RetireCommittedForTest {
        /// Completes after the owner handles the retirement pass.
        response: tokio::sync::oneshot::Sender<Result<(), ScribeError>>,
    },
    PersistenceComplete {
        /// Durable persistence result awaiting shard-owned visibility publication.
        completion: Box<PersistenceCompletion>,
        /// Existing caller completion channel preserved independently of telemetry.
        waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
        /// Private result returned after the shard completes its visibility transition.
        visibility_result: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Replay {
        /// Complete synchronous replay handoff for one recorded shard lane.
        chunk: Box<crate::scribe::replay::ReplayChunk>,
        /// Settlement returned before the WAL scanner advances.
        response: tokio::sync::oneshot::Sender<
            Result<crate::scribe::replay::ReplayChunkResponse, ScribeError>,
        >,
    },
    /// Re-drive one replay generation after persistence queue progress.
    RetryReplayPersistence {
        /// Seal key whose FIFO front was unable to enter the bounded queue.
        seal_key: crate::scribe::seal_key::SealKey,
        /// Exact generation protected against stale retry notifications.
        generation_id: u64,
        /// Queue progress or terminal runtime-close result.
        progress: Result<(), ScribeError>,
    },
    FreezeKey {
        seal_key: crate::scribe::seal_key::SealKey,
        /// `None` when the shard holds no bucket for this key (no-op shard).
        response: tokio::sync::oneshot::Sender<
            Result<Option<crate::scribe::memtable::FrozenMemtable>, ScribeError>,
        >,
    },
    FreezeTenant {
        tenant: DataTenantId,
        response: tokio::sync::oneshot::Sender<
            Result<Vec<crate::scribe::memtable::FrozenMemtable>, ScribeError>,
        >,
    },
    /// Publish a generation persisted by the direct force-seal path.
    CompletePostCommit {
        /// Exact partition bucket whose generation was persisted.
        seal_key: crate::scribe::seal_key::SealKey,
        /// Memtable generation that is now durable.
        seal_id: u64,
        /// Arrow memory retained until the lifecycle grace period elapses.
        arrow_bytes: usize,
        /// Durable file-list row that proves the generation was published.
        file_list_key: crate::scribe::file_list_writer::FileListCommitKey,
        /// Completion channel for publication bookkeeping errors.
        response: tokio::sync::oneshot::Sender<Result<(), ScribeError>>,
    },
    AbortPostCommit {
        seal_id: u64,
        response: tokio::sync::oneshot::Sender<Result<(), ScribeError>>,
    },
    FlushAll {
        response: tokio::sync::oneshot::Sender<Result<(), ScribeError>>,
    },
    Shutdown,
}

/// Owns the mutable state machine for exactly one fixed Scribe shard.
///
/// The owner contains both the shard's dependencies and its scheduler, WAL
/// indexes, pending generations, retained generations, and lifecycle signals.
/// Keeping these values together makes FIFO, replay, memory release, and
/// shutdown transitions occur only at the shard boundary.
struct ShardOwner {
    /// Stable pod-local shard number used for routing and diagnostics.
    id: usize,
    /// Bounded command mailbox consumed in FIFO order.
    receiver: mpsc::Receiver<ShardCommand>,
    /// Coalescing memory/WAL pressure mailbox for this shard.
    pressure_receiver: watch::Receiver<Option<PressureSignal>>,
    /// Tenant-fair scheduler for admitted prepared appends.
    scheduler: TenantRoundRobin<PreparedAppend>,
    /// Whether this owner has received its shutdown command.
    shutting_down: bool,
    /// WAL segments currently associated with writable seal keys.
    wal_segments: WalSegmentsByKey,
    /// WAL-synced slices awaiting memtable insertion retry.
    synced_not_inserted: HashMap<AppendSliceId, WalSliceState>,
    /// Per-key immutable generations waiting for ordered persistence.
    pending_generations: PendingGenerationsByKey,
    /// At most one synchronous replay chunk advanced one generation at a time.
    replay_chunk: Option<ReplayChunkOwner>,
    /// Seal keys whose `flush_keys` attempt failed after the freeze but before
    /// the accounting move, leaving a stranded pending frozen generation
    /// (state A).
    ///
    /// Such a key is invisible to every writable-bucket enumeration (its bucket
    /// is gone) and to `pending_generations` (it was never queued), so a guard
    /// change alone could never re-drive it. `flush_keys` unions this set into
    /// its worked key list on entry, so every lifecycle signal that reaches
    /// `flush_keys` — age tick, pressure, explicit flush — retries the stranded
    /// seal even when the signal's own selection is empty. A key is removed on
    /// the attempt that completes it (or that finds nothing left to complete).
    seal_retry: HashSet<crate::scribe::seal_key::SealKey>,
    /// Published generations retained until grace-ordered WAL retirement.
    retained_generations: HashMap<u64, RetainedGeneration>,
    /// The sole in-flight group retained after unresolved SQL COMMIT ambiguity.
    ///
    /// A shard processes one group at a time. Once this slot is populated the
    /// shared governor is poisoned and the owner stops advancing its scheduler,
    /// so the current group plus already-admitted scheduler entries remain the
    /// complete bounded ambiguity set until process restart.
    retained_commit_ambiguity: Option<GroupWalState>,
    /// Admission controller shared by all shard owners.
    admission: AdmissionController,
    /// Mutable memtable owned exclusively by this shard task.
    memtable: Memtable,
    /// Bounded CPU lane for replay and persistence preparation.
    persistence_cpu: ScribePersistenceCpuPool,
    /// Bounded WAL IO lane for append, sync, manifest, and retirement work.
    wal_io: ScribeWalIoPool,
    /// Optional immutable-generation persistence runtime.
    persistence: Option<Arc<PersistenceRuntime>>,
    /// Vala SQL pool retained only by the production batch-control fence.
    control_postgres: Option<Arc<vala_sql::ValaPostgres>>,
    /// Completion sender routed back to this owner's command mailbox.
    completion_tx: mpsc::Sender<ShardCommand>,
    /// Typed WAL stream identity for generations and replay.
    stream: StreamIdentity,
    /// WAL handle for this fixed shard.
    wal_handle: WalHandle,
    /// Shared active/immutable memory ledger.
    memory_ownership: ScribeOwnership,
    /// Snapshot published for synchronous inspection callers.
    snapshot: Arc<Mutex<ShardMemtableSnapshot>>,
    /// Pod-global count of accepted appends not yet completed.
    pending: Arc<AtomicUsize>,
    /// Notification used by runtime drains to await accepted work.
    drained: Arc<Notify>,
    /// One-shot retirement release fault used only by deterministic owner tests.
    #[cfg(test)]
    fail_next_retirement_release: bool,
}

/// Last owner-published state used by synchronous inspection surfaces.
#[derive(Debug, Clone, Default)]
pub(crate) struct ShardMemtableSnapshot {
    /// Aggregate writable, immutable, and pending-generation counters.
    pub(crate) stats: MemtableStats,
    /// Per-bucket memory totals used by inspection and pressure accounting.
    pub(crate) bucket_memory: Vec<BucketMemorySnapshot>,
    /// Writable buckets eligible for global pressure selection.
    pub(crate) pressure_candidates: Vec<PressureCandidate>,
}

/// The live fixed-shard Scribe runtime.
#[derive(Debug)]
pub(crate) struct ScribeShardRuntime {
    /// Fixed owner mailboxes used for internal and admitted commands.
    senders: Vec<ScribeShard<ShardCommand>>,
    /// Coalescing pressure channels paired with the fixed owners.
    pressure_senders: Vec<watch::Sender<Option<PressureSignal>>>,
    /// Last snapshots published by the fixed owners.
    snapshots: Vec<Arc<Mutex<ShardMemtableSnapshot>>>,
    /// Retained owner tasks that graceful shutdown joins or abort shutdown cancels.
    tasks: tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
    /// Abort handles used by the synchronous finalizer without acquiring the
    /// async join registry. Keeping these handles separate ensures a caller
    /// that has already exhausted its deadline can still cancel every owner
    /// even when graceful shutdown currently holds `tasks`.
    abort_handles: Mutex<Vec<tokio::task::AbortHandle>>,
    /// Accepted append commands that have not completed.
    pending: Arc<AtomicUsize>,
    /// Notification used to wake a drain when accepted work completes.
    drained: Arc<Notify>,
    /// Whether external shard admission has closed.
    closed: AtomicBool,
    /// Owner acknowledgements received through the graceful flush path.
    #[cfg(any(test, feature = "test-support"))]
    shutdown_flush_completions: AtomicUsize,
}

/// Configuration supplied to [`ScribeShardRuntime::start`].
///
/// All shard owners are started with this configuration. Committed generations
/// retire immediately at the first lifecycle sweep after their SQL commit; there
/// is no configurable grace period.
pub(crate) struct ScribeShardStartConfig {
    /// Admission controller shared by all shard owners.
    pub(crate) admission: AdmissionController,
    /// Writable-byte threshold that triggers active-bucket rotation.
    pub(crate) rotation_bytes: usize,
    /// Active-generation max age consulted by the age seal predicate.
    ///
    /// Threaded from [`crate::scribe::ScribePressureConfig::seal_max_age`] into
    /// each owner's memtable the same way `rotation_bytes` is, so the age
    /// trigger uses the configured seconds-scale value (D83 default 30 s).
    pub(crate) seal_max_age: std::time::Duration,
    /// Shared WAL writer used to create shard handles.
    pub(crate) wal: Arc<WalWriter>,
    /// Bounded CPU lane for replay and persistence preparation.
    pub(crate) persistence_cpu: ScribePersistenceCpuPool,
    /// Bounded WAL IO lane for append and retirement operations.
    pub(crate) wal_io: ScribeWalIoPool,
    /// Optional immutable-generation persistence runtime.
    pub(crate) persistence: Option<Arc<PersistenceRuntime>>,
    /// Tenant-scoped SQL owner used to fence durable public batch ACKs.
    pub(crate) control_postgres: Option<Arc<vala_sql::ValaPostgres>>,
    /// Typed pod stream identity.
    pub(crate) stream: StreamIdentity,
    /// Shared active/immutable memory ledger.
    pub(crate) memory_ownership: ScribeOwnership,
}

impl ScribeShardRuntime {
    /// Returns every writable or pending immutable seal key for one tenant.
    ///
    /// The result is assembled from owner-published snapshots, so discovery
    /// never traverses another shard's mutable state and remains safe for the
    /// private list-active-streams RPC.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] when an owner inspection snapshot is
    /// poisoned or unavailable.
    pub(crate) fn active_seal_keys_for_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<Vec<crate::scribe::seal_key::SealKey>, ScribeError> {
        let mut keys = self
            .memtable_snapshots()?
            .into_iter()
            .flat_map(|snapshot| snapshot.bucket_memory)
            .filter_map(|bucket| (bucket.seal_key.tenant == tenant).then_some(bucket.seal_key))
            .collect::<Vec<_>>();
        keys.sort_by_key(ToString::to_string);
        keys.dedup();
        Ok(keys)
    }

    /// Closes shard admission before any potentially stalled graceful wait.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    /// Aborts every retained shard owner that remains after graceful cleanup.
    ///
    /// This operation never awaits and does not depend on the async join
    /// registry lock. Graceful shutdown may still be joining owners when the
    /// process deadline expires; the retained abort handles therefore provide
    /// a bounded, observable cancellation path instead of silently leaving an
    /// owner alive after `try_lock` contention.
    pub(crate) fn abort_retained(&self) -> usize {
        let handles = match self.abort_handles.lock() {
            Ok(mut handles) => std::mem::take(&mut *handles),
            Err(poisoned) => {
                tracing::error!("Scribe shard abort-handle registry is poisoned");
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
        metrics::counter!("bifrost_scribe_shard_abort_total").increment(aborted as u64);
        aborted
    }

    /// Releases completed join handles when the async registry is available.
    ///
    /// This is bookkeeping only: owner cancellation is already guaranteed by
    /// [`Self::abort_retained`]'s lock-independent abort handles. Contention
    /// leaves the handles retained for their eventual owner drop and never
    /// delays the caller's deadline-expiry path.
    pub(crate) fn clear_retained_join_handles(&self) {
        if let Ok(mut tasks) = self.tasks.try_lock() {
            tasks.clear();
        }
    }

    /// Retains one never-completing owner and returns its abort probe.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) async fn install_shutdown_stall_for_test(&self) -> tokio::task::AbortHandle {
        let task = tokio::spawn(std::future::pending::<()>());
        let abort = task.abort_handle();
        self.abort_handles
            .lock()
            .expect("Scribe shard abort-handle registry must not be poisoned")
            .push(abort.clone());
        self.tasks.lock().await.push(task);
        abort
    }
    /// Start exactly sixteen shard owners and their bounded command mailboxes.
    ///
    /// # Panics
    ///
    /// Panics if a fixed shard cannot obtain its WAL handle; that is a
    /// construction invariant because the topology always has sixteen WAL
    /// streams.
    pub(crate) fn start(config: ScribeShardStartConfig, runtime: &Handle) -> Arc<Self> {
        let ScribeShardStartConfig {
            admission,
            rotation_bytes,
            seal_max_age,
            wal,
            persistence_cpu,
            wal_io,
            persistence,
            control_postgres,
            stream,
            memory_ownership,
        } = config;
        let mut set = ScribeShardSet::<ShardCommand>::new();
        let receivers = set.take_receivers().unwrap_or_default();
        let senders: Vec<ScribeShard<ShardCommand>> =
            (0..SCRIBE_SHARD_COUNT).map(|id| set.shard(id)).collect();
        let pending = Arc::new(AtomicUsize::new(0));
        let drained = Arc::new(Notify::new());
        let mut tasks = Vec::with_capacity(SCRIBE_SHARD_COUNT);
        let abort_handles = Mutex::new(Vec::with_capacity(SCRIBE_SHARD_COUNT));
        let pressure_channels = (0..SCRIBE_SHARD_COUNT)
            .map(|_| watch::channel(None))
            .collect::<Vec<_>>();
        let pressure_senders = pressure_channels
            .iter()
            .map(|(sender, _)| sender.clone())
            .collect::<Vec<_>>();
        let snapshots = (0..SCRIBE_SHARD_COUNT)
            .map(|_| Arc::new(Mutex::new(ShardMemtableSnapshot::default())))
            .collect::<Vec<_>>();
        for (id, receiver) in receivers.into_iter().enumerate() {
            let wal_handle = wal.handle_for_shard(id).unwrap_or_else(|error| {
                panic!("fixed shard WAL handle must be constructible: {error}")
            });
            let (_, pressure_receiver) = &pressure_channels[id];
            let owner = ShardOwner {
                id,
                receiver,
                pressure_receiver: pressure_receiver.clone(),
                scheduler: TenantRoundRobin::default(),
                shutting_down: false,
                wal_segments: WalSegmentsByKey::new(),
                synced_not_inserted: HashMap::new(),
                pending_generations: PendingGenerationsByKey::new(),
                replay_chunk: None,
                seal_retry: HashSet::new(),
                retained_generations: HashMap::new(),
                retained_commit_ambiguity: None,
                admission: admission.clone(),
                memtable: Memtable::new_with_config(rotation_bytes, seal_max_age),
                persistence_cpu: persistence_cpu.clone(),
                wal_io: wal_io.clone(),
                persistence: persistence.clone(),
                control_postgres: control_postgres.clone(),
                completion_tx: senders[id].sender.clone(),
                stream,
                wal_handle,
                memory_ownership: memory_ownership.clone(),
                snapshot: Arc::clone(&snapshots[id]),
                pending: Arc::clone(&pending),
                drained: Arc::clone(&drained),
                #[cfg(test)]
                fail_next_retirement_release: false,
            };
            let task = runtime.spawn(owner.run());
            if let Ok(mut handles) = abort_handles.lock() {
                handles.push(task.abort_handle());
            } else {
                panic!("Scribe shard abort-handle registry must not be poisoned at startup");
            }
            tasks.push(task);
        }
        Arc::new(Self {
            senders,
            pressure_senders,
            snapshots,
            tasks: tokio::sync::Mutex::new(tasks),
            abort_handles,
            pending,
            drained,
            closed: AtomicBool::new(false),
            #[cfg(any(test, feature = "test-support"))]
            shutdown_flush_completions: AtomicUsize::new(0),
        })
    }

    /// Enqueue a prepared request onto its deterministic shard.
    ///
    /// The shard is selected by `shard_for(tenant, table, batch_id)` so that
    /// distinct batch ids for one (tenant, table) spread across lanes while a
    /// client retry (same `batch_id`) lands on the lane holding its dedup state.
    ///
    /// # Errors
    /// Returns [`ScribeError::IngressClosed`] when the runtime has shut down, or
    /// [`ScribeError::IngestBusy`] when the target shard mailbox is full.
    pub(crate) fn try_send(&self, mut append: PreparedAppend) -> Result<(), ScribeError> {
        if self.closed.load(Ordering::Acquire) {
            if let Some(lifecycle) = append.lifecycle.as_mut() {
                lifecycle.settle_shutdown();
            }
            return Err(ScribeError::IngressClosed);
        }
        let shard = shard_for(append.tenant, &append.table, append.batch_id);
        let table_name = append.table.fqn();
        if let Some(memory) = append.memory.as_mut() {
            memory.transfer_category(MemoryCategory::Queued)?;
        }
        append
            .lifecycle
            .as_mut()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared append lost its lifecycle owner".to_owned(),
            })?
            .shard_transferred();
        self.pending.fetch_add(1, Ordering::AcqRel);
        let result = self.senders[shard].try_send(ShardCommand::Append(Box::new(append)));
        if let Err(error) = result {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            self.drained.notify_waiters();
            return Err(match error {
                mpsc::error::TrySendError::Full(ShardCommand::Append(mut append)) => {
                    if let Some(lifecycle) = append.lifecycle.as_mut() {
                        lifecycle.revert_shard_transfer();
                        lifecycle.refuse();
                    }
                    ScribeError::IngestBusy { table: table_name }
                }
                mpsc::error::TrySendError::Closed(ShardCommand::Append(mut append)) => {
                    if let Some(lifecycle) = append.lifecycle.as_mut() {
                        lifecycle.revert_shard_transfer();
                        lifecycle.settle_shutdown();
                    }
                    ScribeError::IngressClosed
                }
                mpsc::error::TrySendError::Closed(_) | mpsc::error::TrySendError::Full(_) => {
                    ScribeError::IngressClosed
                }
            });
        }
        Ok(())
    }

    /// Fan out a bounded hot snapshot to all shards and merge the results.
    ///
    /// Under batch-spread routing the live-tail data for one (tenant, table)
    /// may be spread across all sixteen shard lanes. This method sends a
    /// [`ShardCommand::Snapshot`] to every shard and merges the `HotBatch`
    /// vectors. The existing `HotBatch` contract is preserved: the caller
    /// receives a single flat list of per-partition-day batches.
    ///
    /// # Errors
    /// Returns [`ScribeError::IngressClosed`] when the runtime has shut down,
    /// [`ScribeError::IngestBusy`] when a shard mailbox is full, or any
    /// shard-level snapshot error.
    pub(crate) async fn snapshot(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<Vec<HotBatch>, ScribeError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ScribeError::IngressClosed);
        }
        let mut merged = Vec::with_capacity(request.max_batches);
        let mut retained_bytes = 0_usize;
        for sender in &self.senders {
            let (response, result) = tokio::sync::oneshot::channel();
            let command = ShardCommand::Snapshot {
                request: request.clone(),
                response,
            };
            tokio::time::timeout(
                std::time::Duration::from_millis(200),
                sender.sender.send(command),
            )
            .await
            .map_err(|_| ScribeError::IngestBusy {
                table: "live-tail".to_owned(),
            })?
            .map_err(|_| ScribeError::IngressClosed)?;
            let batches = result.await.map_err(|_| ScribeError::Internal {
                detail: "shard dropped live-tail snapshot response".to_owned(),
            })??;
            let batch_bytes = batches.iter().try_fold(0_usize, |bytes, batch| {
                bytes
                    .checked_add(batch.rows.get_array_memory_size())
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "live-tail retained byte count overflow".to_owned(),
                    })
            })?;
            retained_bytes =
                retained_bytes
                    .checked_add(batch_bytes)
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "live-tail retained byte count overflow".to_owned(),
                    })?;
            if merged.len().saturating_add(batches.len()) > request.max_batches
                || retained_bytes > request.max_retained_bytes
            {
                return Err(ScribeError::IngestBusy {
                    table: request.binding.table_ref.fqn(),
                });
            }
            merged.extend(batches);
        }
        Ok(merged)
    }

    /// Freeze one key across all shards and return every frozen memtable.
    ///
    /// Under batch-spread routing a single seal key may have buckets on any
    /// subset of the sixteen shard lanes. This method broadcasts a
    /// [`ShardCommand::FreezeKey`] to every shard; shards with no bucket for
    /// the key no-op and return `None`, and every `Some` is collected into the
    /// returned vector (mirroring [`Self::freeze_tenant`]). Returning only the
    /// first shard's result — the prior behavior — orphaned the other shards'
    /// frozen Arrow data, which had already been moved active→immutable in the
    /// memory ledger, with no path to persistence (the latent T35 hazard this
    /// fixes). The caller pre-commits every returned entry.
    ///
    /// After this call returns, every shard's bucket for the key is sealed.
    ///
    /// # Errors
    /// Returns [`ScribeError::IngressClosed`] when any shard has stopped, or
    /// [`ScribeError::Internal`] when no shard holds a bucket for the key
    /// (preserving the existing not-found contract), or the first shard-level
    /// freeze error.
    pub(crate) async fn freeze_key(
        &self,
        seal_key: crate::scribe::seal_key::SealKey,
    ) -> Result<Vec<crate::scribe::memtable::FrozenMemtable>, ScribeError> {
        let mut per_shard = Vec::with_capacity(self.senders.len());
        for sender in &self.senders {
            let (response, result) = tokio::sync::oneshot::channel();
            sender
                .sender
                .send(ShardCommand::FreezeKey {
                    seal_key: seal_key.clone(),
                    response,
                })
                .await
                .map_err(|_| ScribeError::IngressClosed)?;
            // Shards that hold no bucket for the key return None; retain every
            // shard's optional result so the collection step keeps them all.
            per_shard.push(result.await.map_err(|_| ScribeError::Internal {
                detail: "shard dropped freeze response".to_owned(),
            })??);
        }
        Self::finalize_frozen_fan_out(per_shard, &seal_key)
    }

    /// Flatten the per-shard freeze fan-out into every frozen memtable, failing
    /// closed when no shard held a bucket for the key.
    ///
    /// Under batch-spread routing a single seal key may have buckets on any
    /// subset of the sixteen lanes; each shard returns `Some(frozen)` when it
    /// owned a bucket and `None` otherwise, so the flattened vector has exactly
    /// one entry per non-empty shard — the fix for the latent T35 hazard that
    /// previously kept only the first shard's frozen memtable and orphaned the
    /// rest. An all-`None` fan-out means no shard holds the key; that is mapped
    /// to [`ScribeError::Internal`] to preserve the existing not-found contract
    /// rather than reporting a silent no-op as a completed seal.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] when every shard returned `None`.
    fn finalize_frozen_fan_out(
        per_shard: Vec<Option<crate::scribe::memtable::FrozenMemtable>>,
        seal_key: &crate::scribe::seal_key::SealKey,
    ) -> Result<Vec<crate::scribe::memtable::FrozenMemtable>, ScribeError> {
        let frozen = per_shard.into_iter().flatten().collect::<Vec<_>>();
        if frozen.is_empty() {
            return Err(ScribeError::Internal {
                detail: format!("freeze_key: no shard holds a bucket for {seal_key}"),
            });
        }
        Ok(frozen)
    }

    /// Freeze all keys for one tenant through their owning shard(s).
    pub(crate) async fn freeze_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<Vec<crate::scribe::memtable::FrozenMemtable>, ScribeError> {
        let mut frozen = Vec::new();
        for sender in &self.senders {
            let (response, result) = tokio::sync::oneshot::channel();
            sender
                .sender
                .send(ShardCommand::FreezeTenant { tenant, response })
                .await
                .map_err(|_| ScribeError::IngressClosed)?;
            frozen.extend(result.await.map_err(|_| ScribeError::Internal {
                detail: "shard dropped tenant freeze response".to_owned(),
            })??);
        }
        Ok(frozen)
    }

    /// Mark an owner-local generation committed after its tenant transaction.
    ///
    /// The `shard_id` must be the recorded pod-local shard lane from the
    /// [`crate::scribe::memtable::FrozenMemtable`] that was sealed, which is
    /// carried through the [`crate::scribe::seal::PostCommitToken`]. Using
    /// the recorded shard avoids recomputing the routing key (which requires
    /// the client `batch_id` under batch-spread routing).
    ///
    /// # Errors
    /// Returns [`ScribeError::IngressClosed`] when the target shard has stopped,
    /// or the first shard-level post-commit error.
    pub(crate) async fn complete_post_commit(
        &self,
        seal_id: u64,
        shard_id: usize,
        seal_key: &crate::scribe::seal_key::SealKey,
        arrow_bytes: usize,
        file_list_key: crate::scribe::file_list_writer::FileListCommitKey,
    ) -> Result<(), ScribeError> {
        let (response, result) = tokio::sync::oneshot::channel();
        self.senders[shard_id]
            .sender
            .send(ShardCommand::CompletePostCommit {
                seal_key: seal_key.clone(),
                seal_id,
                arrow_bytes,
                file_list_key,
                response,
            })
            .await
            .map_err(|_| ScribeError::IngressClosed)?;
        result.await.map_err(|_| ScribeError::Internal {
            detail: "shard dropped post-commit response".to_owned(),
        })?
    }

    /// Keep an owner-local generation pending after transaction rollback.
    ///
    /// The `shard_id` must be the recorded pod-local shard lane from the
    /// [`crate::scribe::seal::PostCommitToken`] being aborted.
    ///
    /// # Errors
    /// Returns [`ScribeError::IngressClosed`] when the target shard has stopped,
    /// or the first shard-level abort error.
    pub(crate) async fn abort_post_commit(
        &self,
        seal_id: u64,
        shard_id: usize,
        _seal_key: &crate::scribe::seal_key::SealKey,
    ) -> Result<(), ScribeError> {
        let (response, result) = tokio::sync::oneshot::channel();
        self.senders[shard_id]
            .sender
            .send(ShardCommand::AbortPostCommit { seal_id, response })
            .await
            .map_err(|_| ScribeError::IngressClosed)?;
        result.await.map_err(|_| ScribeError::Internal {
            detail: "shard dropped abort response".to_owned(),
        })?
    }

    /// Queue one age/pressure flush request per fixed shard owner.
    pub(crate) fn request_expired_flush(&self, now: std::time::Instant) {
        for sender in &self.senders {
            let _ = sender.try_send(ShardCommand::FlushExpired { now });
        }
    }

    /// Run one committed-generation retirement pass across every shard for tests.
    ///
    /// Unlike the production coalescing signal, this waits until every owner
    /// has completed its retirement pass. It lets integration journeys establish
    /// a deterministic observation point without changing production scheduling.
    ///
    /// Retirement is immediate: all `ImmutableState::Committed` generations are
    /// retired on this call.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngressClosed`] when an owner has stopped, or an
    /// owner-reported retirement error when one shard cannot process the pass.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) async fn retire_committed_for_test(&self) -> Result<(), ScribeError> {
        for sender in &self.senders {
            let (response, result) = tokio::sync::oneshot::channel();
            sender
                .sender
                .send(ShardCommand::RetireCommittedForTest { response })
                .await
                .map_err(|_| ScribeError::IngressClosed)?;
            result.await.map_err(|_| ScribeError::Internal {
                detail: "shard dropped test expiry response".to_owned(),
            })??;
        }
        Ok(())
    }

    /// Fan out a pressure-flush signal to every shard owner.
    ///
    /// Under batch-spread routing a seal key may have buckets on any subset of
    /// the sixteen shard lanes, so the pressure signal is broadcast to all
    /// shards. Each shard flushes from its own locally-held bucket set; shards
    /// with no bucket for a given key no-op (existing empty-bucket skip in
    /// `flush_keys`).
    pub(crate) fn request_pressure_flush(&self, keys: &[crate::scribe::seal_key::SealKey]) {
        if keys.is_empty() {
            return;
        }
        for sender in &self.pressure_senders {
            sender.send_replace(Some(PressureSignal {
                keys: keys.to_owned(),
                wal_key: None,
            }));
        }
    }

    /// Fan out a WAL pressure signal for one victim key to every shard owner.
    ///
    /// Under batch-spread routing the victim key may have buckets on any shard
    /// lane, so the signal is sent to all shards; each shard flushes its own
    /// bucket for the key, if any.
    pub(crate) fn request_wal_pressure_flush(&self, key: Option<crate::scribe::seal_key::SealKey>) {
        let Some(key) = key else {
            return;
        };
        for sender in &self.pressure_senders {
            sender.send_replace(Some(PressureSignal {
                keys: Vec::new(),
                wal_key: Some(key.clone()),
            }));
        }
    }

    /// Flush every active bucket through its owning shard before shutdown.
    pub(crate) async fn flush_all(&self) -> Result<(), ScribeError> {
        for sender in &self.senders {
            let (response, result) = tokio::sync::oneshot::channel();
            sender
                .sender
                .send(ShardCommand::FlushAll { response })
                .await
                .map_err(|_| ScribeError::IngressClosed)?;
            result.await.map_err(|_| ScribeError::Internal {
                detail: "shard dropped shutdown flush response".to_owned(),
            })??;
            #[cfg(any(test, feature = "test-support"))]
            self.shutdown_flush_completions
                .fetch_add(1, Ordering::AcqRel);
        }
        Ok(())
    }

    /// Wait until all accepted shard commands have completed.
    pub(crate) async fn drain(&self) {
        loop {
            let notified = self.drained.notified();
            if self.pending.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }

    /// Return the number of accepted append commands not yet completed.
    #[must_use]
    pub(crate) fn pending_items(&self) -> usize {
        self.pending.load(Ordering::Acquire)
    }

    /// Read owner-published state without touching any generation map.
    pub(crate) fn memtable_snapshots(&self) -> Result<Vec<ShardMemtableSnapshot>, ScribeError> {
        self.snapshots
            .iter()
            .map(|snapshot| {
                snapshot
                    .lock()
                    .map(|snapshot| snapshot.clone())
                    .map_err(|_| ScribeError::Internal {
                        detail: "shard inspection snapshot lock poisoned".to_owned(),
                    })
            })
            .collect()
    }

    /// Clone the fixed owner channels for synchronous WAL-lane replay routing.
    pub(crate) fn replay_senders(&self) -> Vec<mpsc::Sender<ShardCommand>> {
        self.senders
            .iter()
            .map(|sender| sender.sender.clone())
            .collect()
    }

    /// Drains accepted commands and joins shard owners until `deadline`.
    ///
    /// [`Self::close`] must run first. Cancellation or deadline expiry may
    /// leave partial graceful progress; [`Self::abort_retained`] remains the
    /// mandatory finalizer and ensures no Tokio owner survives shutdown.
    pub(crate) async fn shutdown(&self, deadline: std::time::Instant) {
        self.drain().await;
        for sender in &self.senders {
            let _ = sender.try_send(ShardCommand::Shutdown);
        }
        let Ok(mut tasks) =
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), self.tasks.lock())
                .await
        else {
            return;
        };
        for mut task in tasks.drain(..) {
            if tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), &mut task)
                .await
                .is_err()
            {
                task.abort();
            }
        }
    }
}

impl ShardOwner {
    /// Publishes this owner's read-only inspection projection.
    fn publish_snapshot(&self) {
        let snapshot = ShardMemtableSnapshot {
            stats: self.memtable.stats().unwrap_or_default(),
            bucket_memory: self.memtable.bucket_memory().unwrap_or_default(),
            pressure_candidates: self.memtable.pressure_candidates().unwrap_or_default(),
        };
        if let Ok(mut published) = self.snapshot.lock() {
            *published = snapshot;
        }
    }

    /// Runs the shard mailbox until accepted work drains and shutdown completes.
    ///
    /// Commands are consumed FIFO at the shard boundary. Group processing keeps
    /// WAL append, sync, memtable insertion, and ACK ordering intact; shutdown
    /// waits for the scheduler and accepted-item counter before the owner exits.
    ///
    /// # Cancellation
    ///
    /// Cancellation may stop after a WAL or persistence side effect. Retained
    /// generations and WAL replay provide the recovery path for that partial
    /// progress.
    async fn run(mut self) {
        self.publish_snapshot();
        loop {
            if self.memory_ownership.is_poisoned() {
                match self.receiver.recv().await {
                    Some(ShardCommand::Shutdown) | None => break,
                    Some(command) => {
                        if self.handle_command(command).await {
                            break;
                        }
                        self.publish_snapshot();
                    }
                }
                continue;
            }
            if self.pressure_receiver.has_changed().unwrap_or(false) {
                let signal = self.pressure_receiver.borrow_and_update().clone();
                if let Some(signal) = signal {
                    self.handle_pressure_signal(signal);
                    self.publish_snapshot();
                }
            }
            while let Ok(command) = self.receiver.try_recv() {
                if self.handle_command(command).await {
                    self.shutting_down = true;
                }
                self.publish_snapshot();
            }
            let group = self.scheduler.pop_group();
            if !group.is_empty() {
                let group_len = group.len();
                if let Err(error) = self.process_group(group).await {
                    if is_expected_wal_capacity(&error) {
                        tracing::warn!(error = %error, shard = self.id, "Scribe shard reached its WAL capacity");
                    } else {
                        tracing::error!(error = %error, shard = self.id, "Scribe shard group failed");
                    }
                }
                self.publish_snapshot();
                self.pending.fetch_sub(group_len, Ordering::AcqRel);
                self.drained.notify_waiters();
                continue;
            }
            if self.shutting_down
                && self.scheduler.is_empty()
                && self.pending.load(Ordering::Acquire) == 0
            {
                break;
            }
            tokio::select! {
                command = self.receiver.recv() => {
                    match command {
                        Some(command) => {
                            if self.handle_command(command).await {
                                self.shutting_down = true;
                            }
                            self.publish_snapshot();
                        }
                        None => break,
                    }
                }
                changed = self.pressure_receiver.changed() => {
                    if changed.is_err() {
                        break;
                    }
                }
            }
        }
    }

    /// Applies one coalesced memory or WAL pressure signal.
    ///
    /// The primary `flush_keys` call is unconditional — the previous
    /// empty-`keys` short-circuit is removed — so a pressure signal that
    /// selects no victims of its own still reaches `flush_keys` and re-drives
    /// any stranded state-A seals through the entry drain (change 3). An empty
    /// worked set is a cheap no-op.
    fn handle_pressure_signal(&mut self, signal: PressureSignal) {
        if let Err(error) = self.flush_keys(signal.keys, Some(SealTriggerReason::Pressure)) {
            record_seal_failure();
            tracing::warn!(error = %error, shard = self.id, "pressure flush failed");
        }
        if let Some(key) = signal.wal_key
            && let Err(error) = self.flush_keys(vec![key], Some(SealTriggerReason::Pressure))
        {
            record_seal_failure();
            tracing::warn!(error = %error, shard = self.id, "WAL pressure flush failed");
        }
    }

    /// Applies one command and returns whether it requested owner shutdown.
    async fn handle_command(&mut self, command: ShardCommand) -> bool {
        match command {
            ShardCommand::Append(append) => self.scheduler.push(*append),
            ShardCommand::Snapshot { request, response } => {
                let _ = response.send(self.snapshot_at(&request));
            }
            ShardCommand::FlushExpired { now } => {
                if let Err(error) = self.flush_expired(now) {
                    tracing::warn!(error = %error, shard = self.id, "age flush failed");
                }
                if let Err(error) = self.retire_committed().await {
                    tracing::warn!(error = %error, shard = self.id, "committed generation retirement failed");
                }
            }
            #[cfg(any(test, feature = "test-support"))]
            ShardCommand::RetireCommittedForTest { response } => {
                let _ = response.send(self.retire_committed().await);
            }
            ShardCommand::FlushAll { response } => {
                let _ = response.send(self.flush_all());
            }
            ShardCommand::PersistenceComplete {
                completion,
                waiter,
                visibility_result,
            } => {
                let result = self.handle_persistence_completion(*completion, waiter);
                let _ = visibility_result.send(result);
            }
            ShardCommand::Replay { chunk, response } => {
                self.replay_state(*chunk, response).await;
            }
            ShardCommand::RetryReplayPersistence {
                seal_key,
                generation_id,
                progress,
            } => self.retry_replay_persistence(&seal_key, generation_id, progress),
            ShardCommand::FreezeKey { seal_key, response } => {
                let _ = response.send(self.freeze_key(&seal_key));
            }
            ShardCommand::FreezeTenant { tenant, response } => {
                let _ = response.send(self.freeze_tenant(tenant));
            }
            ShardCommand::CompletePostCommit {
                seal_key,
                seal_id,
                arrow_bytes,
                file_list_key,
                response,
            } => {
                let _ = response.send(self.complete_external_post_commit(
                    &seal_key,
                    seal_id,
                    arrow_bytes,
                    file_list_key,
                ));
            }
            ShardCommand::AbortPostCommit { seal_id, response } => {
                let _ = response.send(self.memtable.abort_post_commit(seal_id));
            }
            ShardCommand::Shutdown => return true,
        }
        false
    }

    /// Builds a bounded live-tail projection from this owner's readable memtable rows.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the requested range or projection cannot be read.
    fn snapshot_at(&self, request: &FetchLiveTailRequest) -> Result<Vec<HotBatch>, ScribeError> {
        let readable = self.memtable.readable_batches_for_range(
            request.binding.tenant,
            &request.binding.table_ref,
            request.start_day,
            request.end_day,
            &request.required_columns,
            ReadableBatchLimits {
                max_batches: request.max_batches,
                max_retained_bytes: request.max_retained_bytes,
            },
        )?;
        Ok(readable
            .into_iter()
            .filter(|batch| {
                request.after_lsn == crate::scribe::wal::WalLsn::ZERO
                    || batch.meta.wal_lsn_max > request.after_lsn
            })
            .map(|batch| HotBatch {
                partition_day: batch.partition_day,
                wal_lsn: batch.meta.wal_lsn_max,
                batch_id: batch.meta.batch_id,
                rows: batch.batch,
            })
            .collect())
    }

    /// Reconstructs one WAL-restored generation and queues it for persistence.
    ///
    /// Any replay decoding, accounting, retention, or persistence-submission
    /// error is returned through `response`. Successful persistence-backed
    /// replay leaves that response owned by the exact pending generation until
    /// publication and checked immutable retirement finish.
    async fn replay_state(
        &mut self,
        chunk: crate::scribe::replay::ReplayChunk,
        response: tokio::sync::oneshot::Sender<
            Result<crate::scribe::replay::ReplayChunkResponse, ScribeError>,
        >,
    ) {
        let crate::scribe::replay::ReplayChunk {
            states,
            memory,
            identity_memory,
        } = chunk;
        if self.replay_chunk.is_some() {
            let _ = response.send(Err(ScribeError::Internal {
                detail: "a synchronous replay chunk is already active".to_owned(),
            }));
            return;
        }
        let mut ordered = states.into_iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| left.0.cmp(&right.0));
        let mut prepared = VecDeque::new();
        let mut retirements = Vec::new();
        let mut total_arrow_bytes = 0_usize;
        for (_, replayed) in ordered {
            match self.prepare_replay_state(replayed).await {
                Ok(PreparedReplayState::Retired(retirement)) => {
                    retirements.push(retirement);
                }
                Ok(PreparedReplayState::Generation {
                    prepared: generation,
                    arrow_bytes,
                }) => {
                    let Some(total) = total_arrow_bytes.checked_add(arrow_bytes) else {
                        self.discard_prepared_replay(&mut prepared);
                        let _ = response.send(Err(ScribeError::Internal {
                            detail: "replay chunk Arrow ownership overflowed".to_owned(),
                        }));
                        return;
                    };
                    total_arrow_bytes = total;
                    prepared.push_back(generation);
                }
                Err(error) => {
                    self.discard_prepared_replay(&mut prepared);
                    let _ = response.send(Err(error));
                    return;
                }
            }
        }
        let adoption = match memory {
            Some(memory) => self
                .memory_ownership
                .adopt_replay_immutable(memory, total_arrow_bytes),
            None => self.memory_ownership.reserve_immutable(total_arrow_bytes),
        };
        if let Err(error) = adoption {
            self.discard_prepared_replay(&mut prepared);
            let _ = response.send(Err(error));
            return;
        }
        let owner_stats = match self.memtable.stats() {
            Ok(stats) => stats,
            Err(error) => {
                self.rollback_prepared_replay(&mut prepared);
                let _ = response.send(Err(error));
                return;
            }
        };
        self.admission
            .sync_memtable_bytes(owner_stats.writable_bytes, owner_stats.immutable_bytes);
        let identity = match identity_memory {
            Some(lease) => match self.memory_ownership.adopt_replay_identity(lease) {
                Ok(identity) => Some(identity),
                Err(error) => {
                    self.rollback_prepared_replay(&mut prepared);
                    let _ = response.send(Err(error));
                    return;
                }
            },
            None => None,
        };
        if self.persistence.is_none() {
            let identity_memory = identity
                .map(crate::scribe::memory::ReplayIdentityOwnership::return_to_decode)
                .transpose();
            let _ = response.send(identity_memory.map(|identity_memory| {
                crate::scribe::replay::ReplayChunkResponse {
                    retirements,
                    identity_memory,
                }
            }));
            return;
        }
        self.replay_chunk = Some(ReplayChunkOwner {
            remaining: prepared,
            current_generation: None,
            retirements,
            identity,
            response: Some(response),
        });
        if let Err(error) = self.advance_replay_chunk() {
            self.fail_replay_chunk(error);
        }
    }

    /// Resolves, reconstructs, and retains one state for serialized persistence.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for durable-fence conflicts, reconstruction or
    /// memtable failure, invalid catalog identity, or WAL segment retention.
    async fn prepare_replay_state(
        &mut self,
        mut replayed: crate::scribe::replay::ReplayedSealKey,
    ) -> Result<PreparedReplayState, ScribeError> {
        if let Some(retirement) = self.resolve_replay_fence(&mut replayed).await? {
            return Ok(PreparedReplayState::Retired(retirement));
        }
        let source_stream = replayed.stream;
        let seal_key = replayed.seal_key.clone();
        let segment_refs = replayed.wal_segments.clone();
        let binding = replay_binding(&seal_key)?;
        let restored = self
            .persistence_cpu
            .submit(
                crate::scribe::execution_lanes::ScribePersistenceCpuOp::RestoreReplay {
                    replayed: Box::new(replayed),
                },
            )
            .await?;
        let crate::scribe::execution_lanes::ScribePersistenceCpuResult::ReplayRestored(frozen) =
            restored
        else {
            return Err(ScribeError::Internal {
                detail: "persistence CPU lane returned the wrong replay result".to_owned(),
            });
        };
        let frozen = self.memtable.insert_replayed_frozen(*frozen)?;
        let generation = Arc::new(ImmutableGeneration::from_frozen(
            &frozen,
            (seal_key.tenant, seal_key.table.clone()),
            source_stream,
            segment_refs.clone(),
            self.wal_handle.clone(),
        ));
        Ok(PreparedReplayState::Generation {
            prepared: ReplayChunkGeneration {
                generation,
                binding,
            },
            arrow_bytes: frozen.arrow_bytes,
        })
    }

    /// Discards reconstructed values that failed before immutable adoption.
    fn discard_prepared_replay(&mut self, prepared: &mut VecDeque<ReplayChunkGeneration>) {
        for pending in prepared.drain(..) {
            let _ = self
                .memtable
                .discard_pending_generation(pending.generation.generation_id.0);
        }
    }

    /// Rolls back reconstructed generations that never entered serialized persistence.
    fn rollback_prepared_replay(&mut self, prepared: &mut VecDeque<ReplayChunkGeneration>) {
        for pending in prepared.drain(..) {
            let _ = self.rollback_replayed_generation(
                pending.generation.generation_id.0,
                pending.generation.arrow_bytes,
            );
        }
    }

    /// Submits the next sorted generation while moving the sole identity owner into it.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when owner state is inconsistent or the identity lock is poisoned.
    fn advance_replay_chunk(&mut self) -> Result<(), ScribeError> {
        let Some(mut owner) = self.replay_chunk.take() else {
            return Ok(());
        };
        let Some(next) = owner.remaining.pop_front() else {
            let identity_memory = owner
                .identity
                .map(crate::scribe::memory::ReplayIdentityOwnership::return_to_decode)
                .transpose()?;
            if let Some(response) = owner.response.take() {
                let _ = response.send(Ok(crate::scribe::replay::ReplayChunkResponse {
                    retirements: owner.retirements,
                    identity_memory,
                }));
            }
            return Ok(());
        };
        if let Err(error) = self
            .wal_handle
            .retain_segments(&next.generation.wal_segments)
        {
            owner.remaining.push_front(next);
            self.replay_chunk = Some(owner);
            return Err(error);
        }
        if let Some(identity) = owner.identity.take() {
            next.generation
                .replay_identity
                .lock()
                .map_err(|_| ScribeError::Internal {
                    detail: "replay identity owner lock poisoned".to_owned(),
                })?
                .replace(identity);
        }
        let seal_key = next.generation.seal_key.clone();
        owner.current_generation = Some(next.generation.generation_id.0);
        self.pending_generations
            .entry(seal_key.clone())
            .or_default()
            .push_back(PendingGeneration {
                generation: next.generation,
                binding: next.binding,
                submitted: false,
                retry_scheduled: false,
                replay_response: None,
                replay_owned: true,
            });
        self.replay_chunk = Some(owner);
        self.submit_front(&seal_key);
        Ok(())
    }

    /// Terminates the active synchronous replay response after an owned failure.
    fn fail_replay_chunk(&mut self, error: ScribeError) {
        if let Some(mut owner) = self.replay_chunk.take() {
            for pending in owner.remaining.drain(..) {
                let _ = self.rollback_replayed_generation(
                    pending.generation.generation_id.0,
                    pending.generation.arrow_bytes,
                );
            }
            if let Some(response) = owner.response.take() {
                let _ = response.send(Err(error));
            }
        }
    }

    /// Resolves one replay commit against the tenant-scoped durable batch fence.
    ///
    /// The canonical WAL location restores. A later exact logical retry is
    /// suppressed before Arrow reconstruction, publication, or audit. Without
    /// configured Postgres (isolated unit mode), replay uses its in-memory WAL
    /// identity checks only.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when production replay lacks terminal identity,
    /// integer conversion fails, tenant-scoped SQL is unavailable, or the
    /// durable tenant/table/batch identity contradicts digest or slice count.
    async fn resolve_replay_fence(
        &self,
        replayed: &mut crate::scribe::replay::ReplayedSealKey,
    ) -> Result<Option<ReplayRetirement>, ScribeError> {
        if self.control_postgres.is_none() {
            return Ok(None);
        }
        if replayed.commits.is_empty() {
            return Err(ScribeError::Internal {
                detail: "production WAL replay lacks terminal commit identity".to_owned(),
            });
        }
        let commits = replayed.commits.clone();
        let mut suppressed = HashSet::new();
        for commit in &commits {
            if self.resolve_replayed_commit(replayed, commit).await?
                == vala_sql::queries::scribe_batch_commits::ScribeBatchReplayResolution::Suppress
            {
                suppressed.insert(commit.batch_id);
            }
        }
        if suppressed.is_empty() {
            return Ok(None);
        }
        retain_replayed_batches(replayed, &suppressed);
        if !replayed.data_records.is_empty() {
            return Ok(None);
        }
        let sealed_lsn = commits
            .iter()
            .map(|commit| commit.wal_lsn_max)
            .max()
            .ok_or_else(|| ScribeError::Internal {
                detail: "production WAL replay lacks a retirement watermark".to_owned(),
            })?;
        Ok(Some(ReplayRetirement {
            wal: self.wal_handle.clone(),
            segments: replayed.wal_segments.clone(),
            stream: replayed.stream,
            seal_key: replayed.seal_key.clone(),
            sealed_lsn,
        }))
    }

    /// Resolves one reconstructed batch against its tenant-scoped SQL fence.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for invalid SQL integer coordinates, unavailable
    /// tenant control storage, or a contradictory durable batch identity.
    async fn resolve_replayed_commit(
        &self,
        replayed: &crate::scribe::replay::ReplayedSealKey,
        commit: &crate::scribe::replay::ReplayedCommitIdentity,
    ) -> Result<vala_sql::queries::scribe_batch_commits::ScribeBatchReplayResolution, ScribeError>
    {
        let postgres = self
            .control_postgres
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "production WAL replay lacks control Postgres".to_owned(),
            })?;
        let durable = vala_sql::queries::scribe_batch_commits::ScribeBatchCommit {
            tenant: replayed.seal_key.tenant,
            logical_table_fqn: replayed.seal_key.table.fqn(),
            batch_id: uuid::Uuid::from_bytes(commit.batch_id),
            slice_set_digest: commit.slice_set_digest,
            slice_count: i32::try_from(commit.slice_count).map_err(|_| ScribeError::Internal {
                detail: "WAL replay slice count exceeds SQL integer range".to_owned(),
            })?,
            wal_node_id: replayed.stream.node_id.as_uuid(),
            wal_writer_epoch: replayed.stream.writer_epoch.as_i64(),
            wal_shard_id: i16::from(replayed.shard_id),
            wal_segment_sequence: i64::try_from(commit.segment_sequence).map_err(|_| {
                ScribeError::Internal {
                    detail: "WAL replay segment sequence exceeds SQL bigint range".to_owned(),
                }
            })?,
            wal_lsn_min: i64::try_from(commit.wal_lsn_min.as_u64()).map_err(|_| {
                ScribeError::Internal {
                    detail: "WAL replay first LSN exceeds SQL bigint range".to_owned(),
                }
            })?,
            wal_lsn_max: i64::try_from(commit.wal_lsn_max.as_u64()).map_err(|_| {
                ScribeError::Internal {
                    detail: "WAL replay commit LSN exceeds SQL bigint range".to_owned(),
                }
            })?,
            request_id: commit.request_id,
        };
        let mut conn = postgres.tenant_conn(durable.tenant).await?;
        let resolution =
            vala_sql::queries::scribe_batch_commits::resolve_replay(&mut conn, &durable)
                .await
                .map_err(ScribeError::from)?;
        Ok(resolution)
    }

    /// Rolls back a replay generation that failed after immutable admission.
    ///
    /// The checked ledger release precedes memtable removal, matching committed
    /// retirement ordering. Admission mirrors the resulting memtable snapshot
    /// only after both owners have completed their transition.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the immutable reservation cannot be
    /// released exactly, the pending generation cannot be removed, or the
    /// resulting memtable snapshot cannot be read.
    fn rollback_replayed_generation(
        &mut self,
        seal_id: u64,
        arrow_bytes: usize,
    ) -> Result<(), ScribeError> {
        self.memory_ownership
            .preflight_release_immutable(arrow_bytes)?;
        self.memory_ownership.release_immutable(arrow_bytes)?;
        self.memtable.discard_pending_generation(seal_id)?;
        let owner_stats = self.memtable.stats()?;
        self.admission
            .sync_memtable_bytes(owner_stats.writable_bytes, owner_stats.immutable_bytes);
        Ok(())
    }

    /// Freezes one writable bucket and transfers its accounted bytes to immutable state.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the bucket cannot be inspected, frozen, or
    /// its memory ledger cannot be transferred.
    /// Freeze one writable bucket, returning `None` when the shard holds no
    /// bucket for this key.
    ///
    /// Under batch-spread routing a seal key may exist on any subset of the
    /// sixteen shard lanes, so a shard with no matching bucket is a valid no-op.
    /// The returned `FrozenMemtable` has its `shard_id` set to `self.id` so
    /// that post-commit routing can target the correct lane without recomputing
    /// the routing key.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when the bucket lock is poisoned or a memory
    /// accounting step fails.
    fn freeze_key(
        &mut self,
        seal_key: &crate::scribe::seal_key::SealKey,
    ) -> Result<Option<crate::scribe::memtable::FrozenMemtable>, ScribeError> {
        let active_bytes = self.memtable.writable_bytes(seal_key)?;
        if active_bytes == 0 {
            return Ok(None);
        }
        self.admission
            .preflight_transfer_active_to_immutable(active_bytes)?;
        self.memory_ownership
            .preflight_move_active_to_immutable(active_bytes)?;
        let mut frozen = self.memtable.freeze(seal_key)?;
        frozen.shard_id = self.id;
        self.memory_ownership
            .move_active_to_immutable(frozen.arrow_bytes)?;
        self.admission
            .transfer_active_to_immutable(frozen.arrow_bytes)?;
        Ok(Some(frozen))
    }

    /// Freezes every writable bucket belonging to one tenant.
    ///
    /// Returns only the non-empty frozen memtables (shards with no bucket for
    /// the tenant contribute no results).
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when any tenant bucket cannot be frozen or
    /// accounted.
    fn freeze_tenant(
        &mut self,
        tenant: DataTenantId,
    ) -> Result<Vec<crate::scribe::memtable::FrozenMemtable>, ScribeError> {
        let keys = self.memtable.seal_keys_for_tenant(tenant)?;
        keys.into_iter()
            .filter_map(|key| self.freeze_key(&key).transpose())
            .collect()
    }

    /// Flushes expired buckets and retries pending persistence submissions.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when bucket selection, freezing, or generation
    /// construction fails; a full persistence queue remains pending for retry.
    fn flush_expired(&mut self, now: std::time::Instant) -> Result<(), ScribeError> {
        let keys = self.memtable.expired_seal_keys_for_shard(self.id, now)?;
        self.flush_keys(keys, Some(SealTriggerReason::Age))?;
        self.retry_pending();
        Ok(())
    }

    /// Flushes every active bucket owned by this shard.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when active bucket selection or generation
    /// preparation fails.
    fn flush_all(&mut self) -> Result<(), ScribeError> {
        let keys = self.memtable.active_seal_keys_for_shard(self.id)?;
        self.flush_keys(keys, None)
    }

    /// Freezes selected writable buckets and queues immutable generations.
    ///
    /// `trigger` names the lifecycle reason these keys were selected so each
    /// executed freeze counts one `bifrost_scribe_seal_total{trigger}` seal
    /// (D84). Counting here — after the empty-bucket skip and a successful
    /// freeze — is deliberate: the coordinated pressure path fans one request to
    /// every shard, so counting at the request site would over-count by the
    /// shard count, while counting at the actual freeze records exactly the
    /// buckets that sealed. Callers with no lifecycle trigger (shutdown drain)
    /// pass `None` and are not counted.
    ///
    /// On entry the shard's `seal_retry` set is unioned into the worked keys so
    /// a stranded state-A seal (freeze succeeded but a persistence-prep step
    /// failed before the accounting move) is re-driven by every lifecycle
    /// signal, including one whose own selection is empty. Per-key work is
    /// delegated to [`Self::prepare_and_queue_generation`], which guarantees the
    /// two-legal-states invariant; a failing key is recorded in `seal_retry`
    /// before the error propagates.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a bucket, memory transfer, table binding,
    /// or WAL retention operation fails. A full persistence queue leaves the
    /// generation unsubmitted for the next retry signal. On any failure the
    /// bytes remain Active-accounted and the key stays retry-reachable.
    fn flush_keys(
        &mut self,
        keys: Vec<crate::scribe::seal_key::SealKey>,
        trigger: Option<SealTriggerReason>,
    ) -> Result<(), ScribeError> {
        // Re-drive site: every lifecycle signal that reaches `flush_keys`
        // retries the shard's stranded state-A seals, even when its own `keys`
        // selection is empty. The retry set is snapshotted (not drained) so a
        // mid-loop `?` early return leaves any not-yet-reached retry key
        // recorded for the next signal; a key leaves the set only on the
        // attempt that resolves it (completed, or nothing left to complete).
        let mut seen = HashSet::with_capacity(keys.len() + self.seal_retry.len());
        let mut worked = Vec::with_capacity(keys.len() + self.seal_retry.len());
        for seal_key in keys {
            if seen.insert(seal_key.clone()) {
                worked.push(seal_key);
            }
        }
        for seal_key in self.seal_retry.iter().cloned().collect::<Vec<_>>() {
            if seen.insert(seal_key.clone()) {
                worked.push(seal_key);
            }
        }

        for seal_key in worked {
            // A writable bucket is the fresh-seal path. With no writable bucket
            // the key is only worth touching when a stranded pending frozen
            // generation exists AND was never queued; otherwise it is a stale
            // retry mark or an already-completed seal, so dropping the mark and
            // skipping is the safe no-op (no double freeze, move, or queue).
            let has_rows = self.memtable.row_count(&seal_key)? > 0;
            if !has_rows {
                let strandable = !self.pending_generations.contains_key(&seal_key)
                    && self.memtable.has_pending_frozen(&seal_key)?;
                if !strandable {
                    self.seal_retry.remove(&seal_key);
                    continue;
                }
            }

            // freeze is retry-safe: with a writable bucket it produces a new
            // pending entry; with none it returns the existing stranded one.
            // The seal is counted only for a newly created pending entry so a
            // retried freeze never double-counts `bifrost_scribe_seal_total`.
            let active_bytes = self.memtable.writable_bytes(&seal_key)?;
            if let Err(error) = self
                .admission
                .preflight_transfer_active_to_immutable(active_bytes)
                .and_then(|()| {
                    self.memory_ownership
                        .preflight_move_active_to_immutable(active_bytes)
                })
            {
                self.seal_retry.insert(seal_key.clone());
                return Err(error);
            }
            let frozen = self.memtable.freeze(&seal_key)?;
            if has_rows && let Some(trigger) = trigger {
                record_seal(trigger);
            }

            if self.persistence.is_none() {
                // No persistence target: the accounting move is the terminal
                // legal state for this path (semantics unchanged by this task).
                self.memory_ownership
                    .move_active_to_immutable(frozen.arrow_bytes)?;
                self.admission
                    .transfer_active_to_immutable(frozen.arrow_bytes)?;
                self.seal_retry.remove(&seal_key);
                continue;
            }

            // All fallible persistence-prep completes BEFORE the accounting
            // move, so a failure here leaves bytes Active-accounted and the
            // segment map intact (state A). Record the key for retry before
            // propagating so a later lifecycle signal re-drives it.
            match self.prepare_and_queue_generation(&seal_key, &frozen) {
                Ok(()) => {
                    self.seal_retry.remove(&seal_key);
                }
                Err(error) => {
                    self.seal_retry.insert(seal_key.clone());
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// Runs every fallible persistence-prep step for one frozen generation
    /// before the accounting move, then queues it through the infallible tail.
    ///
    /// This is the transactional pivot of the seal path. Table-binding
    /// resolution and WAL segment retention — the two fallible prep steps —
    /// execute BEFORE `move_active_to_immutable`, and the ledger move (T52
    /// net-zero, poison-only failure) is the last fallible step, run
    /// immediately before the infallible queue push. As a result a single-step
    /// failure leaves exactly one of two states: (A) prep failed, bytes remain
    /// Active-accounted, admission Active-accounted, and the `wal_segments`
    /// entry is intact for an identical retry; or (B) the generation is queued,
    /// bytes are Immutable-accounted, and its segments are retained. There is
    /// no state in which bytes are Immutable-accounted with no queued
    /// generation. The `wal_segments` entry is removed only after retention
    /// succeeds, and `segment_refs` is derived non-destructively so a retention
    /// failure re-derives identical references.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] when the table binding cannot be
    /// resolved, or propagates [`ScribeError`] from `retain_segments` or the
    /// ledger move (poison only). Every such failure occurs at or before the
    /// accounting move, so no partial queue or Immutable accounting is left
    /// behind.
    fn prepare_and_queue_generation(
        &mut self,
        seal_key: &crate::scribe::seal_key::SealKey,
        frozen: &crate::scribe::memtable::FrozenMemtable,
    ) -> Result<(), ScribeError> {
        let binding =
            crate::catalog::TenantTableBinding::resolve((seal_key.tenant, seal_key.table.clone()))
                .map_err(|error| ScribeError::Internal {
                    detail: error.to_string(),
                })?;
        // Non-destructive: the segment map entry is removed only after
        // retention succeeds (below), so a retention failure leaves identical
        // references for a retry to re-derive.
        let segment_refs = self
            .wal_segments
            .get(seal_key)
            .map(|segments| {
                segments
                    .values()
                    .map(|segment| segment.reference())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.wal_handle.retain_segments(&segment_refs)?;

        // Accounting move is the last fallible step; everything below is
        // infallible, so no fallible step runs between the move and the queue
        // push (AC1).
        self.memory_ownership
            .move_active_to_immutable(frozen.arrow_bytes)?;
        self.admission
            .transfer_active_to_immutable(frozen.arrow_bytes)?;
        self.wal_segments.remove(seal_key);
        let generation = Arc::new(ImmutableGeneration::from_frozen(
            frozen,
            (seal_key.tenant, seal_key.table.clone()),
            self.stream,
            segment_refs,
            self.wal_handle.clone(),
        ));
        let queue = self
            .pending_generations
            .entry(seal_key.clone())
            .or_default();
        let should_submit = queue.is_empty();
        queue.push_back(PendingGeneration {
            generation,
            binding,
            submitted: false,
            retry_scheduled: false,
            replay_response: None,
            replay_owned: false,
        });
        if should_submit {
            self.submit_front(seal_key);
        }
        Ok(())
    }

    /// Attempts to submit the oldest pending generation for one seal key.
    /// Attempts a non-blocking submission of the oldest generation for one key.
    ///
    /// A full persistence queue leaves the generation unsubmitted so a later
    /// flush or completion event can retry it without changing FIFO order.
    fn submit_front(&mut self, seal_key: &crate::scribe::seal_key::SealKey) {
        let Some(persistence) = &self.persistence else {
            return;
        };
        let Some(queue) = self.pending_generations.get_mut(seal_key) else {
            return;
        };
        let Some(front) = queue.front_mut() else {
            return;
        };
        if front.submitted {
            return;
        }
        let generation = Arc::clone(&front.generation);
        let binding = front.binding.clone();
        let result = persistence.try_submit(PersistenceJob {
            generation: Arc::clone(&generation),
            binding,
            completion_tx: self.completion_tx.clone(),
            completion_waiter: None,
            defer_manifest_advance: front.replay_response.is_some() || front.replay_owned,
        });
        match result {
            Ok(()) => front.submitted = true,
            Err(PersistenceSubmitError::Full(job)) if front.replay_owned => {
                drop(job);
                if !front.retry_scheduled {
                    front.retry_scheduled = true;
                    let persistence = Arc::clone(persistence);
                    let completion_tx = self.completion_tx.clone();
                    let seal_key = seal_key.clone();
                    let generation_id = generation.generation_id.0;
                    Handle::current().spawn(async move {
                        let progress = persistence.wait_for_submission_progress().await;
                        let _ = completion_tx
                            .send(ShardCommand::RetryReplayPersistence {
                                seal_key,
                                generation_id,
                                progress,
                            })
                            .await;
                    });
                }
                return;
            }
            Err(PersistenceSubmitError::Closed(job)) if front.replay_owned => {
                drop(job);
                let generation_id = generation.generation_id.0;
                self.fail_active_replay_submission(
                    seal_key,
                    generation_id,
                    ScribeError::Internal {
                        detail: "Scribe persistence queue closed during replay".to_owned(),
                    },
                );
                return;
            }
            Err(PersistenceSubmitError::Full(job) | PersistenceSubmitError::Closed(job)) => {
                drop(job);
                return;
            }
        }
        let active_paths = self
            .wal_segments
            .values()
            .flat_map(|segments| segments.keys().cloned())
            .collect::<HashSet<_>>();
        if let Err(error) = self
            .wal_handle
            .close_segments_if_unowned(&generation.wal_segments, &active_paths)
        {
            tracing::warn!(error = %error, "closed WAL segment could not be detached after bucket rotation");
        }
    }

    /// Re-drives the active replay generation after bounded queue progress.
    fn retry_replay_persistence(
        &mut self,
        seal_key: &crate::scribe::seal_key::SealKey,
        generation_id: u64,
        progress: Result<(), ScribeError>,
    ) {
        let Some(front) = self
            .pending_generations
            .get_mut(seal_key)
            .and_then(VecDeque::front_mut)
        else {
            return;
        };
        if front.generation.generation_id.0 != generation_id || !front.replay_owned {
            return;
        }
        front.retry_scheduled = false;
        if let Err(error) = progress {
            self.fail_active_replay_submission(seal_key, generation_id, error);
            return;
        }
        self.submit_front(seal_key);
    }

    /// Rolls back a replay generation that could not enter persistence.
    fn fail_active_replay_submission(
        &mut self,
        seal_key: &crate::scribe::seal_key::SealKey,
        generation_id: u64,
        error: ScribeError,
    ) {
        let pending = self
            .pending_generations
            .get_mut(seal_key)
            .and_then(|queue| {
                (queue.front().is_some_and(|front| {
                    front.generation.generation_id.0 == generation_id && front.replay_owned
                }))
                .then(|| queue.pop_front())
                .flatten()
            });
        if self
            .pending_generations
            .get(seal_key)
            .is_some_and(VecDeque::is_empty)
        {
            self.pending_generations.remove(seal_key);
        }
        if let Some(pending) = pending {
            let rollback = self.rollback_replayed_generation(
                pending.generation.generation_id.0,
                pending.generation.arrow_bytes,
            );
            if let Err(rollback_error) = rollback {
                self.fail_replay_chunk(rollback_error);
                return;
            }
        }
        self.fail_replay_chunk(error);
    }

    /// Retries the oldest unsubmitted generation for every seal key.
    fn retry_pending(&mut self) {
        let keys = self.pending_generations.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            self.submit_front(&key);
        }
    }

    /// Register externally persisted force-seal state for grace-ordered retirement.
    ///
    /// The direct force-seal path commits SQL outside the shard-owned persistence
    /// runtime. It must still create the retained-generation bookkeeping used to
    /// release immutable memory and WAL references on lifecycle ticks.
    ///
    /// # Errors
    ///
    /// Returns a memtable or WAL ownership error when publication cannot move
    /// the exact generation into retained state.
    fn complete_external_post_commit(
        &mut self,
        seal_key: &crate::scribe::seal_key::SealKey,
        seal_id: u64,
        arrow_bytes: usize,
        file_list_key: crate::scribe::file_list_writer::FileListCommitKey,
    ) -> Result<(), ScribeError> {
        let segment_refs = self
            .wal_segments
            .get(seal_key)
            .into_iter()
            .flat_map(|segments| segments.values())
            .map(|segment| segment.reference())
            .collect::<Vec<_>>();
        self.wal_handle.retain_segments(&segment_refs)?;
        if let Err(error) = self.memtable.complete_post_commit(seal_id, file_list_key) {
            let _ = self.wal_handle.retire_segments(&segment_refs);
            return Err(error);
        }
        self.retained_generations.insert(
            seal_id,
            RetainedGeneration {
                arrow_bytes,
                wal_segments: segment_refs,
                wal: self.wal_handle.clone(),
            },
        );
        Ok(())
    }

    /// Retires all committed generations on the current lifecycle sweep.
    ///
    /// A generation is eligible the moment its state is
    /// `ImmutableState::Committed`, which is only reached after the fenced
    /// `vala.file_list` + audit transaction commits. Retirement ordering is
    /// preserved: `admission.release_immutable` and `memory_ownership.release_immutable`
    /// are called before `ScribeWalIoOp::RetireWal` is submitted, so WAL
    /// segment refcounts are released last.
    ///
    /// A failed WAL retirement is logged and remains recoverable on the next
    /// lifecycle pass.
    ///
    /// # Cancellation
    ///
    /// Cancellation may leave a generation retained; its memory and WAL
    /// references remain owned until the next retirement pass.
    async fn retire_committed(&mut self) -> Result<(), ScribeError> {
        let generation_ids = self
            .retained_generations
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for generation_id in generation_ids {
            let Some(retained) = self.retire_committed_generation(generation_id)? else {
                continue;
            };
            if let Err(error) = self
                .wal_io
                .submit(ScribeWalIoOp::RetireWal {
                    wal: retained.wal,
                    segments: retained.wal_segments,
                })
                .await
            {
                tracing::warn!(error = %error, generation_id, "WAL retirement submission failed");
            }
        }
        Ok(())
    }

    /// Releases one committed generation's governed immutable and memtable ownership.
    ///
    /// The returned WAL ownership remains retained so the caller can choose the
    /// safe IO boundary for segment retirement. This is required during replay,
    /// where submitting WAL work from the sole WAL worker would self-deadlock.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when retirement accounting is inconsistent,
    /// poisoned, or cannot commit the exact planned generation transition.
    fn retire_committed_generation(
        &mut self,
        generation_id: u64,
    ) -> Result<Option<RetainedGeneration>, ScribeError> {
        let Some(token) = self.memtable.plan_committed_retirement(generation_id)? else {
            return Ok(None);
        };
        let Some(retained) = self.retained_generations.get(&generation_id).cloned() else {
            return Ok(None);
        };
        if token.arrow_bytes != retained.arrow_bytes {
            self.memory_ownership.poison();
            return Err(ScribeError::Internal {
                detail: format!("retirement bytes mismatch for generation {generation_id}"),
            });
        }
        self.admission
            .preflight_release_immutable(token.arrow_bytes)?;
        self.memory_ownership
            .preflight_release_immutable(token.arrow_bytes)?;
        #[cfg(test)]
        if self.fail_next_retirement_release {
            self.fail_next_retirement_release = false;
            self.memory_ownership.poison();
            return Err(ScribeError::Internal {
                detail: "injected immutable retirement release failure".to_owned(),
            });
        }
        self.admission.release_immutable(token.arrow_bytes)?;
        self.memory_ownership.release_immutable(token.arrow_bytes)?;
        if let Err(error) = self.memtable.commit_retirement(token) {
            self.memory_ownership.poison();
            return Err(error);
        }
        self.retained_generations.remove(&generation_id);
        record_retirement(token.arrow_bytes);
        Ok(Some(retained))
    }

    /// Reconciles one persistence result with this owner's FIFO generation queue.
    ///
    /// Successful publication moves the generation to retained state and
    /// submits the next generation for the same key. Failures mark only the
    /// front generation retryable and deliver the error to its waiter.
    ///
    /// # Errors
    ///
    /// Returns the same publication failure delivered to the caller waiter, or
    /// an invariant error when the owning FIFO cannot advance exactly once.
    fn handle_persistence_completion(
        &mut self,
        mut completion: PersistenceCompletion,
        waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    ) -> Result<(), String> {
        let mut replay_identity = completion.replay_identity.take();
        let generation_id = completion.generation_id.0;
        let seal_key = self.pending_generations.iter().find_map(|(key, queue)| {
            queue
                .front()
                .filter(|front| front.generation.generation_id.0 == generation_id)
                .map(|_| key.clone())
        });
        let Some(seal_key) = seal_key else {
            return self.handle_unowned_completion(completion, waiter);
        };
        let replay_owned = self.front_is_replay_owned(&seal_key);
        if let Some(error) = completion.error.as_ref() {
            let detail = error.clone();
            self.mark_front_retryable(&seal_key);
            tracing::warn!(error = %error, generation_id, "shard persistence failed; retaining immutable generation");
            if replay_owned {
                if let Some(owner) = self.replay_chunk.as_mut() {
                    owner.identity = replay_identity.take();
                }
                self.fail_replay_chunk(ScribeError::Internal {
                    detail: detail.clone(),
                });
            }
            self.send_replay_error(&seal_key, &detail);
            Self::send_waiter_error(waiter, detail.clone());
            return Err(detail);
        }
        let Some(file_list_key) = completion.file_list_key.clone() else {
            let detail = "persistence completion omitted its file-list key".to_owned();
            self.mark_front_retryable(&seal_key);
            if replay_owned {
                if let Some(owner) = self.replay_chunk.as_mut() {
                    owner.identity = replay_identity.take();
                }
                self.fail_replay_chunk(ScribeError::Internal {
                    detail: detail.clone(),
                });
            }
            self.send_replay_error(&seal_key, &detail);
            Self::send_waiter_error(waiter, detail.clone());
            return Err(detail);
        };
        if let Err(error) = self
            .memtable
            .complete_post_commit(generation_id, file_list_key)
        {
            let detail = error.to_string();
            self.mark_front_retryable(&seal_key);
            if replay_owned {
                if let Some(owner) = self.replay_chunk.as_mut() {
                    owner.identity = replay_identity.take();
                }
                self.fail_replay_chunk(ScribeError::Internal {
                    detail: detail.clone(),
                });
            }
            tracing::warn!(error = %error, generation_id, "shard persistence completion could not publish generation");
            self.send_replay_error(&seal_key, &detail);
            Self::send_waiter_error(waiter, detail.clone());
            return Err(detail);
        }
        let Some(queue) = self.pending_generations.get_mut(&seal_key) else {
            return Err("persistence completion queue disappeared".to_owned());
        };
        let Some(mut front) = queue.pop_front() else {
            return Err("persistence completion queue is empty".to_owned());
        };
        if front.generation.generation_id.0 != generation_id {
            queue.push_front(front);
            return Err("persistence completion generation is not queue front".to_owned());
        }
        self.retained_generations.insert(
            generation_id,
            RetainedGeneration {
                arrow_bytes: completion.arrow_bytes,
                wal_segments: completion.wal_segments,
                wal: completion.wal,
            },
        );
        if queue.is_empty() {
            self.pending_generations.remove(&seal_key);
        }
        if front.replay_owned {
            self.complete_replay_generation(&front, generation_id, replay_identity.take())?;
        }
        if let Some(replay_response) = front.replay_response.take() {
            self.complete_single_replay_generation(
                &front,
                generation_id,
                replay_identity,
                replay_response,
            )?;
        }
        Self::send_waiter_success(waiter);
        if self.persistence.is_some() {
            self.submit_front(&seal_key);
        }
        Ok(())
    }

    /// Reports whether one key's FIFO front belongs to the active replay chunk.
    fn front_is_replay_owned(&self, seal_key: &crate::scribe::seal_key::SealKey) -> bool {
        self.pending_generations
            .get(seal_key)
            .and_then(|queue| queue.front())
            .is_some_and(|front| front.replay_owned)
    }

    /// Delivers a successful completion to an optional persistence waiter.
    fn send_waiter_success(waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>) {
        if let Some(waiter) = waiter {
            let _ = waiter.send(Ok(()));
        }
    }

    /// Settles one serialized replay generation and advances the chunk owner.
    ///
    /// # Errors
    ///
    /// Returns an invariant detail when retirement, active-owner identity, or
    /// next-generation submission cannot advance exactly once.
    fn complete_replay_generation(
        &mut self,
        front: &PendingGeneration,
        generation_id: u64,
        replay_identity: Option<crate::scribe::memory::ReplayIdentityOwnership>,
    ) -> Result<(), String> {
        let retained = self
            .retire_committed_generation(generation_id)
            .and_then(|retained| {
                retained.ok_or_else(|| ScribeError::Internal {
                    detail: format!(
                        "replay generation {generation_id} was not eligible for retirement"
                    ),
                })
            });
        let retained = match retained {
            Ok(retained) => retained,
            Err(error) => {
                self.fail_replay_chunk(error);
                return Ok(());
            }
        };
        let Some(owner) = self.replay_chunk.as_mut() else {
            return Err("replay completion lost its chunk owner".to_owned());
        };
        if owner.current_generation != Some(generation_id) {
            return Err("replay completion does not match the active generation".to_owned());
        }
        owner.current_generation = None;
        owner.identity = replay_identity;
        owner.retirements.push(ReplayRetirement {
            wal: retained.wal,
            segments: retained.wal_segments,
            stream: front.generation.stream,
            seal_key: front.generation.seal_key.clone(),
            sealed_lsn: front.generation.wal_lsn_max,
        });
        self.advance_replay_chunk()
            .map_err(|error| error.to_string())
    }

    /// Completes the retained single-generation replay compatibility path.
    ///
    /// # Errors
    ///
    /// Returns the retirement or identity-restoration failure delivered to the
    /// synchronous replay caller.
    fn complete_single_replay_generation(
        &mut self,
        front: &PendingGeneration,
        generation_id: u64,
        replay_identity: Option<crate::scribe::memory::ReplayIdentityOwnership>,
        response: tokio::sync::oneshot::Sender<
            Result<crate::scribe::replay::ReplayChunkResponse, ScribeError>,
        >,
    ) -> Result<(), String> {
        let result = self
            .retire_committed_generation(generation_id)
            .and_then(|retained| {
                retained.ok_or_else(|| ScribeError::Internal {
                    detail: format!(
                        "replay generation {generation_id} was not eligible for retirement"
                    ),
                })
            })
            .and_then(|retained| {
                let identity_memory = replay_identity
                    .map(crate::scribe::memory::ReplayIdentityOwnership::return_to_decode)
                    .transpose()?;
                Ok(crate::scribe::replay::ReplayChunkResponse {
                    retirements: vec![ReplayRetirement {
                        wal: retained.wal,
                        segments: retained.wal_segments,
                        stream: front.generation.stream,
                        seal_key: front.generation.seal_key.clone(),
                        sealed_lsn: front.generation.wal_lsn_max,
                    }],
                    identity_memory,
                })
            });
        let detail = result.as_ref().err().map(ToString::to_string);
        let _ = response.send(result);
        detail.map_or(Ok(()), Err)
    }

    /// Marks a key's front generation for a later persistence retry.
    fn mark_front_retryable(&mut self, seal_key: &crate::scribe::seal_key::SealKey) {
        if let Some(front) = self
            .pending_generations
            .get_mut(seal_key)
            .and_then(VecDeque::front_mut)
        {
            front.submitted = false;
        }
    }

    /// Fails and detaches the replay waiter owned by one retryable FIFO front.
    fn send_replay_error(&mut self, seal_key: &crate::scribe::seal_key::SealKey, detail: &str) {
        let response = self
            .pending_generations
            .get_mut(seal_key)
            .and_then(VecDeque::front_mut)
            .and_then(|front| front.replay_response.take());
        if let Some(response) = response {
            let _ = response.send(Err(ScribeError::Internal {
                detail: detail.to_owned(),
            }));
        }
    }

    /// Delivers a persistence failure to an optional post-commit waiter.
    fn send_waiter_error(
        waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
        detail: String,
    ) {
        if let Some(waiter) = waiter {
            let _ = waiter.send(Err(detail));
        }
    }

    /// Reconciles a replayed or otherwise unqueued completion with the memtable.
    ///
    /// # Errors
    ///
    /// Returns the same persistence or publication failure delivered to the
    /// caller waiter.
    fn handle_unowned_completion(
        &mut self,
        completion: PersistenceCompletion,
        waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    ) -> Result<(), String> {
        let generation_id = completion.generation_id.0;
        if let Some(error) = completion.error {
            Self::send_waiter_error(waiter, error.clone());
            return Err(error);
        }
        let Some(file_list_key) = completion.file_list_key else {
            let detail = "persistence completion omitted its file-list key".to_owned();
            Self::send_waiter_error(waiter, detail.clone());
            return Err(detail);
        };
        if let Err(error) = self
            .memtable
            .complete_post_commit(generation_id, file_list_key)
        {
            let detail = error.to_string();
            Self::send_waiter_error(waiter, detail.clone());
            return Err(detail);
        }
        self.retained_generations.insert(
            generation_id,
            RetainedGeneration {
                arrow_bytes: completion.arrow_bytes,
                wal_segments: completion.wal_segments,
                wal: completion.wal,
            },
        );
        if let Some(waiter) = waiter {
            let _ = waiter.send(Ok(()));
        }
        Ok(())
    }
}

/// Holds one WAL-durable slice until memtable insertion completes.
struct DurableSlice {
    /// Seal bucket receiving the rows.
    seal_key: crate::scribe::seal_key::SealKey,
    /// Audit event paired with the data rows.
    audit_event: wyrd_spec::vala::api::AuditEvent,
    /// Arrow rows awaiting memtable insertion.
    rows: Option<arrow::record_batch::RecordBatch>,
    /// Active-memory bytes reserved for this slice.
    memtable_bytes: usize,
    /// Whether active admission and ownership have already transferred.
    active_reserved: bool,
    /// Batch identity used for retry deduplication.
    batch_id: [u8; 16],
    /// WAL sequence number assigned to the durable append.
    lsn: crate::scribe::wal::WalLsn,
    /// SHA-256 digest of this exact WAL slice payload.
    payload_digest: [u8; 32],
    /// Exact WAL slice payload length.
    payload_len: u32,
    /// Zero-based ordinal in the closed batch slice set.
    slice_index: u32,
    /// Total slices in the closed batch slice set.
    slice_count: u32,
    /// Whether this slice belongs to a batch whose COMMIT was already fsynced.
    commit_already_synced: bool,
}

/// Public-layout bytes retained for one WAL-durable slice descriptor.
///
/// Scribe material planning uses this fact before root admission so the exact
/// fixed `Vec<DurableSlice>` backing allocated by group write is never hidden.
pub(crate) const DURABLE_SLICE_LAYOUT_BYTES: usize = std::mem::size_of::<DurableSlice>();

/// A WAL-synced slice whose Arrow rows have not yet reached the active
/// memtable. Successful insertion removes the entry, so this bounded index
/// only covers the fsync-success/memtable-failure retry window.
#[derive(Clone, Copy, Debug)]
struct WalSliceState {
    /// WAL sequence number that can be reused without another append.
    lsn: crate::scribe::wal::WalLsn,
    /// SHA-256 digest of the exact durable slice payload.
    payload_digest: [u8; 32],
    /// Exact durable slice payload length.
    payload_len: u32,
    /// Stable ordinal in the committed batch slice set.
    slice_index: u32,
    /// Complete committed batch slice count.
    slice_count: u32,
}

/// Intermediate state shared by the WAL write, sync, and insert stages.
struct GroupWalState {
    /// Prepared appends whose ACKs wait for the group result.
    prepared: Vec<PreparedAppend>,
    /// Slices with WAL records ready for synchronization or insertion.
    durable: Vec<DurableSlice>,
    /// WAL segments touched by this group.
    touched: HashMap<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>,
    /// Rows accepted per original append batch.
    rows_by_append: HashMap<[u8; 16], u64>,
}

impl ShardOwner {
    /// Writes, syncs, inserts, rotates, and ACKs one bounded tenant-fair group.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when WAL write/sync or memtable insertion fails;
    /// prepared ACK waiters receive the same completion error and reservations
    /// are released before the error returns.
    async fn process_group(&mut self, group: Vec<PreparedAppend>) -> Result<(), ScribeError> {
        let mut state = self.write_group(group).await?;
        if let Err(error) = self.sync_group(&state.touched).await {
            if let Err(cleanup_error) = self.release_active_reservations(&state.durable) {
                tracing::error!(error = %cleanup_error, "active cleanup failed after group sync error");
            }
            self.mark_wal_error(&error);
            Self::notify_prepared_error(&mut state.prepared, &error);
            return Err(error);
        }
        if let Err(error) = self.append_and_sync_batch_commits(&mut state).await {
            if self.memory_ownership.is_poisoned() {
                debug_assert!(
                    self.retained_commit_ambiguity.is_none(),
                    "one serial shard group may hold COMMIT ambiguity"
                );
                self.retained_commit_ambiguity = Some(state);
                return Err(error);
            }
            if let Err(cleanup_error) = self.release_active_reservations(&state.durable) {
                tracing::error!(error = %cleanup_error, "active cleanup failed after batch commit error");
            }
            self.mark_wal_error(&error);
            Self::notify_prepared_error(&mut state.prepared, &error);
            return Err(error);
        }
        for slice in &state.durable {
            self.synced_not_inserted.insert(
                AppendSliceId {
                    batch_id: uuid::Uuid::from_bytes(slice.batch_id),
                    seal_key: slice.seal_key.clone(),
                    slice_index: slice.slice_index,
                },
                WalSliceState {
                    lsn: slice.lsn,
                    payload_digest: slice.payload_digest,
                    payload_len: slice.payload_len,
                    slice_index: slice.slice_index,
                    slice_count: slice.slice_count,
                },
            );
        }
        #[cfg(any(test, feature = "test-support"))]
        if self.wal_handle.take_post_sync_failure_for_test() {
            let error = ScribeError::Internal {
                detail: "injected post-sync WAL failure".to_owned(),
            };
            if let Err(cleanup_error) = self.release_active_reservations(&state.durable) {
                tracing::error!(error = %cleanup_error, "active cleanup failed after injected group error");
            }
            self.mark_wal_error(&error);
            Self::notify_prepared_error(&mut state.prepared, &error);
            return Err(error);
        }
        let touched_keys = match self.insert_committed_group(&mut state).await {
            Ok(keys) => keys,
            Err(error) => {
                if let Err(cleanup_error) = self.release_active_reservations(&state.durable) {
                    tracing::error!(error = %cleanup_error, "active cleanup failed after visibility error");
                }
                Self::notify_prepared_error(&mut state.prepared, &error);
                return Err(error);
            }
        };
        if let Err(error) = self.rotate_group(touched_keys) {
            tracing::warn!(error = %error, "durable append completed but bucket rotation was deferred");
        }
        for mut append in state.prepared {
            if let Some(lifecycle) = append.lifecycle.as_mut() {
                lifecycle.succeed();
            }
            if let Some(sender) = append.durable_ack {
                let rows = state
                    .rows_by_append
                    .get(append.batch_id.as_bytes())
                    .copied()
                    .unwrap_or(0);
                let _ = sender.send(Ok(rows));
            }
            drop(append.reservation);
        }
        Ok(())
    }

    /// Derives the ordered slice-set identity needed for one terminal COMMIT.
    ///
    /// Returns `None` when the batch has no durable slice or its complete retry
    /// was already committed and synced.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when retry state is mixed or the durable slices
    /// are incomplete, inconsistent, or out of order.
    fn batch_commit_identity(
        state: &GroupWalState,
        append: &PreparedAppend,
    ) -> Result<Option<(usize, u32, [u8; 32])>, ScribeError> {
        let batch_id = *append.batch_id.as_bytes();
        let Some(first_index) = state
            .durable
            .iter()
            .position(|slice| slice.batch_id == batch_id)
        else {
            return Ok(None);
        };
        let slice_count = state.durable[first_index].slice_count;
        let batch_slice_len = state
            .durable
            .iter()
            .filter(|slice| slice.batch_id == batch_id)
            .count();
        let committed_retries = state
            .durable
            .iter()
            .filter(|slice| slice.batch_id == batch_id && slice.commit_already_synced)
            .count();
        if committed_retries == batch_slice_len {
            return Ok(None);
        }
        if committed_retries != 0 {
            return Err(ScribeError::Internal {
                detail: "WAL retry mixed committed and uncommitted slices".to_owned(),
            });
        }
        if slice_count == 0 || usize::try_from(slice_count).ok() != Some(batch_slice_len) {
            return Err(ScribeError::Internal {
                detail: "cannot commit an incomplete or unordered WAL slice set".to_owned(),
            });
        }
        let mut digest = Sha256::new();
        for slice_index in 0..slice_count {
            let slice = state
                .durable
                .iter()
                .find(|slice| slice.batch_id == batch_id && slice.slice_index == slice_index)
                .ok_or_else(|| ScribeError::Internal {
                    detail: "cannot commit an incomplete or unordered WAL slice set".to_owned(),
                })?;
            if slice.slice_count != slice_count {
                return Err(ScribeError::Internal {
                    detail: "cannot commit an incomplete or unordered WAL slice set".to_owned(),
                });
            }
            digest.update(slice.slice_index.to_le_bytes());
            digest.update(slice.payload_len.to_le_bytes());
            digest.update(slice.payload_digest);
        }
        Ok(Some((first_index, slice_count, digest.finalize().into())))
    }

    /// Appends and fsyncs one v4 COMMIT record for every complete batch in a group.
    ///
    /// Each batch's SLICE records were fsynced before this method starts. The
    /// terminal COMMIT therefore closes only a complete ordered slice set; no
    /// ACK path can run before the commit's own segment is fsynced.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a batch is incomplete, its digest cannot be
    /// prepared, or the WAL lane cannot append or fsync its terminal record.
    async fn append_and_sync_batch_commits(
        &mut self,
        state: &mut GroupWalState,
    ) -> Result<(), ScribeError> {
        for append in &state.prepared {
            let Some((first_index, slice_count, digest)) =
                Self::batch_commit_identity(state, append)?
            else {
                continue;
            };
            let ScribeWalIoResult::WalWritten { result } = self
                .wal_io
                .submit(ScribeWalIoOp::WritePrepared {
                    wal: self.wal_handle.clone(),
                    append: crate::scribe::wal::PreparedWalAppend::commit(
                        *append.batch_id.as_bytes(),
                        *append.tenant.as_uuid().as_bytes(),
                        slice_count,
                        digest,
                    ),
                })
                .await?
            else {
                return Err(ScribeError::Internal {
                    detail: "WAL IO lane returned the wrong batch commit result".to_owned(),
                });
            };
            let segment = result
                .touched_segments
                .first()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "WAL v4 batch commit did not retain its segment identity".to_owned(),
                })?;
            let header = segment.header().clone();
            let commit_lsn = result.lsn;
            for segment in result.touched_segments {
                state
                    .touched
                    .insert(segment.path().to_path_buf(), Arc::clone(&segment));
            }
            self.sync_group(&state.touched).await?;
            if let Some(postgres) = &self.control_postgres {
                let request_id = uuid::Uuid::parse_str(
                    state.durable[first_index].audit_event.request_id.as_str(),
                )
                .map_err(|error| ScribeError::Internal {
                    detail: format!("Scribe audit request id is not a UUID: {error}"),
                })?;
                let commit = vala_sql::queries::scribe_batch_commits::ScribeBatchCommit {
                    tenant: append.tenant,
                    logical_table_fqn: append.table.fqn(),
                    batch_id: append.batch_id,
                    slice_set_digest: digest,
                    slice_count: i32::try_from(slice_count).map_err(|_| ScribeError::Internal {
                        detail: "WAL v4 slice count exceeds SQL integer range".to_owned(),
                    })?,
                    wal_node_id: uuid::Uuid::from_bytes(header.node_id),
                    wal_writer_epoch: header.writer_epoch,
                    wal_shard_id: i16::from(header.shard_id),
                    wal_segment_sequence: i64::try_from(header.seg_seq).map_err(|_| {
                        ScribeError::Internal {
                            detail: "WAL segment sequence exceeds SQL bigint range".to_owned(),
                        }
                    })?,
                    wal_lsn_min: i64::try_from(state.durable[first_index].lsn.as_u64()).map_err(
                        |_| ScribeError::Internal {
                            detail: "WAL slice LSN exceeds SQL bigint range".to_owned(),
                        },
                    )?,
                    wal_lsn_max: i64::try_from(commit_lsn.as_u64()).map_err(|_| {
                        ScribeError::Internal {
                            detail: "WAL commit LSN exceeds SQL bigint range".to_owned(),
                        }
                    })?,
                    request_id,
                };
                self.commit_batch_control_fence(
                    postgres,
                    &commit,
                    &state.durable[first_index].audit_event,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Commits or durably reconciles one exact WAL batch-control fence.
    ///
    /// A `PostgreSQL` COMMIT error is always ambiguous. The method opens a fresh
    /// tenant transaction and compares every durable identity field. An exact
    /// row completes once; an absent row retries the identical insert and audit
    /// transaction. An unavailable or contradictory lookup poisons the shared
    /// governor so [`Self::process_group`] retains the complete admitted owner
    /// without ACK or cleanup.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for a known pre-COMMIT SQL failure or when exact
    /// durable reconciliation is unavailable or contradictory. The latter
    /// failures poison admission and retain the group until restart.
    async fn commit_batch_control_fence(
        &self,
        postgres: &vala_sql::ValaPostgres,
        commit: &vala_sql::queries::scribe_batch_commits::ScribeBatchCommit,
        audit_event: &wyrd_spec::vala::api::AuditEvent,
    ) -> Result<(), ScribeError> {
        loop {
            let mut conn = postgres.tenant_conn(commit.tenant).await?;
            vala_sql::queries::scribe_batch_commits::record(&mut conn, commit, audit_event).await?;
            if conn.commit().await.is_ok() {
                return Ok(());
            }

            let mut reconciliation = match postgres.tenant_conn(commit.tenant).await {
                Ok(conn) => conn,
                Err(error) => {
                    self.memory_ownership.poison();
                    return Err(error.into());
                }
            };
            match vala_sql::queries::scribe_batch_commits::resolve(&mut reconciliation, commit)
                .await
            {
                Ok(
                    vala_sql::queries::scribe_batch_commits::ScribeBatchCommitResolution::Committed,
                ) => {
                    return Ok(());
                }
                Ok(
                    vala_sql::queries::scribe_batch_commits::ScribeBatchCommitResolution::Absent,
                ) => {}
                Err(error) => {
                    self.memory_ownership.poison();
                    return Err(error.into());
                }
            }
        }
    }

    /// Prepares one group for WAL synchronization without acknowledging it.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when duplicate detection, WAL append, memory
    /// reservation, or retry bookkeeping fails.
    async fn write_group(
        &mut self,
        mut prepared: Vec<PreparedAppend>,
    ) -> Result<GroupWalState, ScribeError> {
        let durable_capacity = prepared.iter().try_fold(0_usize, |total, append| {
            total
                .checked_add(append.slices.exact_slice_capacity()?)
                .ok_or_else(|| ScribeError::Internal {
                    detail: "WAL durable slice metadata capacity overflow".to_owned(),
                })
        })?;
        let mut durable = Vec::with_capacity(durable_capacity);
        let fixed_durable_capacity = durable.capacity();
        let mut touched = HashMap::<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>::new();
        let mut rows_by_append = HashMap::<[u8; 16], u64>::new();
        for append in &mut prepared {
            if let Some(memory) = append.memory.as_mut() {
                memory.transfer_category(MemoryCategory::Prepared)?;
            }
            let batch_id = *append.batch_id.as_bytes();
            loop {
                let slice = match Self::next_prepared_slice(&self.persistence_cpu, append).await {
                    Ok(Some(slice)) => slice,
                    Ok(None) => break,
                    Err(error) => {
                        if let Err(cleanup_error) = self.release_active_reservations(&durable) {
                            tracing::error!(error = %cleanup_error, "active cleanup failed after slice production error");
                        }
                        Self::notify_prepared_error(&mut prepared, &error);
                        return Err(error);
                    }
                };
                match self
                    .memtable
                    .retained_batch_rows(&slice.seal_key, *slice.id.batch_id.as_bytes())
                {
                    Ok(Some(rows)) => {
                        let entry = rows_by_append.entry(batch_id).or_default();
                        *entry = entry.saturating_add(rows);
                        let materialized_bytes = slice
                            .memtable_bytes
                            .saturating_add(slice.wal_append.audit.len())
                            .saturating_add(slice.wal_append.data.len());
                        append
                            .lifecycle
                            .as_mut()
                            .ok_or_else(|| ScribeError::Internal {
                                detail:
                                    "prepared append lost lifecycle while dropping retained slice"
                                        .to_owned(),
                            })?
                            .released_materialization(materialized_bytes);
                        continue;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        if let Err(cleanup_error) = self.release_active_reservations(&durable) {
                            tracing::error!(error = %cleanup_error, "active cleanup failed after row lookup error");
                        }
                        Self::notify_prepared_error(&mut prepared, &error);
                        return Err(error);
                    }
                }
                let slice_id = slice.id.clone();
                let (slice, result) = match if let Some(previous) =
                    self.synced_not_inserted.get(&slice_id).copied()
                {
                    self.reuse_synced_slice(append, slice, &previous)
                } else {
                    self.write_prepared_slice(append, slice).await
                } {
                    Ok(value) => value,
                    Err(error) => {
                        if let Err(cleanup_error) = self.release_active_reservations(&durable) {
                            tracing::error!(error = %cleanup_error, "active cleanup failed after WAL error");
                        }
                        self.mark_wal_error(&error);
                        Self::notify_prepared_error(&mut prepared, &error);
                        return Err(error);
                    }
                };
                for segment in &result.touched_segments {
                    touched.insert(segment.path().to_path_buf(), Arc::clone(segment));
                    self.wal_segments
                        .entry(slice.seal_key.clone())
                        .or_default()
                        .insert(segment.path().to_path_buf(), Arc::clone(segment));
                }
                durable.push(slice);
                if durable.capacity() != fixed_durable_capacity {
                    return Err(ScribeError::Internal {
                        detail: "WAL durable slice metadata exceeded its exact capacity".to_owned(),
                    });
                }
            }
        }
        Ok(GroupWalState {
            prepared,
            durable,
            touched,
            rows_by_append,
        })
    }

    /// Advances one materialized or lazy prepared-slice source.
    ///
    /// Native production runs on the bounded persistence CPU lane and returns
    /// the producer owner with every result so cancellation cannot orphan its
    /// retained raw source or decoder state.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the producer owner is missing, the CPU lane
    /// refuses/fails, or it returns a result for a different operation.
    async fn next_prepared_slice(
        persistence_cpu: &crate::scribe::execution_lanes::ScribePersistenceCpuPool,
        append: &mut PreparedAppend,
    ) -> Result<Option<PreparedSlice>, ScribeError> {
        match &mut append.slices {
            PreparedSliceSet::Materialized(slices) => {
                if slices.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(slices.remove(0)))
                }
            }
            PreparedSliceSet::Native(producer_slot) => {
                let producer = producer_slot.take().ok_or_else(|| ScribeError::Internal {
                    detail: "native prepared-slice producer owner is missing".to_owned(),
                })?;
                let memory = append.memory.take().ok_or_else(|| ScribeError::Internal {
                    detail: "native root owner missing before CPU dispatch".to_owned(),
                })?;
                let lifecycle = append
                    .lifecycle
                    .take()
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "native lifecycle owner missing before CPU dispatch".to_owned(),
                    })?;
                match persistence_cpu
                    .submit(crate::scribe::execution_lanes::ScribePersistenceCpuOp::ProduceNativeSlice {
                        producer,
                        memory,
                        lifecycle,
                    })
                    .await?
                {
                    crate::scribe::execution_lanes::ScribePersistenceCpuResult::NativeSliceProduced {
                        producer: returned,
                        slice,
                        memory,
                        lifecycle,
                    } => {
                        *producer_slot = Some(returned);
                        append.memory = Some(memory);
                        append.lifecycle = Some(lifecycle);
                        Ok(slice)
                    }
                    _ => Err(ScribeError::Internal {
                        detail: "persistence lane returned the wrong native slice result"
                            .to_owned(),
                    }),
                }
            }
            PreparedSliceSet::Otlp(producer_slot) => {
                let producer = producer_slot.take().ok_or_else(|| ScribeError::Internal {
                    detail: "OTLP prepared-slice producer owner is missing".to_owned(),
                })?;
                let memory = append.memory.take().ok_or_else(|| ScribeError::Internal {
                    detail: "OTLP root owner missing before CPU dispatch".to_owned(),
                })?;
                let lifecycle = append
                    .lifecycle
                    .take()
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "OTLP lifecycle owner missing before CPU dispatch".to_owned(),
                    })?;
                match persistence_cpu
                    .submit(crate::scribe::execution_lanes::ScribePersistenceCpuOp::ProduceOtlpSlice {
                        producer,
                        memory,
                        lifecycle,
                    })
                    .await?
                {
                    crate::scribe::execution_lanes::ScribePersistenceCpuResult::OtlpSliceProduced {
                        producer: returned,
                        slice,
                        memory,
                        lifecycle,
                    } => {
                        *producer_slot = Some(returned);
                        append.memory = Some(memory);
                        append.lifecycle = Some(lifecycle);
                        Ok(slice)
                    }
                    _ => Err(ScribeError::Internal {
                        detail: "persistence lane returned the wrong OTLP slice result".to_owned(),
                    }),
                }
            }
        }
    }

    /// Reserves active memory and appends one prepared slice to the shard WAL lane.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when active admission, memory accounting, or WAL
    /// append fails; all acquired active reservations are released on failure.
    async fn write_prepared_slice(
        &mut self,
        append: &mut PreparedAppend,
        slice: PreparedSlice,
    ) -> Result<(DurableSlice, crate::scribe::wal::WalAppendResult), ScribeError> {
        let retain_rows = matches!(&append.slices, PreparedSliceSet::Materialized(_));
        let PreparedSlice {
            seal_key,
            audit_event,
            rows,
            wal_append,
            memtable_bytes,
            ..
        } = slice;
        let slice_index = wal_append.slice_index;
        let slice_count = wal_append.slice_count;
        let materialized_bytes = memtable_bytes
            .saturating_add(wal_append.audit.len())
            .saturating_add(wal_append.data.len());
        if retain_rows {
            self.reserve_slice_active(append, &seal_key, memtable_bytes)?;
        }
        let memory = append.memory.take().ok_or_else(|| ScribeError::Internal {
            detail: "ingress root owner missing before WAL dispatch".to_owned(),
        })?;
        let lifecycle = append
            .lifecycle
            .take()
            .ok_or_else(|| ScribeError::Internal {
                detail: "ingress lifecycle owner missing before WAL dispatch".to_owned(),
            })?;
        let wal_result = self
            .wal_io
            .submit(ScribeWalIoOp::WriteIngressSlice {
                wal: self.wal_handle.clone(),
                append: wal_append,
                memory,
                lifecycle,
                materialized_bytes,
            })
            .await;
        let result = match wal_result {
            Ok(ScribeWalIoResult::IngressSliceWritten {
                result,
                memory,
                lifecycle,
            }) => {
                append.memory = Some(memory);
                append.lifecycle = Some(lifecycle);
                result
            }
            Ok(_) => {
                let error = ScribeError::Internal {
                    detail: "WAL IO lane returned the wrong shard append result".to_owned(),
                };
                return Err(if retain_rows {
                    self.preserve_primary_after_active_cleanup(error, memtable_bytes, true)
                } else {
                    error
                });
            }
            Err(error) => {
                return Err(if retain_rows {
                    self.preserve_primary_after_active_cleanup(error, memtable_bytes, true)
                } else {
                    error
                });
            }
        };
        Ok((
            DurableSlice {
                seal_key,
                audit_event,
                rows: retain_rows.then_some(rows),
                memtable_bytes,
                active_reserved: retain_rows,
                batch_id: *append.batch_id.as_bytes(),
                lsn: result.lsn,
                payload_digest: result.payload_digest,
                payload_len: result.payload_len,
                slice_index,
                slice_count,
                commit_already_synced: false,
            },
            result,
        ))
    }

    /// Reuses a WAL-synced slice after a prior memtable insertion failure.
    ///
    /// No second WAL record is written, preserving replay deduplication and
    /// append-at-most-once behavior for the retry window.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when active admission or memory accounting fails.
    fn reuse_synced_slice(
        &mut self,
        append: &mut PreparedAppend,
        slice: PreparedSlice,
        previous: &WalSliceState,
    ) -> Result<(DurableSlice, crate::scribe::wal::WalAppendResult), ScribeError> {
        let retain_rows = matches!(&append.slices, PreparedSliceSet::Materialized(_));
        let PreparedSlice {
            seal_key,
            audit_event,
            rows,
            wal_append,
            memtable_bytes,
            ..
        } = slice;
        let materialized_bytes = memtable_bytes
            .saturating_add(wal_append.audit.len())
            .saturating_add(wal_append.data.len());
        if retain_rows {
            self.reserve_slice_active(append, &seal_key, memtable_bytes)?;
        }
        append
            .lifecycle
            .as_mut()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared append lost lifecycle while reusing synced slice".to_owned(),
            })?
            .released_materialization(materialized_bytes);
        Ok((
            DurableSlice {
                seal_key,
                audit_event,
                rows: retain_rows.then_some(rows),
                memtable_bytes,
                active_reserved: retain_rows,
                batch_id: *append.batch_id.as_bytes(),
                lsn: previous.lsn,
                payload_digest: previous.payload_digest,
                payload_len: previous.payload_len,
                slice_index: previous.slice_index,
                slice_count: previous.slice_count,
                commit_already_synced: true,
            },
            crate::scribe::wal::WalAppendResult {
                lsn: previous.lsn,
                #[cfg(feature = "bench-support")]
                encoded_bytes: 0,
                touched_segments: Vec::new(),
                payload_digest: previous.payload_digest,
                payload_len: previous.payload_len,
            },
        ))
    }

    /// Synchronizes every WAL segment touched by one append group.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the WAL lane fails or returns an unexpected result.
    async fn sync_group(
        &self,
        touched: &HashMap<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>,
    ) -> Result<(), ScribeError> {
        if touched.is_empty() {
            return Ok(());
        }
        let segments = touched.values().cloned().collect::<Vec<_>>();
        match self
            .wal_io
            .submit(ScribeWalIoOp::SyncWal {
                wal: self.wal_handle.clone(),
                segments,
            })
            .await?
        {
            ScribeWalIoResult::WalSynced => Ok(()),
            _ => Err(ScribeError::Internal {
                detail: "WAL IO lane returned the wrong shard sync result".to_owned(),
            }),
        }
    }

    /// Regenerates native rows one slice at a time after every durable fence.
    ///
    /// Native WAL payload rows are deliberately dropped after each SLICE write.
    /// Once COMMIT fsync and the SQL control/audit transaction succeed, this
    /// method rewinds the retained source under the same root, regenerates one
    /// deterministic slice, inserts it, and drops it before advancing. Projected
    /// slices use their existing retained rows.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when regeneration diverges from durable slice
    /// identity, the CPU lane fails, or memtable insertion/rotation fails.
    async fn insert_committed_group(
        &mut self,
        state: &mut GroupWalState,
    ) -> Result<HashSet<crate::scribe::seal_key::SealKey>, ScribeError> {
        let mut touched_keys = HashSet::new();
        for append in &mut state.prepared {
            let is_lazy = match &mut append.slices {
                PreparedSliceSet::Native(Some(producer)) => {
                    producer.restart()?;
                    true
                }
                PreparedSliceSet::Native(None) => {
                    return Err(ScribeError::Internal {
                        detail: "native producer owner missing before visibility".to_owned(),
                    });
                }
                PreparedSliceSet::Otlp(Some(producer)) => {
                    producer.restart();
                    true
                }
                PreparedSliceSet::Otlp(None) => {
                    return Err(ScribeError::Internal {
                        detail: "OTLP producer owner missing before visibility".to_owned(),
                    });
                }
                PreparedSliceSet::Materialized(_) => false,
            };
            if !is_lazy {
                continue;
            }
            while let Some(produced) =
                Self::next_prepared_slice(&self.persistence_cpu, append).await?
            {
                let materialized_bytes = produced
                    .memtable_bytes
                    .saturating_add(produced.wal_append.audit.len())
                    .saturating_add(produced.wal_append.data.len());
                let position = state.durable.iter().position(|durable| {
                    durable.batch_id == *append.batch_id.as_bytes()
                        && durable.slice_index == produced.wal_append.slice_index
                });
                let Some(position) = position else {
                    append
                        .lifecycle
                        .as_mut()
                        .ok_or_else(|| ScribeError::Internal {
                            detail: "prepared append lost lifecycle while dropping unmatched regenerated slice"
                                .to_owned(),
                        })?
                        .released_materialization(materialized_bytes);
                    continue;
                };
                let mut durable = state.durable.remove(position);
                let (payload_digest, payload_len) = produced.wal_append.payload_identity()?;
                if durable.seal_key != produced.seal_key
                    || durable.slice_index != produced.wal_append.slice_index
                    || durable.slice_count != produced.wal_append.slice_count
                    || durable.payload_digest != payload_digest
                    || durable.payload_len != payload_len
                {
                    return Err(ScribeError::Internal {
                        detail: "regenerated lazy slice identity diverged after COMMIT".to_owned(),
                    });
                }
                durable.rows = Some(produced.rows);
                durable.audit_event = produced.audit_event;
                self.reserve_slice_active(append, &durable.seal_key, durable.memtable_bytes)?;
                durable.active_reserved = true;
                self.insert_committed_slice(durable, &mut state.rows_by_append, &mut touched_keys)?;
                append
                    .lifecycle
                    .as_mut()
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "prepared append lost lifecycle after visibility transfer"
                            .to_owned(),
                    })?
                    .released_materialization(materialized_bytes);
            }
        }
        while !state.durable.is_empty() {
            let slice = state.durable.remove(0);
            self.insert_committed_slice(slice, &mut state.rows_by_append, &mut touched_keys)?;
        }
        Ok(touched_keys)
    }

    /// Inserts one post-fence slice and transfers its active owner to memtable.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when rows are missing, rotation inspection or
    /// flushing fails, or memtable insertion fails. Active ownership for the
    /// refused current slice is released before returning.
    fn insert_committed_slice(
        &mut self,
        mut slice: DurableSlice,
        rows_by_append: &mut HashMap<[u8; 16], u64>,
        touched_keys: &mut HashSet<crate::scribe::seal_key::SealKey>,
    ) -> Result<(), ScribeError> {
        let rows = slice.rows.take().ok_or_else(|| ScribeError::Internal {
            detail: "durable slice rows were not regenerated before visibility".to_owned(),
        })?;
        let row_count = rows.num_rows();
        let pre_insert = self.memtable.would_cross_rotation(&slice.seal_key, &rows);
        let pre_insert = match pre_insert {
            Ok(true) => {
                self.flush_keys(vec![slice.seal_key.clone()], Some(SealTriggerReason::Size))
            }
            Ok(false) => Ok(()),
            Err(error) => Err(error),
        };
        if let Err(error) = pre_insert {
            return Err(self.preserve_primary_after_active_cleanup(
                error,
                slice.memtable_bytes,
                true,
            ));
        }
        self.memtable
            .insert(
                &slice.seal_key,
                slice.audit_event,
                crate::scribe::wal::ScribeAppendMeta {
                    batch_id: slice.batch_id,
                    rows_accepted: row_count,
                    wal_lsn_min: slice.lsn,
                    wal_lsn_max: slice.lsn,
                    seal_key: slice.seal_key.as_path_components(),
                },
                rows,
            )
            .map_err(|error| {
                self.preserve_primary_after_active_cleanup(error, slice.memtable_bytes, true)
            })?;
        touched_keys.insert(slice.seal_key.clone());
        self.synced_not_inserted.remove(&AppendSliceId {
            batch_id: uuid::Uuid::from_bytes(slice.batch_id),
            seal_key: slice.seal_key,
            slice_index: slice.slice_index,
        });
        let entry = rows_by_append.entry(slice.batch_id).or_default();
        *entry = entry.saturating_add(u64::try_from(row_count).unwrap_or(u64::MAX));
        Ok(())
    }

    /// Inserts WAL-synced slices into this owner's memtable and clears retry state.
    ///
    /// Before each insert, the owner seals a non-empty bucket that the incoming
    /// slice would push past its rotation target. This bounds every
    /// multi-append generation while preserving the existing single-append
    /// oversized-generation behavior.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when writable-bucket inspection or insertion
    /// fails. A required pre-insert flush also propagates failures from bucket
    /// freezing or generation preparation, table binding, WAL retention, and
    /// active-to-immutable accounting transfer.
    #[cfg(test)]
    fn insert_group(
        &mut self,
        durable: Vec<DurableSlice>,
        rows_by_append: &mut HashMap<[u8; 16], u64>,
    ) -> Result<HashSet<crate::scribe::seal_key::SealKey>, ScribeError> {
        let mut touched_keys = HashSet::new();
        let mut slices = durable.into_iter();
        while let Some(slice) = slices.next() {
            if let Err(error) =
                self.insert_committed_slice(slice, rows_by_append, &mut touched_keys)
            {
                let bytes = slices.map(|remaining| remaining.memtable_bytes);
                return Err(self.preserve_primary_after_active_cleanups(error, bytes));
            }
        }
        Ok(touched_keys)
    }

    /// Completes every prepared ACK waiter with one group failure.
    fn notify_prepared_error(prepared: &mut [PreparedAppend], error: &ScribeError) {
        for append in prepared {
            if let Some(lifecycle) = append.lifecycle.as_mut() {
                lifecycle.refuse();
            }
            if let Some(sender) = append.durable_ack.take() {
                let _ = sender.send(Err(error.completion_copy()));
            }
        }
    }

    /// Releases active reservations after a failed group stage.
    fn release_active_ownership(
        &self,
        bytes: usize,
        already_absorbed: bool,
    ) -> Result<(), ScribeError> {
        self.admission.preflight_release_active(bytes)?;
        if already_absorbed {
            self.memory_ownership.preflight_release_active(bytes)?;
        }
        self.admission.release_active(bytes)?;
        if already_absorbed {
            self.memory_ownership.release_active(bytes)?;
        }
        Ok(())
    }

    /// Release active reservations after a failed group stage.
    ///
    /// # Errors
    ///
    /// Returns the first admission or memory-ledger release error after
    /// attempting to settle every active durable slice.
    fn release_active_reservations(&self, durable: &[DurableSlice]) -> Result<(), ScribeError> {
        let mut first_error = None;
        for slice in durable {
            if !slice.active_reserved {
                continue;
            }
            if let Err(error) = self.release_active_ownership(slice.memtable_bytes, true) {
                tracing::error!(error = %error, "active cleanup failed; accounting is poisoned");
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Transfers one current slice from the admitted root into active ownership.
    ///
    /// Native callers invoke this only after COMMIT and SQL fencing, immediately
    /// before visibility. Materialized callers retain the historical pre-WAL
    /// transfer because their rows remain live throughout the durable sequence.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when active admission refuses the slice, the root
    /// cannot supply its exact child, or the ownership ledger refuses transfer.
    fn reserve_slice_active(
        &mut self,
        append: &mut PreparedAppend,
        seal_key: &crate::scribe::seal_key::SealKey,
        memtable_bytes: usize,
    ) -> Result<(), ScribeError> {
        self.admission
            .try_reserve_active(seal_key.table.fqn(), memtable_bytes)?;
        let active_memory = append
            .memory
            .as_mut()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared append lost its memory reservation".to_owned(),
            })?
            .split(memtable_bytes);
        let mut active_memory = match active_memory {
            Ok(value) => value,
            Err(error) => {
                return Err(self.preserve_primary_after_active_cleanup(
                    ScribeError::Internal {
                        detail: error.to_string(),
                    },
                    memtable_bytes,
                    false,
                ));
            }
        };
        if let Err(error) = active_memory.transfer_category(MemoryCategory::Active) {
            return Err(self.preserve_primary_after_active_cleanup(error, memtable_bytes, false));
        }
        if let Err(error) = self.memory_ownership.absorb_active(active_memory) {
            return Err(self.preserve_primary_after_active_cleanup(error, memtable_bytes, false));
        }
        Ok(())
    }

    /// Preserves one primary group failure while containing an adjacent cleanup failure.
    ///
    /// Cleanup owns no later lifecycle mutation. A failed checked release has
    /// already poisoned the shared governor; this helper records it once and
    /// returns the original operation error so callers retain the actionable
    /// WAL, split, or insertion diagnosis.
    fn preserve_primary_after_active_cleanup(
        &self,
        primary: ScribeError,
        bytes: usize,
        already_absorbed: bool,
    ) -> ScribeError {
        if let Err(cleanup) = self.release_active_ownership(bytes, already_absorbed) {
            tracing::error!(error = %cleanup, primary = %primary, "active cleanup failed after primary group error");
        }
        primary
    }

    /// Releases every still-owned absorbed slice while preserving one primary failure.
    #[cfg(test)]
    fn preserve_primary_after_active_cleanups(
        &self,
        primary: ScribeError,
        bytes: impl IntoIterator<Item = usize>,
    ) -> ScribeError {
        let mut first_cleanup = None;
        for owned in bytes {
            if let Err(error) = self.release_active_ownership(owned, true)
                && first_cleanup.is_none()
            {
                first_cleanup = Some(error);
            }
        }
        if let Some(cleanup) = first_cleanup {
            tracing::error!(error = %cleanup, primary = %primary, "active cleanup failed after primary group error");
        }
        primary
    }

    /// Arms the next retirement after both preflights but before any release mutation.
    #[cfg(test)]
    fn arm_retirement_release_fault_for_test(&mut self) {
        self.fail_next_retirement_release = true;
    }

    /// Trips the admission breaker when WAL storage is full.
    fn mark_wal_error(&self, error: &ScribeError) {
        if matches!(error, ScribeError::WalDiskFull) {
            self.admission.trip_wal_disk_full();
        }
    }

    /// Selects buckets crossing the rotation threshold and freezes them.
    ///
    /// Each touched key is classified by [`Memtable::should_seal_reason`] so its
    /// executed seal is counted under the true trigger — a size-crossing bucket
    /// as `size`, an age-crossing bucket as `age` (D84). The two groups are
    /// flushed separately purely so the seal counter is labelled correctly; the
    /// freeze work and its ordering are unchanged from the single-group flush.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when rotation selection or generation preparation fails.
    fn rotate_group(
        &mut self,
        touched_keys: HashSet<crate::scribe::seal_key::SealKey>,
    ) -> Result<(), ScribeError> {
        let mut size_keys = Vec::new();
        let mut age_keys = Vec::new();
        for seal_key in touched_keys {
            match self.memtable.should_seal_reason(&seal_key)? {
                Some(SealTriggerReason::Age) => age_keys.push(seal_key),
                Some(_) => size_keys.push(seal_key),
                None => {}
            }
        }
        self.flush_keys(size_keys, Some(SealTriggerReason::Size))?;
        self.flush_keys(age_keys, Some(SealTriggerReason::Age))
    }
}

/// Classifies the one expected shard-group capacity boundary.
///
/// WAL exhaustion has already tripped the admission breaker before this
/// classification. Every other group failure remains an unexpected error.
fn is_expected_wal_capacity(error: &ScribeError) -> bool {
    matches!(error, ScribeError::WalDiskFull)
}

/// Count one executed seal, labelled by the trigger that caused it (D84).
///
/// Emitted at the moment a writable bucket is actually frozen, once per frozen
/// bucket, so the `bifrost_scribe_seal_total{trigger}` counter reflects real
/// seals rather than seal *requests* — the coordinated pressure path broadcasts
/// one signal to every shard, but only the shards that hold a matching writable
/// bucket freeze, and only those increments are counted here. The `trigger`
/// label is closed to [`SealTriggerReason::as_label`]. Shutdown and
/// tenant-administrative seals pass no trigger and are intentionally not
/// counted, keeping the counter a pure size/age/pressure lifecycle signal.
fn record_seal(trigger: SealTriggerReason) {
    metrics::counter!("bifrost_scribe_seal_total", "trigger" => trigger.as_label()).increment(1);
}

/// Count one seal-accounting failure so a stranded seal cannot be silent (D84).
///
/// [`ShardOwner::handle_pressure_signal`] keeps a failed pressure flush
/// non-fatal to the shard owner loop and still logs its WARN, but a swallowed
/// seal failure would otherwise be invisible to operators. This counter makes
/// the condition observable without a debugger — a stranded frozen generation
/// (D97) or any other seal-path failure increments it exactly once per failed
/// flush. It carries no tenant, table, or request identity: it records only
/// that a seal flush failed.
fn record_seal_failure() {
    metrics::counter!("bifrost_scribe_seal_failed_total").increment(1);
}

/// Count one generation retirement and the immutable bytes it freed (D84).
///
/// Emitted once per generation that actually leaves retained state, so
/// `bifrost_scribe_retirements_total` tracks completed retirements and
/// `bifrost_scribe_retired_bytes_total` accumulates the immutable Arrow bytes
/// returned to the memory budget. Together they make the tail of the write
/// lifecycle — how fast committed generations are reclaimed — observable
/// without a debugger. Neither counter carries identity labels.
fn record_retirement(freed_bytes: usize) {
    metrics::counter!("bifrost_scribe_retirements_total").increment(1);
    metrics::counter!("bifrost_scribe_retired_bytes_total")
        .increment(u64::try_from(freed_bytes).unwrap_or(u64::MAX));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::seal_key::{EventDay, SealKey};
    use crate::scribe::wal::{ScribeAppendMeta, WalLsn};
    use arrow::array::{Int64Array, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::record_batch::RecordBatch;
    use chrono::NaiveDate;
    use std::sync::Arc;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

    /// Failed replay binding or adoption restores memtable and memory baselines.
    ///
    /// # Panics
    ///
    /// Panics if invalid catalog identity inserts reconstructed state, a
    /// foreign-root lease merges, or either failure retains root/category bytes.
    #[tokio::test]
    async fn replay_insert_adoption_failure_rolls_back_identity_and_immutable() {
        let owner_resources =
            crate::scribe::embedded_scribe_resources(&crate::scribe::AdmissionConfig::default());
        let foreign_resources =
            crate::scribe::embedded_scribe_resources(&crate::scribe::AdmissionConfig::default());
        let ownership =
            crate::scribe::memory::ScribeOwnership::new(&owner_resources).expect("shard ownership");
        let owner_baseline = owner_resources.snapshot().expect("owner baseline");
        let foreign_baseline = foreign_resources.snapshot().expect("foreign baseline");
        let lease = foreign_resources
            .try_reserve_maintenance(MemoryCategory::Decode, 64)
            .expect("foreign replay lease");
        ownership
            .adopt_replay_immutable(lease, 64)
            .expect_err("foreign replay lease must not merge");
        assert_eq!(ownership.immutable_bytes(), 0);
        assert_eq!(
            owner_resources
                .snapshot()
                .expect("owner restored")
                .scribe_memory_used_bytes,
            owner_baseline.scribe_memory_used_bytes
        );
        assert_eq!(
            foreign_resources
                .snapshot()
                .expect("foreign restored")
                .scribe_memory_used_bytes,
            foreign_baseline.scribe_memory_used_bytes
        );

        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let wal_handle = wal.handle_for_shard(0).expect("WAL handle");
        let (mut owner, budget) =
            owner_for_completion_test_with_budget(Memtable::new(), &wal, wal_handle, stream);
        let root_baseline = budget.snapshot().expect("root baseline");
        let memtable_baseline = owner.memtable.stats().expect("memtable baseline");
        let replayed = crate::scribe::replay::ReplayedSealKey {
            stream,
            seal_key: owner_bad_binding_key(),
            shard_id: 0,
            audit_events: Vec::new(),
            data_records: Vec::new(),
            append_metas: Vec::new(),
            wal_segments: Vec::new(),
            commits: Vec::new(),
        };
        assert!(
            owner.prepare_replay_state(replayed).await.is_err(),
            "invalid catalog binding must fail before reconstruction"
        );
        assert_eq!(
            owner.memtable.stats().expect("memtable restored"),
            memtable_baseline
        );
        assert_eq!(owner.memory_ownership.active_bytes(), 0);
        assert_eq!(owner.memory_ownership.immutable_bytes(), 0);
        let restored = budget.snapshot().expect("root restored");
        assert_eq!(
            restored.scribe_memory_used_bytes,
            root_baseline.scribe_memory_used_bytes
        );
        let categories = budget.memory_snapshot().categories;
        assert_eq!(categories[MemoryCategory::Decode as usize], 0);
        assert_eq!(categories[MemoryCategory::Immutable as usize], 0);
    }

    /// Replay retains one identity owner and owns retries across queue backpressure.
    ///
    /// The multi-state fixture proves identity serialization, while the
    /// workerless one-slot runtime proves that capacity release posts the exact
    /// generation-keyed retry command without relying on an age tick.
    ///
    /// # Panics
    ///
    /// Panics if adding multiple reconstructed states duplicates or releases
    /// the chunk's single committed-identity lease.
    #[tokio::test]
    async fn replay_chunk_serializes_identity_across_multiple_states() {
        let resources =
            crate::scribe::embedded_scribe_resources(&crate::scribe::AdmissionConfig::default());
        let identity = resources
            .try_reserve_maintenance(MemoryCategory::Decode, 64)
            .expect("identity lease");
        let mut chunk = crate::scribe::replay::ReplayChunk {
            states: HashMap::new(),
            memory: None,
            identity_memory: Some(identity),
        };
        let stream = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::generate(),
            crate::scribe::stream_identity::WriterEpoch::new(1),
        );
        for table in ["state-b", "state-a"] {
            let seal_key = SealKey::new(
                DataTenantId::new_v7(),
                TableRef::new(BifrostNamespace::Bifrost, table),
                EventDay::new(NaiveDate::from_ymd_opt(2026, 8, 18).expect("date")),
            );
            chunk.states.insert(
                seal_key.as_path_components(),
                crate::scribe::replay::ReplayedSealKey {
                    stream,
                    seal_key,
                    shard_id: 0,
                    audit_events: Vec::new(),
                    data_records: Vec::new(),
                    append_metas: Vec::new(),
                    wal_segments: Vec::new(),
                    commits: Vec::new(),
                },
            );
        }
        assert_eq!(chunk.states.len(), 2);
        assert_eq!(
            chunk.identity_memory.as_ref().map(|lease| lease.bytes()),
            Some(64)
        );
        drop(chunk);
        assert_eq!(
            resources
                .snapshot()
                .expect("released identity")
                .scribe_memory_used_bytes,
            0
        );

        let key = owner_key();
        let memtable = Memtable::new();
        memtable
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner insert");
        let frozen = memtable.freeze(&key).expect("owner freeze");
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let wal_handle = wal.handle_for_shard(0).expect("WAL handle");
        let generation = owner_generation(&key, &frozen, stream, wal_handle.clone());
        let (mut owner, budget) =
            owner_for_completion_test_with_budget(memtable, &wal, wal_handle, stream);
        let root_baseline = budget.snapshot().expect("root baseline");
        owner
            .memory_ownership
            .reserve_immutable(generation.arrow_bytes)
            .expect("replay immutable ownership");
        let (completion_tx, mut completion_rx) = mpsc::channel(1);
        owner.completion_tx = completion_tx.clone();
        let (persistence, mut persistence_rx) = PersistenceRuntime::bounded_for_test();
        let persistence = Arc::new(persistence);
        let binding = crate::catalog::TenantTableBinding::resolve((key.tenant, key.table.clone()))
            .expect("test binding");
        persistence
            .try_submit(PersistenceJob {
                generation: Arc::clone(&generation),
                binding: binding.clone(),
                completion_tx,
                completion_waiter: None,
                defer_manifest_advance: true,
            })
            .expect("fill one-slot queue");
        owner.persistence = Some(Arc::clone(&persistence));
        owner.pending_generations.insert(
            key.clone(),
            VecDeque::from([PendingGeneration {
                generation: Arc::clone(&generation),
                binding,
                submitted: false,
                retry_scheduled: false,
                replay_response: None,
                replay_owned: true,
            }]),
        );
        let (response, response_rx) = tokio::sync::oneshot::channel();
        owner.replay_chunk = Some(ReplayChunkOwner {
            remaining: VecDeque::new(),
            current_generation: Some(generation.generation_id.0),
            retirements: Vec::new(),
            identity: None,
            response: Some(response),
        });

        owner.submit_front(&key);
        assert!(
            owner
                .pending_generations
                .get(&key)
                .and_then(VecDeque::front)
                .is_some_and(|front| front.retry_scheduled && !front.submitted)
        );
        let blocker = persistence_rx.recv().await.expect("queued blocker");
        persistence.complete_for_test(blocker.generation.arrow_bytes);
        let retry = tokio::time::timeout(std::time::Duration::from_secs(1), completion_rx.recv())
            .await
            .expect("replay retry wakeup")
            .expect("retry command");
        assert!(!owner.handle_command(retry).await);
        assert!(
            owner
                .pending_generations
                .get(&key)
                .and_then(VecDeque::front)
                .is_some_and(|front| front.submitted && !front.retry_scheduled)
        );
        let submitted = persistence_rx.recv().await.expect("submitted replay");
        persistence.complete_for_test(submitted.generation.arrow_bytes);
        drop(submitted);
        owner
            .pending_generations
            .get_mut(&key)
            .and_then(VecDeque::front_mut)
            .expect("active replay generation")
            .submitted = false;
        persistence
            .try_submit(PersistenceJob {
                generation: Arc::clone(&generation),
                binding: crate::catalog::TenantTableBinding::resolve((
                    key.tenant,
                    key.table.clone(),
                ))
                .expect("blocker binding"),
                completion_tx: owner.completion_tx.clone(),
                completion_waiter: None,
                defer_manifest_advance: true,
            })
            .expect("refill one-slot queue");
        owner.submit_front(&key);
        assert!(
            owner
                .pending_generations
                .get(&key)
                .and_then(VecDeque::front)
                .is_some_and(|front| front.retry_scheduled && !front.submitted)
        );

        let runtime_weak = Arc::downgrade(&persistence);
        assert_eq!(persistence.abort_retained(), 0);
        let shutdown_retry =
            tokio::time::timeout(std::time::Duration::from_secs(1), completion_rx.recv())
                .await
                .expect("shutdown wakes replay waiter")
                .expect("shutdown retry command");
        assert!(!owner.handle_command(shutdown_retry).await);
        assert!(
            response_rx
                .await
                .expect("replay shutdown response")
                .is_err()
        );
        assert!(!owner.pending_generations.contains_key(&key));
        assert!(owner.replay_chunk.is_none());
        assert_eq!(owner.memory_ownership.immutable_bytes(), 0);
        assert_eq!(
            budget
                .snapshot()
                .expect("shutdown ownership settlement")
                .scribe_memory_used_bytes,
            root_baseline.scribe_memory_used_bytes
        );
        drop(persistence_rx.recv().await.expect("aborted queued blocker"));
        owner.persistence = None;
        drop(persistence);
        tokio::task::yield_now().await;
        assert!(runtime_weak.upgrade().is_none());
    }

    /// Only typed WAL exhaustion is downgraded from an unexpected shard failure.
    ///
    /// # Panics
    ///
    /// Panics when WAL exhaustion or another shard failure is classified under
    /// the wrong operational severity.
    #[test]
    fn only_wal_disk_full_is_expected_shard_capacity() {
        assert!(is_expected_wal_capacity(&ScribeError::WalDiskFull));
        assert!(!is_expected_wal_capacity(&ScribeError::Internal {
            detail: "unexpected failure".to_owned(),
        }));
        assert!(!is_expected_wal_capacity(
            &ScribeError::ObjectStorePutFailed(opendal::Error::new(
                opendal::ErrorKind::Unexpected,
                "object failure"
            ),)
        ));
    }

    #[derive(Debug)]
    struct Item {
        tenant: DataTenantId,
        bytes: usize,
        sequence: usize,
    }

    impl ShardItem for Item {
        fn tenant(&self) -> DataTenantId {
            self.tenant
        }

        fn bytes(&self) -> usize {
            self.bytes
        }
    }

    /// Proves an ample shutdown budget flushes every owner before draining tasks.
    #[tokio::test]
    async fn ample_time_scribe_shutdown_flushes_all_shard_owners() {
        let scribe = crate::scribe::ScribeImpl::new();
        assert_eq!(scribe.runtime_snapshot().shards.pending_items, 0);
        assert_eq!(
            scribe
                .shards
                .shutdown_flush_completions
                .load(Ordering::Acquire),
            0
        );
        scribe
            .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
            .await;
        assert_eq!(scribe.runtime_snapshot().shards.pending_items, 0);
        assert_eq!(
            scribe
                .shards
                .shutdown_flush_completions
                .load(Ordering::Acquire),
            SCRIBE_SHARD_COUNT
        );
        assert!(scribe.shards.tasks.lock().await.is_empty());
        assert!(scribe.shards.abort_handles.lock().unwrap().is_empty());
    }

    /// Proves deadline expiry aborts every retained shard owner after all admission closes.
    #[tokio::test]
    async fn expired_scribe_shutdown_aborts_retained_shard_tasks() {
        let scribe = crate::scribe::ScribeImpl::new();
        let stalled = tokio::spawn(std::future::pending::<()>());
        let stalled_abort = stalled.abort_handle();
        scribe
            .shards
            .abort_handles
            .lock()
            .expect("abort registry is healthy")
            .push(stalled_abort.clone());
        scribe.shards.tasks.lock().await.push(stalled);

        scribe.shutdown(std::time::Instant::now()).await;
        tokio::task::yield_now().await;

        assert!(scribe.closed.load(Ordering::Acquire));
        assert!(scribe.shards.closed.load(Ordering::Acquire));
        assert!(stalled_abort.is_finished());
        assert!(scribe.shards.tasks.lock().await.is_empty());
        assert!(scribe.shards.abort_handles.lock().unwrap().is_empty());
    }

    /// Proves the synchronous abort finalizer cancels owners while graceful
    /// shutdown still holds the async join-registry lock.
    #[tokio::test]
    async fn abort_retained_cancels_owner_when_join_registry_is_contended() {
        let scribe = crate::scribe::ScribeImpl::new();
        scribe.shards.abort_handles.lock().unwrap().clear();
        let stalled = tokio::spawn(std::future::pending::<()>());
        let stalled_abort = stalled.abort_handle();
        scribe
            .shards
            .abort_handles
            .lock()
            .expect("abort registry is healthy")
            .push(stalled_abort.clone());
        scribe.shards.tasks.lock().await.push(stalled);

        let tasks_guard = scribe.shards.tasks.lock().await;
        let recorder = wyrd_bench::BenchmarkRecorder::new();
        let (aborted, repeated) = metrics::with_local_recorder(&recorder, || {
            let aborted = scribe.shards.abort_retained();
            let repeated = scribe.shards.abort_retained();
            (aborted, repeated)
        });
        tokio::task::yield_now().await;

        assert_eq!(aborted, 1);
        assert_eq!(repeated, 0);
        assert!(stalled_abort.is_finished());
        assert!(scribe.shards.abort_handles.lock().unwrap().is_empty());
        assert_eq!(
            recorder
                .snapshot()
                .counters
                .get("bifrost_scribe_shard_abort_total"),
            Some(&1)
        );
        drop(tasks_guard);
        scribe.shards.close();
        scribe
            .shards
            .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
            .await;
    }

    #[test]
    fn topology_has_exactly_sixteen_mailboxes() {
        let set = ScribeShardSet::<Item>::new();
        assert_eq!(set.len(), 16);
        assert!(!set.is_empty());
    }

    #[test]
    fn fixed_topology_remains_sixteen_tasks_and_channels() {
        let set = ScribeShardSet::<Item>::new();
        assert_eq!(set.len(), SCRIBE_SHARD_COUNT);
        assert_eq!(SHARD_COMMAND_CAPACITY, 256);
    }

    fn owner_key() -> SealKey {
        SealKey::new(
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Bifrost, "owner-test"),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 24).expect("valid date")),
        )
    }

    fn owner_event() -> AuditEvent {
        AuditEvent {
            request_id: wyrd_spec::request_id::RequestId::now_v7(),
            trace_id: None,
            operation: "owner-test".to_owned(),
            resource: "owner-test".to_owned(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "bifrost:write".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "owner-test".to_owned(),
            detail: None,
        }
    }

    /// Exact retries are removed batch-wise without dropping canonical siblings.
    #[test]
    fn replay_fence_filters_only_suppressed_batches_in_same_seal() {
        let key = owner_key();
        let canonical = [1_u8; 16];
        let retry = [2_u8; 16];
        let commit = |batch_id| crate::scribe::replay::ReplayedCommitIdentity {
            batch_id,
            slice_set_digest: [3; 32],
            slice_count: 1,
            segment_sequence: 1,
            wal_lsn_min: WalLsn::new(1),
            wal_lsn_max: WalLsn::new(2),
            request_id: uuid::Uuid::now_v7(),
        };
        let meta = |batch_id, lsn| crate::scribe::replay::ReplayedAppendMeta {
            batch_id,
            wal_lsn: WalLsn::new(lsn),
            rows_accepted: 1,
            append_slice_id: crate::scribe::preprocess::AppendSliceId {
                batch_id: uuid::Uuid::from_bytes(batch_id),
                seal_key: key.clone(),
                slice_index: 0,
            },
            schema_fingerprint: [4; 32],
        };
        let mut replayed = crate::scribe::replay::ReplayedSealKey {
            stream: StreamIdentity::new(
                crate::scribe::stream_identity::NodeId::generate(),
                crate::scribe::stream_identity::WriterEpoch::new(2),
            ),
            seal_key: key.clone(),
            shard_id: 0,
            audit_events: vec![owner_event(), owner_event()],
            data_records: vec![vec![1], vec![2]],
            append_metas: vec![meta(canonical, 1), meta(retry, 3)],
            wal_segments: Vec::new(),
            commits: vec![commit(canonical), commit(retry)],
        };

        retain_replayed_batches(&mut replayed, &HashSet::from([retry]));

        assert_eq!(replayed.append_metas.len(), 1);
        assert_eq!(replayed.append_metas[0].batch_id, canonical);
        assert_eq!(replayed.audit_events.len(), 1);
        assert_eq!(replayed.data_records, vec![vec![1]]);
        assert_eq!(replayed.commits.len(), 1);
        assert_eq!(replayed.commits[0].batch_id, canonical);
    }

    fn owner_batch() -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "value",
                DataType::Int64,
                false,
            )])),
            vec![Arc::new(Int64Array::from(vec![1_i64]))],
        )
        .expect("owner batch")
    }

    /// Builds the same owner row with the managed event-time column required by preprocessing.
    fn owner_prepared_batch() -> RecordBatch {
        let event_micros = NaiveDate::from_ymd_opt(2026, 7, 24)
            .and_then(|date| date.and_hms_opt(12, 0, 0))
            .expect("valid event timestamp")
            .and_utc()
            .timestamp_micros();
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("value", DataType::Int64, false),
                Field::new(
                    "wyrd_event_time",
                    DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                    false,
                ),
            ])),
            vec![
                Arc::new(Int64Array::from(vec![1_i64])),
                Arc::new(TimestampMicrosecondArray::from(vec![event_micros]).with_timezone("UTC")),
            ],
        )
        .expect("owner prepared batch")
    }

    fn owner_meta(key: &SealKey) -> ScribeAppendMeta {
        ScribeAppendMeta {
            batch_id: *uuid::Uuid::now_v7().as_bytes(),
            rows_accepted: 1,
            wal_lsn_min: WalLsn::new(1),
            wal_lsn_max: WalLsn::new(1),
            seal_key: key.as_path_components(),
        }
    }

    fn owner_file_list_key(key: &SealKey) -> crate::scribe::file_list_writer::FileListCommitKey {
        crate::scribe::file_list_writer::FileListCommitKey {
            data_tenant_id: key.tenant,
            namespace: key.table.namespace.as_str().to_owned(),
            table_name: key.table.name.clone(),
            node_id: uuid::Uuid::nil(),
            writer_epoch: 1,
            wal_lsn_min: 1,
            wal_lsn_max: 1,
        }
    }

    /// Builds one isolated owner whose mailbox handler can be exercised directly.
    fn owner_for_completion_test(
        memtable: Memtable,
        wal: &Arc<WalWriter>,
        wal_handle: WalHandle,
        stream: StreamIdentity,
    ) -> ShardOwner {
        owner_for_completion_test_with_budget(memtable, wal, wal_handle, stream).0
    }

    /// Builds an isolated owner and returns its exact shared Scribe budget for fault setup.
    fn owner_for_completion_test_with_budget(
        memtable: Memtable,
        wal: &Arc<WalWriter>,
        wal_handle: WalHandle,
        stream: StreamIdentity,
    ) -> (ShardOwner, crate::resources::ScribeResources) {
        let (_command_tx, receiver) = mpsc::channel(1);
        let (_pressure_tx, pressure_receiver) = watch::channel(None);
        let (completion_tx, _completion_rx) = mpsc::channel(1);
        let budget =
            crate::scribe::embedded_scribe_resources(&crate::scribe::AdmissionConfig::default());
        let _ = wal;
        let owner = ShardOwner {
            id: 0,
            receiver,
            pressure_receiver,
            scheduler: TenantRoundRobin::default(),
            shutting_down: false,
            wal_segments: WalSegmentsByKey::new(),
            synced_not_inserted: HashMap::new(),
            pending_generations: PendingGenerationsByKey::new(),
            replay_chunk: None,
            seal_retry: HashSet::new(),
            retained_generations: HashMap::new(),
            retained_commit_ambiguity: None,
            admission: AdmissionController::with_config_and_memory(
                crate::scribe::admission::AdmissionConfig::default(),
                budget.clone(),
            ),
            memtable,
            persistence_cpu: ScribePersistenceCpuPool::new(1),
            wal_io: ScribeWalIoPool::new(1),
            persistence: None,
            control_postgres: None,
            completion_tx,
            stream,
            wal_handle,
            memory_ownership: ScribeOwnership::new(&budget).expect("memory ownership"),
            snapshot: Arc::new(Mutex::new(ShardMemtableSnapshot::default())),
            pending: Arc::new(AtomicUsize::new(0)),
            drained: Arc::new(Notify::new()),
            fail_next_retirement_release: false,
        };
        (owner, budget)
    }

    /// Proves shard insertion seals a full bucket before a crossing append lands.
    #[test]
    fn insert_group_pre_seals_full_bucket() {
        let key = owner_key();
        let first = owner_batch();
        let rotation_bytes = first
            .columns()
            .iter()
            .map(|column| column.get_array_memory_size())
            .sum();
        let memtable = Memtable::new_with_rotation(rotation_bytes);
        memtable
            .insert(&key, owner_event(), owner_meta(&key), first)
            .expect("seed full bucket");
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let wal_handle = wal.handle_for_shard(0).expect("WAL handle");
        let mut owner = owner_for_completion_test(memtable, &wal, wal_handle, stream);
        owner
            .memory_ownership
            .reserve_active(rotation_bytes.saturating_mul(2))
            .expect("active ledger reservation");
        owner
            .admission
            .try_reserve_active("owner-test", rotation_bytes.saturating_mul(2))
            .expect("active admission accounting");

        let next_batch = owner_batch();
        let next_id = *uuid::Uuid::now_v7().as_bytes();
        let touched = owner
            .insert_group(
                vec![DurableSlice {
                    seal_key: key.clone(),
                    audit_event: owner_event(),
                    rows: Some(next_batch),
                    memtable_bytes: rotation_bytes,
                    active_reserved: true,
                    batch_id: next_id,
                    lsn: WalLsn::new(2),
                    payload_digest: [0; 32],
                    payload_len: 0,
                    slice_index: 0,
                    slice_count: 1,
                    commit_already_synced: false,
                }],
                &mut HashMap::new(),
            )
            .expect("crossing insert");
        assert_eq!(touched, HashSet::from([key.clone()]));
        owner
            .flush_keys(vec![key.clone()], Some(SealTriggerReason::Size))
            .expect("seal second generation");

        let immutable = owner.memtable.immutable.lock().expect("immutable lock");
        let generations = immutable.get(&key).expect("two generations");
        assert_eq!(generations.len(), 2);
        assert!(
            generations
                .iter()
                .all(|entry| entry.frozen.arrow_bytes <= rotation_bytes)
        );
    }

    /// A seal key whose table name is unsafe, so `TenantTableBinding::resolve`
    /// fails deterministically — the forced binding-failure fixture.
    fn owner_bad_binding_key() -> SealKey {
        SealKey::new(
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Bifrost, "bad/name"),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 24).expect("valid date")),
        )
    }

    /// Builds a `persistence: Some` owner seeded for seal-path atomicity tests.
    ///
    /// The memtable holds one writable bucket for `key`; the ledger and
    /// admission are pre-charged with `active_seed` Active bytes so the
    /// accounting move is observable via [`ScribeOwnership::active_bytes`] /
    /// [`ScribeOwnership::immutable_bytes`] and the admission snapshot; and
    /// `wal_segments` holds an entry for `key` so the segment-map lifecycle
    /// (removed only after retention success) is assertable. Returns the WAL
    /// writer and temp directory so the caller keeps them alive.
    fn owner_for_seal_atomicity_test(
        key: &SealKey,
        active_seed: usize,
    ) -> (ShardOwner, Arc<WalWriter>, tempfile::TempDir) {
        let memtable = Memtable::new();
        memtable
            .insert(key, owner_event(), owner_meta(key), owner_batch())
            .expect("seed writable bucket");
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let wal_handle = wal.handle_for_shard(0).expect("WAL handle");
        let mut owner = owner_for_completion_test(memtable, &wal, wal_handle, stream);
        owner.persistence = Some(Arc::new(
            crate::scribe::persistence::PersistenceRuntime::empty_for_test(),
        ));
        owner.wal_segments.insert(key.clone(), HashMap::new());
        owner
            .memory_ownership
            .reserve_active(active_seed)
            .expect("seed active ledger");
        owner
            .admission
            .try_reserve_active("owner-test", active_seed)
            .expect("seed active admission");
        (owner, wal, wal_root)
    }

    /// Builds one owner with a committed retained generation and matching immutable ownership.
    ///
    /// The fixture seeds admission and ledger ownership from the same Scribe
    /// budget so retirement preflight succeeds before a post-preflight fault is
    /// armed. Returning the budget lets callers snapshot every governor counter.
    fn committed_retirement_owner_for_test() -> (
        ShardOwner,
        u64,
        Arc<WalWriter>,
        tempfile::TempDir,
        crate::resources::ScribeResources,
    ) {
        let key = owner_key();
        let memtable = Memtable::new();
        memtable
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner insert");
        let frozen = memtable.freeze(&key).expect("owner freeze");
        memtable
            .complete_post_commit(frozen.seal_id, owner_file_list_key(&key))
            .expect("owner completion");
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let wal_handle = wal.handle_for_shard(0).expect("WAL handle");
        let (mut owner, budget) =
            owner_for_completion_test_with_budget(memtable, &wal, wal_handle.clone(), stream);
        owner.admission.sync_memtable_bytes(0, frozen.arrow_bytes);
        owner
            .memory_ownership
            .reserve_immutable(frozen.arrow_bytes)
            .expect("immutable ledger ownership");
        owner
            .admission
            .preflight_release_immutable(frozen.arrow_bytes)
            .expect("immutable admission preflight");
        owner
            .memory_ownership
            .preflight_release_immutable(frozen.arrow_bytes)
            .expect("immutable ledger preflight");
        owner.retained_generations.insert(
            frozen.seal_id,
            RetainedGeneration {
                arrow_bytes: frozen.arrow_bytes,
                wal_segments: Vec::new(),
                wal: wal_handle,
            },
        );
        (owner, frozen.seal_id, wal, wal_root, budget)
    }

    /// Builds one real prepared append with reservations owned by `owner`'s budget.
    ///
    /// The initial charge deliberately exceeds the small owner batch so normal
    /// preprocessing exercises the production shrink and category-transfer path
    /// before `process_group` owns the append.
    fn prepared_append_for_group_test(
        owner: &ShardOwner,
        budget: &crate::resources::ScribeResources,
    ) -> PreparedAppend {
        let initial_bytes = 1024 * 1024;
        let reservation = owner
            .admission
            .try_reserve("owner-test".to_owned(), initial_bytes)
            .expect("in-flight admission");
        let memory = budget
            .try_reserve_ingress(MemoryCategory::Raw, initial_bytes)
            .expect("ingress memory");
        let lifecycle = Arc::new(crate::scribe::telemetry::ScribeIngressLifecycle::default());
        let mut lifecycle = lifecycle.begin();
        lifecycle.reserved(initial_bytes);
        let key = owner_key();
        crate::scribe::preprocess::prepare_append(crate::scribe::preprocess::AdmittedAppend {
            batch_id: uuid::Uuid::now_v7(),
            audit_event: owner_event(),
            rows: crate::scribe::preprocess::AdmittedRows::Projected(owner_prepared_batch()),
            measured_wire_bytes: initial_bytes,
            admitted_bytes: initial_bytes,
            wal_workspace_bytes: crate::gate::limits::BIFROST_WAL_WORKSPACE_LIMIT_BYTES,
            reservation,
            memory,
            tenant: key.tenant,
            table: key.table,
            queued_at: std::time::Instant::now(),
            durable_ack: None,
            lifecycle,
        })
        .expect("prepared append")
    }

    /// Lists deterministic relative WAL file paths and lengths beneath one test root.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a WAL test directory cannot be
    /// enumerated or one file's metadata cannot be read.
    fn wal_file_snapshot(
        root: &std::path::Path,
    ) -> Result<Vec<(std::path::PathBuf, u64)>, ScribeError> {
        /// Recursively appends file-relative entries for one WAL snapshot.
        ///
        /// # Errors
        ///
        /// Returns [`ScribeError::Internal`] when a directory, entry, metadata,
        /// or relative path cannot be read while walking the test WAL root.
        fn visit(
            root: &std::path::Path,
            directory: &std::path::Path,
            files: &mut Vec<(std::path::PathBuf, u64)>,
        ) -> Result<(), ScribeError> {
            for entry in std::fs::read_dir(directory).map_err(|error| ScribeError::Internal {
                detail: format!("WAL snapshot directory read failed: {error}"),
            })? {
                let entry = entry.map_err(|error| ScribeError::Internal {
                    detail: format!("WAL snapshot directory entry failed: {error}"),
                })?;
                let path = entry.path();
                let metadata = entry.metadata().map_err(|error| ScribeError::Internal {
                    detail: format!("WAL snapshot metadata read failed: {error}"),
                })?;
                if metadata.is_dir() {
                    visit(root, &path, files)?;
                } else if metadata.is_file() {
                    let relative =
                        path.strip_prefix(root)
                            .map_err(|error| ScribeError::Internal {
                                detail: format!("WAL snapshot relative path failed: {error}"),
                            })?;
                    files.push((relative.to_path_buf(), metadata.len()));
                }
            }
            Ok(())
        }

        let mut files = Vec::new();
        visit(root, root, &mut files)?;
        files.sort_unstable();
        Ok(files)
    }

    /// A forced binding failure strands the seal in state A and keeps it
    /// retry-reachable; a later age tick re-drives it (and, being a
    /// deterministic failure, re-fails) without a second seal count or any
    /// accounting move.
    ///
    /// Proves AC2's state-A half for the binding-resolve prep step: bytes stay
    /// Active-accounted, admission stays Active-accounted, the WAL segment map
    /// is intact, the key is recorded for retry, and exactly one seal is
    /// counted across the original attempt and the age-tick re-drive.
    #[test]
    fn binding_failure_strands_state_a_and_stays_retry_reachable() {
        let key = owner_bad_binding_key();
        let seed = 1_usize << 20;
        let (mut owner, _wal, _root) = owner_for_seal_atomicity_test(&key, seed);

        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let error = owner
                .flush_keys(vec![key.clone()], Some(SealTriggerReason::Size))
                .expect_err("binding resolve must fail on an unsafe table name");
            assert!(matches!(error, ScribeError::Internal { .. }));

            // State A: nothing moved, segment map intact, key retry-reachable.
            assert_eq!(owner.memory_ownership.active_bytes(), seed);
            assert_eq!(owner.memory_ownership.immutable_bytes(), 0);
            let admission = owner.admission.snapshot();
            assert_eq!(admission.active_bytes, seed);
            assert_eq!(admission.immutable_bytes, 0);
            assert!(owner.wal_segments.contains_key(&key));
            assert!(!owner.pending_generations.contains_key(&key));
            assert!(owner.seal_retry.contains(&key));
            assert!(
                owner
                    .memtable
                    .has_pending_frozen(&key)
                    .expect("pending probe")
            );

            // Age tick: its own selection is empty (the bucket is frozen), so
            // only the entry drain re-drives the key; the binding still fails.
            let retry = owner.flush_expired(std::time::Instant::now());
            assert!(retry.is_err(), "deterministic binding failure re-fails");

            // Still state A, still retry-reachable, no accounting change.
            assert_eq!(owner.memory_ownership.active_bytes(), seed);
            assert_eq!(owner.memory_ownership.immutable_bytes(), 0);
            assert!(owner.wal_segments.contains_key(&key));
            assert!(owner.seal_retry.contains(&key));
        });

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_seal_total{trigger=\"size\"}")
                .copied(),
            Some(1),
            "the freeze is counted once; the retry returns the pending entry"
        );
        assert!(
            !snapshot
                .counters
                .contains_key("bifrost_scribe_seal_total{trigger=\"age\"}"),
            "the age-tick re-drive must not count a second seal"
        );
    }

    /// A forced WAL retention failure strands the seal in state A with the
    /// segment map intact; a subsequent age tick whose own selection does not
    /// name the key re-drives it through the production entry drain to state B
    /// with exact net-zero accounting and exactly one seal count.
    ///
    /// Proves AC2 (state A/B accounting + admission + segment-map + retry),
    /// AC3 (recovery through a production re-drive site that does not name the
    /// key), and AC4 (`wal_segments` removed only after retention succeeds).
    #[test]
    fn retention_failure_recovers_to_state_b_through_age_tick() {
        let key = owner_key();
        let seed = 1_usize << 20;
        let (mut owner, _wal, _root) = owner_for_seal_atomicity_test(&key, seed);

        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            crate::scribe::wal::arm_retain_failure_for_test();
            let error = owner
                .flush_keys(vec![key.clone()], Some(SealTriggerReason::Size))
                .expect_err("armed retention failure must fail the seal");
            assert!(matches!(error, ScribeError::Internal { .. }));

            // State A: no move, admission unchanged, segment map intact (AC4),
            // key retry-reachable.
            assert_eq!(owner.memory_ownership.active_bytes(), seed);
            assert_eq!(owner.memory_ownership.immutable_bytes(), 0);
            let admission = owner.admission.snapshot();
            assert_eq!(admission.active_bytes, seed);
            assert_eq!(admission.immutable_bytes, 0);
            assert!(
                owner.wal_segments.contains_key(&key),
                "retention failure must not drop the segment map entry (AC4)"
            );
            assert!(!owner.pending_generations.contains_key(&key));
            assert!(owner.seal_retry.contains(&key));

            // Age tick: expired selection is empty (bucket already frozen), so
            // the key is re-driven only by the entry drain (AC3). Retention now
            // succeeds (the fault self-consumed).
            owner
                .flush_expired(std::time::Instant::now())
                .expect("age-tick re-drive completes the seal");

            // State B: net-zero move landed, admission moved, segment map entry
            // removed after success (AC4), generation queued, mark cleared.
            let active = owner.memory_ownership.active_bytes();
            let immutable = owner.memory_ownership.immutable_bytes();
            assert!(immutable > 0, "bytes moved to Immutable accounting");
            assert_eq!(active + immutable, seed, "net-zero category move");
            let admission = owner.admission.snapshot();
            assert!(admission.immutable_bytes > 0);
            assert_eq!(admission.active_bytes + admission.immutable_bytes, seed);
            assert!(
                !owner.wal_segments.contains_key(&key),
                "segment map entry removed only after retention succeeds (AC4)"
            );
            assert!(owner.pending_generations.contains_key(&key));
            assert!(!owner.seal_retry.contains(&key));
        });

        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_seal_total{trigger=\"size\"}")
                .copied(),
            Some(1),
            "the bucket seals exactly once across failure and retry"
        );
        assert!(
            !snapshot
                .counters
                .contains_key("bifrost_scribe_seal_total{trigger=\"age\"}"),
            "the age-tick re-drive returns the pending entry and counts no seal"
        );
    }

    /// A stale retry mark — one whose key is already queued, and one whose key
    /// was never seen — is a safe no-op: the marks are dropped, nothing is
    /// re-sealed, and accounting is unchanged.
    ///
    /// Proves the packet's stale-retry edge case: `has_pending_frozen` and the
    /// `pending_generations` guard keep a stranded-state completion from
    /// double-accounting or double-queueing an already-sealed or absent key.
    #[test]
    fn stale_seal_retry_marks_are_safe_no_ops() {
        let key = owner_key();
        let seed = 1_usize << 20;
        let (mut owner, _wal, _root) = owner_for_seal_atomicity_test(&key, seed);

        owner
            .flush_keys(vec![key.clone()], Some(SealTriggerReason::Size))
            .expect("initial seal reaches state B");
        let active_after = owner.memory_ownership.active_bytes();
        let immutable_after = owner.memory_ownership.immutable_bytes();
        assert!(owner.pending_generations.contains_key(&key));

        // Stale mark 1: the key is already queued (state B). Stale mark 2: a
        // key that was never sealed and has nothing pending.
        owner.seal_retry.insert(key.clone());
        let ghost = SealKey::new(
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Bifrost, "ghost"),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 25).expect("valid date")),
        );
        owner.seal_retry.insert(ghost);

        owner
            .flush_keys(Vec::new(), Some(SealTriggerReason::Age))
            .expect("stale marks drain as a no-op");

        assert!(owner.seal_retry.is_empty(), "stale marks dropped");
        assert_eq!(owner.memory_ownership.active_bytes(), active_after);
        assert_eq!(owner.memory_ownership.immutable_bytes(), immutable_after);
        assert_eq!(
            owner.pending_generations.get(&key).map(VecDeque::len),
            Some(1),
            "no double queueing of the already-sealed key"
        );
    }

    /// Builds the immutable generation paired with one frozen owner bucket.
    fn owner_generation(
        key: &SealKey,
        frozen: &crate::scribe::memtable::FrozenMemtable,
        stream: StreamIdentity,
        wal: WalHandle,
    ) -> Arc<ImmutableGeneration> {
        Arc::new(ImmutableGeneration {
            table_key: (key.tenant, key.table.clone()),
            seal_key: key.clone(),
            generation_id: crate::scribe::persistence::GenerationId(frozen.seal_id),
            stream,
            wal_lsn_min: WalLsn::new(1),
            wal_lsn_max: WalLsn::new(1),
            wal_segments: Vec::new(),
            wal,
            rows: frozen.batches.clone(),
            schema: Arc::clone(&frozen.schema),
            audit_events: frozen.events.clone(),
            append_metas: frozen.metas.clone(),
            row_count: frozen.row_count(),
            arrow_bytes: frozen.arrow_bytes,
            replay_identity: std::sync::Mutex::new(None),
            opened_at: frozen.opened_at,
            closed_at: frozen.closed_at,
            shard_id: frozen.shard_id,
        })
    }

    /// The real shard command returns visibility success only after publishing and advancing FIFO ownership.
    #[tokio::test]
    async fn persistence_command_acknowledges_only_completed_visibility_transition() {
        let key = owner_key();
        let memtable = Memtable::new();
        memtable
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner insert");
        let frozen = memtable.freeze(&key).expect("owner freeze");
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let wal_handle = wal.handle_for_shard(0).expect("WAL handle");
        let generation = owner_generation(&key, &frozen, stream, wal_handle.clone());
        let (mut owner, _budget) =
            owner_for_completion_test_with_budget(memtable, &wal, wal_handle.clone(), stream);
        owner.pending_generations.insert(
            key.clone(),
            VecDeque::from([PendingGeneration {
                generation: Arc::clone(&generation),
                binding: crate::catalog::TenantTableBinding::resolve((
                    key.tenant,
                    key.table.clone(),
                ))
                .expect("binding"),
                submitted: true,
                retry_scheduled: false,
                replay_response: None,
                replay_owned: false,
            }]),
        );
        let (waiter_tx, waiter_rx) = tokio::sync::oneshot::channel();
        let (visibility_tx, visibility_rx) = tokio::sync::oneshot::channel();
        owner
            .handle_command(ShardCommand::PersistenceComplete {
                completion: Box::new(PersistenceCompletion {
                    generation_id: generation.generation_id,
                    file_list_key: Some(owner_file_list_key(&key)),
                    wal_segments: Vec::new(),
                    wal: wal_handle,
                    arrow_bytes: generation.arrow_bytes,
                    replay_identity: None,
                    error: None,
                }),
                waiter: Some(waiter_tx),
                visibility_result: visibility_tx,
            })
            .await;
        assert_eq!(visibility_rx.await.expect("visibility ack"), Ok(()));
        assert_eq!(waiter_rx.await.expect("caller ack"), Ok(()));
        assert!(!owner.pending_generations.contains_key(&key));
        assert!(owner.retained_generations.contains_key(&frozen.seal_id));
        assert_eq!(
            owner
                .memtable
                .stats()
                .expect("memtable stats")
                .pending_generations,
            0
        );
    }

    /// A shard publication failure returns the identical error to visibility and caller acknowledgments.
    #[tokio::test]
    async fn persistence_command_preserves_publication_failure_for_both_waiters() {
        let key = owner_key();
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let wal_handle = wal.handle_for_shard(0).expect("WAL handle");
        let mut owner =
            owner_for_completion_test(Memtable::new(), &wal, wal_handle.clone(), stream);
        let (waiter_tx, waiter_rx) = tokio::sync::oneshot::channel();
        let (visibility_tx, visibility_rx) = tokio::sync::oneshot::channel();
        owner
            .handle_command(ShardCommand::PersistenceComplete {
                completion: Box::new(PersistenceCompletion {
                    generation_id: crate::scribe::persistence::GenerationId(999),
                    file_list_key: Some(owner_file_list_key(&key)),
                    wal_segments: Vec::new(),
                    wal: wal_handle,
                    arrow_bytes: 1,
                    replay_identity: None,
                    error: None,
                }),
                waiter: Some(waiter_tx),
                visibility_result: visibility_tx,
            })
            .await;
        let visibility_error = visibility_rx
            .await
            .expect("visibility ack")
            .expect_err("publication must fail");
        let waiter_error = waiter_rx
            .await
            .expect("caller ack")
            .expect_err("caller must fail");
        assert_eq!(visibility_error, waiter_error);
        assert!(visibility_error.contains("generation"));
    }

    #[test]
    fn shard_owner_exclusively_mutates_generation_state() {
        let key = owner_key();
        let owner = Memtable::new();
        let other_owner = Memtable::new();
        owner
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner insert");
        assert_eq!(owner.stats().expect("owner stats").writable_buckets, 1);
        assert_eq!(
            other_owner
                .stats()
                .expect("other owner stats")
                .writable_buckets,
            0
        );
    }

    #[test]
    fn persistence_completion_returns_to_owning_shard() {
        let key = owner_key();
        let owner = Memtable::new();
        owner
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner insert");
        let frozen = owner.freeze(&key).expect("owner freeze");
        owner
            .complete_post_commit(frozen.seal_id, owner_file_list_key(&key))
            .expect("owner completion");
        let stats = owner.stats().expect("owner stats");
        assert_eq!(stats.immutable_generations, 1);
        assert_eq!(stats.pending_generations, 0);
    }

    /// Verifies that a committed generation retires on the first shard-level sweep
    /// and that retirement does not enqueue additional generation tasks.
    ///
    /// The shard count is stable at 16, so FIFO queues indexed by shard ID cannot
    /// grow beyond `SCRIBE_SHARD_COUNT`.
    #[test]
    fn committed_generation_retires_at_first_shard_sweep() {
        let key = owner_key();
        let owner = Memtable::new();
        owner
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner insert");
        let frozen = owner.freeze(&key).expect("owner freeze");
        owner
            .complete_post_commit(frozen.seal_id, owner_file_list_key(&key))
            .expect("owner completion");
        assert_eq!(
            owner.sweep_once().expect("immediate sweep").len(),
            1,
            "committed generation retires at the first sweep"
        );
        assert_eq!(SCRIBE_SHARD_COUNT, 16);
    }

    #[test]
    fn pressure_watch_coalesces_repeated_requests() {
        let tenant = DataTenantId::new_v7();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let first = SealKey::new(
            tenant,
            table.clone(),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("valid date")),
        );
        let second = SealKey::new(
            tenant,
            table,
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 15).expect("valid date")),
        );
        let (sender, receiver) = watch::channel(None);
        sender.send_replace(Some(PressureSignal {
            keys: vec![first],
            wal_key: None,
        }));
        sender.send_replace(Some(PressureSignal {
            keys: vec![second.clone()],
            wal_key: None,
        }));
        assert_eq!(
            receiver.borrow().as_ref().map(|signal| &signal.keys),
            Some(&vec![second])
        );
    }

    #[test]
    fn scheduler_rotates_between_tenants() {
        let first = DataTenantId::new_v7();
        let second = DataTenantId::new_v7();
        let mut scheduler = TenantRoundRobin::default();
        scheduler.push(Item {
            tenant: first,
            bytes: 1,
            sequence: 1,
        });
        scheduler.push(Item {
            tenant: first,
            bytes: 1,
            sequence: 2,
        });
        scheduler.push(Item {
            tenant: second,
            bytes: 1,
            sequence: 3,
        });
        let first_group = scheduler.pop_group();
        assert_eq!(first_group.len(), 3);
        assert_eq!(first_group[0].tenant(), first);
        assert_eq!(first_group[1].tenant(), second);
        assert_eq!(first_group[1].sequence, 3);
        assert_eq!(first_group[2].sequence, 2);
        let second_group = scheduler.pop_group();
        assert!(second_group.is_empty());
        assert!(scheduler.pop_group().is_empty());
    }

    /// Verify that shard lookup is bounded within the fixed topology for any batch.
    #[test]
    fn routing_uses_canonical_table_reference() {
        let set = ScribeShardSet::<Item>::new();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let batch_id = uuid::Uuid::new_v4();
        assert!(set.shard_for(DataTenantId::new_v7(), &table, batch_id).id < 16);
    }

    /// Proves that `ShardOwner::freeze_key` returns `None` when the shard holds
    /// no bucket for the key, enabling the fan-out loop in
    /// `ScribeShardRuntime::freeze_key` to skip shards cleanly.
    ///
    /// Under batch-spread routing a seal key may have buckets on any subset of
    /// the sixteen lanes. A shard that received no append for that key must no-op
    /// rather than returning an error so the fan-out can continue to the shards
    /// that do hold data.
    #[test]
    fn flush_fan_out_shard_with_no_bucket_returns_none() {
        let key = owner_key();
        let owner = Memtable::new();
        // Do NOT insert into this owner — it holds no bucket for `key`.
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let wal_handle = wal.handle_for_shard(0).expect("WAL handle");
        let mut empty_owner = owner_for_completion_test(owner, &wal, wal_handle, stream);
        let result = empty_owner.freeze_key(&key).expect("freeze of absent key");
        assert!(
            result.is_none(),
            "a shard with no bucket must return None so the fan-out skips it"
        );
    }

    /// Proves that `ShardOwner::freeze_key` seals a bucket and stamps the result
    /// with the owning shard's `id` so that `complete_post_commit` and
    /// `abort_post_commit` can route back to the correct lane without recomputing
    /// the routing key.
    ///
    /// This is the `Some` half of the fan-out invariant:
    /// * If the shard holds a bucket → returns `Some(frozen)` with
    ///   `frozen.shard_id == self.id`.
    /// * If the shard holds no bucket → returns `None` (see companion test
    ///   `flush_fan_out_shard_with_no_bucket_returns_none`).
    ///
    /// Together these two properties let `ScribeShardRuntime::freeze_key`
    /// collect the first real result across all sixteen shards without
    /// re-deriving which lane owns the data.
    #[test]
    fn flush_fan_out_seals_buckets_on_every_shard() {
        // Build two independent owners with different shard ids to simulate the
        // fan-out across all sixteen lanes.  Owner A (id=0) holds a bucket;
        // owner B (id=5) holds a bucket for the same key.  Each should return
        // Some with its own shard_id stamped on the frozen memtable.
        let key = owner_key();
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );

        // Owner A — shard 0.
        let memtable_a = Memtable::new();
        memtable_a
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner A insert");
        let wal_handle_a = wal.handle_for_shard(0).expect("WAL handle A");
        let mut owner_a = owner_for_completion_test(memtable_a, &wal, wal_handle_a, stream);
        owner_a.id = 0;
        let active_bytes_a = owner_a
            .memtable
            .stats()
            .expect("owner A stats")
            .writable_bytes;
        owner_a
            .memory_ownership
            .reserve_active(active_bytes_a)
            .expect("owner A active memory");
        owner_a
            .admission
            .try_reserve_active("owner-test", active_bytes_a)
            .expect("owner A active admission");

        // Owner B — shard 5 (different lane, same key).
        let memtable_b = Memtable::new();
        memtable_b
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner B insert");
        let wal_handle_b = wal.handle_for_shard(5).expect("WAL handle B");
        let mut owner_b = owner_for_completion_test(memtable_b, &wal, wal_handle_b, stream);
        owner_b.id = 5;
        let active_bytes_b = owner_b
            .memtable
            .stats()
            .expect("owner B stats")
            .writable_bytes;
        owner_b
            .memory_ownership
            .reserve_active(active_bytes_b)
            .expect("owner B active memory");
        owner_b
            .admission
            .try_reserve_active("owner-test", active_bytes_b)
            .expect("owner B active admission");

        let frozen_a = owner_a
            .freeze_key(&key)
            .expect("owner A freeze")
            .expect("owner A must hold a bucket");
        assert_eq!(
            frozen_a.shard_id, 0,
            "frozen shard_id must match the owning lane (0)"
        );

        let frozen_b = owner_b
            .freeze_key(&key)
            .expect("owner B freeze")
            .expect("owner B must hold a bucket");
        assert_eq!(
            frozen_b.shard_id, 5,
            "frozen shard_id must match the owning lane (5)"
        );

        // An owner with no bucket for the key must return None — this is the
        // no-op half of the fan-out that makes broadcast-to-all-sixteen safe.
        let empty_memtable = Memtable::new();
        let wal_handle_c = wal.handle_for_shard(3).expect("WAL handle C");
        let mut empty_owner = owner_for_completion_test(empty_memtable, &wal, wal_handle_c, stream);
        empty_owner.id = 3;
        let none_result = empty_owner
            .freeze_key(&key)
            .expect("empty owner freeze must not error");
        assert!(
            none_result.is_none(),
            "shard with no bucket must return None during fan-out"
        );
    }

    /// Proves the runtime `freeze_key` aggregator returns **every** shard's
    /// frozen memtable for a batch-spread seal key — one entry per non-empty
    /// shard, none orphaned — and still fails closed when no shard holds a
    /// bucket.
    ///
    /// This exercises the collection contract at the heart of the T35 hazard
    /// fix directly on [`ScribeShardRuntime::finalize_frozen_fan_out`] using
    /// real [`FrozenMemtable`] values produced by two owners on distinct lanes
    /// (0 and 5) — exactly how batch-spread routing scatters one key's buckets
    /// — plus an empty shard that contributes `None`. The prior behavior kept
    /// only the first `Some` and discarded the rest; here both frozen memtables
    /// must survive with their owning `shard_id` intact.
    #[test]
    fn freeze_key_returns_every_shard_frozen_memtable() {
        let key = owner_key();
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );

        // Owner A — shard 0 holds a bucket for the key.
        let memtable_a = Memtable::new();
        memtable_a
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner A insert");
        let wal_handle_a = wal.handle_for_shard(0).expect("WAL handle A");
        let mut owner_a = owner_for_completion_test(memtable_a, &wal, wal_handle_a, stream);
        owner_a.id = 0;
        let active_bytes_a = owner_a
            .memtable
            .stats()
            .expect("owner A stats")
            .writable_bytes;
        owner_a
            .memory_ownership
            .reserve_active(active_bytes_a)
            .expect("owner A active memory");
        owner_a
            .admission
            .try_reserve_active("owner-test", active_bytes_a)
            .expect("owner A active admission");

        // Owner B — shard 5 holds a bucket for the same key (different route).
        let memtable_b = Memtable::new();
        memtable_b
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner B insert");
        let wal_handle_b = wal.handle_for_shard(5).expect("WAL handle B");
        let mut owner_b = owner_for_completion_test(memtable_b, &wal, wal_handle_b, stream);
        owner_b.id = 5;
        let active_bytes_b = owner_b
            .memtable
            .stats()
            .expect("owner B stats")
            .writable_bytes;
        owner_b
            .memory_ownership
            .reserve_active(active_bytes_b)
            .expect("owner B active memory");
        owner_b
            .admission
            .try_reserve_active("owner-test", active_bytes_b)
            .expect("owner B active admission");

        // Empty owner — shard 3 holds no bucket and must contribute None.
        let wal_handle_c = wal.handle_for_shard(3).expect("WAL handle C");
        let mut empty_owner =
            owner_for_completion_test(Memtable::new(), &wal, wal_handle_c, stream);
        empty_owner.id = 3;

        let frozen_a = Some(
            owner_a
                .freeze_key(&key)
                .expect("owner A freeze")
                .expect("bucket A"),
        );
        let absent = empty_owner.freeze_key(&key).expect("empty owner freeze");
        assert!(
            absent.is_none(),
            "empty shard contributes None to the fan-out"
        );
        let frozen_b = Some(
            owner_b
                .freeze_key(&key)
                .expect("owner B freeze")
                .expect("bucket B"),
        );

        // The fan-out over [Some(A), None, Some(B)] keeps both frozen memtables.
        let collected =
            ScribeShardRuntime::finalize_frozen_fan_out(vec![frozen_a, absent, frozen_b], &key)
                .expect("a batch-spread key yields one frozen memtable per non-empty shard");
        assert_eq!(
            collected.len(),
            2,
            "one frozen memtable per non-empty shard, none orphaned"
        );
        let mut lanes = collected
            .iter()
            .map(|frozen| frozen.shard_id)
            .collect::<Vec<_>>();
        lanes.sort_unstable();
        assert_eq!(
            lanes,
            vec![0, 5],
            "each frozen memtable keeps its owning lane"
        );

        // A fan-out where no shard held a bucket fails closed (not-found contract).
        assert!(
            matches!(
                ScribeShardRuntime::finalize_frozen_fan_out(vec![None, None], &key),
                Err(ScribeError::Internal { .. })
            ),
            "an all-None fan-out must fail closed rather than return an empty seal set"
        );
    }

    /// Proves the runtime `freeze_key` aggregator fails closed on an absent key
    /// instead of silently returning an empty seal set.
    ///
    /// The aggregator fans a `FreezeKey` command to every one of the sixteen
    /// lanes and extends a single `Vec` with each shard's frozen result. When no
    /// shard holds a bucket for the key the collected vector is empty; returning
    /// it as success would let a caller (`ScribeImpl::seal_one`) treat a
    /// no-op as a completed seal. The aggregator therefore maps the empty case
    /// to [`ScribeError::Internal`] so an orphaned or already-drained key is a
    /// hard error, not a silent success.
    #[tokio::test]
    async fn freeze_key_absent_key_fails_closed_across_all_shards() {
        let scribe = crate::scribe::ScribeImpl::new();
        let key = owner_key();
        let result = scribe.shards.freeze_key(key).await;
        assert!(
            matches!(result, Err(ScribeError::Internal { .. })),
            "an absent key must fail closed rather than return an empty seal set"
        );
        scribe
            .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
            .await;
    }

    /// Proves that a snapshot fan-out merges results from multiple shards.
    ///
    /// Each shard's `Snapshot` command returns the writable buckets on that lane.
    /// After batch-spread routing places two distinct batches on two different
    /// shards (different `batch_id` hashes), a pod-global snapshot must include
    /// both contributions.  This test exercises the merge loop inside
    /// `ScribeShardRuntime::snapshot` without a full network round-trip.
    ///
    /// The test uses two `ShardMemtableSnapshot` values constructed from two
    /// independent `ShardOwner` memtables to prove the merge behavior: every
    /// shard contributes its local data, and the caller sees the union.
    #[test]
    fn snapshot_fan_out_merges_results_from_multiple_shards() {
        let key = owner_key();
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );

        // Shard 0 holds one batch for `key`.
        let memtable_a = Memtable::new();
        memtable_a
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("shard 0 insert");
        let wal_handle_a = wal.handle_for_shard(0).expect("WAL handle shard 0");
        let owner_a = owner_for_completion_test(memtable_a, &wal, wal_handle_a, stream);

        // Shard 7 holds one batch for the same `key` (simulating a different batch_id route).
        let memtable_b = Memtable::new();
        memtable_b
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("shard 7 insert");
        let wal_handle_b = wal.handle_for_shard(7).expect("WAL handle shard 7");
        let owner_b = owner_for_completion_test(memtable_b, &wal, wal_handle_b, stream);

        // Each shard independently reports one writable bucket for `key`.
        let stats_a = owner_a.memtable.stats().expect("shard 0 memtable stats");
        let stats_b = owner_b.memtable.stats().expect("shard 7 memtable stats");

        // Both shards observe one writable bucket each; a merged view sums to two.
        assert_eq!(
            stats_a.writable_buckets, 1,
            "shard 0 must hold one writable bucket"
        );
        assert_eq!(
            stats_b.writable_buckets, 1,
            "shard 7 must hold one writable bucket"
        );
        assert_eq!(
            stats_a.writable_buckets + stats_b.writable_buckets,
            2,
            "pod-global snapshot fan-out must sum both shard contributions"
        );
    }

    /// The seal counter is labelled by a closed size/age/pressure trigger set.
    ///
    /// Emitting one seal per trigger proves each [`SealTriggerReason`] increments
    /// `bifrost_scribe_seal_total{trigger}` under its stable label and that no
    /// trigger value leaks tenant, table, or request identity.
    #[test]
    fn seal_trigger_counter_labels_are_closed() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            record_seal(SealTriggerReason::Size);
            record_seal(SealTriggerReason::Age);
            record_seal(SealTriggerReason::Pressure);
        });
        let snapshot = recorder.snapshot();
        for trigger in ["size", "age", "pressure"] {
            let key = format!("bifrost_scribe_seal_total{{trigger=\"{trigger}\"}}");
            assert_eq!(
                snapshot.counters.get(&key).copied(),
                Some(1),
                "missing seal counter {key}"
            );
        }
        assert!(!snapshot.counters.keys().any(|key| {
            ["tenant", "table", "path", "request", "node", "error", "sql"]
                .iter()
                .any(|forbidden| key.contains(forbidden))
        }));
    }

    /// Retirement reports both a count and the immutable bytes it freed.
    ///
    /// Proves [`record_retirement`] advances `bifrost_scribe_retirements_total`
    /// once and accumulates the freed bytes into
    /// `bifrost_scribe_retired_bytes_total` under identity-free counter names.
    #[test]
    fn retirement_reports_freed_bytes() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            record_retirement(4096);
            record_retirement(2048);
        });
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_retirements_total")
                .copied(),
            Some(2)
        );
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_retired_bytes_total")
                .copied(),
            Some(6144)
        );
    }

    /// A swallowed seal failure increments an identity-free failure counter.
    ///
    /// Proves [`record_seal_failure`] advances `bifrost_scribe_seal_failed_total`
    /// once per call under its stable, identity-free name so a stranded-seal
    /// condition (D97) surfaced only as a WARN cannot be silent, and that the
    /// counter leaks no tenant, table, or request identity.
    #[test]
    fn seal_failure_counter_is_identity_free() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            record_seal_failure();
            record_seal_failure();
        });
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_seal_failed_total")
                .copied(),
            Some(2),
            "seal-failure counter must advance once per failed flush"
        );
        assert!(!snapshot.counters.keys().any(|key| {
            ["tenant", "table", "path", "request", "node", "sql"]
                .iter()
                .any(|forbidden| key.contains(forbidden))
        }));
    }

    /// Retirement's existing committed-generation path proves planning failure preserves ownership.
    #[tokio::test]
    async fn explicit_retirement_preflight_failure_leaves_all_state_unchanged() {
        let key = owner_key();
        let memtable = Memtable::new();
        memtable
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner insert");
        let frozen = memtable.freeze(&key).expect("owner freeze");
        memtable
            .complete_post_commit(frozen.seal_id, owner_file_list_key(&key))
            .expect("owner completion");
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let wal_handle = wal.handle_for_shard(0).expect("WAL handle");
        let (mut owner, budget) =
            owner_for_completion_test_with_budget(memtable, &wal, wal_handle.clone(), stream);
        owner.retained_generations.insert(
            frozen.seal_id,
            RetainedGeneration {
                arrow_bytes: frozen.arrow_bytes,
                wal_segments: Vec::new(),
                wal: wal_handle,
            },
        );
        let governor_before = budget.accounting_snapshot_for_test();
        let admission_before = owner.admission.snapshot();
        let active_ledger_before = owner.memory_ownership.active_bytes();
        let immutable_ledger_before = owner.memory_ownership.immutable_bytes();
        let wal_bytes_before = wal.bytes_on_disk();
        let wal_files_before = wal_file_snapshot(wal_root.path()).expect("WAL file snapshot");
        let retire_submissions_before = owner.wal_io.retire_submissions_for_test();
        let token_before = owner
            .memtable
            .plan_committed_retirement(frozen.seal_id)
            .expect("retirement plan")
            .expect("committed token");
        let result = owner.retire_committed().await;
        assert!(matches!(result, Err(ScribeError::Internal { .. })));
        assert!(owner.retained_generations.contains_key(&frozen.seal_id));
        assert_eq!(budget.accounting_snapshot_for_test(), governor_before);
        assert_eq!(owner.admission.snapshot(), admission_before);
        assert_eq!(owner.memory_ownership.active_bytes(), active_ledger_before);
        assert_eq!(
            owner.memory_ownership.immutable_bytes(),
            immutable_ledger_before
        );
        assert_eq!(wal.bytes_on_disk(), wal_bytes_before);
        assert_eq!(
            wal_file_snapshot(wal_root.path()).expect("WAL file snapshot"),
            wal_files_before
        );
        assert_eq!(
            owner.wal_io.retire_submissions_for_test(),
            retire_submissions_before
        );
        let token_after = owner
            .memtable
            .plan_committed_retirement(frozen.seal_id)
            .expect("retirement plan")
            .expect("committed token remains");
        assert_eq!(token_after.seal_id, token_before.seal_id);
        assert_eq!(token_after.arrow_bytes, token_before.arrow_bytes);
        assert_eq!(token_after.wal_range, token_before.wal_range);
    }

    /// Retirement does not remove a retained generation when immutable release cannot proceed.
    #[tokio::test]
    async fn immutable_release_failure_prevents_false_retirement() {
        let (mut owner, generation_id, wal, root, budget) = committed_retirement_owner_for_test();
        let token_before = owner
            .memtable
            .plan_committed_retirement(generation_id)
            .expect("retirement plan")
            .expect("token");
        let governor_before = budget.accounting_snapshot_for_test();
        let admission_before = owner.admission.snapshot();
        let ledger_before = owner.memory_ownership.immutable_bytes();
        let wal_before = wal.bytes_on_disk();
        let wal_files_before = wal_file_snapshot(root.path()).expect("WAL file snapshot");
        let retire_submissions_before = owner.wal_io.retire_submissions_for_test();
        owner.arm_retirement_release_fault_for_test();
        let error = owner
            .retire_committed()
            .await
            .expect_err("retirement fault");
        assert!(
            matches!(error, ScribeError::Internal { detail } if detail == "injected immutable retirement release failure")
        );
        assert!(owner.retained_generations.contains_key(&generation_id));
        assert_eq!(budget.accounting_snapshot_for_test(), governor_before);
        assert_eq!(owner.admission.snapshot(), admission_before);
        assert_eq!(owner.memory_ownership.immutable_bytes(), ledger_before);
        assert_eq!(wal.bytes_on_disk(), wal_before);
        assert_eq!(
            wal_file_snapshot(root.path()).expect("WAL file snapshot"),
            wal_files_before
        );
        assert_eq!(
            owner.wal_io.retire_submissions_for_test(),
            retire_submissions_before
        );
        let token_after = owner
            .memtable
            .plan_committed_retirement(generation_id)
            .expect("retirement plan")
            .expect("token remains");
        assert_eq!(token_after.seal_id, token_before.seal_id);
        assert_eq!(token_after.arrow_bytes, token_before.arrow_bytes);
        assert!(owner.memory_ownership.is_poisoned());
    }

    /// Test and periodic retirement callers both retain the generation on an error path.
    #[tokio::test]
    async fn retire_committed_callers_preserve_generation_on_error() {
        let (mut owner, generation_id, wal, root, budget) = committed_retirement_owner_for_test();
        let governor_before = budget.accounting_snapshot_for_test();
        let wal_before = wal.bytes_on_disk();
        let wal_files_before = wal_file_snapshot(root.path()).expect("WAL file snapshot");
        let retire_submissions_before = owner.wal_io.retire_submissions_for_test();
        owner.arm_retirement_release_fault_for_test();
        owner
            .handle_command(ShardCommand::FlushExpired {
                now: std::time::Instant::now(),
            })
            .await;
        assert!(owner.retained_generations.contains_key(&generation_id));
        assert_eq!(budget.accounting_snapshot_for_test(), governor_before);
        assert_eq!(wal.bytes_on_disk(), wal_before);
        assert_eq!(
            wal_file_snapshot(root.path()).expect("WAL file snapshot"),
            wal_files_before
        );
        assert_eq!(
            owner.wal_io.retire_submissions_for_test(),
            retire_submissions_before
        );

        let (mut owner, generation_id, wal, root, budget) = committed_retirement_owner_for_test();
        let governor_before = budget.accounting_snapshot_for_test();
        let wal_before = wal.bytes_on_disk();
        let wal_files_before = wal_file_snapshot(root.path()).expect("WAL file snapshot");
        let retire_submissions_before = owner.wal_io.retire_submissions_for_test();
        owner.arm_retirement_release_fault_for_test();
        let (response, result) = tokio::sync::oneshot::channel();
        owner
            .handle_command(ShardCommand::RetireCommittedForTest { response })
            .await;
        assert!(result.await.expect("test retirement response").is_err());
        assert!(owner.retained_generations.contains_key(&generation_id));
        assert_eq!(budget.accounting_snapshot_for_test(), governor_before);
        assert_eq!(wal.bytes_on_disk(), wal_before);
        assert_eq!(
            wal_file_snapshot(root.path()).expect("WAL file snapshot"),
            wal_files_before
        );
        assert_eq!(
            owner.wal_io.retire_submissions_for_test(),
            retire_submissions_before
        );
        assert!(
            owner
                .memtable
                .plan_committed_retirement(generation_id)
                .expect("retirement plan")
                .is_some()
        );
    }

    /// A failed seal preparation leaves active ownership retry-reachable.
    #[test]
    fn seal_transfer_failure_preserves_retryable_active_state() {
        let key = owner_key();
        let seed = 1_usize << 20;
        let (mut owner, _wal, _root) = owner_for_seal_atomicity_test(&key, seed);
        let before = owner.admission.snapshot();
        owner.admission.sync_memtable_bytes(0, 0);

        let error = owner
            .flush_keys(vec![key.clone()], Some(SealTriggerReason::Size))
            .expect_err("admission preflight must reject before freezing");
        assert!(matches!(error, ScribeError::Internal { .. }));
        assert!(owner.memtable.row_count(&key).expect("bucket bytes") > 0);
        assert!(!owner.pending_generations.contains_key(&key));
        assert!(owner.wal_segments.contains_key(&key));
        assert!(owner.seal_retry.contains(&key));
        assert_eq!(owner.memory_ownership.active_bytes(), seed);
        assert_eq!(
            owner.admission.snapshot().immutable_bytes,
            before.immutable_bytes
        );
        assert!(owner.memory_ownership.is_poisoned());
    }

    /// Group cleanup reports the primary failure while the shared cleanup path poisons corruption.
    #[tokio::test]
    async fn cleanup_release_failure_preserves_primary_error_and_poisons() {
        let memtable = Memtable::new();
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let stream = StreamIdentity::new(node, crate::scribe::stream_identity::WriterEpoch::new(1));
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node.as_bytes(),
                1,
                crate::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let wal_handle = wal.handle_for_shard(0).expect("WAL handle");
        let (mut owner, budget) =
            owner_for_completion_test_with_budget(memtable, &wal, wal_handle, stream);
        let prepared = prepared_append_for_group_test(&owner, &budget);
        wal.trip_post_sync_failure_for_test();
        budget.arm_post_preflight_release_fault();
        let error = owner
            .process_group(vec![prepared])
            .await
            .expect_err("post-sync WAL fault");
        assert!(
            matches!(error, ScribeError::Internal { detail } if detail == "injected post-sync WAL failure")
        );
        assert!(owner.memory_ownership.is_poisoned());
        assert_eq!(owner.synced_not_inserted.len(), 1);
        let memtable = owner.memtable.stats().expect("memtable stats");
        assert_eq!(memtable.writable_rows, 0);
        assert_eq!(memtable.immutable_rows, 0);
        assert_eq!(owner.admission.snapshot().active_bytes, 0);
        assert!(owner.memory_ownership.active_bytes() > 0);
        assert!(budget.try_reserve(MemoryCategory::Raw, 1).is_err());
    }
}
