//! Scribe implementation — WAL append, fsync, replay, and memtable (/).

pub mod admission;
pub mod audit_envelope;
pub mod execution_lanes;
pub mod file_list_writer;
pub mod filename;
mod ingress;
pub mod manifest;
pub mod memory;
pub mod memtable;
pub mod parquet_writer;
pub mod persistence;
pub mod preprocess;
pub mod registry;
pub mod replay;
pub mod routing;
pub mod seal;
pub mod seal_key;
pub mod shards;
pub mod stream_identity;
pub mod tail_rpc;
pub mod telemetry;
pub mod wal;
#[cfg(test)]
mod writer;

#[cfg(test)]
mod acceptance;

use crate::catalog::TenantTableBinding;
pub use crate::contracts::ScribeAppend;
use crate::contracts::{FrameAdmission, Scribe, ScribeError, ScribeIngressFrame};
use crate::scribe::admission::{AdmissionConfig, AdmissionController};
pub use crate::scribe::execution_lanes::{
    ScribeIngressCpuPool, ScribePostAckCpuPool, ScribeWalIoPool,
};
use crate::scribe::memtable::Memtable;
pub use crate::scribe::persistence::ScribePersistenceConfig;
use crate::scribe::seal_key::SealKey;
use crate::scribe::tail_rpc::{FetchLiveTailRequest, FetchLiveTailService, TailFrame};
use crate::scribe::telemetry::{ScribeDurabilitySnapshot, ScribeRuntimeSnapshot};
use async_trait::async_trait;
use num_traits::ToPrimitive;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Handle;
use vala_sql::TenantConn;

/// Memtable key for per-bucket row-count inspection.
///
/// Type alias for [`SealKey`] — memtable buckets are keyed by
/// (`tenant`, `table`, `event_day`).
pub type MemtableKey = SealKey;

/// The three concrete execution lanes owned by the Bifrost server boot path.
///
/// Scribe accepts this value but never constructs or sizes a lane itself. The
/// concrete types keep the lane boundary narrow and preserve the distinct
/// worker pools required by the write lifecycle.
#[derive(Debug, Clone)]
pub struct ScribeExecutionPools {
    ingress_cpu: ScribeIngressCpuPool,
    post_ack_cpu: ScribePostAckCpuPool,
    wal_io: ScribeWalIoPool,
}

/// Bounds for one opportunistic WAL commit group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScribeCommitConfig {
    /// Maximum frames drained into one group.
    pub max_group_frames: usize,
    /// Maximum retained frame bytes drained into one group.
    pub max_group_bytes: usize,
}

impl ScribeCommitConfig {
    /// Construct a validated commit-group configuration.
    pub fn new(max_group_frames: usize, max_group_bytes: usize) -> Result<Self, ScribeError> {
        if max_group_frames == 0 || max_group_bytes == 0 {
            return Err(ScribeError::Internal {
                detail: "Scribe commit group limits must be greater than zero".to_owned(),
            });
        }
        Ok(Self {
            max_group_frames,
            max_group_bytes,
        })
    }
}

impl Default for ScribeCommitConfig {
    fn default() -> Self {
        Self {
            max_group_frames: 64,
            max_group_bytes: 64 * 1024 * 1024,
        }
    }
}

impl ScribeExecutionPools {
    /// Assemble the server-owned execution lanes for one Scribe instance.
    #[must_use]
    pub fn new(
        ingress_cpu: ScribeIngressCpuPool,
        post_ack_cpu: ScribePostAckCpuPool,
        wal_io: ScribeWalIoPool,
    ) -> Self {
        Self {
            ingress_cpu,
            post_ack_cpu,
            wal_io,
        }
    }
}

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
/// insertion, and `sync_data` in order. Rotation is a consumer-owned state swap;
/// immutable generations are handed to the bounded persistence runtime.
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
    /// Pod-global Bifrost/Scribe memory governor.
    memory: memory::BifrostMemoryGovernor,
    /// Bounded post-ACK CPU lane retained for replay and seal preparation.
    post_ack_cpu: ScribePostAckCpuPool,
    /// Bounded WAL IO lane retained for recovery and writer execution.
    wal_io: ScribeWalIoPool,
    /// Bounded Rayon lane for pre-ACK native decode and projection work.
    ingress_cpu: ScribeIngressCpuPool,
    /// Fixed sixteen-lane shard owners for the live ingest path.
    shards: Arc<shards::ScribeShardRuntime>,
    /// Bounded global raw-ingress dispatcher.
    ingress_queue: ingress::ScribeIngressQueue,
    /// Resolved opportunistic WAL group limits.
    commit_config: ScribeCommitConfig,
    /// Optional server-provisioned immutable persistence runtime.
    persistence: Option<Arc<persistence::PersistenceRuntime>>,
}

pub struct ScribeBuildConfig {
    /// Admission bounds for retained frames and active writers.
    pub admission: AdmissionConfig,
    /// Runtime used for Scribe coordination tasks.
    pub coordination_runtime: Handle,
    /// Server-provisioned CPU and WAL execution pools.
    pub execution_pools: ScribeExecutionPools,
    /// Maximum number of queued ingress items.
    pub ingress_queue_items: usize,
    /// Maximum bytes represented by queued ingress items.
    pub ingress_queue_bytes: usize,
    /// Opportunistic writer group limits.
    pub commit_config: ScribeCommitConfig,
    /// Optional server-provisioned immutable persistence dependencies.
    pub persistence: Option<ScribePersistenceConfig>,
}

impl ScribeImpl {
    /// Construct a new `ScribeImpl` with empty memtable and provided dependencies.
    pub fn new_for_embedded_with_deps(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
    ) -> Self {
        Self::new_for_embedded_with_runtime(operator, wal, node_id, writer_epoch, Handle::current())
    }

    /// Construct the embedded Scribe with a deterministic WAL sync delay.
    ///
    /// This seam is used only by real benchmark and failure-injection
    /// harnesses. Production boot supplies its own explicit pools through
    /// [`ScribeBuildConfig`] and always uses a zero WAL delay.
    pub fn try_new_for_embedded_with_wal_sync_delay(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        sync_delay: std::time::Duration,
    ) -> Result<Self, String> {
        Self::try_new_for_embedded_with_wal_sync_delay_and_admission(
            operator,
            wal,
            node_id,
            writer_epoch,
            sync_delay,
            AdmissionConfig::default(),
        )
    }

    /// Construct the embedded Scribe with deterministic sync delay and
    /// explicit test-tier admission limits.
    pub fn try_new_for_embedded_with_wal_sync_delay_and_admission(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        sync_delay: std::time::Duration,
        admission: AdmissionConfig,
    ) -> Result<Self, String> {
        let lane = ScribeLaneConfig::resolved();
        let execution_pools = ScribeExecutionPools::new(
            ScribeIngressCpuPool::try_new_with_capacity(
                lane.ingress_cpu_threads,
                lane.ingress_queue_items,
            )
            .map_err(|error| format!("ingress CPU pool failed: {error}"))?,
            ScribePostAckCpuPool::try_new_with_capacity(
                lane.post_ack_cpu_threads,
                lane.post_ack_queue_items,
            )
            .map_err(|error| format!("post-ACK CPU pool failed: {error}"))?,
            ScribeWalIoPool::try_new_with_capacity_and_delay(
                lane.wal_io_threads,
                lane.wal_io_queue_items,
                sync_delay,
            )
            .map_err(|error| format!("WAL IO pool failed: {error}"))?,
        );
        Ok(Self::new_with_execution_pools(
            operator,
            wal,
            node_id,
            writer_epoch,
            ScribeBuildConfig {
                admission,
                coordination_runtime: Handle::current(),
                execution_pools,
                ingress_queue_items: lane.ingress_queue_items,
                ingress_queue_bytes: lane.ingress_queue_bytes,
                commit_config: ScribeCommitConfig::default(),
                persistence: None,
            },
        ))
    }

    /// Construct a Scribe using an explicitly owned Tokio coordination runtime.
    ///
    /// Server deployments use [`Self::new_with_execution_pools`]. Tests and
    /// embedded callers may use this constructor on the current runtime.
    pub fn new_for_embedded_with_runtime(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        coordination_runtime: Handle,
    ) -> Self {
        Self::new_for_embedded_with_runtime_config(
            operator,
            wal,
            node_id,
            writer_epoch,
            ScribeLaneConfig::default(),
            coordination_runtime,
        )
    }

    /// Construct Scribe with explicitly provisioned execution lanes.
    pub fn new_for_embedded_with_runtime_config(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        lane_config: ScribeLaneConfig,
        coordination_runtime: Handle,
    ) -> Self {
        Self::new_for_embedded_with_runtime_config_and_admission(
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
    pub fn new_for_embedded_with_runtime_config_and_admission(
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
                coordination_runtime,
                execution_pools: ScribeExecutionPools::new(
                    ScribeIngressCpuPool::new_with_capacity(
                        lane_config.ingress_cpu_threads,
                        lane_config.ingress_queue_items,
                    ),
                    ScribePostAckCpuPool::new_with_capacity(
                        lane_config.post_ack_cpu_threads,
                        lane_config.post_ack_queue_items,
                    ),
                    ScribeWalIoPool::new_with_capacity(
                        lane_config.wal_io_threads,
                        lane_config.wal_io_queue_items,
                    ),
                ),
                ingress_queue_items: lane_config.ingress_queue_items,
                ingress_queue_bytes: lane_config.ingress_queue_bytes,
                commit_config: ScribeCommitConfig::default(),
                persistence: None,
            },
        )
    }

    /// Construct Scribe from execution lanes provisioned by server boot.
    pub fn new_with_execution_pools(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        config: ScribeBuildConfig,
    ) -> Self {
        Self::new_with_components(operator, wal, node_id, writer_epoch, config)
    }

    fn new_with_components(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        config: ScribeBuildConfig,
    ) -> Self {
        let memory = memory::BifrostMemoryGovernor::new(config.admission.memory_limit_bytes)
            .or_else(|_| memory::BifrostMemoryGovernor::detect(1024 * 1024 * 1024))
            .unwrap_or_else(|_| {
                memory::BifrostMemoryGovernor::new(1024 * 1024 * 1024)
                    .expect("one-gibibyte fallback memory budget is valid")
            });
        let memtable = Arc::new(Memtable::new_with_limits(
            Duration::from_mins(1),
            memory.active_bucket_target_bytes(),
        ));
        let admission = AdmissionController::with_config(config.admission);
        let ScribeBuildConfig {
            coordination_runtime,
            execution_pools,
            ingress_queue_items,
            ingress_queue_bytes,
            commit_config,
            persistence: persistence_config,
            admission: _,
        } = config;
        let ScribeExecutionPools {
            ingress_cpu,
            post_ack_cpu,
            wal_io,
        } = execution_pools;
        let persistence = persistence_config.map(|config| {
            persistence::PersistenceRuntime::start(
                config,
                persistence::PersistenceRuntimeContext {
                    operator: Arc::clone(&operator),
                    wal: Arc::clone(&wal),
                    post_ack_cpu: post_ack_cpu.clone(),
                    wal_io: wal_io.clone(),
                    node_id: node_id.clone(),
                    writer_epoch,
                },
                &coordination_runtime,
            )
        });
        let stream = stream_identity::StreamIdentity::new(
            stream_identity::NodeId::new(
                uuid::Uuid::parse_str(&node_id).unwrap_or(uuid::Uuid::nil()),
            ),
            stream_identity::WriterEpoch::new(writer_epoch),
        );
        let shards = shards::ScribeShardRuntime::start(
            shards::ScribeShardStartConfig {
                admission: admission.clone(),
                memtable: Arc::clone(&memtable),
                wal: Arc::clone(&wal),
                post_ack_cpu: post_ack_cpu.clone(),
                wal_io: wal_io.clone(),
                persistence: persistence.clone(),
                stream,
            },
            &coordination_runtime,
        );
        let ingress_queue = ingress::ScribeIngressQueue::new(
            ingress_queue_items,
            ingress_queue_bytes,
            admission.clone(),
            ingress_cpu.clone(),
            memory.clone(),
            Arc::clone(&shards),
            &coordination_runtime,
        );
        Self {
            memtable,
            operator,
            wal,
            node_id,
            writer_epoch,
            admission,
            memory,
            post_ack_cpu,
            wal_io,
            ingress_cpu,
            shards,
            ingress_queue,
            commit_config,
            persistence,
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
                wal::WalConfig::default(),
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
                coordination_runtime: Handle::current(),
                execution_pools: ScribeExecutionPools::new(
                    ScribeIngressCpuPool::new(1),
                    post_ack_cpu,
                    wal_io,
                ),
                ingress_queue_items: 256,
                ingress_queue_bytes: 512 * 1024 * 1024,
                commit_config: ScribeCommitConfig::default(),
                persistence: None,
            },
        )
    }

    /// Stop accepting new writer work and drain the bounded execution lanes.
    pub async fn shutdown(&self) {
        let started = std::time::Instant::now();
        self.ingress_queue.close_and_drain().await;
        self.shards.drain().await;
        if let Some(persistence) = &self.persistence {
            persistence.close_and_drain().await;
        }
        self.shards.shutdown().await;
        self.post_ack_cpu.drain().await;
        self.wal_io.drain().await;
        self.ingress_cpu.drain().await;
        metrics::histogram!("bifrost_scribe_shutdown_seconds")
            .record(started.elapsed().as_secs_f64());
    }

    /// Retire idle writers after the pod lifecycle scanner's tick.
    pub async fn retire_idle(&self, now: std::time::Instant) {
        let _ = now;
        self.shards.drain().await;
    }

    /// Publish one coalescing lifecycle age tick to every writer consumer.
    pub fn check_age(&self, now: std::time::Instant) {
        let Ok(keys) = self.memtable.expired_seal_keys(now) else {
            return;
        };
        for key in keys {
            if let Ok(frozen) = self.memtable.freeze(&key) {
                self.admission
                    .transfer_active_to_immutable(frozen.arrow_bytes);
            }
        }
    }

    /// Return the current admission, lane, and writer health metrics.
    #[must_use]
    pub fn runtime_snapshot(&self) -> ScribeRuntimeSnapshot {
        let post_ack = self.post_ack_cpu.snapshot();
        let wal_io = self.wal_io.snapshot();
        let ingress = self.ingress_cpu.snapshot();
        let lanes = crate::scribe::telemetry::ExecutorSnapshot {
            depth: post_ack.depth.saturating_add(wal_io.depth),
            capacity: post_ack.capacity.saturating_add(wal_io.capacity),
            saturation_events: post_ack
                .saturation_events
                .saturating_add(wal_io.saturation_events),
            completed: post_ack.completed.saturating_add(wal_io.completed),
            failed: post_ack.failed.saturating_add(wal_io.failed),
            panicked: post_ack.panicked.saturating_add(wal_io.panicked),
        };
        ScribeRuntimeSnapshot {
            admission: self.admission.snapshot(),
            executor: crate::scribe::telemetry::ExecutorSnapshot {
                depth: lanes.depth.saturating_add(ingress.depth),
                capacity: lanes.capacity.saturating_add(ingress.capacity),
                saturation_events: lanes
                    .saturation_events
                    .saturating_add(ingress.saturation_events),
                completed: lanes.completed.saturating_add(ingress.completed),
                failed: lanes.failed.saturating_add(ingress.failed),
                panicked: lanes.panicked.saturating_add(ingress.panicked),
            },
            ingress,
            post_ack,
            wal_io,
            writers: crate::scribe::telemetry::WriterHealthSnapshot {
                writers: self.memtable.logical_writer_count(),
                unhealthy_writers: 0,
                pending_items: 0,
                terminal_errors: 0,
            },
            durability: ScribeDurabilitySnapshot::default(),
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

        let driver = SealDriver::new_with_lane(self.operator.clone(), self.post_ack_cpu.clone());
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
        metrics::gauge!("bifrost_scribe_active_memtable_bytes")
            .set(stats.writable_bytes.to_f64().unwrap_or(f64::MAX));
        metrics::gauge!("bifrost_scribe_immutable_memtable_bytes")
            .set(stats.immutable_bytes.to_f64().unwrap_or(f64::MAX));
        metrics::gauge!("bifrost_scribe_immutable_generation_count")
            .set(stats.immutable_generations.to_f64().unwrap_or(f64::MAX));
        Ok(stats)
    }

    /// Return the current pod-global admission counters.
    #[must_use]
    pub fn admission_snapshot(&self) -> admission::AdmissionSnapshot {
        self.admission.snapshot()
    }

    /// Return pod-global Bifrost memory accounting.
    #[must_use]
    pub fn memory_snapshot(&self) -> memory::MemorySnapshot {
        self.memory.snapshot()
    }

    /// Trip the WAL availability breaker for deterministic test-tier probes.
    pub fn trip_wal_disk_full_for_test(&self) {
        self.admission.trip_wal_disk_full();
    }

    /// Return the resolved writer group limits used by every active writer.
    #[must_use]
    pub const fn commit_config(&self) -> ScribeCommitConfig {
        self.commit_config
    }

    /// Return the bounded ingress CPU pool for Gate protocol projection.
    #[must_use]
    pub fn ingress_cpu_pool(&self) -> ScribeIngressCpuPool {
        self.ingress_cpu.clone()
    }

    /// Return the number of active tenant/table writer actors.
    #[must_use]
    pub fn writer_count(&self) -> usize {
        self.memtable.logical_writer_count()
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
        self.shards.drain().await;
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
        let started = std::time::Instant::now();
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
        let mut restored = 0;
        for state in replayed.values() {
            let result = self
                .post_ack_cpu
                .submit(
                    crate::scribe::execution_lanes::ScribePostAckCpuOp::RestoreReplay {
                        memtable: Arc::clone(&self.memtable),
                        replayed: Box::new(state.clone()),
                    },
                )
                .await?;
            if !matches!(
                result,
                crate::scribe::execution_lanes::ScribePostAckCpuResult::ReplayRestored
            ) {
                return Err(ScribeError::Internal {
                    detail: "post-ACK CPU lane returned the wrong replay result".to_owned(),
                });
            }
            restored += 1;
        }
        self.memtable_stats()?;
        metrics::histogram!("bifrost_scribe_replay_seconds")
            .record(started.elapsed().as_secs_f64());
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
