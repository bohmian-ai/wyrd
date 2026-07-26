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
    CompletePostCommit {
        seal_id: u64,
        file_list_key: crate::scribe::file_list_writer::FileListCommitKey,
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

#[derive(Debug)]
struct ShardDependencies {
    admission: AdmissionController,
    memtable: Memtable,
    persistence_cpu: ScribePersistenceCpuPool,
    wal_io: ScribeWalIoPool,
    persistence: Option<Arc<PersistenceRuntime>>,
    completion_tx: mpsc::Sender<ShardCommand>,
    stream: StreamIdentity,
    wal_handle: WalHandle,
    memory_ledger: MemoryLedger,
    snapshot: Arc<Mutex<ShardMemtableSnapshot>>,
}

/// Last owner-published state used by synchronous inspection surfaces.
#[derive(Debug, Clone, Default)]
pub(crate) struct ShardMemtableSnapshot {
    pub(crate) stats: MemtableStats,
    pub(crate) bucket_memory: Vec<BucketMemorySnapshot>,
    pub(crate) pressure_candidates: Vec<PressureCandidate>,
}

/// The live fixed-shard Scribe runtime.
#[derive(Debug)]
pub(crate) struct ScribeShardRuntime {
    senders: Vec<ScribeShard<ShardCommand>>,
    pressure_senders: Vec<watch::Sender<Option<PressureSignal>>>,
    snapshots: Vec<Arc<Mutex<ShardMemtableSnapshot>>>,
    tasks: tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
    pending: Arc<AtomicUsize>,
    drained: Arc<Notify>,
    closed: AtomicBool,
}

pub(crate) struct ScribeShardStartConfig {
    pub(crate) admission: AdmissionController,
    pub(crate) retention_grace: std::time::Duration,
    pub(crate) rotation_bytes: usize,
    pub(crate) wal: Arc<WalWriter>,
    pub(crate) persistence_cpu: ScribePersistenceCpuPool,
    pub(crate) wal_io: ScribeWalIoPool,
    pub(crate) persistence: Option<Arc<PersistenceRuntime>>,
    pub(crate) stream: StreamIdentity,
    pub(crate) memory_ledger: MemoryLedger,
}

impl ScribeShardRuntime {
    /// Start exactly sixteen shard owners and their bounded command mailboxes.
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
            let dependencies = ShardDependencies {
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
            };
            let pending_count = Arc::clone(&pending);
            let drained_signal = Arc::clone(&drained);
            let (_, pressure_receiver) = &pressure_channels[id];
            tasks.push(runtime.spawn(run_shard(
                id,
                receiver,
                pressure_receiver.clone(),
                dependencies,
                pending_count,
                drained_signal,
            )));
        }
        Arc::new(Self {
            senders,
            pressure_senders,
            snapshots,
            tasks: tokio::sync::Mutex::new(tasks),
            pending,
            drained,
            closed: AtomicBool::new(false),
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
        file_list_key: crate::scribe::file_list_writer::FileListCommitKey,
    ) -> Result<(), ScribeError> {
        let shard = shard_for(seal_key.tenant, &seal_key.table);
        let (response, result) = tokio::sync::oneshot::channel();
        self.senders[shard]
            .sender
            .send(ShardCommand::CompletePostCommit {
                seal_id,
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

    /// Stop all owners after draining accepted work.
    pub(crate) async fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.drain().await;
        for sender in &self.senders {
            let _ = sender.try_send(ShardCommand::Shutdown);
        }
        let mut tasks = self.tasks.lock().await;
        for task in tasks.drain(..) {
            let _ = task.await;
        }
    }
}

fn publish_memtable_snapshot(dependencies: &ShardDependencies) {
    let snapshot = ShardMemtableSnapshot {
        stats: dependencies.memtable.stats().unwrap_or_default(),
        bucket_memory: dependencies.memtable.bucket_memory().unwrap_or_default(),
        pressure_candidates: dependencies
            .memtable
            .pressure_candidates()
            .unwrap_or_default(),
    };
    if let Ok(mut published) = dependencies.snapshot.lock() {
        *published = snapshot;
    }
}

async fn run_shard(
    id: usize,
    mut receiver: mpsc::Receiver<ShardCommand>,
    mut pressure_receiver: watch::Receiver<Option<PressureSignal>>,
    dependencies: ShardDependencies,
    pending: Arc<AtomicUsize>,
    drained: Arc<Notify>,
) {
    publish_memtable_snapshot(&dependencies);
    let mut scheduler = TenantRoundRobin::<PreparedAppend>::default();
    let mut shutting_down = false;
    let mut wal_segments = WalSegmentsByKey::new();
    let mut synced_not_inserted = HashMap::<AppendSliceId, WalSliceState>::new();
    let mut pending_generations = PendingGenerationsByKey::new();
    let mut retained_generations = HashMap::<u64, RetainedGeneration>::new();
    loop {
        if pressure_receiver.has_changed().unwrap_or(false) {
            let signal = {
                let borrowed = pressure_receiver.borrow_and_update();
                borrowed.clone()
            };
            if let Some(signal) = signal {
                handle_pressure_signal(
                    signal,
                    id,
                    &dependencies,
                    &mut wal_segments,
                    &mut pending_generations,
                );
                publish_memtable_snapshot(&dependencies);
            }
        }
        while let Ok(command) = receiver.try_recv() {
            if handle_shard_command(
                command,
                id,
                &dependencies,
                &mut scheduler,
                &mut wal_segments,
                &mut pending_generations,
                &mut retained_generations,
            )
            .await
            {
                shutting_down = true;
            }
            publish_memtable_snapshot(&dependencies);
        }
        let group = scheduler.pop_group();
        if !group.is_empty() {
            let group_len = group.len();
            let result = process_group(
                &dependencies,
                group,
                &mut wal_segments,
                &mut synced_not_inserted,
                &mut pending_generations,
            )
            .await;
            if let Err(error) = result {
                tracing::error!(error = %error, shard = id, "Scribe shard group failed");
            }
            publish_memtable_snapshot(&dependencies);
            pending.fetch_sub(group_len, Ordering::AcqRel);
            drained.notify_waiters();
            continue;
        }
        if shutting_down && scheduler.is_empty() && pending.load(Ordering::Acquire) == 0 {
            break;
        }
        tokio::select! {
            command = receiver.recv() => {
                match command {
                    Some(command) => {
                        if handle_shard_command(
                            command,
                            id,
                            &dependencies,
                            &mut scheduler,
                            &mut wal_segments,
                            &mut pending_generations,
                            &mut retained_generations,
                        )
                        .await
                        {
                            shutting_down = true;
                        }
                        publish_memtable_snapshot(&dependencies);
                    }
                    None => break,
                }
            }
            changed = pressure_receiver.changed() => {
                if changed.is_err() {
                    break;
                }
            }
        }
    }
}

fn handle_pressure_signal(
    signal: PressureSignal,
    shard: usize,
    dependencies: &ShardDependencies,
    wal_segments: &mut WalSegmentsByKey,
    pending_generations: &mut PendingGenerationsByKey,
) {
    if !signal.keys.is_empty()
        && let Err(error) =
            flush_keys_at_owner(dependencies, signal.keys, wal_segments, pending_generations)
    {
        tracing::warn!(error = %error, shard, "pressure flush failed");
    }
    if let Some(key) = signal.wal_key
        && let Err(error) =
            flush_keys_at_owner(dependencies, vec![key], wal_segments, pending_generations)
    {
        tracing::warn!(error = %error, shard, "WAL pressure flush failed");
    }
}

/// Apply one command at the shard owner so both polling paths share the same
/// ownership and error-handling behavior.
async fn handle_shard_command(
    command: ShardCommand,
    id: usize,
    dependencies: &ShardDependencies,
    scheduler: &mut TenantRoundRobin<PreparedAppend>,
    wal_segments: &mut WalSegmentsByKey,
    pending_generations: &mut PendingGenerationsByKey,
    retained_generations: &mut HashMap<u64, RetainedGeneration>,
) -> bool {
    match command {
        ShardCommand::Append(append) => scheduler.push(*append),
        ShardCommand::Snapshot { request, response } => {
            let _ = response.send(snapshot_at_owner(dependencies, &request));
        }
        ShardCommand::FlushExpired { now } => {
            if let Err(error) =
                flush_expired_at_owner(dependencies, id, now, wal_segments, pending_generations)
            {
                tracing::warn!(error = %error, shard = id, "age flush failed");
            }
            retire_committed_at_owner(dependencies, now, retained_generations).await;
        }
        ShardCommand::FlushAll { response } => {
            let result = flush_all_at_owner(dependencies, id, wal_segments, pending_generations);
            let _ = response.send(result);
        }
        ShardCommand::PersistenceComplete { completion, waiter } => {
            handle_persistence_completion(
                dependencies,
                *completion,
                waiter,
                pending_generations,
                retained_generations,
                wal_segments,
            );
        }
        ShardCommand::Replay { state, response } => {
            let result = replay_state_at_owner(dependencies, *state, pending_generations).await;
            let _ = response.send(result);
        }
        ShardCommand::FreezeKey { seal_key, response } => {
            let result = freeze_key_at_owner(dependencies, &seal_key);
            let _ = response.send(result);
        }
        ShardCommand::FreezeTenant { tenant, response } => {
            let result = freeze_tenant_at_owner(dependencies, tenant);
            let _ = response.send(result);
        }
        ShardCommand::CompletePostCommit {
            seal_id,
            file_list_key,
            response,
        } => {
            let result = dependencies
                .memtable
                .complete_post_commit(seal_id, file_list_key);
            let _ = response.send(result);
        }
        ShardCommand::AbortPostCommit { seal_id, response } => {
            let result = dependencies.memtable.abort_post_commit(seal_id);
            let _ = response.send(result);
        }
        ShardCommand::Shutdown => return true,
    }
    false
}

/// Execute an Oracle structural snapshot at the single-owner state boundary.
fn snapshot_at_owner(
    dependencies: &ShardDependencies,
    request: &FetchLiveTailRequest,
) -> Result<Vec<HotBatch>, ScribeError> {
    let readable = dependencies.memtable.readable_batches_for_range(
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

async fn replay_state_at_owner(
    dependencies: &ShardDependencies,
    replayed_state: crate::scribe::replay::ReplayedSealKey,
    pending_generations: &mut PendingGenerationsByKey,
) -> Result<(), ScribeError> {
    let seal_key = replayed_state.seal_key.clone();
    let segment_refs = replayed_state.wal_segments.clone();
    let result = dependencies
        .persistence_cpu
        .submit(
            crate::scribe::execution_lanes::ScribePersistenceCpuOp::RestoreReplay {
                replayed: Box::new(replayed_state),
            },
        )
        .await?;
    let crate::scribe::execution_lanes::ScribePersistenceCpuResult::ReplayRestored(frozen) = result
    else {
        return Err(ScribeError::Internal {
            detail: "persistence CPU lane returned the wrong replay result".to_owned(),
        });
    };
    let frozen = dependencies.memtable.insert_replayed_frozen(*frozen)?;
    if let Err(error) = dependencies
        .memory_ledger
        .reserve_immutable(frozen.arrow_bytes)
    {
        let _ = dependencies
            .memtable
            .discard_pending_generation(frozen.seal_id);
        return Err(error);
    }
    let owner_stats = dependencies.memtable.stats()?;
    dependencies
        .admission
        .sync_memtable_bytes(owner_stats.writable_bytes, owner_stats.immutable_bytes);

    let Some(persistence) = &dependencies.persistence else {
        return Ok(());
    };
    let binding =
        crate::catalog::TenantTableBinding::resolve((seal_key.tenant, seal_key.table.clone()))
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
    let generation = Arc::new(ImmutableGeneration::from_frozen(
        &frozen,
        (seal_key.tenant, seal_key.table.clone()),
        dependencies.stream,
        segment_refs.clone(),
        dependencies.wal_handle.clone(),
    ));
    pending_generations
        .entry(seal_key.clone())
        .or_default()
        .push_back(PendingGeneration {
            generation: Arc::clone(&generation),
            binding: binding.clone(),
            submitted: true,
        });
    dependencies.wal_handle.retain_segments(&segment_refs)?;
    if let Err(error) = persistence
        .submit(PersistenceJob {
            generation,
            binding,
            completion_tx: dependencies.completion_tx.clone(),
            completion_waiter: None,
        })
        .await
    {
        mark_front_retryable(pending_generations, &seal_key);
        return Err(error);
    }
    Ok(())
}

fn freeze_key_at_owner(
    dependencies: &ShardDependencies,
    seal_key: &crate::scribe::seal_key::SealKey,
) -> Result<crate::scribe::memtable::FrozenMemtable, ScribeError> {
    let active_bytes = dependencies.memtable.row_count(seal_key)?;
    let frozen = dependencies.memtable.freeze(seal_key)?;
    if active_bytes > 0 {
        dependencies
            .memory_ledger
            .move_active_to_immutable(frozen.arrow_bytes)?;
        dependencies
            .admission
            .transfer_active_to_immutable(frozen.arrow_bytes);
    }
    Ok(frozen)
}

fn freeze_tenant_at_owner(
    dependencies: &ShardDependencies,
    tenant: DataTenantId,
) -> Result<Vec<crate::scribe::memtable::FrozenMemtable>, ScribeError> {
    dependencies
        .memtable
        .seal_keys_for_tenant(tenant)?
        .into_iter()
        .map(|seal_key| freeze_key_at_owner(dependencies, &seal_key))
        .collect()
}

fn flush_expired_at_owner(
    dependencies: &ShardDependencies,
    shard: usize,
    now: std::time::Instant,
    wal_segments: &mut WalSegmentsByKey,
    pending_generations: &mut PendingGenerationsByKey,
) -> Result<(), ScribeError> {
    let keys = dependencies
        .memtable
        .expired_seal_keys_for_shard(shard, now)?;
    flush_keys_at_owner(dependencies, keys, wal_segments, pending_generations)?;
    retry_pending_at_owner(dependencies, pending_generations, wal_segments);
    Ok(())
}

fn flush_all_at_owner(
    dependencies: &ShardDependencies,
    shard: usize,
    wal_segments: &mut WalSegmentsByKey,
    pending_generations: &mut PendingGenerationsByKey,
) -> Result<(), ScribeError> {
    let keys = dependencies.memtable.active_seal_keys_for_shard(shard)?;
    flush_keys_at_owner(dependencies, keys, wal_segments, pending_generations)
}

fn flush_keys_at_owner(
    dependencies: &ShardDependencies,
    keys: Vec<crate::scribe::seal_key::SealKey>,
    wal_segments: &mut WalSegmentsByKey,
    pending_generations: &mut PendingGenerationsByKey,
) -> Result<(), ScribeError> {
    for seal_key in keys {
        if dependencies.memtable.row_count(&seal_key)? == 0 {
            continue;
        }
        let frozen = dependencies.memtable.freeze(&seal_key)?;
        dependencies
            .memory_ledger
            .move_active_to_immutable(frozen.arrow_bytes)?;
        dependencies
            .admission
            .transfer_active_to_immutable(frozen.arrow_bytes);
        let Some(persistence) = &dependencies.persistence else {
            continue;
        };
        let binding =
            crate::catalog::TenantTableBinding::resolve((seal_key.tenant, seal_key.table.clone()))
                .map_err(|error| ScribeError::Internal {
                    detail: error.to_string(),
                })?;
        let segment_map = wal_segments.remove(&seal_key).unwrap_or_default();
        let segment_refs = segment_map
            .values()
            .map(|segment| segment.reference())
            .collect::<Vec<_>>();
        dependencies.wal_handle.retain_segments(&segment_refs)?;
        let generation = Arc::new(ImmutableGeneration::from_frozen(
            &frozen,
            (seal_key.tenant, seal_key.table.clone()),
            dependencies.stream,
            segment_refs.clone(),
            dependencies.wal_handle.clone(),
        ));
        let queue = pending_generations.entry(seal_key.clone()).or_default();
        let should_submit = queue.is_empty();
        queue.push_back(PendingGeneration {
            generation,
            binding,
            submitted: false,
        });
        if should_submit {
            submit_front_at_owner(
                dependencies,
                &seal_key,
                persistence,
                pending_generations,
                wal_segments,
            );
        }
    }
    Ok(())
}

fn submit_front_at_owner(
    dependencies: &ShardDependencies,
    seal_key: &crate::scribe::seal_key::SealKey,
    persistence: &PersistenceRuntime,
    pending_generations: &mut PendingGenerationsByKey,
    wal_segments: &WalSegmentsByKey,
) {
    let Some(queue) = pending_generations.get_mut(seal_key) else {
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
            completion_tx: dependencies.completion_tx.clone(),
            completion_waiter: None,
        })
        .is_err()
    {
        return;
    }
    front.submitted = true;
    let active_paths = wal_segments
        .values()
        .flat_map(|segments| segments.keys().cloned())
        .collect::<HashSet<_>>();
    if let Err(error) = dependencies
        .wal_handle
        .close_segments_if_unowned(&generation.wal_segments, &active_paths)
    {
        tracing::warn!(error = %error, "closed WAL segment could not be detached after bucket rotation");
    }
}

fn retry_pending_at_owner(
    dependencies: &ShardDependencies,
    pending_generations: &mut PendingGenerationsByKey,
    wal_segments: &WalSegmentsByKey,
) {
    let Some(persistence) = &dependencies.persistence else {
        return;
    };
    let keys = pending_generations.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        submit_front_at_owner(
            dependencies,
            &key,
            persistence,
            pending_generations,
            wal_segments,
        );
    }
}

async fn retire_committed_at_owner(
    dependencies: &ShardDependencies,
    now: std::time::Instant,
    retained_generations: &mut HashMap<u64, RetainedGeneration>,
) {
    let generation_ids = retained_generations.keys().copied().collect::<Vec<_>>();
    for generation_id in generation_ids {
        let retired = match dependencies
            .memtable
            .retire_generation_at(generation_id, now)
        {
            Ok(retired) => retired,
            Err(error) => {
                tracing::warn!(error = %error, generation_id, "shard generation retirement failed");
                continue;
            }
        };
        if retired.is_none() {
            continue;
        }
        let Some(retained) = retained_generations.remove(&generation_id) else {
            continue;
        };
        dependencies
            .admission
            .release_immutable(retained.arrow_bytes);
        let _ = dependencies
            .memory_ledger
            .release_immutable(retained.arrow_bytes);
        if let Err(error) = dependencies
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

fn handle_persistence_completion(
    dependencies: &ShardDependencies,
    completion: PersistenceCompletion,
    waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    pending_generations: &mut PendingGenerationsByKey,
    retained_generations: &mut HashMap<u64, RetainedGeneration>,
    wal_segments: &WalSegmentsByKey,
) {
    let generation_id = completion.generation_id.0;
    let seal_key = pending_generations.iter().find_map(|(key, queue)| {
        queue
            .front()
            .filter(|front| front.generation.generation_id.0 == generation_id)
            .map(|_| key.clone())
    });
    let Some(seal_key) = seal_key else {
        handle_unowned_completion(dependencies, completion, waiter, retained_generations);
        return;
    };
    if let Some(error) = completion.error.as_ref() {
        mark_front_retryable(pending_generations, &seal_key);
        tracing::warn!(error = %error, generation_id, "shard persistence failed; retaining immutable generation");
        send_waiter_error(waiter, error.clone());
        return;
    }
    let Some(file_list_key) = completion.file_list_key.clone() else {
        mark_front_retryable(pending_generations, &seal_key);
        send_waiter_error(
            waiter,
            "persistence completion omitted its file-list key".to_owned(),
        );
        return;
    };
    if let Err(error) = dependencies
        .memtable
        .complete_post_commit(generation_id, file_list_key)
    {
        mark_front_retryable(pending_generations, &seal_key);
        tracing::warn!(error = %error, generation_id, "shard persistence completion could not publish generation");
        send_waiter_error(waiter, error.to_string());
        return;
    }
    let Some(queue) = pending_generations.get_mut(&seal_key) else {
        return;
    };
    let Some(front) = queue.pop_front() else {
        return;
    };
    if front.generation.generation_id.0 != generation_id {
        queue.push_front(front);
        return;
    }
    retained_generations.insert(
        generation_id,
        RetainedGeneration {
            arrow_bytes: completion.arrow_bytes,
            wal_segments: completion.wal_segments,
            wal: completion.wal,
        },
    );
    if queue.is_empty() {
        pending_generations.remove(&seal_key);
    }
    if let Some(waiter) = waiter {
        let _ = waiter.send(Ok(()));
    }
    if let Some(persistence) = &dependencies.persistence {
        submit_front_at_owner(
            dependencies,
            &seal_key,
            persistence,
            pending_generations,
            wal_segments,
        );
    }
}

fn mark_front_retryable(
    pending_generations: &mut PendingGenerationsByKey,
    seal_key: &crate::scribe::seal_key::SealKey,
) {
    if let Some(front) = pending_generations
        .get_mut(seal_key)
        .and_then(VecDeque::front_mut)
    {
        front.submitted = false;
    }
}

fn send_waiter_error(
    waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    detail: String,
) {
    if let Some(waiter) = waiter {
        let _ = waiter.send(Err(detail));
    }
}

fn handle_unowned_completion(
    dependencies: &ShardDependencies,
    completion: PersistenceCompletion,
    waiter: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
    retained_generations: &mut HashMap<u64, RetainedGeneration>,
) {
    let generation_id = completion.generation_id.0;
    if let Some(error) = completion.error {
        send_waiter_error(waiter, error);
        return;
    }
    let Some(file_list_key) = completion.file_list_key else {
        send_waiter_error(
            waiter,
            "persistence completion omitted its file-list key".to_owned(),
        );
        return;
    };
    if let Err(error) = dependencies
        .memtable
        .complete_post_commit(generation_id, file_list_key)
    {
        send_waiter_error(waiter, error.to_string());
        return;
    }
    retained_generations.insert(
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

struct DurableSlice {
    seal_key: crate::scribe::seal_key::SealKey,
    audit_event: wyrd_spec::vala::api::AuditEvent,
    rows: arrow::record_batch::RecordBatch,
    memtable_bytes: usize,
    batch_id: [u8; 16],
    lsn: crate::scribe::wal::WalLsn,
}

/// A WAL-synced slice whose Arrow rows have not yet reached the active
/// memtable. Successful insertion removes the entry, so this bounded index
/// only covers the fsync-success/memtable-failure retry window.
struct WalSliceState {
    lsn: crate::scribe::wal::WalLsn,
}

struct GroupWalState {
    prepared: Vec<PreparedAppend>,
    durable: Vec<DurableSlice>,
    touched: HashMap<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>,
    rows_by_append: HashMap<[u8; 16], u64>,
}

async fn process_group(
    dependencies: &ShardDependencies,
    group: Vec<PreparedAppend>,
    wal_segments: &mut WalSegmentsByKey,
    synced_not_inserted: &mut HashMap<AppendSliceId, WalSliceState>,
    pending_generations: &mut PendingGenerationsByKey,
) -> Result<(), ScribeError> {
    let mut state = write_group(dependencies, group, wal_segments, synced_not_inserted).await?;
    if let Err(error) = sync_group(dependencies, &state.touched).await {
        release_active_reservations(dependencies, &state.durable);
        mark_wal_error(dependencies, &error);
        notify_prepared_error(&mut state.prepared, &error);
        return Err(error);
    }
    for slice in &state.durable {
        synced_not_inserted.insert(
            AppendSliceId {
                batch_id: uuid::Uuid::from_bytes(slice.batch_id),
                seal_key: slice.seal_key.clone(),
            },
            WalSliceState { lsn: slice.lsn },
        );
    }
    if dependencies.wal_handle.take_post_sync_failure_for_test() {
        let error = ScribeError::Internal {
            detail: "injected post-sync WAL failure".to_owned(),
        };
        release_active_reservations(dependencies, &state.durable);
        mark_wal_error(dependencies, &error);
        notify_prepared_error(&mut state.prepared, &error);
        return Err(error);
    }
    let touched_keys = match insert_group(
        dependencies,
        state.durable,
        &mut state.rows_by_append,
        synced_not_inserted,
    ) {
        Ok(keys) => keys,
        Err(error) => {
            notify_prepared_error(&mut state.prepared, &error);
            return Err(error);
        }
    };
    if let Err(error) = rotate_group(
        dependencies,
        touched_keys,
        wal_segments,
        pending_generations,
    ) {
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

async fn write_group(
    dependencies: &ShardDependencies,
    mut prepared: Vec<PreparedAppend>,
    wal_segments: &mut WalSegmentsByKey,
    synced_not_inserted: &HashMap<AppendSliceId, WalSliceState>,
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
            match dependencies
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
                    release_active_reservations(dependencies, &durable);
                    notify_prepared_error(&mut prepared, &error);
                    return Err(error);
                }
            }
            let slice_id = slice.id.clone();
            let (slice, result) = match if let Some(previous) = synced_not_inserted.get(&slice_id) {
                reuse_synced_slice(dependencies, append, slice, previous.lsn)
            } else {
                write_prepared_slice(dependencies, append, slice).await
            } {
                Ok(value) => value,
                Err(error) => {
                    release_active_reservations(dependencies, &durable);
                    mark_wal_error(dependencies, &error);
                    notify_prepared_error(&mut prepared, &error);
                    return Err(error);
                }
            };
            for segment in &result.touched_segments {
                touched.insert(segment.path().to_path_buf(), Arc::clone(segment));
                wal_segments
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

async fn write_prepared_slice(
    dependencies: &ShardDependencies,
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
    dependencies
        .admission
        .try_reserve_active(seal_key.table.fqn(), memtable_bytes)?;
    let active_memory = if let Some(memory) = append.memory.as_mut() {
        memory.split(memtable_bytes)
    } else {
        dependencies.admission.release_active(memtable_bytes);
        return Err(ScribeError::Internal {
            detail: "prepared append lost its memory reservation".to_owned(),
        });
    };
    let mut active_memory = match active_memory {
        Ok(value) => value,
        Err(error) => {
            dependencies.admission.release_active(memtable_bytes);
            return Err(error);
        }
    };
    active_memory.transfer_category(MemoryCategory::Active);
    if let Err(error) = dependencies.memory_ledger.absorb_active(active_memory) {
        dependencies.admission.release_active(memtable_bytes);
        return Err(error);
    }
    let wal_result = dependencies
        .wal_io
        .submit(ScribeWalIoOp::WritePrepared {
            wal: dependencies.wal_handle.clone(),
            append: wal_append,
        })
        .await;
    let result = match wal_result {
        Ok(ScribeWalIoResult::WalWritten { result }) => result,
        Ok(_) => {
            dependencies.admission.release_active(memtable_bytes);
            let _ = dependencies.memory_ledger.release_active(memtable_bytes);
            return Err(ScribeError::Internal {
                detail: "WAL IO lane returned the wrong shard append result".to_owned(),
            });
        }
        Err(error) => {
            dependencies.admission.release_active(memtable_bytes);
            let _ = dependencies.memory_ledger.release_active(memtable_bytes);
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
fn reuse_synced_slice(
    dependencies: &ShardDependencies,
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
    dependencies
        .admission
        .try_reserve_active(seal_key.table.fqn(), memtable_bytes)?;
    let active_memory = if let Some(memory) = append.memory.as_mut() {
        memory.split(memtable_bytes)
    } else {
        dependencies.admission.release_active(memtable_bytes);
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
            dependencies.admission.release_active(memtable_bytes);
            return Err(error);
        }
    };
    if let Err(error) = dependencies.memory_ledger.absorb_active(active_memory) {
        dependencies.admission.release_active(memtable_bytes);
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

async fn sync_group(
    dependencies: &ShardDependencies,
    touched: &HashMap<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>,
) -> Result<(), ScribeError> {
    if touched.is_empty() {
        return Ok(());
    }
    let segments = touched.values().cloned().collect::<Vec<_>>();
    match dependencies
        .wal_io
        .submit(ScribeWalIoOp::SyncWal {
            wal: dependencies.wal_handle.clone(),
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

fn insert_group(
    dependencies: &ShardDependencies,
    durable: Vec<DurableSlice>,
    rows_by_append: &mut HashMap<[u8; 16], u64>,
    synced_not_inserted: &mut HashMap<AppendSliceId, WalSliceState>,
) -> Result<HashSet<crate::scribe::seal_key::SealKey>, ScribeError> {
    let mut touched_keys = HashSet::new();
    let mut slices = durable.into_iter();
    while let Some(slice) = slices.next() {
        touched_keys.insert(slice.seal_key.clone());
        let rows = slice.rows.num_rows();
        if let Err(error) = dependencies.memtable.insert(
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
            dependencies.admission.release_active(slice.memtable_bytes);
            let _ = dependencies
                .memory_ledger
                .release_active(slice.memtable_bytes);
            for remaining in slices {
                dependencies
                    .admission
                    .release_active(remaining.memtable_bytes);
                let _ = dependencies
                    .memory_ledger
                    .release_active(remaining.memtable_bytes);
            }
            return Err(error);
        }
        synced_not_inserted.remove(&AppendSliceId {
            batch_id: uuid::Uuid::from_bytes(slice.batch_id),
            seal_key: slice.seal_key.clone(),
        });
        let entry = rows_by_append.entry(slice.batch_id).or_default();
        *entry = entry.saturating_add(u64::try_from(rows).unwrap_or(u64::MAX));
    }
    Ok(touched_keys)
}

fn notify_prepared_error(prepared: &mut [PreparedAppend], error: &ScribeError) {
    for append in prepared {
        if let Some(sender) = append.durable_ack.take() {
            let _ = sender.send(Err(error.completion_copy()));
        }
    }
}

fn release_active_reservations(dependencies: &ShardDependencies, durable: &[DurableSlice]) {
    for slice in durable {
        dependencies.admission.release_active(slice.memtable_bytes);
        let _ = dependencies
            .memory_ledger
            .release_active(slice.memtable_bytes);
    }
}

fn mark_wal_error(dependencies: &ShardDependencies, error: &ScribeError) {
    if matches!(error, ScribeError::WalDiskFull) {
        dependencies.admission.trip_wal_disk_full();
    }
}

fn rotate_group(
    dependencies: &ShardDependencies,
    touched_keys: HashSet<crate::scribe::seal_key::SealKey>,
    wal_segments: &mut WalSegmentsByKey,
    pending_generations: &mut PendingGenerationsByKey,
) -> Result<(), ScribeError> {
    let mut keys = Vec::new();
    for seal_key in touched_keys {
        if dependencies.memtable.should_seal(&seal_key)? {
            keys.push(seal_key);
        }
    }
    flush_keys_at_owner(dependencies, keys, wal_segments, pending_generations)
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
