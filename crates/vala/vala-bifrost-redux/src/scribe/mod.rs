//! Scribe implementation — WAL append, fsync, replay, and memtable (/).

pub mod admission;
pub mod audit_envelope;
pub mod execution_lanes;
pub mod file_list_writer;
pub mod filename;
pub mod manifest;
pub mod memtable;
pub mod parquet_writer;
pub mod preprocess;
pub mod registry;
pub mod replay;
pub mod seal;
pub mod seal_key;
pub mod stream_identity;
pub mod tail_rpc;
pub mod telemetry;
pub mod wal;
pub mod writer;

#[cfg(test)]
mod acceptance;

use crate::catalog::TenantTableBinding;
pub use crate::contracts::ScribeAppend;
use crate::contracts::{FrameAdmission, Scribe, ScribeError, ScribeIngressFrame};
use crate::scribe::admission::{
    AdmissionConfig, AdmissionController, IngressQueueBudget, IngressQueueConfig,
    IngressQueueReservation, MAX_REQUEST_BYTES, REQUEST_OVERHEAD_BYTES,
};
use crate::scribe::execution_lanes::{ScribeIngressCpuPool, ScribePostAckCpuPool, ScribeWalIoPool};
use crate::scribe::memtable::Memtable;
use crate::scribe::preprocess::AdmittedAppend;
use crate::scribe::seal_key::SealKey;
use crate::scribe::tail_rpc::{FetchLiveTailRequest, FetchLiveTailService, TailFrame};
use crate::scribe::telemetry::{ScribeRuntimeSnapshot, ScribeTelemetry};
use crate::scribe::writer::TenantTableWriterRegistry;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};
use vala_sql::TenantConn;

/// Memtable key for per-bucket row-count inspection.
///
/// Type alias for [`SealKey`] — memtable buckets are keyed by
/// (`tenant`, `table`, `event_day`).
pub type MemtableKey = SealKey;

/// Explicit boot-time execution-lane provisioning for Scribe.
#[derive(Debug, Clone, Copy)]
pub struct ScribeLaneConfig {
    /// Rayon workers reserved for pre-ACK decode, validation, and projection.
    pub ingress_cpu_threads: usize,
    /// Rayon workers reserved for post-ACK splitting and serialization.
    pub post_ack_cpu_threads: usize,
    /// Rayon workers reserved for WAL filesystem operations.
    pub wal_io_threads: usize,
    /// Bounded ingress CPU submissions.
    pub ingress_queue_items: usize,
    /// Bounded raw ingress bytes.
    pub ingress_queue_bytes: usize,
    /// Bounded post-ACK CPU submissions.
    pub post_ack_queue_items: usize,
    /// Bounded WAL IO submissions.
    pub wal_io_queue_items: usize,
}

impl Default for ScribeLaneConfig {
    fn default() -> Self {
        Self::resolved()
    }
}

impl ScribeLaneConfig {
    /// Resolve the boot defaults from the host's available parallelism.
    #[must_use]
    pub fn resolved() -> Self {
        let available = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
        let cpu_budget = available.saturating_sub(2).max(2);
        let ingress_cpu_threads = (cpu_budget / 3).max(1);
        Self {
            ingress_cpu_threads,
            post_ack_cpu_threads: cpu_budget.saturating_sub(ingress_cpu_threads).max(1),
            wal_io_threads: 4,
            ingress_queue_items: 256,
            ingress_queue_bytes: 512 * 1024 * 1024,
            post_ack_queue_items: 64,
            wal_io_queue_items: 256,
        }
    }
}

/// Scribe implementation with bounded admission, FIFO writers, WAL, memtable, and seal.
///
/// `append` admits a complete request to a bounded writer queue. The writer
/// consumer performs event-day splitting, serialization, WAL writes, memtable
/// insertion, and `sync_data` in order. Seal predicate triggers freeze at first-of:
/// 50k rows | 1s | 128 MiB | 5s inactivity. Seal state machine executes:
/// Freeze → Parquet → PUT → PG tx (`file_list` + audit) → manifest → retire WAL.
#[derive(Debug)]
pub struct ScribeImpl {
    /// In-memory memtable keyed by seal-key.
    memtable: Arc<Memtable>,
    /// Opendal operator for object store (shared across seal drivers).
    operator: Arc<opendal::Operator>,
    /// WAL writer for durable append fsync.
    wal: Arc<wal::WalWriter>,
    /// Pod identity (`node_id`, `writer_epoch`).
    node_id: String,
    writer_epoch: i64,
    /// Pod-global request and writer admission counters.
    admission: AdmissionController,
    /// Bounded WAL IO lane retained for recovery and writer execution.
    wal_io: ScribeWalIoPool,
    /// Bounded Rayon lane for pre-ACK native decode and projection work.
    ingress_cpu: ScribeIngressCpuPool,
    /// Atomic get-or-create registry for tenant/table FIFO writers.
    registry: Arc<TenantTableWriterRegistry>,
    /// Bounded global raw-ingress dispatcher.
    ingress_queue: ScribeIngressQueue,
    /// Optional stage recorder used by benchmark and journey harnesses.
    telemetry: Option<Arc<ScribeTelemetry>>,
}

struct ScribeBuildConfig {
    admission: AdmissionConfig,
    telemetry: Option<Arc<ScribeTelemetry>>,
    coordination_runtime: Handle,
    post_ack_cpu: ScribePostAckCpuPool,
    wal_io: ScribeWalIoPool,
    ingress_cpu: ScribeIngressCpuPool,
    ingress_queue_items: usize,
    ingress_queue_bytes: usize,
}

struct IngressJob {
    frame: ScribeIngressFrame,
    reservation: IngressQueueReservation,
    response: oneshot::Sender<Result<FrameAdmission, ScribeError>>,
}

#[derive(Debug)]
struct ScribeIngressQueue {
    sender: mpsc::Sender<IngressJob>,
    budget: IngressQueueBudget,
    telemetry: Arc<Mutex<Option<Arc<ScribeTelemetry>>>>,
    _task: tokio::task::JoinHandle<()>,
}

impl ScribeIngressQueue {
    fn new(
        items: usize,
        bytes: usize,
        admission: AdmissionController,
        ingress_cpu: ScribeIngressCpuPool,
        registry: Arc<TenantTableWriterRegistry>,
        telemetry: Option<Arc<ScribeTelemetry>>,
        coordination_runtime: Handle,
    ) -> Self {
        let capacity = items.max(1);
        let budget = IngressQueueBudget::with_config(IngressQueueConfig {
            max_items: capacity,
            max_bytes: bytes,
        });
        let telemetry_slot = Arc::new(Mutex::new(telemetry));
        let task_telemetry = Arc::clone(&telemetry_slot);
        let (sender, mut receiver) = mpsc::channel(capacity);
        let task = coordination_runtime.spawn(async move {
            while let Some(job) = receiver.recv().await {
                let IngressJob {
                    frame,
                    mut reservation,
                    response,
                } = job;
                let telemetry = task_telemetry
                    .lock()
                    .ok()
                    .and_then(|telemetry| telemetry.clone());
                let result = process_ingress_frame(
                    frame,
                    &mut reservation,
                    &admission,
                    &ingress_cpu,
                    &registry,
                    telemetry.as_ref(),
                )
                .await;
                let _ = response.send(result);
            }
        });
        Self {
            sender,
            budget,
            telemetry: telemetry_slot,
            _task: task,
        }
    }

    fn set_telemetry(&self, telemetry: Arc<ScribeTelemetry>) {
        if let Ok(mut slot) = self.telemetry.lock() {
            *slot = Some(telemetry);
        }
    }

    async fn submit(&self, frame: ScribeIngressFrame) -> Result<FrameAdmission, ScribeError> {
        if frame.measured_wire_bytes > MAX_REQUEST_BYTES {
            return Err(ScribeError::PayloadTooLarge {
                bytes: frame.measured_wire_bytes,
            });
        }
        let table = frame.binding.table_ref.fqn();
        let bytes = frame
            .measured_wire_bytes
            .saturating_add(REQUEST_OVERHEAD_BYTES);
        let reservation = self.budget.try_reserve(table.clone(), bytes)?;
        let (response, result) = oneshot::channel();
        match self.sender.try_send(IngressJob {
            frame,
            reservation,
            response,
        }) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                return Err(ScribeError::IngestBusy { table });
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                return Err(ScribeError::Internal {
                    detail: "ingress dispatcher is closed".to_owned(),
                });
            }
        }
        result.await.map_err(|_| ScribeError::Internal {
            detail: "ingress dispatcher dropped its frame result".to_owned(),
        })?
    }
}

impl ScribeImpl {
    /// Construct a new `ScribeImpl` with empty memtable and provided dependencies.
    pub fn new_with_deps(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
    ) -> Self {
        Self::new_with_runtime(operator, wal, node_id, writer_epoch, Handle::current())
    }

    /// Construct a Scribe using an explicitly owned Tokio coordination runtime.
    ///
    /// Server deployments pass the handle for their dedicated WAL coordination
    /// runtime here. Tests and embedded callers can use [`Self::new_with_deps`]
    /// to schedule consumers on the current runtime.
    pub fn new_with_runtime(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        coordination_runtime: Handle,
    ) -> Self {
        Self::new_with_runtime_config(
            operator,
            wal,
            node_id,
            writer_epoch,
            ScribeLaneConfig::default(),
            coordination_runtime,
        )
    }

    /// Construct Scribe with explicitly provisioned execution lanes.
    pub fn new_with_runtime_config(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        lane_config: ScribeLaneConfig,
        coordination_runtime: Handle,
    ) -> Self {
        Self::new_with_runtime_config_and_admission(
            operator,
            wal,
            node_id,
            writer_epoch,
            lane_config,
            AdmissionConfig::default(),
            coordination_runtime,
        )
    }

    /// Construct Scribe with explicit execution lanes and admission bounds.
    pub fn new_with_runtime_config_and_admission(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        lane_config: ScribeLaneConfig,
        admission: AdmissionConfig,
        coordination_runtime: Handle,
    ) -> Self {
        Self::new_with_components(
            operator,
            wal,
            node_id,
            writer_epoch,
            ScribeBuildConfig {
                admission,
                telemetry: None,
                coordination_runtime,
                post_ack_cpu: ScribePostAckCpuPool::new_with_capacity(
                    lane_config.post_ack_cpu_threads,
                    lane_config.post_ack_queue_items,
                ),
                wal_io: ScribeWalIoPool::new_with_capacity(
                    lane_config.wal_io_threads,
                    lane_config.wal_io_queue_items,
                ),
                ingress_cpu: ScribeIngressCpuPool::new_with_capacity(
                    lane_config.ingress_cpu_threads,
                    lane_config.ingress_queue_items,
                ),
                ingress_queue_items: lane_config.ingress_queue_items,
                ingress_queue_bytes: lane_config.ingress_queue_bytes,
            },
        )
    }

    fn new_with_components(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        config: ScribeBuildConfig,
    ) -> Self {
        let memtable = Arc::new(Memtable::new());
        let admission = AdmissionController::with_config(config.admission);
        let ScribeBuildConfig {
            telemetry,
            coordination_runtime,
            post_ack_cpu,
            wal_io,
            ingress_cpu,
            ingress_queue_items,
            ingress_queue_bytes,
            ..
        } = config;
        let registry = TenantTableWriterRegistry::new_with_runtime(
            admission.clone(),
            Arc::clone(&memtable),
            Arc::clone(&wal),
            post_ack_cpu.clone(),
            wal_io.clone(),
            telemetry.clone(),
            coordination_runtime.clone(),
        );
        let ingress_queue = ScribeIngressQueue::new(
            ingress_queue_items,
            ingress_queue_bytes,
            admission.clone(),
            ingress_cpu.clone(),
            Arc::clone(&registry),
            telemetry.clone(),
            coordination_runtime,
        );
        Self {
            memtable,
            operator,
            wal,
            node_id,
            writer_epoch,
            admission,
            wal_io,
            ingress_cpu,
            registry,
            ingress_queue,
            telemetry,
        }
    }

    /// Construct a stub `ScribeImpl` for tests (memory backend, stub node identity, temp WAL).
    ///
    /// # Panics
    /// Panics if temp WAL directory or operator init fails.
    #[must_use]
    #[cfg(test)]
    pub fn new() -> Self {
        Self::new_with_test_lanes(ScribePostAckCpuPool::new(1), ScribeWalIoPool::new(1))
    }

    #[cfg(test)]
    pub(crate) fn new_with_test_lanes(
        post_ack_cpu: ScribePostAckCpuPool,
        wal_io: ScribeWalIoPool,
    ) -> Self {
        Self::new_with_test_config(post_ack_cpu, wal_io, AdmissionConfig::default())
    }

    #[cfg(test)]
    pub(crate) fn new_with_test_config(
        post_ack_cpu: ScribePostAckCpuPool,
        wal_io: ScribeWalIoPool,
        admission_config: AdmissionConfig,
    ) -> Self {
        let operator = Arc::new(
            opendal::Operator::new(opendal::services::Memory::default())
                .expect("memory backend init")
                .finish(),
        );

        let temp_dir = tempfile::tempdir().expect("temp WAL dir");
        let node_id_bytes = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000")
            .expect("valid UUID")
            .as_bytes()
            .to_owned();
        let wal = Arc::new(
            wal::WalWriter::new(
                temp_dir.path(),
                node_id_bytes,
                1,
                wyrd_spec::ids::DataTenantId::new_v7(),
                None,
            )
            .expect("test WAL init"),
        );

        // Leak temp_dir to keep WAL files for the test lifetime
        std::mem::forget(temp_dir);

        Self::new_with_components(
            operator,
            wal,
            "00000000-0000-0000-0000-000000000000".to_string(),
            1,
            ScribeBuildConfig {
                admission: admission_config,
                telemetry: None,
                coordination_runtime: Handle::current(),
                post_ack_cpu,
                wal_io,
                ingress_cpu: ScribeIngressCpuPool::new(1),
                ingress_queue_items: 256,
                ingress_queue_bytes: 512 * 1024 * 1024,
            },
        )
    }

    /// Attach an opt-in stage recorder to this Scribe instance.
    #[must_use]
    pub fn with_telemetry(mut self, telemetry: Arc<ScribeTelemetry>) -> Self {
        self.registry.set_telemetry(Arc::clone(&telemetry));
        self.ingress_queue.set_telemetry(Arc::clone(&telemetry));
        self.telemetry = Some(telemetry);
        self
    }

    /// Stop accepting new writer work and drain the bounded execution lanes.
    pub async fn shutdown(&self) {
        self.registry.shutdown().await;
    }

    /// Retire idle writers after the pod lifecycle scanner's tick.
    pub async fn retire_idle(&self, now: std::time::Instant) {
        self.registry.retire_idle(now).await;
    }

    /// Return a snapshot of the attached stage samples.
    #[must_use]
    pub fn telemetry(&self) -> Option<Vec<telemetry::ScribeStageSample>> {
        self.telemetry.as_ref().map(|recorder| recorder.snapshot())
    }

    /// Return the current admission, lane, and writer health metrics.
    #[must_use]
    pub fn runtime_snapshot(&self) -> ScribeRuntimeSnapshot {
        let lanes = self.registry.lane_snapshot();
        let ingress = self.ingress_cpu.snapshot();
        ScribeRuntimeSnapshot {
            admission: self.admission.snapshot(),
            executor: crate::scribe::telemetry::ExecutorSnapshot {
                depth: lanes.depth.saturating_add(ingress.depth),
                capacity: lanes.capacity.saturating_add(ingress.capacity),
                saturation_events: lanes
                    .saturation_events
                    .saturating_add(ingress.saturation_events),
            },
            writers: self.registry.health_snapshot(),
            durability: self
                .telemetry
                .as_ref()
                .map_or_else(Default::default, |telemetry| {
                    telemetry.durability_snapshot()
                }),
        }
    }

    /// Execute seal pre-commit stages (Freeze → Parquet → PUT → PG tx) for a
    /// specific seal-key on the caller's tenant-scoped transaction.
    ///
    /// Returns a `SealCommit` handle whose post-commit token must be completed
    /// only after the caller commits the transaction. The caller owns commit/rollback.
    ///
    /// Repo rule (`check:from-pools-allowlist`): this signature MUST take
    /// `&mut vala_sql::TenantConn<'_>` and MUST NOT accept `sqlx::PgPool`.
    ///
    /// # Errors
    /// Returns [`ScribeError`] if any seal stage fails.
    pub async fn seal_one(
        &self,
        seal_key: &SealKey,
        conn: &mut TenantConn<'_>,
    ) -> Result<seal::SealCommit, ScribeError> {
        use crate::scribe::seal::SealDriver;

        let binding = TenantTableBinding::resolve((seal_key.tenant, seal_key.table.clone()))
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        binding
            .validate_authenticated_tenant(conn.data_tenant_id())
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;

        let driver = SealDriver::new_with_telemetry_and_lane(
            self.operator.clone(),
            self.telemetry.clone(),
            self.registry.post_ack_cpu(),
        );
        driver
            .pre_commit(
                &self.memtable,
                seal_key,
                &binding,
                conn,
                &self.node_id,
                self.writer_epoch,
            )
            .await
    }
}

#[cfg(test)]
impl Default for ScribeImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scribe for ScribeImpl {
    // Keep this method as the Gate→Scribe composition seam. It validates and
    // reserves synchronously, then transfers the canonical RecordBatch to the
    // writer queue; preprocessing and all blocking WAL work happen afterward.
    async fn ingest_frame(&self, frame: ScribeIngressFrame) -> Result<FrameAdmission, ScribeError> {
        self.ingress_queue.submit(frame).await
    }
}

async fn process_ingress_frame(
    frame: ScribeIngressFrame,
    raw_reservation: &mut IngressQueueReservation,
    admission: &AdmissionController,
    ingress_cpu: &ScribeIngressCpuPool,
    registry: &TenantTableWriterRegistry,
    telemetry: Option<&Arc<ScribeTelemetry>>,
) -> Result<FrameAdmission, ScribeError> {
    let append_started = Instant::now();
    frame
        .binding
        .validate_authenticated_tenant(frame.principal.tenant_id)
        .map_err(|error| ScribeError::Internal {
            detail: error.to_string(),
        })?;
    let rows = ingress_cpu
        .decode(
            frame.payload,
            frame.principal.clone(),
            frame.expected_schema_fingerprint,
            frame.request_id.clone(),
            frame.batch_id,
        )
        .await?;
    let append_rows = rows.num_rows();
    let total_rows = frame
        .stream_rows_before
        .saturating_add(u64::try_from(append_rows).unwrap_or(u64::MAX));
    if total_rows > frame.stream_rows_limit {
        return Err(ScribeError::TooManyRows {
            rows: total_rows,
            limit: frame.stream_rows_limit,
        });
    }
    let append_wire_bytes = frame.measured_wire_bytes;
    let estimated_bytes = rows
        .get_array_memory_size()
        .saturating_add(frame.measured_wire_bytes)
        .saturating_add(REQUEST_OVERHEAD_BYTES);
    let reservation = raw_reservation.transfer_to_retained(
        admission,
        frame.binding.table_ref.fqn(),
        estimated_bytes,
    )?;
    let admitted = AdmittedAppend {
        request_id: frame.request_id,
        batch_id: frame.batch_id,
        frame_sequence: frame.frame_sequence,
        audit_event: frame.audit_event,
        rows,
        measured_wire_bytes: frame.measured_wire_bytes,
        admitted_bytes: reservation.bytes(),
        reservation,
        tenant: frame.principal.tenant_id,
        table: frame.binding.table_ref.clone(),
        queued_at: Instant::now(),
    };
    let rows_accepted = admitted.rows.num_rows() as u64;
    let (writer, created) = registry.get_or_create(frame.binding)?;
    if let Err(error) = TenantTableWriterRegistry::enqueue(&writer, admitted) {
        if created {
            registry.remove_if(&writer.key, writer.instance_id);
        }
        return Err(error);
    }
    if let Some(telemetry) = telemetry {
        telemetry.record_accepted(append_rows);
        telemetry.record(
            "append",
            append_started.elapsed(),
            append_rows,
            append_wire_bytes,
        );
    }
    Ok(FrameAdmission { rows_accepted })
}

impl ScribeImpl {
    /// Sum of pending (un-fsynced or un-truncated) WAL bytes on this pod.
    #[must_use]
    pub fn wal_pending_bytes(&self) -> u64 {
        self.wal.bytes_on_disk()
    }

    /// Return the number of bytes currently retained by this pod's WAL.
    #[must_use]
    pub fn wal_bytes_on_disk(&self) -> u64 {
        self.wal.bytes_on_disk()
    }

    /// Return aggregate writable and immutable memtable state.
    pub fn memtable_stats(&self) -> Result<memtable::MemtableStats, ScribeError> {
        let stats = self.memtable.stats()?;
        self.admission
            .sync_memtable_bytes(stats.writable_bytes, stats.immutable_bytes);
        Ok(stats)
    }

    /// Return the current pod-global admission counters.
    #[must_use]
    pub fn admission_snapshot(&self) -> admission::AdmissionSnapshot {
        self.admission.snapshot()
    }

    /// Return the number of active tenant/table writer actors.
    #[must_use]
    pub fn writer_count(&self) -> usize {
        self.registry.writer_count()
    }

    /// Row count in the writable bucket for `key` on this pod; 0 if no bucket.
    #[must_use]
    pub fn memtable_row_count(&self, key: &MemtableKey) -> usize {
        self.memtable.row_count(key).unwrap_or(0)
    }

    /// Every Parquet path this pod has sealed since boot.
    #[must_use]
    pub fn sealed_parquet_paths(&self) -> Vec<String> {
        // TODO : Query vala.file_list for sealed paths for this node
        vec![]
    }

    /// Force-seal every non-empty writable or pending bucket on this pod.
    ///
    /// The returned batch is only a set of post-commit capabilities. The
    /// caller must commit its `TenantConn` first, then pass the batch to
    /// [`Self::complete_post_commit`].
    ///
    /// # Errors
    /// Returns `ScribeError` if any seal stage fails.
    pub async fn force_seal(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<seal::PostCommitBatch, ScribeError> {
        self.registry.drain().await;
        // A `TenantConn` is bound to exactly one tenant. Seal only the
        // memtable buckets whose seal-key belongs to that tenant; the harness
        // iterates tenants and opens a fresh `TenantConn` per tenant.
        //
        let tenant = conn.data_tenant_id();
        let keys = self.memtable.seal_keys_for_tenant(tenant)?;

        let mut tokens = Vec::with_capacity(keys.len());
        for key in keys {
            match self.seal_one(&key, conn).await {
                Ok(handle) => {
                    self.admission
                        .transfer_active_to_immutable(handle.token.memtable_bytes);
                    tokens.push(handle.token);
                }
                Err(error) => {
                    for token in tokens {
                        let _ = self.abort_post_commit(token);
                    }
                    return Err(error);
                }
            }
        }

        Ok(seal::PostCommitBatch(tokens))
    }

    /// Complete one or more seal generations after the caller commits SQL.
    pub fn complete_post_commit<T>(&self, post_commit: T) -> Result<(), ScribeError>
    where
        T: Into<seal::PostCommitBatch>,
    {
        for token in post_commit.into().0 {
            self.memtable
                .complete_post_commit(token.seal_id, token.file_list_key)?;
        }
        Ok(())
    }

    /// Abort one or more post-commit capabilities after SQL rollback.
    pub fn abort_post_commit<T>(&self, post_commit: T) -> Result<(), ScribeError>
    where
        T: Into<seal::PostCommitBatch>,
    {
        for token in post_commit.into().0 {
            self.admission
                .transfer_immutable_to_active(token.memtable_bytes);
            self.memtable.abort_post_commit(token.seal_id)?;
        }
        Ok(())
    }

    /// Reconcile a pending generation against the exact durable file-list row.
    ///
    /// The lookup runs through the caller's tenant-bound connection. A found
    /// row completes the generation; a missing row deliberately leaves it
    /// pending so WAL replay can retry the seal.
    pub async fn reconcile_post_commit(
        &self,
        token: &seal::PostCommitToken,
        conn: &mut TenantConn<'_>,
    ) -> Result<bool, ScribeError> {
        let key = &token.file_list_key;
        if conn.data_tenant_id() != key.data_tenant_id {
            return Err(ScribeError::Internal {
                detail: format!(
                    "post-commit reconciliation tenant mismatch: key={} connection={}",
                    key.data_tenant_id,
                    conn.data_tenant_id()
                ),
            });
        }
        let row: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id
               FROM vala.file_list
              WHERE data_tenant_id = $1
                AND namespace = $2
                AND table_name = $3
                AND node_id = $4
                AND writer_epoch = $5
                AND wal_lsn_min = $6
                AND wal_lsn_max = $7",
        )
        .bind(key.data_tenant_id.as_uuid())
        .bind(&key.namespace)
        .bind(&key.table_name)
        .bind(key.node_id)
        .bind(key.writer_epoch)
        .bind(key.wal_lsn_min)
        .bind(key.wal_lsn_max)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(|error| ScribeError::Internal {
            detail: format!("file-list post-commit reconciliation failed: {error}"),
        })?;

        if row.is_some() {
            self.memtable
                .complete_post_commit(token.seal_id, key.clone())?;
            Ok(true)
        } else {
            self.memtable.abort_post_commit(token.seal_id)?;
            Ok(false)
        }
    }

    /// Restore one replayed WAL range as a pending immutable generation.
    pub fn restore_replayed(
        &self,
        replayed: &replay::ReplayedSealKey,
    ) -> Result<memtable::FrozenMemtable, ScribeError> {
        self.memtable.restore_replayed(replayed)
    }

    /// Replay every complete, unretired WAL frame into pending immutable state.
    ///
    /// This is the boot recovery boundary: replay performs paired audit/data
    /// validation and `(batch_id, seal_key)` deduplication before the restored
    /// Arrow batches become visible to the normal seal/reconciliation path.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when the WAL cannot be read or a frame cannot be
    /// reconstructed as Arrow state.
    pub fn replay_wal(&self) -> Result<usize, ScribeError> {
        let replayed = replay::replay_wal_directory(self.wal.base_dir())?;
        let restored = replayed
            .values()
            .map(|state| self.restore_replayed(state).map(|_| ()))
            .collect::<Result<Vec<_>, _>>()?
            .len();
        self.memtable_stats()?;
        Ok(restored)
    }

    /// Replay WAL through the bounded filesystem lane before readiness.
    pub async fn replay_wal_async(&self) -> Result<usize, ScribeError> {
        let crate::scribe::execution_lanes::ScribeWalIoResult::Replayed(replayed) = self
            .wal_io
            .submit(
                crate::scribe::execution_lanes::ScribeWalIoOp::ReplayDirectory {
                    path: self.wal.base_dir().to_path_buf(),
                },
            )
            .await?
        else {
            return Err(ScribeError::Internal {
                detail: "WAL IO lane returned the wrong replay result".to_owned(),
            });
        };
        let restored = replayed
            .values()
            .map(|state| self.restore_replayed(state).map(|_| ()))
            .collect::<Result<Vec<_>, _>>()?
            .len();
        self.memtable_stats()?;
        Ok(restored)
    }

    /// Sweep committed immutable generations whose grace period elapsed.
    pub fn sweep_once_at(
        &self,
        now: std::time::Instant,
    ) -> Result<Vec<(SealKey, memtable::WalRange)>, ScribeError> {
        self.memtable.sweep_once_at(now)
    }

    /// Construct the pod-local typed tail reader.
    pub fn tail_service(&self) -> Result<FetchLiveTailService, ScribeError> {
        let node_id =
            uuid::Uuid::parse_str(&self.node_id).map_err(|error| ScribeError::Internal {
                detail: format!("invalid Scribe node_id: {error}"),
            })?;
        let stream = stream_identity::StreamIdentity::new(
            stream_identity::NodeId::new(node_id),
            stream_identity::WriterEpoch::new(self.writer_epoch),
        );
        Ok(FetchLiveTailService::new(
            stream,
            Arc::clone(&self.memtable),
        ))
    }

    /// Fetch the typed live tail from this pod's memtable.
    pub async fn fetch_live_tail(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<Vec<TailFrame>, ScribeError> {
        self.tail_service()?.fetch_live_tail(request).await
    }
}
