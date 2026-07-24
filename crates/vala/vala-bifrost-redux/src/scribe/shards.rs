//! Fixed pod-local Scribe shard mailboxes and tenant round-robin scheduling.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::sync::mpsc;
use uuid::Uuid;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::scribe::admission::AdmissionController;
use crate::scribe::execution_lanes::{
    ScribePostAckCpuOp, ScribePostAckCpuPool, ScribePostAckCpuResult, ScribeWalIoOp,
    ScribeWalIoPool, ScribeWalIoResult,
};
use crate::scribe::memtable::Memtable;
use crate::scribe::persistence::{
    ImmutableGeneration, PersistenceJob, PersistenceRuntime, WriterControl,
};
use crate::scribe::preprocess::{AdmittedAppend, AppendSliceId, PreparedAppend, PreparedSlice};
use crate::scribe::routing::{SCRIBE_SHARD_COUNT, shard_for};
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::{WalHandle, WalWriter};

/// Capacity of one shard command mailbox.
pub const SHARD_COMMAND_CAPACITY: usize = 256;

/// Maximum requests in one tenant scheduling group.
pub const MAX_GROUP_ITEMS: usize = 64;

/// Maximum bytes in one tenant scheduling group.
pub const MAX_GROUP_BYTES: usize = 64 * 1024 * 1024;

/// A value that can be scheduled by a Scribe shard.
pub trait ShardItem {
    /// Authenticated tenant owning the item.
    fn tenant(&self) -> DataTenantId;
    /// Retained bytes charged to the item.
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

    /// Drain one bounded group from the next tenant in round-robin order.
    pub fn pop_group(&mut self) -> Vec<T> {
        let mut result = Vec::new();
        let mut bytes = 0_usize;
        let Some(tenant) = self.active_tenants.pop_front() else {
            return result;
        };
        let Some(queue) = self.pending_by_tenant.get_mut(&tenant) else {
            return result;
        };
        while result.len() < MAX_GROUP_ITEMS {
            let Some(item) = queue.front() else {
                break;
            };
            if !result.is_empty() && bytes.saturating_add(item.bytes()) > MAX_GROUP_BYTES {
                break;
            }
            let Some(item) = queue.pop_front() else {
                break;
            };
            bytes = bytes.saturating_add(item.bytes());
            result.push(item);
        }
        if queue.is_empty() {
            self.pending_by_tenant.remove(&tenant);
        } else {
            self.active_tenants.push_back(tenant);
        }
        result
    }

    /// Whether the shard has no queued requests.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.active_tenants.is_empty()
    }
}

impl ShardItem for AdmittedAppend {
    fn tenant(&self) -> DataTenantId {
        self.tenant
    }

    fn bytes(&self) -> usize {
        self.admitted_bytes
    }
}

/// One command accepted by a fixed shard owner.
#[derive(Debug)]
pub(crate) enum ShardCommand {
    Append(Box<AdmittedAppend>),
    Shutdown,
}

#[derive(Debug)]
struct ShardDependencies {
    admission: AdmissionController,
    memtable: Arc<Memtable>,
    wal: Arc<WalWriter>,
    post_ack_cpu: ScribePostAckCpuPool,
    wal_io: ScribeWalIoPool,
    persistence: Option<Arc<PersistenceRuntime>>,
    completion_tx: mpsc::Sender<WriterControl>,
    stream: StreamIdentity,
    writer_instance_id: Uuid,
}

/// The live fixed-shard Scribe runtime.
#[derive(Debug)]
pub(crate) struct ScribeShardRuntime {
    senders: Vec<ScribeShard<ShardCommand>>,
    tasks: tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
    pending: Arc<AtomicUsize>,
    drained: Arc<Notify>,
    closed: AtomicBool,
    completion_tx: mpsc::Sender<WriterControl>,
}

pub(crate) struct ScribeShardStartConfig {
    pub(crate) admission: AdmissionController,
    pub(crate) memtable: Arc<Memtable>,
    pub(crate) wal: Arc<WalWriter>,
    pub(crate) post_ack_cpu: ScribePostAckCpuPool,
    pub(crate) wal_io: ScribeWalIoPool,
    pub(crate) persistence: Option<Arc<PersistenceRuntime>>,
    pub(crate) stream: StreamIdentity,
}

impl ScribeShardRuntime {
    /// Start exactly sixteen shard owners and their bounded command mailboxes.
    pub(crate) fn start(config: ScribeShardStartConfig, runtime: &Handle) -> Arc<Self> {
        let ScribeShardStartConfig {
            admission,
            memtable,
            wal,
            post_ack_cpu,
            wal_io,
            persistence,
            stream,
        } = config;
        let mut set = ScribeShardSet::<ShardCommand>::new();
        let receivers = set.take_receivers().unwrap_or_default();
        let senders = (0..SCRIBE_SHARD_COUNT).map(|id| set.shard(id)).collect();
        let pending = Arc::new(AtomicUsize::new(0));
        let drained = Arc::new(Notify::new());
        let mut tasks = Vec::with_capacity(SCRIBE_SHARD_COUNT);
        let (completion_tx, completion_rx) = mpsc::channel(256);
        tasks.push(runtime.spawn(run_persistence_controls(
            completion_rx,
            Arc::clone(&memtable),
            admission.clone(),
            wal_io.clone(),
        )));
        for (id, receiver) in receivers.into_iter().enumerate() {
            let dependencies = ShardDependencies {
                admission: admission.clone(),
                memtable: Arc::clone(&memtable),
                wal: Arc::clone(&wal),
                post_ack_cpu: post_ack_cpu.clone(),
                wal_io: wal_io.clone(),
                persistence: persistence.clone(),
                completion_tx: completion_tx.clone(),
                stream,
                writer_instance_id: Uuid::now_v7(),
            };
            let pending_count = Arc::clone(&pending);
            let drained_signal = Arc::clone(&drained);
            tasks.push(runtime.spawn(run_shard(
                id,
                receiver,
                dependencies,
                pending_count,
                drained_signal,
            )));
        }
        Arc::new(Self {
            senders,
            tasks: tokio::sync::Mutex::new(tasks),
            pending,
            drained,
            closed: AtomicBool::new(false),
            completion_tx,
        })
    }

    /// Enqueue a prepared request onto its deterministic shard.
    pub(crate) fn try_send(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        append: AdmittedAppend,
    ) -> Result<(), ScribeError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ScribeError::IngressClosed);
        }
        let shard = shard_for(tenant, table);
        self.pending.fetch_add(1, Ordering::AcqRel);
        let result = self.senders[shard].try_send(ShardCommand::Append(Box::new(append)));
        if let Err(error) = result {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            self.drained.notify_waiters();
            return Err(match error {
                mpsc::error::TrySendError::Full(ShardCommand::Append(_)) => {
                    ScribeError::IngestBusy { table: table.fqn() }
                }
                mpsc::error::TrySendError::Closed(
                    ShardCommand::Append(_) | ShardCommand::Shutdown,
                )
                | mpsc::error::TrySendError::Full(ShardCommand::Shutdown) => {
                    ScribeError::IngressClosed
                }
            });
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

    /// Stop all owners after draining accepted work.
    pub(crate) async fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.drain().await;
        for sender in &self.senders {
            let _ = sender.try_send(ShardCommand::Shutdown);
        }
        let _ = self.completion_tx.send(WriterControl::Shutdown).await;
        let mut tasks = self.tasks.lock().await;
        for task in tasks.drain(..) {
            let _ = task.await;
        }
    }
}

async fn run_shard(
    id: usize,
    mut receiver: mpsc::Receiver<ShardCommand>,
    dependencies: ShardDependencies,
    pending: Arc<AtomicUsize>,
    drained: Arc<Notify>,
) {
    let mut scheduler = TenantRoundRobin::<AdmittedAppend>::default();
    let mut shutting_down = false;
    let mut seen_slices = HashSet::<AppendSliceId>::new();
    let mut seen_rows = HashMap::<AppendSliceId, u64>::new();
    let mut wal_handles = HashMap::<crate::scribe::seal_key::SealKey, WalHandle>::new();
    let mut wal_bytes = HashMap::<crate::scribe::seal_key::SealKey, u64>::new();
    loop {
        while let Ok(command) = receiver.try_recv() {
            match command {
                ShardCommand::Append(append) => scheduler.push(*append),
                ShardCommand::Shutdown => shutting_down = true,
            }
        }
        let group = scheduler.pop_group();
        if !group.is_empty() {
            let group_len = group.len();
            let result = process_group(
                &dependencies,
                group,
                &mut seen_slices,
                &mut seen_rows,
                &mut wal_handles,
                &mut wal_bytes,
            )
            .await;
            if let Err(error) = result {
                tracing::error!(error = %error, shard = id, "Scribe shard group failed");
            }
            pending.fetch_sub(group_len, Ordering::AcqRel);
            drained.notify_waiters();
            continue;
        }
        if shutting_down && scheduler.is_empty() && pending.load(Ordering::Acquire) == 0 {
            break;
        }
        match receiver.recv().await {
            Some(ShardCommand::Append(append)) => scheduler.push(*append),
            Some(ShardCommand::Shutdown) => shutting_down = true,
            None => break,
        }
    }
}

async fn run_persistence_controls(
    mut receiver: mpsc::Receiver<WriterControl>,
    memtable: Arc<Memtable>,
    admission: AdmissionController,
    wal_io: ScribeWalIoPool,
) {
    while let Some(control) = receiver.recv().await {
        match control {
            WriterControl::Shutdown => break,
            WriterControl::PersistenceComplete(completion) => {
                if let Some(error) = completion.error {
                    tracing::warn!(
                        error,
                        "shard persistence failed; retaining immutable generation"
                    );
                    continue;
                }
                let Some(file_list_key) = completion.file_list_key.clone() else {
                    continue;
                };
                if let Err(error) =
                    memtable.complete_post_commit(completion.generation_id.0, file_list_key)
                {
                    tracing::warn!(error = %error, "shard persistence completion could not publish generation");
                    continue;
                }
                let memtable = Arc::clone(&memtable);
                let admission = admission.clone();
                let wal_io = wal_io.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(crate::scribe::memtable::ACTIVE_GENERATION_MAX_AGE / 10)
                        .await;
                    match memtable
                        .retire_generation_at(completion.generation_id.0, std::time::Instant::now())
                    {
                        Ok(Some(_)) => {
                            admission.release_immutable(completion.arrow_bytes);
                            let _ = wal_io
                                .submit(ScribeWalIoOp::RetireWal {
                                    wal: completion.wal,
                                    segments: completion.wal_segments,
                                })
                                .await;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            tracing::warn!(error = %error, "shard generation retirement failed");
                        }
                    }
                });
            }
            WriterControl::GraceExpired(_) => {}
        }
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

struct GroupWalState {
    prepared: Vec<PreparedAppend>,
    durable: Vec<DurableSlice>,
    touched: HashMap<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>,
    touched_by_key: HashMap<
        crate::scribe::seal_key::SealKey,
        HashMap<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>,
    >,
    rows_by_append: HashMap<[u8; 16], u64>,
}

async fn process_group(
    dependencies: &ShardDependencies,
    group: Vec<AdmittedAppend>,
    seen_slices: &mut HashSet<AppendSliceId>,
    seen_rows: &mut HashMap<AppendSliceId, u64>,
    wal_handles: &mut HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
    wal_bytes: &mut HashMap<crate::scribe::seal_key::SealKey, u64>,
) -> Result<(), ScribeError> {
    let prepared = prepare_group(dependencies, group).await?;
    let mut state = write_group(
        dependencies,
        prepared,
        seen_slices,
        seen_rows,
        wal_handles,
        wal_bytes,
    )
    .await?;
    sync_group(dependencies, &state.touched).await?;
    let touched_keys = insert_group(dependencies, state.durable, &mut state.rows_by_append)?;
    rotate_group(
        dependencies,
        touched_keys,
        &mut state.touched_by_key,
        wal_handles,
        wal_bytes,
    )?;
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

async fn prepare_group(
    dependencies: &ShardDependencies,
    group: Vec<AdmittedAppend>,
) -> Result<Vec<PreparedAppend>, ScribeError> {
    let mut prepared = Vec::with_capacity(group.len());
    for append in group {
        let prepared_append = match dependencies
            .post_ack_cpu
            .submit(ScribePostAckCpuOp::Preprocess(Box::new(append)))
            .await?
        {
            ScribePostAckCpuResult::Prepared(value) => value,
            ScribePostAckCpuResult::ParquetEncoded(_) | ScribePostAckCpuResult::ReplayRestored => {
                return Err(ScribeError::Internal {
                    detail: "post-ACK lane returned the wrong shard result".to_owned(),
                });
            }
        };
        prepared.push(prepared_append);
    }
    Ok(prepared)
}

async fn write_group(
    dependencies: &ShardDependencies,
    mut prepared: Vec<PreparedAppend>,
    seen_slices: &mut HashSet<AppendSliceId>,
    seen_rows: &mut HashMap<AppendSliceId, u64>,
    wal_handles: &mut HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
    wal_bytes: &mut HashMap<crate::scribe::seal_key::SealKey, u64>,
) -> Result<GroupWalState, ScribeError> {
    let mut durable = Vec::new();
    let mut touched = HashMap::<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>::new();
    let mut touched_by_key = HashMap::<
        crate::scribe::seal_key::SealKey,
        HashMap<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>,
    >::new();
    let mut rows_by_append = HashMap::<[u8; 16], u64>::new();
    for append in &mut prepared {
        let batch_id = *append.batch_id.as_bytes();
        let slices = std::mem::take(&mut append.slices);
        for slice in slices {
            if !seen_slices.insert(slice.id.clone()) {
                let rows = seen_rows.get(&slice.id).copied().unwrap_or(0);
                let entry = rows_by_append.entry(batch_id).or_default();
                *entry = entry.saturating_add(rows);
                continue;
            }
            let slice_id = slice.id.clone();
            let handle = if let Some(handle) = wal_handles.get(&slice.seal_key) {
                handle.clone()
            } else {
                let handle = dependencies.wal.handle_for_seal_key(slice.seal_key.clone());
                wal_handles.insert(slice.seal_key.clone(), handle.clone());
                handle
            };
            let PreparedSlice {
                seal_key,
                audit_event,
                rows,
                wal_append,
                memtable_bytes,
                ..
            } = slice;
            let ScribeWalIoResult::WalWritten { wal, result } = dependencies
                .wal_io
                .submit(ScribeWalIoOp::WritePrepared {
                    wal: handle,
                    append: wal_append,
                })
                .await?
            else {
                return Err(ScribeError::Internal {
                    detail: "WAL IO lane returned the wrong shard append result".to_owned(),
                });
            };
            let encoded_bytes = result.encoded_bytes;
            let row_count = u64::try_from(rows.num_rows()).unwrap_or(u64::MAX);
            seen_rows.insert(slice_id, row_count);
            for segment in &result.touched_segments {
                touched.insert(segment.path().to_path_buf(), Arc::clone(segment));
                touched_by_key
                    .entry(seal_key.clone())
                    .or_default()
                    .insert(segment.path().to_path_buf(), Arc::clone(segment));
            }
            let entry = wal_bytes.entry(seal_key.clone()).or_default();
            *entry = entry.saturating_add(encoded_bytes);
            durable.push(DurableSlice {
                seal_key,
                audit_event,
                rows,
                memtable_bytes,
                batch_id,
                lsn: result.lsn,
            });
            let _ = wal;
        }
    }
    Ok(GroupWalState {
        prepared,
        durable,
        touched,
        touched_by_key,
        rows_by_append,
    })
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
        .submit(ScribeWalIoOp::SyncWal { segments })
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
) -> Result<HashSet<crate::scribe::seal_key::SealKey>, ScribeError> {
    let mut touched_keys = HashSet::new();
    for slice in durable {
        touched_keys.insert(slice.seal_key.clone());
        dependencies
            .admission
            .try_reserve_active(slice.seal_key.table.fqn(), slice.memtable_bytes)?;
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
            return Err(error);
        }
        let entry = rows_by_append.entry(slice.batch_id).or_default();
        *entry = entry.saturating_add(u64::try_from(rows).unwrap_or(u64::MAX));
    }
    Ok(touched_keys)
}

fn rotate_group(
    dependencies: &ShardDependencies,
    touched_keys: HashSet<crate::scribe::seal_key::SealKey>,
    touched_by_key: &mut HashMap<
        crate::scribe::seal_key::SealKey,
        HashMap<std::path::PathBuf, Arc<crate::scribe::wal::WalSegment>>,
    >,
    wal_handles: &HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
    wal_bytes: &mut HashMap<crate::scribe::seal_key::SealKey, u64>,
) -> Result<(), ScribeError> {
    for seal_key in touched_keys {
        let wal_due = wal_bytes.get(&seal_key).copied().unwrap_or(0)
            >= u64::try_from(crate::scribe::memtable::WAL_ROTATION_BYTES).unwrap_or(u64::MAX);
        if wal_due || dependencies.memtable.should_seal(&seal_key)? {
            let frozen = dependencies.memtable.freeze(&seal_key)?;
            dependencies
                .admission
                .transfer_active_to_immutable(frozen.arrow_bytes);
            wal_bytes.remove(&seal_key);
            if let Some(persistence) = &dependencies.persistence {
                let binding = crate::catalog::TenantTableBinding::resolve((
                    seal_key.tenant,
                    seal_key.table.clone(),
                ))
                .map_err(|error| ScribeError::Internal {
                    detail: error.to_string(),
                })?;
                let wal =
                    wal_handles
                        .get(&seal_key)
                        .cloned()
                        .ok_or_else(|| ScribeError::Internal {
                            detail: format!("missing WAL handle for rotated seal-key {seal_key}"),
                        })?;
                let wal_segments = touched_by_key
                    .remove(&seal_key)
                    .unwrap_or_default()
                    .into_values()
                    .map(|segment| segment.reference())
                    .collect();
                let generation = Arc::new(ImmutableGeneration::from_frozen(
                    &frozen,
                    (seal_key.tenant, seal_key.table.clone()),
                    dependencies.writer_instance_id,
                    dependencies.stream,
                    wal_segments,
                    wal,
                ));
                persistence
                    .try_submit(PersistenceJob {
                        generation,
                        binding,
                        completion_tx: dependencies.completion_tx.clone(),
                    })
                    .map_err(|_| ScribeError::IngestBusy {
                        table: seal_key.table.fqn(),
                    })?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::BifrostNamespace;

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
        assert_eq!(first_group.len(), 2);
        assert_eq!(first_group[0].tenant(), first);
        assert_eq!(first_group[1].sequence, 2);
        let second_group = scheduler.pop_group();
        assert_eq!(second_group.len(), 1);
        assert_eq!(second_group[0].tenant(), second);
        assert!(scheduler.pop_group().is_empty());
    }

    #[test]
    fn routing_uses_canonical_table_reference() {
        let set = ScribeShardSet::<Item>::new();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        assert!(set.shard_for(DataTenantId::new_v7(), &table).id < 16);
    }
}
