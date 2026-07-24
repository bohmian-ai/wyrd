//! Per-tenant/table FIFO writer actors and pod-global writer registry.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use num_traits::ToPrimitive;
use tokio::runtime::Handle;
use tokio::sync::{Notify, mpsc, watch};
use uuid::Uuid;

use crate::catalog::{TenantTableBinding, TenantTableKey};
use crate::contracts::ScribeError;
use crate::scribe::admission::{AdmissionController, WriterLease};
use crate::scribe::execution_lanes::{
    ScribePostAckCpuOp, ScribePostAckCpuPool, ScribePostAckCpuResult, ScribeWalIoOp,
    ScribeWalIoPool, ScribeWalIoResult,
};
use crate::scribe::memtable::Memtable;
use crate::scribe::persistence::{
    ImmutableGeneration, PersistenceCompletion, PersistenceJob, PersistenceRuntime,
};
use crate::scribe::preprocess::{AdmittedAppend, AppendSliceId, PreparedSlice};
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::{ScribeAppendMeta, WalHandle, WalWriter};

const CONTROL_QUEUE_CAPACITY: usize = 256;
const MAX_DEDUPE_SLICES: usize = 1_048_576;

#[derive(Debug)]
struct WriterState {
    accepting: AtomicBool,
    healthy: AtomicBool,
    pending: AtomicUsize,
    reserved_sends: AtomicUsize,
    terminal_errors: AtomicUsize,
    last_activity: Mutex<Instant>,
    stopping: AtomicBool,
    finished: AtomicBool,
    stopped: Notify,
}

#[derive(Debug)]
pub(crate) enum WriterControl {
    PersistenceComplete(PersistenceCompletion),
    GraceExpired(PersistenceCompletion),
    Shutdown,
}

#[derive(Debug)]
struct WriterRuntime {
    writer_identity: String,
    writer_key: TenantTableKey,
    binding: TenantTableBinding,
    instance_id: Uuid,
    stream: StreamIdentity,
    state: Arc<WriterState>,
    admission: AdmissionController,
    memtable: Arc<Memtable>,
    wal: Arc<WalWriter>,
    post_ack_cpu: ScribePostAckCpuPool,
    wal_io: ScribeWalIoPool,
    persistence: Option<Arc<PersistenceRuntime>>,
    commit_config: crate::scribe::ScribeCommitConfig,
}

#[derive(Debug)]
struct WriterDependencies {
    admission: AdmissionController,
    memtable: Arc<Memtable>,
    wal: Arc<WalWriter>,
    post_ack_cpu: ScribePostAckCpuPool,
    wal_io: ScribeWalIoPool,
    persistence: Option<Arc<PersistenceRuntime>>,
    stream: StreamIdentity,
    coordination_runtime: Handle,
    writer_queue_items: usize,
    writer_idle_ttl: std::time::Duration,
    commit_config: crate::scribe::ScribeCommitConfig,
}

pub(crate) struct WriterRegistryRuntime {
    pub(crate) coordination_runtime: Handle,
    pub(crate) commit_config: crate::scribe::ScribeCommitConfig,
    pub(crate) persistence: Option<Arc<PersistenceRuntime>>,
    pub(crate) stream: StreamIdentity,
}

/// One FIFO writer for a tenant/table binding.
#[derive(Debug)]
pub(crate) struct TenantTableWriter {
    pub(crate) key: TenantTableKey,
    pub(crate) binding: TenantTableBinding,
    pub(crate) instance_id: Uuid,
    state: Arc<WriterState>,
    data_tx: mpsc::Sender<AdmittedAppend>,
    control_tx: mpsc::Sender<WriterControl>,
    age_tx: watch::Sender<Instant>,
    idle_ttl: std::time::Duration,
    _lease: WriterLease,
    _task: tokio::task::JoinHandle<()>,
}

impl TenantTableWriter {
    /// Create one FIFO writer actor for a resolved tenant/table binding.
    fn new(
        binding: TenantTableBinding,
        lease: WriterLease,
        dependencies: WriterDependencies,
    ) -> Arc<Self> {
        let writer_identity = format!("{}:{}", binding.tenant, binding.table_ref.fqn());
        let instance_id = Uuid::now_v7();
        let WriterDependencies {
            admission,
            memtable,
            wal,
            post_ack_cpu,
            wal_io,
            persistence,
            stream,
            commit_config,
            coordination_runtime,
            writer_queue_items,
            writer_idle_ttl,
        } = dependencies;
        let (data_tx, data_rx) = mpsc::channel(writer_queue_items.max(1));
        let (control_tx, control_rx) = mpsc::channel(CONTROL_QUEUE_CAPACITY);
        let (age_tx, age_rx) = watch::channel(Instant::now());
        let state = Arc::new(WriterState {
            accepting: AtomicBool::new(true),
            healthy: AtomicBool::new(true),
            pending: AtomicUsize::new(0),
            reserved_sends: AtomicUsize::new(0),
            terminal_errors: AtomicUsize::new(0),
            last_activity: Mutex::new(Instant::now()),
            stopping: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            stopped: Notify::new(),
        });
        let runtime = WriterRuntime {
            writer_identity,
            writer_key: (binding.tenant, binding.table_ref.clone()),
            binding: binding.clone(),
            instance_id,
            stream,
            state: Arc::clone(&state),
            admission,
            memtable,
            wal,
            post_ack_cpu,
            wal_io,
            persistence,
            commit_config,
        };
        let task = coordination_runtime.spawn(run_writer(
            runtime,
            data_rx,
            control_rx,
            age_rx,
            control_tx.clone(),
        ));
        let key = (binding.tenant, binding.table_ref.clone());
        Arc::new(Self {
            key,
            binding,
            instance_id,
            state,
            data_tx,
            control_tx,
            age_tx,
            idle_ttl: writer_idle_ttl,
            _lease: lease,
            _task: task,
        })
    }

    pub(crate) fn is_available(&self) -> bool {
        self.state.accepting.load(Ordering::Acquire) && self.state.healthy.load(Ordering::Acquire)
    }

    pub(crate) fn pending(&self) -> usize {
        self.state.pending.load(Ordering::Acquire)
    }

    pub(crate) fn is_healthy(&self) -> bool {
        self.state.healthy.load(Ordering::Acquire)
    }

    pub(crate) fn terminal_errors(&self) -> usize {
        self.state.terminal_errors.load(Ordering::Acquire)
    }

    fn is_idle(&self, now: Instant) -> bool {
        let inactive = match self.state.last_activity.lock() {
            Ok(last) => now.saturating_duration_since(*last) >= self.idle_ttl,
            Err(_) => false,
        };
        self.state.accepting.load(Ordering::Acquire)
            && self.state.healthy.load(Ordering::Acquire)
            && self.pending() == 0
            && self.state.reserved_sends.load(Ordering::Acquire) == 0
            && inactive
    }

    fn close_admission(&self) {
        self.state.stopping.store(true, Ordering::Release);
        self.state.accepting.store(false, Ordering::Release);
    }

    async fn send_control(&self, control: WriterControl) -> Result<(), ScribeError> {
        self.control_tx
            .send(control)
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!("writer control queue closed: {error}"),
            })
    }

    #[cfg(test)]
    pub(crate) fn make_idle_for_test(&self) {
        if let Ok(mut last_activity) = self.state.last_activity.lock() {
            *last_activity = Instant::now()
                .checked_sub(std::time::Duration::from_mins(11))
                .expect("test clock supports idle writer offset");
        }
    }

    #[cfg(test)]
    pub(crate) fn stop_accepting_for_test(&self) {
        self.state.accepting.store(false, Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) fn mark_unhealthy_for_test(&self) {
        self.state.healthy.store(false, Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) async fn close_with_reserved_send_for_test(
        &self,
        append: AdmittedAppend,
    ) -> Result<(), ScribeError> {
        let stopped = self.state.stopped.notified();
        let permit = self
            .data_tx
            .try_reserve()
            .map_err(|_| ScribeError::IngestBusy {
                table: self.binding.table_ref.fqn(),
            })?;
        self.state.reserved_sends.fetch_add(1, Ordering::AcqRel);
        self.state.pending.fetch_add(1, Ordering::AcqRel);
        self.state.accepting.store(false, Ordering::Release);
        self.control_tx
            .send(WriterControl::Shutdown)
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!("writer shutdown test control failed: {error}"),
            })?;
        permit.send(append);
        self.state.reserved_sends.fetch_sub(1, Ordering::AcqRel);
        stopped.await;
        Ok(())
    }
}

/// Pod-global registry of active tenant/table writers.
#[derive(Debug)]
pub(crate) struct TenantTableWriterRegistry {
    entries: Mutex<HashMap<TenantTableKey, Arc<TenantTableWriter>>>,
    admission: AdmissionController,
    memtable: Arc<Memtable>,
    wal: Arc<WalWriter>,
    post_ack_cpu: ScribePostAckCpuPool,
    wal_io: ScribeWalIoPool,
    persistence: Option<Arc<PersistenceRuntime>>,
    stream: StreamIdentity,
    coordination_runtime: Handle,
    drained: Notify,
    commit_config: crate::scribe::ScribeCommitConfig,
}

impl TenantTableWriterRegistry {
    #[cfg(test)]
    pub(crate) fn new(
        admission: AdmissionController,
        memtable: Arc<Memtable>,
        wal: Arc<WalWriter>,
        post_ack_cpu: ScribePostAckCpuPool,
        wal_io: ScribeWalIoPool,
    ) -> Arc<Self> {
        Self::new_with_runtime(
            admission,
            memtable,
            wal,
            post_ack_cpu,
            wal_io,
            WriterRegistryRuntime {
                coordination_runtime: Handle::current(),
                commit_config: crate::scribe::ScribeCommitConfig::default(),
                persistence: None,
                stream: StreamIdentity::new(
                    crate::scribe::stream_identity::NodeId::new(Uuid::nil()),
                    crate::scribe::stream_identity::WriterEpoch::new(0),
                ),
            },
        )
    }

    pub(crate) fn new_with_runtime(
        admission: AdmissionController,
        memtable: Arc<Memtable>,
        wal: Arc<WalWriter>,
        post_ack_cpu: ScribePostAckCpuPool,
        wal_io: ScribeWalIoPool,
        runtime: WriterRegistryRuntime,
    ) -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(HashMap::new()),
            admission,
            memtable,
            wal,
            post_ack_cpu,
            wal_io,
            persistence: runtime.persistence,
            stream: runtime.stream,
            coordination_runtime: runtime.coordination_runtime,
            drained: Notify::new(),
            commit_config: runtime.commit_config,
        })
    }

    pub(crate) fn get_or_create(
        &self,
        binding: TenantTableBinding,
    ) -> Result<(Arc<TenantTableWriter>, bool), ScribeError> {
        let key = (binding.tenant, binding.table_ref.clone());
        let mut entries = self.entries.lock().map_err(|error| ScribeError::Internal {
            detail: format!("writer registry lock poisoned: {error}"),
        })?;
        if let Some(writer) = entries.get(&key) {
            if writer.is_available() {
                return Ok((Arc::clone(writer), false));
            }
            return Err(ScribeError::IngestBusy {
                table: binding.table_ref.fqn(),
            });
        }

        // Hold the registry lock through lease acquisition and insertion. This
        // makes the first-write path one atomic get-or-create operation: racing
        // callers cannot each spawn an unregistered FIFO consumer.
        let lease = self.admission.try_reserve_writer(binding.table_ref.fqn())?;
        let writer = TenantTableWriter::new(
            binding,
            lease,
            WriterDependencies {
                admission: self.admission.clone(),
                memtable: Arc::clone(&self.memtable),
                wal: Arc::clone(&self.wal),
                post_ack_cpu: self.post_ack_cpu.clone(),
                wal_io: self.wal_io.clone(),
                persistence: self.persistence.clone(),
                stream: self.stream,
                commit_config: self.commit_config,
                coordination_runtime: self.coordination_runtime.clone(),
                writer_queue_items: self.admission.config().writer_queue_items,
                writer_idle_ttl: self.admission.config().writer_idle_ttl,
            },
        );
        entries.insert(key, Arc::clone(&writer));
        Ok((writer, true))
    }

    /// Try to admit one complete request into the writer's bounded FIFO queue.
    ///
    /// The sender permit and pending counter are reserved before the accepting
    /// flag is checked, which lets retirement distinguish an accepted race from
    /// a request that must be rejected.
    pub(crate) fn enqueue(
        writer: &Arc<TenantTableWriter>,
        append: AdmittedAppend,
    ) -> Result<(), ScribeError> {
        if !writer.is_available() {
            return Err(ScribeError::IngestBusy {
                table: writer.binding.table_ref.fqn(),
            });
        }

        let permit = writer
            .data_tx
            .try_reserve()
            .map_err(|_| {
                metrics::counter!("bifrost_scribe_admission_rejections_total", "reason" => "writer_queue_full")
                    .increment(1);
                ScribeError::IngestBusy {
                    table: writer.binding.table_ref.fqn(),
                }
            })?;

        writer.state.reserved_sends.fetch_add(1, Ordering::AcqRel);
        if !writer.state.accepting.load(Ordering::Acquire) {
            metrics::counter!("bifrost_scribe_admission_rejections_total", "reason" => "writer_unavailable")
                .increment(1);
            writer.state.reserved_sends.fetch_sub(1, Ordering::AcqRel);
            return Err(ScribeError::IngestBusy {
                table: writer.binding.table_ref.fqn(),
            });
        }
        writer.state.pending.fetch_add(1, Ordering::AcqRel);
        metrics::gauge!("bifrost_scribe_writer_queue_depth")
            .set(writer.pending().to_f64().unwrap_or(f64::MAX));

        permit.send(append);
        writer.state.reserved_sends.fetch_sub(1, Ordering::AcqRel);
        if let Ok(mut last_activity) = writer.state.last_activity.lock() {
            *last_activity = Instant::now();
        }
        let _ = writer.age_tx.send(Instant::now());
        Ok(())
    }

    pub(crate) fn remove_if(&self, key: &TenantTableKey, instance_id: Uuid) {
        if let Ok(mut entries) = self.entries.lock()
            && entries
                .get(key)
                .is_some_and(|writer| writer.instance_id == instance_id)
        {
            entries.remove(key);
        }
        self.drained.notify_waiters();
    }

    pub(crate) async fn drain(&self) {
        loop {
            let pending = self.entries.lock().map_or(0, |entries| {
                entries.values().map(|writer| writer.pending()).sum()
            });
            if pending == 0 {
                return;
            }
            tokio::select! {
                () = self.drained.notified() => {},
                () = tokio::time::sleep(std::time::Duration::from_millis(1)) => {},
            }
        }
    }

    pub(crate) fn writer_count(&self) -> usize {
        self.entries.lock().map_or(0, |entries| entries.len())
    }

    pub(crate) fn health_snapshot(&self) -> WriterHealthSnapshot {
        let entries = match self.entries.lock() {
            Ok(entries) => entries,
            Err(poisoned) => poisoned.into_inner(),
        };
        WriterHealthSnapshot {
            writers: entries.len(),
            unhealthy_writers: entries
                .values()
                .filter(|writer| !writer.is_healthy())
                .count(),
            pending_items: entries.values().map(|writer| writer.pending()).sum(),
            terminal_errors: entries
                .values()
                .map(|writer| writer.terminal_errors())
                .sum(),
        }
    }

    pub(crate) fn lane_snapshot(&self) -> crate::scribe::telemetry::ExecutorSnapshot {
        let post_ack = self.post_ack_cpu.snapshot();
        let wal_io = self.wal_io.snapshot();
        crate::scribe::telemetry::ExecutorSnapshot {
            depth: post_ack.depth.saturating_add(wal_io.depth),
            capacity: post_ack.capacity.saturating_add(wal_io.capacity),
            saturation_events: post_ack
                .saturation_events
                .saturating_add(wal_io.saturation_events),
            completed: post_ack.completed.saturating_add(wal_io.completed),
            failed: post_ack.failed.saturating_add(wal_io.failed),
            panicked: post_ack.panicked.saturating_add(wal_io.panicked),
        }
    }

    pub(crate) fn post_ack_snapshot(&self) -> crate::scribe::telemetry::ExecutorSnapshot {
        self.post_ack_cpu.snapshot()
    }

    pub(crate) fn wal_io_snapshot(&self) -> crate::scribe::telemetry::ExecutorSnapshot {
        self.wal_io.snapshot()
    }

    pub(crate) fn post_ack_cpu(&self) -> ScribePostAckCpuPool {
        self.post_ack_cpu.clone()
    }

    pub(crate) async fn shutdown(&self) {
        let writers = match self.entries.lock() {
            Ok(entries) => entries.values().cloned().collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        for writer in &writers {
            writer.close_admission();
        }
        self.drain().await;
        for writer in writers {
            let stopped = writer.state.stopped.notified();
            if writer.state.finished.load(Ordering::Acquire) {
                continue;
            }
            if writer.send_control(WriterControl::Shutdown).await.is_ok()
                && !writer.state.finished.load(Ordering::Acquire)
            {
                stopped.await;
            }
        }
    }

    /// Publish the latest lifecycle age value to every writer watch channel.
    pub(crate) fn publish_age(&self, now: Instant) {
        let writers = match self.entries.lock() {
            Ok(entries) => entries.values().cloned().collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        for writer in writers {
            let _ = writer.age_tx.send(now);
        }
    }

    /// Retire writers that have been idle for the lifecycle grace interval.
    pub(crate) async fn retire_idle(&self, now: Instant) {
        let writers = match self.entries.lock() {
            Ok(entries) => entries.values().cloned().collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        for writer in writers {
            if !writer.is_idle(now) || self.memtable.has_state_for_table(&writer.key) {
                continue;
            }
            let stopped = writer.state.stopped.notified();
            if writer
                .state
                .accepting
                .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }
            if writer.send_control(WriterControl::Shutdown).await.is_err() {
                self.remove_if(&writer.key, writer.instance_id);
                continue;
            }
            stopped.await;
            self.remove_if(&writer.key, writer.instance_id);
        }
    }
}

/// Point-in-time health and queue metrics for active logical writers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterHealthSnapshot {
    /// Active tenant/table consumers.
    pub writers: usize,
    /// Consumers that have failed after admission.
    pub unhealthy_writers: usize,
    /// Items retained in writer data queues or being processed.
    pub pending_items: usize,
    /// Terminal post-ACK consumer errors.
    pub terminal_errors: usize,
}

struct WriterLoopState {
    seen_slices: HashSet<AppendSliceId>,
    wal_handles: HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
    wal_bytes: HashMap<crate::scribe::seal_key::SealKey, usize>,
    pending_jobs: Vec<PersistenceJob>,
    retained: Option<AdmittedAppend>,
    group: Vec<AdmittedAppend>,
    max_frames: usize,
}

impl WriterLoopState {
    fn new(max_frames: usize) -> Self {
        Self {
            seen_slices: HashSet::new(),
            wal_handles: HashMap::new(),
            wal_bytes: HashMap::new(),
            pending_jobs: Vec::new(),
            retained: None,
            group: Vec::with_capacity(max_frames),
            max_frames,
        }
    }
}

/// Run one writer's control and data queues on the dedicated coordination runtime.
async fn run_writer(
    runtime: WriterRuntime,
    mut data_rx: mpsc::Receiver<AdmittedAppend>,
    mut control_rx: mpsc::Receiver<WriterControl>,
    mut age_rx: watch::Receiver<Instant>,
    control_tx: mpsc::Sender<WriterControl>,
) {
    let mut controls_closed = false;
    let mut shutdown_requested = false;
    let max_frames = runtime.commit_config.max_group_frames;
    let max_bytes = runtime.commit_config.max_group_bytes;
    let mut state = WriterLoopState::new(max_frames);

    loop {
        if shutdown_requested
            && runtime.state.pending.load(Ordering::Acquire) == 0
            && runtime.state.reserved_sends.load(Ordering::Acquire) == 0
        {
            let _ = rotate_all(
                &runtime,
                &state.wal_handles,
                &mut state.wal_bytes,
                &mut state.pending_jobs,
                &control_tx,
            )
            .await;
            submit_pending(&runtime, &mut state.pending_jobs);
            break;
        }
        let first = if state.retained.is_some() {
            state.retained.take()
        } else {
            tokio::select! {
                biased;
                control = control_rx.recv(), if !controls_closed => {
                    match control {
                        Some(WriterControl::Shutdown) => {
                            runtime.state.accepting.store(false, Ordering::Release);
                            shutdown_requested = true;
                        }
                        Some(control) => {
                            process_control(&runtime, control, &control_tx).await;
                        }
                        None => controls_closed = true,
                    }
                    None
                }
                _ = age_rx.changed() => {
                    let now = *age_rx.borrow();
                    let _ = rotate_expired(
                        &runtime,
                        now,
                        &state.wal_handles,
                        &mut state.wal_bytes,
                        &mut state.pending_jobs,
                        &control_tx,
                    )
                    .await;
                    submit_pending(&runtime, &mut state.pending_jobs);
                    None
                }
                append = data_rx.recv() => append,
            }
        };
        let Some(first) = first else {
            if controls_closed && data_rx.is_empty() {
                break;
            }
            continue;
        };

        if !process_group_and_rotate(
            &runtime,
            &mut data_rx,
            first,
            &mut state,
            max_bytes,
            &control_tx,
        )
        .await
        {
            break;
        }
    }
    runtime.state.finished.store(true, Ordering::Release);
    runtime.state.stopped.notify_waiters();
}

async fn process_group_and_rotate(
    runtime: &WriterRuntime,
    data_rx: &mut mpsc::Receiver<AdmittedAppend>,
    first: AdmittedAppend,
    state: &mut WriterLoopState,
    max_bytes: usize,
    control_tx: &mpsc::Sender<WriterControl>,
) -> bool {
    let group_bytes = fill_group(
        data_rx,
        first,
        &mut state.retained,
        &mut state.group,
        state.max_frames,
        max_bytes,
    );
    let group_len = state.group.len();
    let result = process_group(
        runtime,
        &mut state.group,
        group_bytes,
        &mut state.seen_slices,
        &mut state.wal_handles,
        &mut state.wal_bytes,
    )
    .await;
    for _ in 0..group_len {
        runtime.state.pending.fetch_sub(1, Ordering::AcqRel);
    }
    metrics::gauge!("bifrost_scribe_writer_queue_depth").set(
        runtime
            .state
            .pending
            .load(Ordering::Acquire)
            .to_f64()
            .unwrap_or(f64::MAX),
    );
    if let Err(error) = result {
        runtime.state.healthy.store(false, Ordering::Release);
        runtime.state.terminal_errors.fetch_add(1, Ordering::AcqRel);
        if matches!(&error, ScribeError::WalDiskFull) {
            runtime.admission.trip_wal_disk_full();
        }
        tracing::error!(error = %error, group_frames = group_len, group_bytes, stage = "wal_group", "Scribe writer became unhealthy");
        metrics::counter!("bifrost_scribe_writer_groups_total", "status" => "failed").increment(1);
        metrics::counter!("bifrost_scribe_writer_unhealthy_total", "reason" => "wal_group")
            .increment(1);
        while let Ok(queued) = data_rx.try_recv() {
            drop(queued);
            runtime.state.pending.fetch_sub(1, Ordering::AcqRel);
        }
        if state.retained.take().is_some() {
            runtime.state.pending.fetch_sub(1, Ordering::AcqRel);
        }
        return false;
    }
    for seal_key in append_stats_keys(runtime, &state.wal_bytes) {
        let due = state.wal_bytes.get(&seal_key).copied().unwrap_or(0)
            >= crate::scribe::memtable::WAL_ROTATION_BYTES
            || runtime.memtable.should_seal(&seal_key).unwrap_or(false);
        if due {
            let _ = rotate_one(
                runtime,
                &seal_key,
                &state.wal_handles,
                &mut state.wal_bytes,
                &mut state.pending_jobs,
                control_tx,
            )
            .await;
        }
    }
    submit_pending(runtime, &mut state.pending_jobs);
    true
}

fn append_stats_keys(
    _runtime: &WriterRuntime,
    wal_bytes: &HashMap<crate::scribe::seal_key::SealKey, usize>,
) -> Vec<crate::scribe::seal_key::SealKey> {
    wal_bytes.keys().cloned().collect()
}

fn submit_pending(runtime: &WriterRuntime, pending_jobs: &mut Vec<PersistenceJob>) {
    let Some(persistence) = &runtime.persistence else {
        pending_jobs.clear();
        return;
    };
    let mut remaining = Vec::with_capacity(pending_jobs.len());
    for job in pending_jobs.drain(..) {
        if let Err(job) = persistence.try_submit(job) {
            remaining.push(*job);
        }
    }
    *pending_jobs = remaining;
}

async fn rotate_expired(
    runtime: &WriterRuntime,
    now: Instant,
    wal_handles: &HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
    wal_bytes: &mut HashMap<crate::scribe::seal_key::SealKey, usize>,
    pending_jobs: &mut Vec<PersistenceJob>,
    control_tx: &mpsc::Sender<WriterControl>,
) -> Result<(), ScribeError> {
    let keys = runtime
        .memtable
        .expired_seal_keys_for_table(&runtime.writer_key, now)?;
    for seal_key in keys {
        rotate_one(
            runtime,
            &seal_key,
            wal_handles,
            wal_bytes,
            pending_jobs,
            control_tx,
        )
        .await?;
    }
    Ok(())
}

async fn rotate_all(
    runtime: &WriterRuntime,
    wal_handles: &HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
    wal_bytes: &mut HashMap<crate::scribe::seal_key::SealKey, usize>,
    pending_jobs: &mut Vec<PersistenceJob>,
    control_tx: &mpsc::Sender<WriterControl>,
) -> Result<(), ScribeError> {
    let mut keys = runtime.memtable.seal_keys_for_table(&runtime.writer_key)?;
    keys.extend(wal_bytes.keys().cloned());
    keys.sort_by_key(ToString::to_string);
    keys.dedup();
    for seal_key in keys {
        if runtime.memtable.has_writable(&seal_key)? {
            rotate_one(
                runtime,
                &seal_key,
                wal_handles,
                wal_bytes,
                pending_jobs,
                control_tx,
            )
            .await?;
        }
    }
    Ok(())
}

async fn rotate_one(
    runtime: &WriterRuntime,
    seal_key: &crate::scribe::seal_key::SealKey,
    wal_handles: &HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
    wal_bytes: &mut HashMap<crate::scribe::seal_key::SealKey, usize>,
    pending_jobs: &mut Vec<PersistenceJob>,
    control_tx: &mpsc::Sender<WriterControl>,
) -> Result<(), ScribeError> {
    if !runtime.memtable.has_writable(seal_key)? {
        return Ok(());
    }
    if runtime.memtable.immutable_count(seal_key)? >= 2 {
        runtime.state.accepting.store(false, Ordering::Release);
        metrics::counter!("bifrost_scribe_admission_rejections_total", "reason" => "immutable_bound")
            .increment(1);
        return Err(ScribeError::IngestBusy {
            table: seal_key.table.fqn(),
        });
    }

    let wal = wal_handles.get(seal_key).cloned();
    let wal_segments = if let Some(wal) = &wal {
        if let Some(segment) = wal.current_segment()? {
            match runtime
                .wal_io
                .submit(ScribeWalIoOp::SyncWal {
                    segments: vec![Arc::clone(&segment)],
                })
                .await?
            {
                ScribeWalIoResult::WalSynced => {}
                _ => {
                    return Err(ScribeError::Internal {
                        detail: "WAL IO lane returned the wrong rotation sync result".to_owned(),
                    });
                }
            }
            vec![segment.reference()]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    let Some(wal) = wal else {
        return Ok(());
    };
    let frozen = runtime.memtable.freeze(seal_key)?;
    runtime
        .admission
        .transfer_active_to_immutable(frozen.arrow_bytes);
    let generation = Arc::new(ImmutableGeneration::from_frozen(
        &frozen,
        runtime.writer_key.clone(),
        runtime.instance_id,
        runtime.stream,
        wal_segments,
        wal,
    ));
    wal_bytes.insert(seal_key.clone(), 0);
    metrics::histogram!("bifrost_scribe_generation_swap_seconds").record(
        generation
            .closed_at
            .duration_since(generation.opened_at)
            .as_secs_f64(),
    );

    let Some(persistence) = &runtime.persistence else {
        return Ok(());
    };
    let job = PersistenceJob {
        generation,
        binding: runtime.binding.clone(),
        completion_tx: control_tx.clone(),
    };
    if let Err(job) = persistence.try_submit(job) {
        pending_jobs.push(*job);
    }
    Ok(())
}

async fn process_control(
    runtime: &WriterRuntime,
    control: WriterControl,
    control_tx: &mpsc::Sender<WriterControl>,
) {
    match control {
        WriterControl::Shutdown => {
            runtime.state.stopping.store(true, Ordering::Release);
            runtime.state.accepting.store(false, Ordering::Release);
        }
        WriterControl::PersistenceComplete(completion) => {
            if completion.writer_instance_id != runtime.instance_id
                || completion.seal_key.table != runtime.writer_key.1
                || completion.seal_key.tenant != runtime.writer_key.0
            {
                metrics::counter!("bifrost_scribe_stale_completion_total").increment(1);
                return;
            }
            if let Some(error) = completion.error {
                tracing::warn!(
                    writer_instance_id = %completion.writer_instance_id,
                    generation_id = completion.generation_id.0,
                    error,
                    "immutable generation persistence failed; retaining WAL and rows"
                );
                return;
            }
            let Some(file_list_key) = completion.file_list_key.clone() else {
                return;
            };
            if let Err(error) = runtime
                .memtable
                .complete_post_commit(completion.generation_id.0, file_list_key)
            {
                tracing::warn!(error = %error, "persistence completion could not mark generation published");
                return;
            }
            let tx = control_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(crate::scribe::memtable::ACTIVE_GENERATION_MAX_AGE / 10).await;
                let _ = tx.send(WriterControl::GraceExpired(completion)).await;
            });
        }
        WriterControl::GraceExpired(completion) => {
            if completion.writer_instance_id != runtime.instance_id {
                metrics::counter!("bifrost_scribe_stale_completion_total").increment(1);
                return;
            }
            let now = Instant::now();
            match runtime
                .memtable
                .retire_generation_at(completion.generation_id.0, now)
            {
                Ok(Some(_)) => {
                    runtime.admission.release_immutable(completion.arrow_bytes);
                    let grace_seconds = now
                        .saturating_duration_since(completion.published_at)
                        .as_secs_f64();
                    metrics::histogram!("bifrost_scribe_generation_grace_seconds")
                        .record(grace_seconds);
                    if !runtime.state.stopping.load(Ordering::Acquire)
                        && runtime.state.healthy.load(Ordering::Acquire)
                    {
                        runtime.state.accepting.store(true, Ordering::Release);
                    }
                    let _ = runtime
                        .wal_io
                        .submit(ScribeWalIoOp::RetireWal {
                            wal: completion.wal,
                            segments: completion.wal_segments,
                        })
                        .await;
                    metrics::histogram!("bifrost_scribe_wal_retirement_delay_seconds")
                        .record(grace_seconds);
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(error = %error, "immutable grace retirement failed"),
            }
        }
    }
}

fn fill_group(
    data_rx: &mut mpsc::Receiver<AdmittedAppend>,
    first: AdmittedAppend,
    retained: &mut Option<AdmittedAppend>,
    group: &mut Vec<AdmittedAppend>,
    max_frames: usize,
    max_bytes: usize,
) -> usize {
    group.clear();
    let mut group_bytes = first.admitted_bytes;
    group.push(first);
    while group.len() < max_frames {
        match data_rx.try_recv() {
            Ok(next) if group.len() == 1 && next.admitted_bytes > max_bytes => {
                *retained = Some(next);
                break;
            }
            Ok(next) if group_bytes.saturating_add(next.admitted_bytes) <= max_bytes => {
                group_bytes = group_bytes.saturating_add(next.admitted_bytes);
                group.push(next);
            }
            Ok(next) => {
                *retained = Some(next);
                break;
            }
            Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => {
                break;
            }
        }
    }
    group_bytes
}

/// Prepare, append, and sync one opportunistic FIFO group.
async fn process_group(
    runtime: &WriterRuntime,
    group: &mut Vec<AdmittedAppend>,
    group_bytes: usize,
    seen_slices: &mut HashSet<AppendSliceId>,
    wal_handles: &mut HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
    wal_bytes: &mut HashMap<crate::scribe::seal_key::SealKey, usize>,
) -> Result<(), ScribeError> {
    let span = tracing::info_span!(
        "scribe_wal_group",
        writer_identity = %runtime.writer_identity,
        frame_count = group.len(),
        group_bytes,
        append_duration_us = tracing::field::Empty,
        sync_duration_us = tracing::field::Empty,
        touched_segment_count = tracing::field::Empty,
    );
    let _entered = span.enter();
    let mut prepared = prepare_group(runtime, group).await?;

    let append_stats = append_group(runtime, &mut prepared, seen_slices, wal_handles).await?;
    for (seal_key, bytes) in append_stats.by_key {
        let entry = wal_bytes.entry(seal_key).or_default();
        *entry = entry.saturating_add(bytes);
    }
    let touched = append_stats.touched;
    let durable_frames = append_stats.durable_frames;
    let durable_rows = append_stats.durable_rows;
    let wal_bytes = append_stats.wal_bytes;
    let append_duration = append_stats.elapsed;
    span.record("append_duration_us", append_duration.as_micros());
    let sync_started = Instant::now();
    for (_, segment) in touched.values() {
        match runtime
            .wal_io
            .submit(ScribeWalIoOp::SyncWal {
                segments: vec![Arc::clone(segment)],
            })
            .await?
        {
            ScribeWalIoResult::WalSynced => {}
            _ => {
                return Err(ScribeError::Internal {
                    detail: "WAL IO lane returned the wrong sync result".to_owned(),
                });
            }
        }
    }
    let sync_duration = sync_started.elapsed();
    span.record("sync_duration_us", sync_duration.as_micros());
    span.record("touched_segment_count", touched.len());
    metrics::counter!("bifrost_scribe_writer_groups_total", "status" => "completed").increment(1);
    metrics::histogram!("bifrost_scribe_writer_group_frames")
        .record(durable_frames.to_f64().unwrap_or(f64::MAX));
    metrics::histogram!("bifrost_scribe_writer_group_bytes")
        .record(group_bytes.to_f64().unwrap_or(f64::MAX));
    metrics::histogram!("bifrost_scribe_wal_append_seconds").record(append_duration.as_secs_f64());
    metrics::histogram!("bifrost_scribe_wal_sync_seconds").record(sync_duration.as_secs_f64());
    metrics::counter!("bifrost_scribe_wal_fsync_total", "status" => "completed")
        .increment(touched.len() as u64);
    metrics::counter!("bifrost_scribe_wal_bytes_total").increment(wal_bytes);
    metrics::counter!("bifrost_scribe_frames_total", "status" => "fsynced")
        .increment(durable_frames);
    metrics::counter!("bifrost_scribe_rows_total", "status" => "fsynced").increment(durable_rows);
    for prepared_append in prepared {
        drop(prepared_append.reservation);
    }
    Ok(())
}

async fn prepare_group(
    runtime: &WriterRuntime,
    group: &mut Vec<AdmittedAppend>,
) -> Result<Vec<crate::scribe::preprocess::PreparedAppend>, ScribeError> {
    let mut prepared = Vec::with_capacity(group.len());
    for append in group.drain(..) {
        let prepared_append = match runtime
            .post_ack_cpu
            .submit(ScribePostAckCpuOp::Preprocess(Box::new(append)))
            .await?
        {
            ScribePostAckCpuResult::Prepared(prepared) => prepared,
            ScribePostAckCpuResult::ParquetEncoded(_) | ScribePostAckCpuResult::ReplayRestored => {
                return Err(ScribeError::Internal {
                    detail: "post-ACK lane returned the wrong writer result".to_owned(),
                });
            }
        };
        prepared.push(prepared_append);
    }
    Ok(prepared)
}

struct AppendGroupStats {
    touched: HashMap<std::path::PathBuf, (WalHandle, Arc<crate::scribe::wal::WalSegment>)>,
    durable_frames: u64,
    durable_rows: u64,
    wal_bytes: u64,
    by_key: HashMap<crate::scribe::seal_key::SealKey, usize>,
    elapsed: std::time::Duration,
}

async fn append_group(
    runtime: &WriterRuntime,
    prepared: &mut Vec<crate::scribe::preprocess::PreparedAppend>,
    seen_slices: &mut HashSet<AppendSliceId>,
    wal_handles: &mut HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
) -> Result<AppendGroupStats, ScribeError> {
    let started = Instant::now();
    let mut stats = AppendGroupStats {
        touched: HashMap::new(),
        durable_frames: 0,
        durable_rows: 0,
        wal_bytes: 0,
        by_key: HashMap::new(),
        elapsed: std::time::Duration::ZERO,
    };
    for prepared_append in prepared {
        let batch_id = *prepared_append.batch_id.as_bytes();
        if append_prepared(
            runtime,
            prepared_append,
            batch_id,
            seen_slices,
            wal_handles,
            &mut stats,
        )
        .await?
        {
            stats.durable_frames = stats.durable_frames.saturating_add(1);
        }
    }
    stats.elapsed = started.elapsed();
    Ok(stats)
}

async fn append_prepared(
    runtime: &WriterRuntime,
    prepared_append: &mut crate::scribe::preprocess::PreparedAppend,
    batch_id: [u8; 16],
    seen_slices: &mut HashSet<AppendSliceId>,
    wal_handles: &mut HashMap<crate::scribe::seal_key::SealKey, WalHandle>,
    stats: &mut AppendGroupStats,
) -> Result<bool, ScribeError> {
    let slices = std::mem::take(&mut prepared_append.slices);
    let mut frame_had_work = false;
    for slice in slices {
        if seen_slices.contains(&slice.id) {
            continue;
        }
        if seen_slices.len() >= MAX_DEDUPE_SLICES {
            metrics::counter!("bifrost_scribe_admission_rejections_total", "reason" => "dedupe_capacity")
                .increment(1);
            return Err(ScribeError::IngestBusy {
                table: slice.seal_key.table.fqn(),
            });
        }
        seen_slices.insert(slice.id.clone());
        metrics::gauge!("bifrost_scribe_dedupe_bytes").set(
            seen_slices
                .len()
                .saturating_mul(256)
                .to_f64()
                .unwrap_or(f64::MAX),
        );
        frame_had_work = true;
        let seal_key = slice.seal_key.clone();
        let wal_handle = if let Some(handle) = wal_handles.get(&seal_key) {
            handle.clone()
        } else {
            let handle = runtime.wal.handle_for_seal_key(seal_key.clone())?;
            wal_handles.insert(seal_key.clone(), handle.clone());
            handle
        };
        let PreparedSlice {
            audit_event,
            rows,
            wal_append,
            memtable_bytes,
            ..
        } = slice;
        let ScribeWalIoResult::WalWritten { wal, result } = runtime
            .wal_io
            .submit(ScribeWalIoOp::WritePrepared {
                wal: wal_handle,
                append: wal_append,
            })
            .await?
        else {
            return Err(ScribeError::Internal {
                detail: "WAL IO lane returned the wrong write result".to_owned(),
            });
        };
        let lsn = result.lsn;
        stats.wal_bytes = stats.wal_bytes.saturating_add(result.encoded_bytes);
        let key_bytes = usize::try_from(result.encoded_bytes).unwrap_or(usize::MAX);
        let entry = stats.by_key.entry(seal_key.clone()).or_default();
        *entry = entry.saturating_add(key_bytes);
        for segment in result.touched_segments {
            stats
                .touched
                .insert(segment.path().to_path_buf(), (wal.clone(), segment));
        }
        let row_count = rows.num_rows();
        stats.durable_rows = stats
            .durable_rows
            .saturating_add(u64::try_from(row_count).unwrap_or(u64::MAX));
        runtime
            .admission
            .try_reserve_active(seal_key.table.fqn(), memtable_bytes)?;
        if let Err(error) = runtime.memtable.insert(
            &seal_key,
            audit_event,
            ScribeAppendMeta {
                batch_id,
                rows_accepted: row_count,
                wal_lsn_min: lsn,
                wal_lsn_max: lsn,
                seal_key: seal_key.as_path_components(),
            },
            rows,
        ) {
            runtime.admission.release_active(memtable_bytes);
            return Err(error);
        }
    }
    Ok(frame_had_work)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{ArrayRef, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::record_batch::RecordBatch;
    use std::path::Path;
    use tempfile::TempDir;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

    use crate::catalog::{TableRef, TenantTableBinding};
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::admission::AdmissionConfig;
    use crate::scribe::execution_lanes::{ScribePostAckCpuPool, ScribeWalIoPool};
    use crate::scribe::wal::WalConfig;

    fn wal_root(temp_dir: &TempDir) -> Arc<WalWriter> {
        Arc::new(
            WalWriter::new(
                Path::new(temp_dir.path()),
                [0_u8; 16],
                1,
                crate::test_support::tenant(),
                WalConfig::default(),
            )
            .expect("wal"),
        )
    }

    #[tokio::test]
    async fn writer_processes_one_append_in_fifo_order() {
        let temp_dir = TempDir::new().expect("temp dir");
        let memtable = Arc::new(Memtable::new());
        let admission = AdmissionController::with_config(AdmissionConfig::default());
        let post_ack_cpu = ScribePostAckCpuPool::new(1);
        let wal_io = ScribeWalIoPool::new(1);
        let registry = TenantTableWriterRegistry::new(
            admission.clone(),
            Arc::clone(&memtable),
            wal_root(&temp_dir),
            post_ack_cpu,
            wal_io,
        );
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let binding = TenantTableBinding::resolve((crate::test_support::tenant(), table.clone()))
            .expect("binding");
        let (writer, _) = registry.get_or_create(binding).expect("writer");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        )]));
        let rows = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(TimestampMicrosecondArray::from(vec![1_721_003_400_000_000])) as ArrayRef,
            ],
        )
        .expect("rows");
        let reservation = admission.try_reserve(table.fqn(), 1).expect("admission");
        TenantTableWriterRegistry::enqueue(
            &writer,
            AdmittedAppend {
                batch_id: Uuid::now_v7(),
                frame_sequence: 0,
                audit_event: AuditEvent {
                    request_id: RequestId::now_v7(),
                    trace_id: None,
                    operation: "bifrost.append".to_string(),
                    resource: table.fqn(),
                    card_ref: None,
                    principal_id: PrincipalId::new(Uuid::new_v4()),
                    principal_kind: PrincipalKindTag::User,
                    auth_method: AuthMethod::Jwt,
                    permission: "bifrost:append".to_string(),
                    decision: AuditDecision::Allow,
                    result: AuditResult::Success,
                    payload_summary: "1 rows".to_string(),
                    detail: None,
                },
                rows,
                measured_wire_bytes: 0,
                admitted_bytes: 1,
                reservation,
                tenant: crate::test_support::tenant(),
                table: table.clone(),
                queued_at: Instant::now(),
            },
        )
        .expect("enqueue");
        registry.drain().await;

        let key = crate::scribe::seal_key::SealKey::new(
            crate::test_support::tenant(),
            table,
            crate::scribe::seal_key::EventDay::new(
                chrono::NaiveDate::from_ymd_opt(2024, 7, 15).expect("date"),
            ),
        );
        assert_eq!(memtable.row_count(&key).expect("row count"), 1);
        assert_eq!(admission.snapshot().items, 0);
    }
}
