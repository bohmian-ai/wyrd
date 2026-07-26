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
use crate::catalog::TenantTableBinding;
pub use crate::contracts::ScribeAppend;
use crate::contracts::{FrameAdmission, Scribe, ScribeError, ScribeIngressFrame};
use crate::scribe::admission::{AdmissionConfig, AdmissionController};
pub use crate::scribe::execution_lanes::{
    ScribeIngressCpuPool, ScribePersistenceCpuPool, ScribeWalIoPool,
};
pub use crate::scribe::persistence::ScribePersistenceConfig;
use crate::scribe::seal_key::SealKey;
use crate::scribe::tail_rpc::{FetchLiveTailRequest, FetchLiveTailService, TailFrame};
use crate::scribe::telemetry::{
    ScribeBucketMemorySnapshot, ScribeInspectionSnapshot, ScribeRuntimeSnapshot,
};
use async_trait::async_trait;
use num_traits::ToPrimitive;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    persistence_cpu: ScribePersistenceCpuPool,
    wal_io: ScribeWalIoPool,
}

impl ScribeExecutionPools {
    /// Assemble the server-owned execution lanes for one Scribe instance.
    #[must_use]
    pub fn new(
        ingress_cpu: ScribeIngressCpuPool,
        persistence_cpu: ScribePersistenceCpuPool,
        wal_io: ScribeWalIoPool,
    ) -> Self {
        Self {
            ingress_cpu,
            persistence_cpu,
            wal_io,
        }
    }
}

/// Explicit boot-time execution-lane provisioning for Scribe.
#[derive(Debug, Clone, Copy)]
pub struct ScribeLaneConfig {
    /// Rayon workers reserved for pre-ACK decode, validation, and projection.
    pub ingress_cpu_threads: usize,
    /// Rayon workers reserved for persistence splitting and serialization.
    pub persistence_cpu_threads: usize,
    /// Rayon workers reserved for WAL filesystem operations.
    pub wal_io_threads: usize,
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
            persistence_cpu_threads: cpu_budget.saturating_sub(ingress_cpu_threads).max(1),
            wal_io_threads: 4,
        }
    }
}

/// Scribe implementation with bounded admission, fixed shards, WAL, memtable, and seal.
///
/// `append` decodes and prepares a complete request on the bounded CPU lanes,
/// then admits one owned prepared packet to a fixed shard queue. The shard
/// owner performs WAL writes, memtable insertion, and `sync_data` in order.
/// Rotation is a consumer-owned state swap; immutable generations are handed
/// to the bounded persistence runtime.
#[derive(Debug)]
pub struct ScribeImpl {
    /// Opendal operator for object store (shared across seal drivers).
    operator: Arc<opendal::Operator>,
    /// WAL writer for durable append fsync.
    wal: Arc<wal::WalWriter>,
    /// Pod identity (`node_id`, `writer_epoch`).
    node_id: String,
    writer_epoch: i64,
    /// Pod-global request and shard admission counters.
    admission: AdmissionController,
    /// Scribe-only child capability over the pod-global Bifrost governor.
    memory: memory::ScribeMemoryBudget,
    /// Shared active/immutable Arrow ownership ledger.
    memory_ledger: memory::MemoryLedger,
    /// Bounded persistence CPU lane retained for replay and seal preparation.
    persistence_cpu: ScribePersistenceCpuPool,
    /// Bounded WAL IO lane retained for recovery and writer execution.
    wal_io: ScribeWalIoPool,
    /// Bounded Rayon lane for pre-ACK native decode and projection work.
    ingress_cpu: ScribeIngressCpuPool,
    /// Fixed sixteen-lane shard owners for the live ingest path.
    shards: Arc<shards::ScribeShardRuntime>,
    /// Lifecycle gate closed before shard draining begins.
    closed: AtomicBool,
    /// False while startup WAL recovery is active or has failed.
    recovery_ready: AtomicBool,
    /// Optional server-provisioned immutable persistence runtime.
    persistence: Option<Arc<persistence::PersistenceRuntime>>,
}

pub struct ScribeBuildConfig {
    /// Admission bounds for in-flight frames and active memory.
    pub admission: AdmissionConfig,
    /// Runtime used for Scribe coordination tasks.
    pub coordination_runtime: Handle,
    /// Server-provisioned CPU and WAL execution pools.
    pub execution_pools: ScribeExecutionPools,
    /// Optional server-provisioned immutable persistence dependencies.
    pub persistence: Option<ScribePersistenceConfig>,
    /// Scribe child budget provisioned by server boot.
    pub memory_budget: Option<memory::ScribeMemoryBudget>,
}

/// Runtime, admission, and memory inputs for an embedded Scribe.
pub struct ScribeEmbeddedConfig {
    /// Execution lane sizes for the embedded Scribe.
    pub lane_config: ScribeLaneConfig,
    /// Admission bounds for the embedded Scribe.
    pub admission: AdmissionConfig,
    /// Runtime used for Scribe coordination tasks.
    pub coordination_runtime: Handle,
    /// Optional server-provisioned Scribe child budget.
    pub memory_budget: Option<memory::ScribeMemoryBudget>,
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
        Self::try_new_for_embedded_with_wal_sync_delay_and_admission_and_memory(
            operator,
            wal,
            node_id,
            writer_epoch,
            sync_delay,
            ScribeEmbeddedConfig {
                lane_config: ScribeLaneConfig::resolved(),
                admission,
                coordination_runtime: Handle::current(),
                memory_budget: None,
            },
        )
    }

    /// Construct the embedded Scribe with deterministic sync delay, explicit
    /// admission limits, and an optional server-provisioned memory governor.
    pub fn try_new_for_embedded_with_wal_sync_delay_and_admission_and_memory(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        sync_delay: std::time::Duration,
        config: ScribeEmbeddedConfig,
    ) -> Result<Self, String> {
        let execution_pools = ScribeExecutionPools::new(
            ScribeIngressCpuPool::try_new_with_capacity(
                config.lane_config.ingress_cpu_threads,
                256,
            )
            .map_err(|error| format!("ingress CPU pool failed: {error}"))?,
            ScribePersistenceCpuPool::try_new_with_capacity(
                config.lane_config.persistence_cpu_threads,
                64,
            )
            .map_err(|error| format!("persistence CPU pool failed: {error}"))?,
            ScribeWalIoPool::try_new_with_capacity_and_delay(
                config.lane_config.wal_io_threads,
                256,
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
                admission: config.admission,
                coordination_runtime: config.coordination_runtime,
                execution_pools,
                persistence: None,
                memory_budget: config.memory_budget,
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
        Self::new_for_embedded_with_runtime_config_and_admission_and_memory(
            operator,
            wal,
            node_id,
            writer_epoch,
            ScribeEmbeddedConfig {
                lane_config,
                admission,
                coordination_runtime,
                memory_budget: None,
            },
        )
    }

    /// Construct Scribe with explicit execution lanes, admission bounds, and
    /// an optional server-provisioned memory governor.
    pub fn new_for_embedded_with_runtime_config_and_admission_and_memory(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: String,
        writer_epoch: i64,
        config: ScribeEmbeddedConfig,
    ) -> Self {
        Self::new_with_components(
            operator,
            wal,
            node_id,
            writer_epoch,
            ScribeBuildConfig {
                admission: config.admission,
                coordination_runtime: config.coordination_runtime,
                execution_pools: ScribeExecutionPools::new(
                    ScribeIngressCpuPool::new_with_capacity(
                        config.lane_config.ingress_cpu_threads,
                        256,
                    ),
                    ScribePersistenceCpuPool::new_with_capacity(
                        config.lane_config.persistence_cpu_threads,
                        64,
                    ),
                    ScribeWalIoPool::new_with_capacity(config.lane_config.wal_io_threads, 256),
                ),
                persistence: None,
                memory_budget: config.memory_budget,
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
        let memory = config.memory_budget.clone().unwrap_or_else(|| {
            memory::BifrostMemoryGovernor::new_with_scribe_limit(
                config.admission.memory_limit_bytes,
                config.admission.scribe_memory_limit_bytes,
            )
            .or_else(|_| memory::BifrostMemoryGovernor::detect(1024 * 1024 * 1024))
            .unwrap_or_else(|_| {
                memory::BifrostMemoryGovernor::new(1024 * 1024 * 1024)
                    .expect("one-gibibyte fallback memory budget is valid")
            })
            .scribe_budget()
        });
        let memory_ledger = memory::MemoryLedger::new(&memory)
            .expect("zero-sized memory ledger reservations must be valid");
        let admission = AdmissionController::with_config(config.admission);
        let ScribeBuildConfig {
            coordination_runtime,
            execution_pools,
            persistence: persistence_config,
            admission: _,
            memory_budget: _,
        } = config;
        let ScribeExecutionPools {
            ingress_cpu,
            persistence_cpu,
            wal_io,
        } = execution_pools;
        let persistence = persistence_config.map(|config| {
            let faults = config.faults.clone();
            persistence::PersistenceRuntime::start(
                config,
                persistence::PersistenceRuntimeContext {
                    operator: Arc::clone(&operator),
                    wal: Arc::clone(&wal),
                    persistence_cpu: persistence_cpu.clone(),
                    wal_io: wal_io.clone(),
                    node_id: node_id.clone(),
                    writer_epoch,
                    memory: memory.clone(),
                    faults,
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
                retention_grace: Duration::from_mins(1),
                rotation_bytes: memory.active_bucket_target_bytes(),
                wal: Arc::clone(&wal),
                persistence_cpu: persistence_cpu.clone(),
                wal_io: wal_io.clone(),
                persistence: persistence.clone(),
                stream,
                memory_ledger: memory_ledger.clone(),
            },
            &coordination_runtime,
        );
        Self {
            operator,
            wal,
            node_id,
            writer_epoch,
            admission,
            memory,
            memory_ledger,
            persistence_cpu,
            wal_io,
            ingress_cpu,
            shards,
            closed: AtomicBool::new(false),
            recovery_ready: AtomicBool::new(true),
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
        Self::new_with_test_lanes(ScribePersistenceCpuPool::new(1), ScribeWalIoPool::new(1))
    }

    #[cfg(test)]
    pub(crate) fn new_with_test_lanes(
        persistence_cpu: ScribePersistenceCpuPool,
        wal_io: ScribeWalIoPool,
    ) -> Self {
        Self::new_with_test_config(persistence_cpu, wal_io, AdmissionConfig::default())
    }

    #[cfg(test)]
    pub(crate) fn new_with_test_config(
        persistence_cpu: ScribePersistenceCpuPool,
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
            wal::WalWriter::new(temp_dir.path(), node_id_bytes, 1, wal::WalConfig::default())
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
                    persistence_cpu,
                    wal_io,
                ),
                persistence: None,
                memory_budget: None,
            },
        )
    }

    /// Stop accepting new shard work and drain the bounded execution lanes.
    pub async fn shutdown(&self) {
        let started = std::time::Instant::now();
        self.closed.store(true, Ordering::Release);
        let _ = self.shards.flush_all().await;
        self.shards.drain().await;
        if let Some(persistence) = &self.persistence {
            persistence.close_and_drain().await;
        }
        self.shards.shutdown().await;
        self.persistence_cpu.drain().await;
        self.wal_io.drain().await;
        self.ingress_cpu.drain().await;
        metrics::histogram!("bifrost_scribe_shutdown_seconds")
            .record(started.elapsed().as_secs_f64());
    }

    /// Return whether Scribe has completed recovery and still accepts writes.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        !self.closed.load(Ordering::Acquire) && self.recovery_ready.load(Ordering::Acquire)
    }

    /// Drain shard work after the pod lifecycle scanner's tick.
    pub async fn retire_idle(&self, now: std::time::Instant) {
        let _ = now;
        self.shards.drain().await;
    }

    /// Flush writable generations through the bounded persistence runtime.
    ///
    /// This is a test-tier control seam for exercising owner FIFO and retry
    /// behavior without bypassing the shard command boundary.
    pub async fn flush_writable_for_test(&self) -> Result<(), ScribeError> {
        self.shards.flush_all().await
    }

    /// Return the bounded persistence queue depth for test-tier drain checks.
    #[must_use]
    pub fn persistence_queue_depth_for_test(&self) -> usize {
        self.persistence
            .as_ref()
            .map_or(0, |persistence| persistence.queue_depth())
    }

    /// Publish one coalescing lifecycle age tick to every shard owner.
    pub fn check_age(&self, now: std::time::Instant) {
        self.shards.request_expired_flush(now);
        let snapshot = self.memory.snapshot();
        let owner_snapshots = self.shards.memtable_snapshots().unwrap_or_default();
        if snapshot.effective_pressure_percent() >= 75 {
            let target = snapshot.scribe_limit_bytes.saturating_mul(65) / 100;
            let candidates = owner_snapshots
                .iter()
                .flat_map(|snapshot| snapshot.pressure_candidates.clone())
                .collect::<Vec<_>>();
            let victims = memtable::Memtable::select_pressure_victims(
                candidates,
                snapshot.total_bytes().saturating_sub(target),
            );
            self.shards.request_pressure_flush(victims);
        }
        if self.wal.disk_pressure().soft {
            let candidates = owner_snapshots
                .iter()
                .flat_map(|snapshot| snapshot.pressure_candidates.clone())
                .collect::<Vec<_>>();
            let victim = memtable::Memtable::select_oldest_wal_victim(&candidates);
            self.shards.request_wal_pressure_flush(victim);
        }
    }

    /// Return the current admission, lane, and shard health metrics.
    #[must_use]
    pub fn runtime_snapshot(&self) -> ScribeRuntimeSnapshot {
        let persistence = self.persistence_cpu.snapshot();
        let wal_io = self.wal_io.snapshot();
        let ingress = self.ingress_cpu.snapshot();
        let lanes = crate::scribe::telemetry::ExecutorSnapshot {
            depth: persistence.depth.saturating_add(wal_io.depth),
            capacity: persistence.capacity.saturating_add(wal_io.capacity),
            saturation_events: persistence
                .saturation_events
                .saturating_add(wal_io.saturation_events),
            completed: persistence.completed.saturating_add(wal_io.completed),
            failed: persistence.failed.saturating_add(wal_io.failed),
            panicked: persistence.panicked.saturating_add(wal_io.panicked),
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
            persistence,
            wal_io,
            shards: crate::scribe::telemetry::ShardHealthSnapshot {
                shard_tasks: crate::scribe::routing::SCRIBE_SHARD_COUNT,
                shard_channels: crate::scribe::routing::SCRIBE_SHARD_COUNT,
                pending_items: self.shards.pending_items(),
                terminal_errors: 0,
            },
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

        let driver = SealDriver::new_with_lane(self.operator.clone(), self.persistence_cpu.clone());
        let frozen = self.shards.freeze_key(seal_key.clone()).await?;
        driver
            .pre_commit_frozen(
                &frozen,
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
    fn is_ready(&self) -> bool {
        Self::is_ready(self)
    }

    // Keep this method as the Gate→Scribe composition seam. It prepares the
    // request before dispatch and waits for the shard's durable completion.
    async fn ingest_frame(&self, frame: ScribeIngressFrame) -> Result<FrameAdmission, ScribeError> {
        if !self.is_ready() {
            return Err(ScribeError::IngressClosed);
        }
        ingress::process_ingress_frame(
            frame,
            &self.admission,
            &self.memory,
            &self.ingress_cpu,
            &self.persistence_cpu,
            &self.shards,
        )
        .await
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
        let mut stats = memtable::MemtableStats::default();
        for owner in self.shards.memtable_snapshots()? {
            stats.writable_rows += owner.stats.writable_rows;
            stats.writable_bytes += owner.stats.writable_bytes;
            stats.immutable_rows += owner.stats.immutable_rows;
            stats.immutable_bytes += owner.stats.immutable_bytes;
            stats.immutable_generations += owner.stats.immutable_generations;
            stats.pending_generations += owner.stats.pending_generations;
            stats.writable_buckets += owner.stats.writable_buckets;
            stats.immutable_buckets += owner.stats.immutable_buckets;
        }
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

    /// Return the bounded setup and ownership snapshot used by test harnesses.
    pub fn inspection_snapshot(&self) -> Result<ScribeInspectionSnapshot, ScribeError> {
        let stats = self.memtable_stats()?;
        let memory = self.memory.snapshot();
        let owner_snapshots = self.shards.memtable_snapshots()?;
        let bucket_memory = owner_snapshots
            .iter()
            .flat_map(|snapshot| snapshot.bucket_memory.clone())
            .collect::<Vec<_>>();
        let mut memory_by_shard = [0_usize; crate::scribe::routing::SCRIBE_SHARD_COUNT];
        let mut memory_by_bucket = Vec::with_capacity(bucket_memory.len());
        let mut bucket_total = 0_usize;
        for bucket in bucket_memory {
            let bytes = bucket.writable_bytes.saturating_add(bucket.immutable_bytes);
            let shard =
                crate::scribe::routing::shard_for(bucket.seal_key.tenant, &bucket.seal_key.table);
            memory_by_shard[shard] = memory_by_shard[shard].saturating_add(bytes);
            bucket_total = bucket_total.saturating_add(bytes);
            memory_by_bucket.push(ScribeBucketMemorySnapshot {
                seal_key: bucket.seal_key,
                writable_bytes: bucket.writable_bytes,
                immutable_bytes: bucket.immutable_bytes,
            });
        }
        for (shard, bytes) in self.memory.shard_snapshot().into_iter().enumerate() {
            memory_by_shard[shard] = memory_by_shard[shard].saturating_add(bytes);
        }
        let transient_total = self.memory.shard_snapshot().into_iter().sum::<usize>();
        if bucket_total.saturating_add(transient_total) != memory.total_bytes() {
            return Err(ScribeError::Internal {
                detail: format!(
                    "inspection found {} bytes without a bucket or shard owner",
                    memory
                        .total_bytes()
                        .saturating_sub(bucket_total.saturating_add(transient_total))
                ),
            });
        }
        Ok(ScribeInspectionSnapshot {
            shard_task_count: crate::scribe::routing::SCRIBE_SHARD_COUNT,
            shard_channel_count: crate::scribe::routing::SCRIBE_SHARD_COUNT,
            open_wal_stream_count: self.wal.open_stream_count(),
            queued_items: self.shards.pending_items(),
            writable_bucket_count: stats.writable_buckets,
            immutable_bucket_count: stats.immutable_buckets,
            memory_by_category: memory.categories,
            memory_by_shard,
            memory_by_bucket,
            total_accounted_memory: memory.total_bytes(),
            parent_used_memory: memory.bifrost_total_bytes,
            parent_memory_limit: memory.bifrost_limit_bytes,
            scribe_used_memory: memory.scribe_total_bytes,
            scribe_memory_limit: memory.scribe_limit_bytes,
            wal_disk_bytes: self.wal.bytes_on_disk(),
        })
    }

    /// Trip the WAL availability breaker for deterministic test-tier probes.
    pub fn trip_wal_disk_full_for_test(&self) {
        self.admission.trip_wal_disk_full();
    }

    /// Return the bounded ingress CPU pool for Gate protocol projection.
    #[must_use]
    pub fn ingress_cpu_pool(&self) -> ScribeIngressCpuPool {
        self.ingress_cpu.clone()
    }

    /// Row count in the writable bucket for `key` on this pod; 0 if no bucket.
    #[must_use]
    pub fn memtable_row_count(&self, key: &MemtableKey) -> usize {
        self.shards
            .memtable_snapshots()
            .ok()
            .into_iter()
            .flatten()
            .flat_map(|snapshot| snapshot.bucket_memory)
            .find(|bucket| bucket.seal_key == *key)
            .map_or(0, |bucket| bucket.row_count)
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
        let frozen = self.shards.freeze_tenant(tenant).await?;
        let driver =
            seal::SealDriver::new_with_lane(self.operator.clone(), self.persistence_cpu.clone());

        let mut tokens = Vec::with_capacity(frozen.len());
        for frozen in frozen {
            let key = frozen.seal_key.clone();
            match driver
                .pre_commit_frozen(
                    &frozen,
                    &key,
                    &TenantTableBinding::resolve((key.tenant, key.table.clone())).map_err(
                        |error| ScribeError::Internal {
                            detail: error.to_string(),
                        },
                    )?,
                    conn,
                    &self.node_id,
                    self.writer_epoch,
                )
                .await
            {
                Ok(handle) => {
                    tokens.push(handle.token);
                }
                Err(error) => {
                    let _ = self.abort_post_commit(seal::PostCommitBatch(tokens)).await;
                    return Err(error);
                }
            }
        }

        Ok(seal::PostCommitBatch(tokens))
    }

    /// Complete one or more seal generations after the caller commits SQL.
    pub async fn complete_post_commit<T>(&self, post_commit: T) -> Result<(), ScribeError>
    where
        T: Into<seal::PostCommitBatch>,
    {
        for token in post_commit.into().0 {
            self.shards
                .complete_post_commit(token.seal_id, &token.seal_key, token.file_list_key)
                .await?;
        }
        Ok(())
    }

    /// Abort one or more post-commit capabilities after SQL rollback.
    pub async fn abort_post_commit<T>(&self, post_commit: T) -> Result<(), ScribeError>
    where
        T: Into<seal::PostCommitBatch>,
    {
        for token in post_commit.into().0 {
            self.shards
                .abort_post_commit(token.seal_id, &token.seal_key)
                .await?;
            self.admission
                .transfer_immutable_to_active(token.memtable_bytes);
            self.memory_ledger
                .move_immutable_to_active(token.memtable_bytes)
                .map_err(|error| ScribeError::Internal {
                    detail: error.to_string(),
                })?;
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
            self.shards
                .complete_post_commit(token.seal_id, &token.seal_key, key.clone())
                .await?;
            Ok(true)
        } else {
            self.shards
                .abort_post_commit(token.seal_id, &token.seal_key)
                .await?;
            Ok(false)
        }
    }

    /// Replay every complete, unretired WAL frame into pending immutable state.
    ///
    /// This is the boot recovery boundary: replay validates complete
    /// audit-plus-data records and `(batch_id, seal_key)` deduplication before the restored
    /// Arrow batches become visible to the normal seal/reconciliation path.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when the WAL cannot be read or a frame cannot be
    /// reconstructed as Arrow state.
    pub fn replay_wal(&self) -> Result<usize, ScribeError> {
        Err(ScribeError::Internal {
            detail: "synchronous WAL replay is not supported; use replay_wal_async".to_owned(),
        })
    }

    /// Replay WAL through the bounded filesystem lane before readiness.
    pub async fn replay_wal_async(&self) -> Result<usize, ScribeError> {
        let started = std::time::Instant::now();
        self.recovery_ready.store(false, Ordering::Release);
        let result = self
            .wal_io
            .submit(
                crate::scribe::execution_lanes::ScribeWalIoOp::ReplayDirectoryStream {
                    path: self.wal.base_dir().to_path_buf(),
                    shard_senders: self.shards.replay_senders(),
                    memory: self.memory.clone(),
                },
            )
            .await;
        let result = match result {
            Ok(crate::scribe::execution_lanes::ScribeWalIoResult::ReplayStreamCompleted {
                restored,
            }) => {
                self.memtable_stats()?;
                Ok(restored)
            }
            Ok(_) => Err(ScribeError::Internal {
                detail: "WAL IO lane returned the wrong replay result".to_owned(),
            }),
            Err(error) => Err(error),
        };
        if result.is_ok() {
            self.recovery_ready.store(true, Ordering::Release);
            metrics::histogram!("bifrost_scribe_replay_seconds")
                .record(started.elapsed().as_secs_f64());
        }
        result
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
        Ok(FetchLiveTailService::with_runtime(
            stream,
            Arc::clone(&self.shards),
            crate::scribe::tail_rpc::TailConfig::default(),
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
