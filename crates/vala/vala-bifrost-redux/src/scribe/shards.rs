//! Fixed pod-local Scribe shard mailboxes and tenant round-robin scheduling.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

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
use crate::scribe::memory::{MemoryCategory, MemoryLedger};
use crate::scribe::memtable::{BucketMemorySnapshot, Memtable, MemtableStats, PressureCandidate};
use crate::scribe::persistence::{
    ImmutableGeneration, PersistenceCompletion, PersistenceJob, PersistenceRuntime,
};
use crate::scribe::preprocess::{AppendSliceId, PreparedAppend, PreparedSlice};
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
}

type PendingGenerationsByKey =
    HashMap<crate::scribe::seal_key::SealKey, VecDeque<PendingGeneration>>;

#[derive(Debug)]
struct RetainedGeneration {
    arrow_bytes: usize,
    wal_segments: Vec<crate::scribe::wal::WalSegmentRef>,
    wal: crate::scribe::wal::WalHandle,
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

    /// Return the canonical shard for an authenticated table.
    #[must_use]
    pub fn shard_for(&self, tenant: DataTenantId, table: &TableRef) -> &ScribeShard<T> {
        &self.shards[shard_for(tenant, table)]
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
    #[cfg(any(test, feature = "test-support"))]
    RetireCommittedForTest {
        /// Monotonic instant used for retention decisions.
        now: std::time::Instant,
        /// Completes after the owner handles the retirement pass.
        response: tokio::sync::oneshot::Sender<Result<(), ScribeError>>,
    },
    PersistenceComplete {
        completion: Box<PersistenceCompletion>,
        waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    },
    Replay {
        state: Box<crate::scribe::replay::ReplayedSealKey>,
        response: tokio::sync::oneshot::Sender<Result<(), ScribeError>>,
    },
    FreezeKey {
        seal_key: crate::scribe::seal_key::SealKey,
        response: tokio::sync::oneshot::Sender<
            Result<crate::scribe::memtable::FrozenMemtable, ScribeError>,
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
#[derive(Debug)]
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
    /// Published generations retained until grace-ordered WAL retirement.
    retained_generations: HashMap<u64, RetainedGeneration>,
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
    /// Completion sender routed back to this owner's command mailbox.
    completion_tx: mpsc::Sender<ShardCommand>,
    /// Typed WAL stream identity for generations and replay.
    stream: StreamIdentity,
    /// WAL handle for this fixed shard.
    wal_handle: WalHandle,
    /// Shared active/immutable memory ledger.
    memory_ledger: MemoryLedger,
    /// Snapshot published for synchronous inspection callers.
    snapshot: Arc<Mutex<ShardMemtableSnapshot>>,
    /// Pod-global count of accepted appends not yet completed.
    pending: Arc<AtomicUsize>,
    /// Notification used by runtime drains to await accepted work.
    drained: Arc<Notify>,
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

pub(crate) struct ScribeShardStartConfig {
    /// Admission controller shared by all shard owners.
    pub(crate) admission: AdmissionController,
    /// Grace period before committed generations may retire.
    pub(crate) retention_grace: std::time::Duration,
    /// Writable-byte threshold that triggers rotation.
    pub(crate) rotation_bytes: usize,
    /// Shared WAL writer used to create shard handles.
    pub(crate) wal: Arc<WalWriter>,
    /// Bounded CPU lane for replay and persistence preparation.
    pub(crate) persistence_cpu: ScribePersistenceCpuPool,
    /// Bounded WAL IO lane for append and retirement operations.
    pub(crate) wal_io: ScribeWalIoPool,
    /// Optional immutable-generation persistence runtime.
    pub(crate) persistence: Option<Arc<PersistenceRuntime>>,
    /// Typed pod stream identity.
    pub(crate) stream: StreamIdentity,
    /// Shared active/immutable memory ledger.
    pub(crate) memory_ledger: MemoryLedger,
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
            retention_grace,
            rotation_bytes,
            wal,
            persistence_cpu,
            wal_io,
            persistence,
            stream,
            memory_ledger,
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
                retained_generations: HashMap::new(),
                admission: admission.clone(),
                memtable: Memtable::new_with_limits(retention_grace, rotation_bytes),
                persistence_cpu: persistence_cpu.clone(),
                wal_io: wal_io.clone(),
                persistence: persistence.clone(),
                completion_tx: senders[id].sender.clone(),
                stream,
                wal_handle,
                memory_ledger: memory_ledger.clone(),
                snapshot: Arc::clone(&snapshots[id]),
                pending: Arc::clone(&pending),
                drained: Arc::clone(&drained),
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
    pub(crate) fn try_send(&self, mut append: PreparedAppend) -> Result<(), ScribeError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ScribeError::IngressClosed);
        }
        let shard = shard_for(append.tenant, &append.table);
        let table_name = append.table.fqn();
        if let Some(memory) = append.memory.as_mut() {
            memory.transfer_category(MemoryCategory::Queued);
        }
        self.pending.fetch_add(1, Ordering::AcqRel);
        let result = self.senders[shard].try_send(ShardCommand::Append(Box::new(append)));
        if let Err(error) = result {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            self.drained.notify_waiters();
            return Err(match error {
                mpsc::error::TrySendError::Full(ShardCommand::Append(_)) => {
                    ScribeError::IngestBusy { table: table_name }
                }
                mpsc::error::TrySendError::Closed(_) | mpsc::error::TrySendError::Full(_) => {
                    ScribeError::IngressClosed
                }
            });
        }
        Ok(())
    }

    /// Submit one bounded hot snapshot to the owning shard.
    pub(crate) async fn snapshot(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<Vec<HotBatch>, ScribeError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ScribeError::IngressClosed);
        }
        let shard = request.shard_id();
        let (response, result) = tokio::sync::oneshot::channel();
        let command = ShardCommand::Snapshot { request, response };
        tokio::time::timeout(
            std::time::Duration::from_millis(200),
            self.senders[shard].sender.send(command),
        )
        .await
        .map_err(|_| ScribeError::IngestBusy {
            table: "live-tail".to_owned(),
        })?
        .map_err(|_| ScribeError::IngressClosed)?;
        result.await.map_err(|_| ScribeError::Internal {
            detail: "shard dropped live-tail snapshot response".to_owned(),
        })?
    }

    /// Freeze one key through its owning shard.
    pub(crate) async fn freeze_key(
        &self,
        seal_key: crate::scribe::seal_key::SealKey,
    ) -> Result<crate::scribe::memtable::FrozenMemtable, ScribeError> {
        let shard = shard_for(seal_key.tenant, &seal_key.table);
        let (response, result) = tokio::sync::oneshot::channel();
        self.senders[shard]
            .sender
            .send(ShardCommand::FreezeKey { seal_key, response })
            .await
            .map_err(|_| ScribeError::IngressClosed)?;
        result.await.map_err(|_| ScribeError::Internal {
            detail: "shard dropped freeze response".to_owned(),
        })?
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
    pub(crate) async fn complete_post_commit(
        &self,
        seal_id: u64,
        seal_key: &crate::scribe::seal_key::SealKey,
        arrow_bytes: usize,
        file_list_key: crate::scribe::file_list_writer::FileListCommitKey,
    ) -> Result<(), ScribeError> {
        let shard = shard_for(seal_key.tenant, &seal_key.table);
        let (response, result) = tokio::sync::oneshot::channel();
        self.senders[shard]
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
    pub(crate) async fn abort_post_commit(
        &self,
        seal_id: u64,
        seal_key: &crate::scribe::seal_key::SealKey,
    ) -> Result<(), ScribeError> {
        let shard = shard_for(seal_key.tenant, &seal_key.table);
        let (response, result) = tokio::sync::oneshot::channel();
        self.senders[shard]
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

    /// Run one acknowledged committed-generation retirement pass for every shard in tests.
    ///
    /// Unlike the production coalescing signal, this waits until every owner
    /// has observed `now`. It lets integration journeys establish an exact
    /// hot-tail retirement boundary without changing production scheduling.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngressClosed`] when an owner has stopped or an
    /// owner-reported retirement error when one shard cannot process the pass.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) async fn retire_committed_for_test(
        &self,
        now: std::time::Instant,
    ) -> Result<(), ScribeError> {
        for sender in &self.senders {
            let (response, result) = tokio::sync::oneshot::channel();
            sender
                .sender
                .send(ShardCommand::RetireCommittedForTest { now, response })
                .await
                .map_err(|_| ScribeError::IngressClosed)?;
            result.await.map_err(|_| ScribeError::Internal {
                detail: "shard dropped test expiry response".to_owned(),
            })??;
        }
        Ok(())
    }

    /// Replace one coalescing pressure signal per fixed shard owner.
    pub(crate) fn request_pressure_flush(&self, keys: Vec<crate::scribe::seal_key::SealKey>) {
        let mut by_shard = vec![Vec::new(); SCRIBE_SHARD_COUNT];
        for key in keys {
            by_shard[shard_for(key.tenant, &key.table)].push(key);
        }
        for (shard, keys) in by_shard.into_iter().enumerate() {
            if !keys.is_empty() {
                self.pressure_senders[shard].send_replace(Some(PressureSignal {
                    keys,
                    wal_key: None,
                }));
            }
        }
    }

    /// Replace one coalescing WAL pressure signal for its global victim.
    pub(crate) fn request_wal_pressure_flush(&self, key: Option<crate::scribe::seal_key::SealKey>) {
        let Some(key) = key else {
            return;
        };
        let shard = shard_for(key.tenant, &key.table);
        self.pressure_senders[shard].send_replace(Some(PressureSignal {
            keys: Vec::new(),
            wal_key: Some(key),
        }));
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
    fn handle_pressure_signal(&mut self, signal: PressureSignal) {
        if !signal.keys.is_empty()
            && let Err(error) = self.flush_keys(signal.keys)
        {
            tracing::warn!(error = %error, shard = self.id, "pressure flush failed");
        }
        if let Some(key) = signal.wal_key
            && let Err(error) = self.flush_keys(vec![key])
        {
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
                self.retire_committed(now).await;
            }
            #[cfg(any(test, feature = "test-support"))]
            ShardCommand::RetireCommittedForTest { now, response } => {
                self.retire_committed(now).await;
                let result = self
                    .memtable
                    .sweep_once_for_test(now)
                    .and_then(|retired_bytes| {
                        self.admission.release_immutable(retired_bytes);
                        self.memory_ledger.release_immutable(retired_bytes)?;
                        Ok(())
                    });
                let _ = response.send(result);
            }
            ShardCommand::FlushAll { response } => {
                let _ = response.send(self.flush_all());
            }
            ShardCommand::PersistenceComplete { completion, waiter } => {
                self.handle_persistence_completion(*completion, waiter);
            }
            ShardCommand::Replay { state, response } => {
                let _ = response.send(self.replay_state(*state).await);
            }
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
    /// # Errors
    ///
    /// Returns [`ScribeError`] when replay decoding, memory accounting, WAL
    /// retention, or persistence submission fails.
    async fn replay_state(
        &mut self,
        replayed_state: crate::scribe::replay::ReplayedSealKey,
    ) -> Result<(), ScribeError> {
        let seal_key = replayed_state.seal_key.clone();
        let segment_refs = replayed_state.wal_segments.clone();
        let result = self
            .persistence_cpu
            .submit(
                crate::scribe::execution_lanes::ScribePersistenceCpuOp::RestoreReplay {
                    replayed: Box::new(replayed_state),
                },
            )
            .await?;
        let crate::scribe::execution_lanes::ScribePersistenceCpuResult::ReplayRestored(frozen) =
            result
        else {
            return Err(ScribeError::Internal {
                detail: "persistence CPU lane returned the wrong replay result".to_owned(),
            });
        };
        let frozen = self.memtable.insert_replayed_frozen(*frozen)?;
        if let Err(error) = self.memory_ledger.reserve_immutable(frozen.arrow_bytes) {
            let _ = self.memtable.discard_pending_generation(frozen.seal_id);
            return Err(error);
        }
        let owner_stats = self.memtable.stats()?;
        self.admission
            .sync_memtable_bytes(owner_stats.writable_bytes, owner_stats.immutable_bytes);
        if self.persistence.is_none() {
            return Ok(());
        }
        let binding =
            crate::catalog::TenantTableBinding::resolve((seal_key.tenant, seal_key.table.clone()))
                .map_err(|error| ScribeError::Internal {
                    detail: error.to_string(),
                })?;
        let generation = Arc::new(ImmutableGeneration::from_frozen(
            &frozen,
            (seal_key.tenant, seal_key.table.clone()),
            self.stream,
            segment_refs.clone(),
            self.wal_handle.clone(),
        ));
        self.pending_generations
            .entry(seal_key.clone())
            .or_default()
            .push_back(PendingGeneration {
                generation,
                binding,
                submitted: false,
            });
        self.wal_handle.retain_segments(&segment_refs)?;
        self.submit_front(&seal_key);
        Ok(())
    }

    /// Freezes one writable bucket and transfers its accounted bytes to immutable state.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the bucket cannot be inspected, frozen, or
    /// its memory ledger cannot be transferred.
    fn freeze_key(
        &mut self,
        seal_key: &crate::scribe::seal_key::SealKey,
    ) -> Result<crate::scribe::memtable::FrozenMemtable, ScribeError> {
        let active_bytes = self.memtable.row_count(seal_key)?;
        let frozen = self.memtable.freeze(seal_key)?;
        if active_bytes > 0 {
            self.memory_ledger
                .move_active_to_immutable(frozen.arrow_bytes)?;
            self.admission
                .transfer_active_to_immutable(frozen.arrow_bytes);
        }
        Ok(frozen)
    }

    /// Freezes every writable bucket belonging to one tenant.
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
        keys.into_iter().map(|key| self.freeze_key(&key)).collect()
    }

    /// Flushes expired buckets and retries pending persistence submissions.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when bucket selection, freezing, or generation
    /// construction fails; a full persistence queue remains pending for retry.
    fn flush_expired(&mut self, now: std::time::Instant) -> Result<(), ScribeError> {
        let keys = self.memtable.expired_seal_keys_for_shard(self.id, now)?;
        self.flush_keys(keys)?;
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
        self.flush_keys(keys)
    }

    /// Freezes selected writable buckets and queues immutable generations.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a bucket, memory transfer, table binding,
    /// or WAL retention operation fails. A full persistence queue leaves the
    /// generation unsubmitted for the next retry signal.
    fn flush_keys(
        &mut self,
        keys: Vec<crate::scribe::seal_key::SealKey>,
    ) -> Result<(), ScribeError> {
        for seal_key in keys {
            if self.memtable.row_count(&seal_key)? == 0 {
                continue;
            }
            let frozen = self.memtable.freeze(&seal_key)?;
            self.memory_ledger
                .move_active_to_immutable(frozen.arrow_bytes)?;
            self.admission
                .transfer_active_to_immutable(frozen.arrow_bytes);
            if self.persistence.is_none() {
                continue;
            }
            let binding = crate::catalog::TenantTableBinding::resolve((
                seal_key.tenant,
                seal_key.table.clone(),
            ))
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
            let segment_map = self.wal_segments.remove(&seal_key).unwrap_or_default();
            let segment_refs = segment_map
                .values()
                .map(|segment| segment.reference())
                .collect::<Vec<_>>();
            self.wal_handle.retain_segments(&segment_refs)?;
            let generation = Arc::new(ImmutableGeneration::from_frozen(
                &frozen,
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
            });
            if should_submit {
                self.submit_front(&seal_key);
            }
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
        if persistence
            .try_submit(PersistenceJob {
                generation: Arc::clone(&generation),
                binding,
                completion_tx: self.completion_tx.clone(),
                completion_waiter: None,
            })
            .is_err()
        {
            return;
        }
        front.submitted = true;
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

    /// Retires committed generations after their retention grace period.
    ///
    /// Memory is released before WAL retirement is submitted. A failed WAL
    /// retirement is logged and remains recoverable on a later lifecycle pass.
    ///
    /// # Cancellation
    ///
    /// Cancellation may leave a generation retained; its memory and WAL
    /// references remain owned until the next retirement pass.
    async fn retire_committed(&mut self, now: std::time::Instant) {
        let generation_ids = self
            .retained_generations
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for generation_id in generation_ids {
            let retired = match self.memtable.retire_generation_at(generation_id, now) {
                Ok(retired) => retired,
                Err(error) => {
                    tracing::warn!(error = %error, generation_id, "shard generation retirement failed");
                    continue;
                }
            };
            if retired.is_none() {
                continue;
            }
            let Some(retained) = self.retained_generations.remove(&generation_id) else {
                continue;
            };
            self.admission.release_immutable(retained.arrow_bytes);
            let _ = self.memory_ledger.release_immutable(retained.arrow_bytes);
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
    }

    /// Reconciles one persistence result with this owner's FIFO generation queue.
    ///
    /// Successful publication moves the generation to retained state and
    /// submits the next generation for the same key. Failures mark only the
    /// front generation retryable and deliver the error to its waiter.
    fn handle_persistence_completion(
        &mut self,
        completion: PersistenceCompletion,
        waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    ) {
        let generation_id = completion.generation_id.0;
        let seal_key = self.pending_generations.iter().find_map(|(key, queue)| {
            queue
                .front()
                .filter(|front| front.generation.generation_id.0 == generation_id)
                .map(|_| key.clone())
        });
        let Some(seal_key) = seal_key else {
            self.handle_unowned_completion(completion, waiter);
            return;
        };
        if let Some(error) = completion.error.as_ref() {
            self.mark_front_retryable(&seal_key);
            tracing::warn!(error = %error, generation_id, "shard persistence failed; retaining immutable generation");
            Self::send_waiter_error(waiter, error.clone());
            return;
        }
        let Some(file_list_key) = completion.file_list_key.clone() else {
            self.mark_front_retryable(&seal_key);
            Self::send_waiter_error(
                waiter,
                "persistence completion omitted its file-list key".to_owned(),
            );
            return;
        };
        if let Err(error) = self
            .memtable
            .complete_post_commit(generation_id, file_list_key)
        {
            self.mark_front_retryable(&seal_key);
            tracing::warn!(error = %error, generation_id, "shard persistence completion could not publish generation");
            Self::send_waiter_error(waiter, error.to_string());
            return;
        }
        let Some(queue) = self.pending_generations.get_mut(&seal_key) else {
            return;
        };
        let Some(front) = queue.pop_front() else {
            return;
        };
        if front.generation.generation_id.0 != generation_id {
            queue.push_front(front);
            return;
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
        if let Some(waiter) = waiter {
            let _ = waiter.send(Ok(()));
        }
        if self.persistence.is_some() {
            self.submit_front(&seal_key);
        }
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

    /// Delivers a persistence failure to an optional post-commit waiter.
    fn send_waiter_error(
        waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
        detail: String,
    ) {
        if let Some(waiter) = waiter {
            let _ = waiter.send(Err(detail));
        }
    }

    fn handle_unowned_completion(
        &mut self,
        completion: PersistenceCompletion,
        waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    ) {
        let generation_id = completion.generation_id.0;
        if let Some(error) = completion.error {
            Self::send_waiter_error(waiter, error);
            return;
        }
        let Some(file_list_key) = completion.file_list_key else {
            Self::send_waiter_error(
                waiter,
                "persistence completion omitted its file-list key".to_owned(),
            );
            return;
        };
        if let Err(error) = self
            .memtable
            .complete_post_commit(generation_id, file_list_key)
        {
            Self::send_waiter_error(waiter, error.to_string());
            return;
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
    }
}

/// Holds one WAL-durable slice until memtable insertion completes.
struct DurableSlice {
    /// Seal bucket receiving the rows.
    seal_key: crate::scribe::seal_key::SealKey,
    /// Audit event paired with the data rows.
    audit_event: wyrd_spec::vala::api::AuditEvent,
    /// Arrow rows awaiting memtable insertion.
    rows: arrow::record_batch::RecordBatch,
    /// Active-memory bytes reserved for this slice.
    memtable_bytes: usize,
    /// Batch identity used for retry deduplication.
    batch_id: [u8; 16],
    /// WAL sequence number assigned to the durable append.
    lsn: crate::scribe::wal::WalLsn,
}

/// A WAL-synced slice whose Arrow rows have not yet reached the active
/// memtable. Successful insertion removes the entry, so this bounded index
/// only covers the fsync-success/memtable-failure retry window.
#[derive(Debug)]
struct WalSliceState {
    /// WAL sequence number that can be reused without another append.
    lsn: crate::scribe::wal::WalLsn,
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
            self.release_active_reservations(&state.durable);
            self.mark_wal_error(&error);
            Self::notify_prepared_error(&mut state.prepared, &error);
            return Err(error);
        }
        for slice in &state.durable {
            self.synced_not_inserted.insert(
                AppendSliceId {
                    batch_id: uuid::Uuid::from_bytes(slice.batch_id),
                    seal_key: slice.seal_key.clone(),
                },
                WalSliceState { lsn: slice.lsn },
            );
        }
        #[cfg(any(test, feature = "test-support"))]
        if self.wal_handle.take_post_sync_failure_for_test() {
            let error = ScribeError::Internal {
                detail: "injected post-sync WAL failure".to_owned(),
            };
            self.release_active_reservations(&state.durable);
            self.mark_wal_error(&error);
            Self::notify_prepared_error(&mut state.prepared, &error);
            return Err(error);
        }
        let touched_keys = match self.insert_group(state.durable, &mut state.rows_by_append) {
            Ok(keys) => keys,
            Err(error) => {
                Self::notify_prepared_error(&mut state.prepared, &error);
                return Err(error);
            }
        };
        if let Err(error) = self.rotate_group(touched_keys) {
            tracing::warn!(error = %error, "durable append completed but bucket rotation was deferred");
        }
        for append in state.prepared {
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
        let mut durable = Vec::new();
        let mut touched = HashMap::<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>::new();
        let mut rows_by_append = HashMap::<[u8; 16], u64>::new();
        for append in &mut prepared {
            if let Some(memory) = append.memory.as_mut() {
                memory.transfer_category(MemoryCategory::Prepared);
            }
            let batch_id = *append.batch_id.as_bytes();
            let slices = std::mem::take(&mut append.slices);
            for slice in slices {
                match self
                    .memtable
                    .retained_batch_rows(&slice.seal_key, *slice.id.batch_id.as_bytes())
                {
                    Ok(Some(rows)) => {
                        let entry = rows_by_append.entry(batch_id).or_default();
                        *entry = entry.saturating_add(rows);
                        continue;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.release_active_reservations(&durable);
                        Self::notify_prepared_error(&mut prepared, &error);
                        return Err(error);
                    }
                }
                let slice_id = slice.id.clone();
                let (slice, result) =
                    match if let Some(previous) = self.synced_not_inserted.get(&slice_id) {
                        self.reuse_synced_slice(append, slice, previous.lsn)
                    } else {
                        self.write_prepared_slice(append, slice).await
                    } {
                        Ok(value) => value,
                        Err(error) => {
                            self.release_active_reservations(&durable);
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
            }
        }
        Ok(GroupWalState {
            prepared,
            durable,
            touched,
            rows_by_append,
        })
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
        let PreparedSlice {
            seal_key,
            audit_event,
            rows,
            wal_append,
            memtable_bytes,
            ..
        } = slice;
        self.admission
            .try_reserve_active(seal_key.table.fqn(), memtable_bytes)?;
        let active_memory = if let Some(memory) = append.memory.as_mut() {
            memory.split(memtable_bytes)
        } else {
            self.admission.release_active(memtable_bytes);
            return Err(ScribeError::Internal {
                detail: "prepared append lost its memory reservation".to_owned(),
            });
        };
        let mut active_memory = match active_memory {
            Ok(value) => value,
            Err(error) => {
                self.admission.release_active(memtable_bytes);
                return Err(error);
            }
        };
        active_memory.transfer_category(MemoryCategory::Active);
        if let Err(error) = self.memory_ledger.absorb_active(active_memory) {
            self.admission.release_active(memtable_bytes);
            return Err(error);
        }
        let wal_result = self
            .wal_io
            .submit(ScribeWalIoOp::WritePrepared {
                wal: self.wal_handle.clone(),
                append: wal_append,
            })
            .await;
        let result = match wal_result {
            Ok(ScribeWalIoResult::WalWritten { result }) => result,
            Ok(_) => {
                self.admission.release_active(memtable_bytes);
                let _ = self.memory_ledger.release_active(memtable_bytes);
                return Err(ScribeError::Internal {
                    detail: "WAL IO lane returned the wrong shard append result".to_owned(),
                });
            }
            Err(error) => {
                self.admission.release_active(memtable_bytes);
                let _ = self.memory_ledger.release_active(memtable_bytes);
                return Err(error);
            }
        };
        Ok((
            DurableSlice {
                seal_key,
                audit_event,
                rows,
                memtable_bytes,
                batch_id: *append.batch_id.as_bytes(),
                lsn: result.lsn,
            },
            result,
        ))
    }

    /// Reuse a slice whose WAL record was already synced before a prior memtable
    /// insertion failed. The retry supplies the Arrow rows again, but it must not
    /// append a second WAL record.
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
        lsn: crate::scribe::wal::WalLsn,
    ) -> Result<(DurableSlice, crate::scribe::wal::WalAppendResult), ScribeError> {
        let PreparedSlice {
            seal_key,
            audit_event,
            rows,
            memtable_bytes,
            ..
        } = slice;
        self.admission
            .try_reserve_active(seal_key.table.fqn(), memtable_bytes)?;
        let active_memory = if let Some(memory) = append.memory.as_mut() {
            memory.split(memtable_bytes)
        } else {
            self.admission.release_active(memtable_bytes);
            return Err(ScribeError::Internal {
                detail: "prepared append lost its memory reservation during WAL retry".to_owned(),
            });
        };
        let active_memory = match active_memory {
            Ok(mut value) => {
                value.transfer_category(MemoryCategory::Active);
                value
            }
            Err(error) => {
                self.admission.release_active(memtable_bytes);
                return Err(error);
            }
        };
        if let Err(error) = self.memory_ledger.absorb_active(active_memory) {
            self.admission.release_active(memtable_bytes);
            return Err(error);
        }
        Ok((
            DurableSlice {
                seal_key,
                audit_event,
                rows,
                memtable_bytes,
                batch_id: *append.batch_id.as_bytes(),
                lsn,
            },
            crate::scribe::wal::WalAppendResult {
                lsn,
                #[cfg(feature = "bench-support")]
                encoded_bytes: 0,
                touched_segments: Vec::new(),
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

    /// Inserts WAL-synced slices into this owner's memtable and clears retry state.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when insertion or active-memory release fails.
    fn insert_group(
        &mut self,
        durable: Vec<DurableSlice>,
        rows_by_append: &mut HashMap<[u8; 16], u64>,
    ) -> Result<HashSet<crate::scribe::seal_key::SealKey>, ScribeError> {
        let mut touched_keys = HashSet::new();
        let mut slices = durable.into_iter();
        while let Some(slice) = slices.next() {
            touched_keys.insert(slice.seal_key.clone());
            let rows = slice.rows.num_rows();
            if let Err(error) = self.memtable.insert(
                &slice.seal_key,
                slice.audit_event,
                crate::scribe::wal::ScribeAppendMeta {
                    batch_id: slice.batch_id,
                    rows_accepted: rows,
                    wal_lsn_min: slice.lsn,
                    wal_lsn_max: slice.lsn,
                    seal_key: slice.seal_key.as_path_components(),
                },
                slice.rows,
            ) {
                self.admission.release_active(slice.memtable_bytes);
                let _ = self.memory_ledger.release_active(slice.memtable_bytes);
                for remaining in slices {
                    self.admission.release_active(remaining.memtable_bytes);
                    let _ = self.memory_ledger.release_active(remaining.memtable_bytes);
                }
                return Err(error);
            }
            self.synced_not_inserted.remove(&AppendSliceId {
                batch_id: uuid::Uuid::from_bytes(slice.batch_id),
                seal_key: slice.seal_key.clone(),
            });
            let entry = rows_by_append.entry(slice.batch_id).or_default();
            *entry = entry.saturating_add(u64::try_from(rows).unwrap_or(u64::MAX));
        }
        Ok(touched_keys)
    }

    /// Completes every prepared ACK waiter with one group failure.
    fn notify_prepared_error(prepared: &mut [PreparedAppend], error: &ScribeError) {
        for append in prepared {
            if let Some(sender) = append.durable_ack.take() {
                let _ = sender.send(Err(error.completion_copy()));
            }
        }
    }

    /// Releases active reservations after a failed group stage.
    fn release_active_reservations(&self, durable: &[DurableSlice]) {
        for slice in durable {
            self.admission.release_active(slice.memtable_bytes);
            let _ = self.memory_ledger.release_active(slice.memtable_bytes);
        }
    }

    /// Trips the admission breaker when WAL storage is full.
    fn mark_wal_error(&self, error: &ScribeError) {
        if matches!(error, ScribeError::WalDiskFull) {
            self.admission.trip_wal_disk_full();
        }
    }

    /// Selects buckets crossing the rotation threshold and freezes them.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when rotation selection or generation preparation fails.
    fn rotate_group(
        &mut self,
        touched_keys: HashSet<crate::scribe::seal_key::SealKey>,
    ) -> Result<(), ScribeError> {
        let mut keys = Vec::new();
        for seal_key in touched_keys {
            if self.memtable.should_seal(&seal_key)? {
                keys.push(seal_key);
            }
        }
        self.flush_keys(keys)
    }
}

/// Classifies the one expected shard-group capacity boundary.
///
/// WAL exhaustion has already tripped the admission breaker before this
/// classification. Every other group failure remains an unexpected error.
fn is_expected_wal_capacity(error: &ScribeError) -> bool {
    matches!(error, ScribeError::WalDiskFull)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::seal_key::{EventDay, SealKey};
    use crate::scribe::wal::{ScribeAppendMeta, WalLsn};
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use chrono::NaiveDate;
    use std::sync::Arc;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

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

    #[test]
    fn grace_expiry_does_not_add_generation_tasks() {
        let key = owner_key();
        let owner = Memtable::new_with_retention(std::time::Duration::from_secs(1));
        owner
            .insert(&key, owner_event(), owner_meta(&key), owner_batch())
            .expect("owner insert");
        let frozen = owner.freeze(&key).expect("owner freeze");
        owner
            .complete_post_commit(frozen.seal_id, owner_file_list_key(&key))
            .expect("owner completion");
        assert_eq!(
            owner
                .sweep_once_at(std::time::Instant::now() + std::time::Duration::from_secs(2))
                .expect("grace sweep")
                .len(),
            1
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

    #[test]
    fn routing_uses_canonical_table_reference() {
        let set = ScribeShardSet::<Item>::new();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        assert!(set.shard_for(DataTenantId::new_v7(), &table).id < 16);
    }
}
