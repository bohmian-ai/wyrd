//! Scribe implementation — WAL append, fsync, replay, and memtable (/).

pub mod admission;
pub mod audit_envelope;
mod direct_logs;
mod direct_metrics;
mod direct_traces;
pub mod execution_lanes;
pub mod file_list_writer;
pub mod filename;
mod fixed_ipc;
mod ingress;
pub mod manifest;
mod material_plan;
pub mod memory;
pub mod memtable;
mod otlp_managed;
pub mod parquet_writer;
pub mod persistence;
pub mod preprocess;
pub mod registry;
pub mod replay;
pub mod routing;
pub mod seal;
pub mod seal_key;
pub mod shards;
pub(crate) mod staging;
pub mod stream_identity;
pub mod tail_rpc;
pub mod telemetry;
#[cfg(any(test, feature = "test-support"))]
pub mod test_projection_oracle;
pub mod wal;
mod write_recipe;

#[cfg(test)]
#[path = "tests/pg_scribe_crash_injection.rs"]
mod pg_scribe_crash_injection;
#[cfg(test)]
#[path = "tests/pg_scribe_restart.rs"]
mod pg_scribe_restart;
#[cfg(test)]
#[path = "tests/scribe_persistence_path.rs"]
mod scribe_persistence_path;
#[cfg(test)]
#[path = "tests/time_partition.rs"]
mod tests;
#[cfg(test)]
#[path = "tests/wal_closeout.rs"]
mod wal_closeout;
use crate::catalog::{BifrostCatalog, TenantTableBinding};
use crate::contracts::{
    FrameAdmission, Scribe, ScribeError, ScribeIngressFrame,
};
use crate::maintenance::StagingFilePublisher;
use crate::scribe::admission::{AdmissionConfig, AdmissionController};
pub use crate::scribe::execution_lanes::{
    ScribeIngressCpuPool, ScribePersistenceCpuPool, ScribeWalIoPool,
};
pub use crate::scribe::memory::ScribeRejectionCeiling;
pub use crate::scribe::memtable::SealTriggerReason;
pub use crate::scribe::persistence::ScribePersistenceConfig;
use crate::scribe::seal::SealDriver;
use crate::scribe::seal_key::SealKey;
use crate::scribe::tail_rpc::FetchLiveTailService;
pub use crate::scribe::tail_rpc::{
    LocalTailReadTransport, ScribeTailReader, TailFenceConfig, TailReadTransport,
    TonicTailReadTransport,
};

/// Derives the largest replayable Scribe envelope admitted by configured limits.
///
/// Server boot compares this intrinsic requirement with the detected root
/// capability before accepting traffic or starting WAL replay.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] when configured bound
/// arithmetic cannot be represented on this platform.
pub fn configured_maximum_envelope_bytes(
    limits: crate::gate::limits::IngestLimits,
) -> Result<usize, ScribeError> {
    material_plan::configured_maximum_envelope_bytes(limits)
}
use crate::scribe::telemetry::{
    ScribeBucketMemorySnapshot, ScribeIngressLifecycle, ScribeInspectionSnapshot,
    ScribeRuntimeSnapshot,
};
use async_trait::async_trait;
#[cfg(any(test, feature = "test-support"))]
use datafusion::execution::memory_pool::{GreedyMemoryPool, MemoryPool};
use num_traits::ToPrimitive;
use std::sync::Arc;
#[cfg(any(test, feature = "test-support"))]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::runtime::Handle;
use vala_sql::TenantConn;

/// Awaits one already-signalled Scribe cleanup phase within the caller deadline.
///
/// The phase is never polled after deadline expiry. Cancellation drops only
/// the graceful wait; concrete owners retain their task handles for the
/// unconditional abort finalizer in [`ScribeImpl::shutdown`].
async fn await_shutdown_phase<T>(
    deadline: std::time::Instant,
    phase: impl std::future::Future<Output = T>,
) -> bool {
    if tokio::time::Instant::now() >= tokio::time::Instant::from_std(deadline) {
        return false;
    }
    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), phase)
        .await
        .is_ok()
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use std::time::Duration;

    /// Proves a stalled Scribe phase returns exactly at the caller-owned deadline.
    #[tokio::test]
    async fn stalled_shutdown_phase_obeys_caller_deadline() {
        let deadline = std::time::Instant::now() + Duration::from_millis(10);
        let completed = await_shutdown_phase(deadline, std::future::pending::<()>()).await;

        assert!(!completed);
        assert!(std::time::Instant::now() >= deadline);
    }

    /// Proves an expired budget skips a later Scribe phase without polling it.
    #[tokio::test]
    async fn expired_shutdown_phase_is_not_polled() {
        let polled = Arc::new(AtomicBool::new(false));
        let probe = Arc::clone(&polled);
        let completed = await_shutdown_phase(std::time::Instant::now(), async move {
            probe.store(true, Ordering::Release);
        })
        .await;

        assert!(!completed);
        assert!(!polled.load(Ordering::Acquire));
    }
}

/// Build a test-tier `DataFusion` pool with an explicit bounded ceiling.
#[cfg(any(test, feature = "test-support"))]
#[must_use]
pub fn constrained_datafusion_memory_pool(limit_bytes: usize) -> Arc<dyn MemoryPool> {
    Arc::new(GreedyMemoryPool::new(limit_bytes.max(1)))
}

/// Memtable key for per-bucket row-count inspection.
///
/// Type alias for [`SealKey`] — memtable buckets are keyed by
/// (`tenant`, `table`, `time_partition`).
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
pub struct ScribeImpl {
    /// Catalog owner used to validate and resolve logical transport frames.
    catalog: Option<Arc<crate::catalog::BifrostCatalog>>,
    /// Opendal operator for object store (shared across seal drivers).
    operator: Arc<opendal::Operator>,
    /// WAL writer for durable append fsync.
    wal: Arc<wal::WalWriter>,
    /// Pod identity (`node_id`, `writer_epoch`).
    node_id: String,
    /// Typed actor stream retained for source-filtered startup recovery.
    stream: stream_identity::StreamIdentity,
    writer_epoch: i64,
    /// Pod-global request and shard admission counters.
    admission: AdmissionController,
    /// Scribe-only child capability over the pod-global Bifrost governor.
    memory: crate::resources::ScribeResources,
    /// Immutable boot-selected ingest ceilings shared with Gate.
    ingest_limits: crate::gate::limits::IngestLimits,
    /// Runtime pressure and lifecycle thresholds (D83 watermarks and max age).
    ///
    /// Shared by the admission path and the periodic age scanner so the
    /// flush-first hysteresis is defined once.
    pressure_config: ScribePressureConfig,
    /// Shared active/immutable Arrow ownership ledger.
    memory_ownership: memory::ScribeOwnership,
    /// Bounded persistence CPU lane retained for replay and seal preparation.
    persistence_cpu: ScribePersistenceCpuPool,
    /// Bounded WAL IO lane retained for recovery and writer execution.
    wal_io: ScribeWalIoPool,
    /// Bounded Rayon lane for pre-ACK native decode and projection work.
    ingress_cpu: ScribeIngressCpuPool,
    /// Fixed sixteen-lane shard owners for the live ingest path.
    shards: Arc<shards::ScribeShardRuntime>,
    /// Fixed-size lifecycle ledger shared by move-only ingress root owners.
    ingress_lifecycle: Arc<ScribeIngressLifecycle>,
    /// Lifecycle gate closed before shard draining begins.
    closed: AtomicBool,
    /// Coordinates the recoverable running/draining/finalizing/stopped lifecycle.
    shutdown_state: std::sync::atomic::AtomicU8,
    /// Wakes callers waiting for the shutdown owner to finish.
    shutdown_notify: tokio::sync::Notify,
    /// Wakes test-tier observers after the owner enters draining.
    #[cfg(any(test, feature = "test-support"))]
    shutdown_draining_notify: tokio::sync::Notify,
    /// False while startup WAL recovery is active or has failed.
    recovery_ready: AtomicBool,
    /// Cooperative V1 recovery cancellation observed between WAL records and phases.
    recovery_cancelled: Arc<AtomicBool>,
    /// Optional server-provisioned immutable persistence runtime.
    persistence: Option<Arc<persistence::PersistenceRuntime>>,
    /// Optional local wake-up publisher retained for caller-owned commits.
    staging_file_publisher: Option<StagingFilePublisher>,
    #[cfg(any(test, feature = "test-support"))]
    ingest_stall: Arc<std::sync::Mutex<Option<Arc<IngestStall>>>>,
    /// Optional decoded-request ceiling used only by bounded regression tests;
    /// zero selects the production active-bucket target.
    #[cfg(any(test, feature = "test-support"))]
    decoded_request_limit_for_test: AtomicUsize,
    /// Passive typed lifecycle observer populated only by production owner boundaries.
    #[cfg(feature = "test-support")]
    publication_observer: ScribePublicationObserver,
}

/// Typed publication evidence emitted at Scribe's production durability boundaries.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScribePublicationEvent {
    /// The WAL owner returned a durable public acknowledgement.
    Acknowledged {
        /// Exact idempotency identity returned to the public writer.
        batch_id: uuid::Uuid,
        /// Number of rows accepted by the durable WAL owner.
        rows: u64,
    },
    /// The seal owner produced an immutable object and pending catalog capability.
    Sealed {
        /// Local immutable-generation identity.
        seal_id: u64,
        /// Durable file-list identity reserved by the seal transaction.
        file_list_row_id: uuid::Uuid,
        /// Ordered public batch identities represented by the generation.
        batch_ids: Vec<uuid::Uuid>,
        /// First WAL position represented by the immutable generation.
        wal_lsn_min: u64,
        /// Last WAL position represented by the immutable generation.
        wal_lsn_max: u64,
    },
    /// The post-commit owner published the exact file-list identity.
    Published {
        /// Local immutable-generation identity completed after commit.
        seal_id: u64,
        /// File-list row whose transaction committed before this event.
        file_list_row_id: uuid::Uuid,
        /// Ordered public batch identities represented by the published file.
        batch_ids: Vec<uuid::Uuid>,
        /// Fully-qualified table receiving the published immutable generation.
        table: String,
    },
}

/// Passive event stream for deterministic Scribe journey coordination.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Default)]
pub struct ScribePublicationObserver {
    /// Ordered events recorded synchronously by their production owners.
    events: Arc<std::sync::Mutex<Vec<ScribePublicationEvent>>>,
    /// Wakeup for bounded journey waiters.
    ready: Arc<tokio::sync::Notify>,
}

#[cfg(feature = "test-support")]
impl ScribePublicationObserver {
    /// Return a stable snapshot of all observed lifecycle events.
    #[must_use]
    pub fn events(&self) -> Vec<ScribePublicationEvent> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Wait until an event matching `predicate` has been recorded.
    pub async fn wait_for(
        &self,
        predicate: impl Fn(&ScribePublicationEvent) -> bool,
    ) -> ScribePublicationEvent {
        loop {
            let notified = self.ready.notified();
            if let Some(event) = self.events().into_iter().find(&predicate) {
                return event;
            }
            notified.await;
        }
    }

    /// Record one event after its production transition completes.
    fn record(&self, event: ScribePublicationEvent) {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
        self.ready.notify_waiters();
    }
}

/// Scribe is accepting work and no shutdown owner exists.
const SHUTDOWN_RUNNING: u8 = 0;
/// One graceful owner is draining accepted work and remains recoverable on drop.
const SHUTDOWN_DRAINING: u8 = 1;
/// One synchronous finalizer owns closure of every retained worker.
const SHUTDOWN_FINALIZING: u8 = 2;
/// Every retained Scribe owner has been closed or aborted.
const SHUTDOWN_STOPPED: u8 = 3;

/// Cancellation finalizer for the caller-owned graceful shutdown future.
struct ShutdownCancellationFinalizer<'a> {
    /// Scribe whose draining state must never be stranded by cancellation.
    scribe: &'a ScribeImpl,
    /// False only after graceful shutdown or a competing finalizer completes.
    armed: bool,
}

impl ShutdownCancellationFinalizer<'_> {
    /// Disarm the guard after the Scribe reaches its stopped state.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ShutdownCancellationFinalizer<'_> {
    /// Recover an interrupted graceful drain through the synchronous abort path.
    fn drop(&mut self) {
        if self.armed {
            self.scribe.abort_shutdown();
        }
    }
}

/// Deterministic test-tier barrier held at the public Scribe ingest seam.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct IngestStall {
    /// Records that one public write reached the barrier.
    ///
    /// The flag is the durable edge; [`IngestStall::entered_notify`] only wakes
    /// a waiter that was already parked. Both are required because the write
    /// runs on its own task and may reach the barrier before the asserting task
    /// first polls [`IngestStall::wait_entered`].
    entered: AtomicBool,
    /// Wakes a parked waiter after a public write reaches the barrier.
    entered_notify: tokio::sync::Notify,
    /// Signals a waiting write to continue when the test releases it.
    release: tokio::sync::Notify,
    /// Records that the stalled write future released its Scribe owner.
    completed: AtomicBool,
    /// Wakes lifecycle assertions after normal release or cancellation.
    completed_notify: tokio::sync::Notify,
}

#[cfg(any(test, feature = "test-support"))]
impl IngestStall {
    /// Wait until one public write is blocked at this barrier.
    ///
    /// The waiter is registered before the flag is observed.
    /// [`tokio::sync::Notify::notify_waiters`] wakes only already-registered
    /// waiters, and a `Notified` future does not register until it is first
    /// polled, so observing the flag first would lose the edge published by a
    /// write that reaches the barrier in between and park this caller forever.
    pub async fn wait_entered(&self) {
        Self::wait_for_edge(&self.entered, &self.entered_notify).await;
    }

    /// Release a blocked public write without changing its outcome.
    pub fn release(&self) {
        self.release.notify_waiters();
    }

    /// Wait until the stalled write future releases its Scribe owner.
    ///
    /// Cancelling the returned future abandons the observation only; the
    /// published edge stays latched, so a later caller still observes it.
    pub async fn wait_completed(&self) {
        Self::wait_for_edge(&self.completed, &self.completed_notify).await;
    }

    /// Publish the exact entry edge owned by the stalled write future.
    fn enter(&self) {
        self.entered.store(true, Ordering::Release);
        self.entered_notify.notify_waiters();
    }

    /// Publish the exact completion edge owned by the stalled write future.
    fn complete(&self) {
        self.completed.store(true, Ordering::Release);
        self.completed_notify.notify_waiters();
    }

    /// Await one latched barrier edge without losing a concurrent publication.
    ///
    /// Registers the waiter, then re-checks the flag, so an edge published
    /// between the previous check and this registration is still observed.
    async fn wait_for_edge(flag: &AtomicBool, notify: &tokio::sync::Notify) {
        loop {
            let notified = notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if flag.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
}

/// Drop guard that publishes release of one stalled public write owner.
#[cfg(any(test, feature = "test-support"))]
struct IngestStallCompletion<'a>(&'a IngestStall);

#[cfg(any(test, feature = "test-support"))]
impl Drop for IngestStallCompletion<'_> {
    /// Publish completion whether the write resumes or its transport is cancelled.
    fn drop(&mut self) {
        self.0.complete();
    }
}

/// Runtime-tunable Scribe pressure and lifecycle thresholds (D83).
///
/// This carrier owns the three knobs that govern the flush-first response to
/// ingress memory pressure: the ingress-occupancy high-water mark that triggers
/// a coordinated pressure seal, the low-water mark that seal drains toward, and
/// the active-generation max age that bounds trickle-workload visibility. It is
/// owned by the Scribe runtime and constructed beside the other Scribe build
/// config; the D83 defaults are production-ready with no config file required.
///
/// The `low_water < high_water` hysteresis invariant is enforced at
/// construction ([`ScribePressureConfig::new`] / [`ScribePressureConfig::default`]),
/// so an invalid config can never invert the watermark decision.
///
/// The field names and their D83 defaults are a normative interface: T40
/// (18-H2) reads them to set the watermark gauges and the seal-trigger labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScribePressureConfig {
    /// Ingress occupancy fraction (percent of `ingress_limit_bytes`) at or
    /// above which a coordinated pressure seal is triggered. Default 75.
    pub ingress_high_water_percent: usize,
    /// Ingress occupancy fraction (percent of `ingress_limit_bytes`) that
    /// pressure sealing drains toward. Default 50. Must be < high-water.
    pub ingress_low_water_percent: usize,
    /// Maximum age of an active generation before age-based sealing.
    /// Default 600 s.
    pub seal_max_age: Duration,
}

impl ScribePressureConfig {
    /// Construct a pressure config, clamping the low-water mark strictly below
    /// the high-water mark to preserve the hysteresis invariant.
    ///
    /// A caller-supplied `ingress_low_water_percent` at or above
    /// `ingress_high_water_percent` is clamped to `high_water - 1` (and a
    /// zero high-water is treated as low-water `0`), so the returned config can
    /// never invert the seal decision. `seal_max_age` is stored verbatim.
    #[must_use]
    pub fn new(
        ingress_high_water_percent: usize,
        ingress_low_water_percent: usize,
        seal_max_age: Duration,
    ) -> Self {
        let ingress_low_water_percent = if ingress_low_water_percent >= ingress_high_water_percent {
            ingress_high_water_percent.saturating_sub(1)
        } else {
            ingress_low_water_percent
        };
        Self {
            ingress_high_water_percent,
            ingress_low_water_percent,
            seal_max_age,
        }
    }
}

impl Default for ScribePressureConfig {
    /// The rotation defaults: 75% high-water, 50% low-water, 600 s max age.
    fn default() -> Self {
        Self::new(75, 50, Duration::from_mins(10))
    }
}

/// Complete server-provisioned dependencies used to construct one Scribe graph.
///
/// The execution lanes and coordination runtime are supplied by the embedding
/// server so Scribe does not create an unbounded runtime or hide resource
/// sizing inside a durable data-plane component.
pub struct ScribeBuildConfig {
    /// Catalog owner used by Scribe before physical binding or projection.
    pub catalog: Option<Arc<crate::catalog::BifrostCatalog>>,
    /// Object-store operator used by the persistence runtime.
    pub operator: Arc<opendal::Operator>,
    /// WAL writer used by every fixed shard and persistence worker.
    pub wal: Arc<wal::WalWriter>,
    /// Validated node/epoch identity shared by all child owners.
    pub stream: stream_identity::StreamIdentity,
    /// Admission bounds for in-flight frames and active memory.
    pub admission: AdmissionConfig,
    /// Runtime used for Scribe coordination tasks.
    pub coordination_runtime: Handle,
    /// Server-provisioned CPU and WAL execution pools.
    pub execution_pools: ScribeExecutionPools,
    /// Optional server-provisioned immutable persistence dependencies.
    pub persistence: Option<ScribePersistenceConfig>,
    /// Scribe child budget provisioned by server boot.
    pub resources: crate::resources::ScribeResources,
    /// Immutable boot-selected ingest ceilings shared with Gate.
    pub ingest_limits: crate::gate::limits::IngestLimits,
    /// Encoded and uncompressed WAL target for whole-shard rotation.
    pub wal_rotation_bytes: u64,
    /// Target bytes for one non-empty memtable before rotation.
    pub memtable_rotation_bytes: usize,
    /// Maximum age of one active memtable before rotation.
    pub memtable_max_age: Duration,
    /// Optional bounded local wake-up publisher for committed staging files.
    pub staging_file_publisher: Option<StagingFilePublisher>,
}

/// Test-support input for exercising the private native transport seam.
///
/// This DTO exists only under `test-support`; production callers cannot bypass
/// Gate to construct a `ScribeIngressFrame`.
#[cfg(feature = "test-support")]
pub struct NativeIngressTestFrame {
    /// Server-verified principal used by the fixture.
    pub principal: wyrd_runtime::principal::Principal,
    /// Requested logical table preserved through the private seam.
    pub table: crate::catalog::TableRef,
    /// Expected logical source-schema fingerprint for the native IPC stream.
    pub expected_schema_fingerprint: crate::schema::fingerprint::SchemaFingerprint,
    /// Stable request correlation identifier.
    pub request_id: wyrd_spec::request_id::RequestId,
    /// Stable idempotency identifier for this test batch.
    pub batch_id: uuid::Uuid,
    /// Server-shaped audit event committed with the fixture batch.
    pub audit_event: wyrd_spec::vala::api::AuditEvent,
    /// Exact native IPC bytes supplied to the private decoder.
    pub payload: bytes::Bytes,
}

/// Runtime, admission, and memory inputs for an embedded Scribe.
pub struct ScribeEmbeddedConfig {
    /// Optional catalog owner required when the embedded Scribe serves public ingress.
    pub catalog: Option<Arc<BifrostCatalog>>,
    /// Execution lane sizes for the embedded Scribe.
    pub lane_config: ScribeLaneConfig,
    /// Admission bounds for the embedded Scribe.
    pub admission: AdmissionConfig,
    /// Runtime used for Scribe coordination tasks.
    pub coordination_runtime: Handle,
    /// Optional tenant-scoped immutable persistence runtime.
    pub persistence: Option<ScribePersistenceConfig>,
    /// Optional server-provisioned Scribe child budget.
    pub resources: crate::resources::ScribeResources,
    /// Optional bounded publisher for post-commit Forge wake-ups.
    pub staging_file_publisher: Option<StagingFilePublisher>,
    /// Test-tier override for the existing writer-wide size rotation target.
    ///
    /// Production boot constructs [`ScribeBuildConfig`] directly from
    /// `ScribeRuntimeConfig`; this embedded projection exists only so real
    /// server journeys can make the already-configurable production boundary
    /// small without adding a second rotation policy.
    #[cfg(any(test, feature = "test-support"))]
    pub rotation_for_test: Option<ScribeRotationTestConfig>,
}

impl ScribeEmbeddedConfig {
    /// Selects the writer's configured WAL target unless test support overrides it.
    fn wal_rotation_bytes(&self, wal: &wal::WalWriter) -> u64 {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(rotation) = self.rotation_for_test {
            return rotation.wal_rotation_bytes;
        }
        wal.segment_bytes()
    }

    /// Selects the production memtable target unless test support overrides it.
    fn memtable_rotation_bytes(&self) -> usize {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(rotation) = self.rotation_for_test {
            return rotation.memtable_rotation_bytes;
        }
        memtable::MEMTABLE_ROTATION_BYTES
    }

    /// Selects the production age target unless test support overrides it.
    fn memtable_max_age(&self) -> Duration {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(rotation) = self.rotation_for_test {
            return rotation.memtable_max_age;
        }
        ScribePressureConfig::default().seal_max_age
    }
}

/// Test-only projection of the existing production rotation thresholds.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Copy)]
pub struct ScribeRotationTestConfig {
    /// Target bytes for a non-empty WAL generation before whole-shard rotation.
    pub wal_rotation_bytes: u64,
    /// Target bytes for one active shard memtable before whole-shard rotation.
    pub memtable_rotation_bytes: usize,
    /// Maximum active shard-generation age before whole-shard rotation.
    pub memtable_max_age: Duration,
}

/// Composes the embedded Scribe capability through the production root path.
///
/// # Panics
///
/// Panics when the embedded admission configuration cannot cover the protected
/// unmanaged reserve and Scribe floor, which is a construction invariant.
fn embedded_scribe_resources(config: &AdmissionConfig) -> crate::resources::ScribeResources {
    let memory_limit_bytes = config.memory_limit_bytes.max(
        crate::resources::MIN_UNMANAGED_RESERVE_BYTES + crate::resources::ROLE_MEMORY_FLOOR_BYTES,
    );
    let runtime = crate::resources::BifrostRuntimeResources::from_snapshot(
        crate::resources::SystemResourceSnapshot {
            memory_limit_bytes,
            effective_cpu: 1,
            scratch_capacity_bytes: 2 * crate::resources::MIN_SCRATCH_FREE_BYTES,
            scratch_available_bytes: 2 * crate::resources::MIN_SCRATCH_FREE_BYTES,
            memory_source: crate::resources::ResourceSource::Injected,
            cpu_source: crate::resources::ResourceSource::Injected,
        },
        crate::resources::BifrostResourcePolicy {
            roles: [crate::resources::BifrostRole::Scribe]
                .into_iter()
                .collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: Some(crate::resources::MIN_SCRATCH_FREE_BYTES),
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: std::path::PathBuf::new(),
            volume_roots: None,
        },
    )
    .expect("embedded Scribe resource policy must satisfy its configured floor");
    runtime
        .compose_roles()
        .expect("embedded Scribe root must remain healthy")
        .scribe()
        .expect("embedded Scribe role must be enabled")
}

/// Acquires one real root-backed decode owner for crate-local routing tests.
///
/// # Panics
///
/// Panics when the embedded test resource floor cannot admit `bytes`, which
/// indicates the fixture requested more capacity than its production-shaped
/// Scribe root can own.
#[cfg(test)]
pub(crate) fn otlp_decode_owner_for_test(bytes: usize) -> crate::contracts::OtlpDecodeOwner {
    let resources = embedded_scribe_resources(&AdmissionConfig::default());
    let memory = resources
        .try_reserve_ingress(memory::MemoryCategory::Decode, bytes)
        .expect("test OTLP decode owner must fit the embedded Scribe root");
    crate::contracts::OtlpDecodeOwner { memory }
}

/// Builds one production-shaped Scribe root for direct tail fixtures.
///
/// Integration tests pass this capability into the same fence owner used by
/// production readers, so pre-materialization admission and terminal release
/// cannot be bypassed by the direct-memtable adapter.
#[cfg(feature = "test-support")]
#[must_use]
pub fn tail_resources_for_test() -> crate::resources::ScribeResources {
    embedded_scribe_resources(&AdmissionConfig::default())
}

impl ScribeImpl {
    /// Admits one native IPC fixture through the crate-private logical seam.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for the same validation, decode, admission, and
    /// persistence failures as production ingress.
    #[cfg(feature = "test-support")]
    pub async fn ingest_native_for_test(
        &self,
        frame: NativeIngressTestFrame,
    ) -> Result<FrameAdmission, ScribeError> {
        let measured_wire_bytes = frame.payload.len();
        Scribe::ingest_frame(
            self,
            ScribeIngressFrame {
                authenticated_tenant: frame.principal.tenant_id,
                principal: frame.principal,
                table: frame.table,
                expected_schema_fingerprint: Some(frame.expected_schema_fingerprint),
                request_id: frame.request_id,
                batch_id: frame.batch_id,
                audit_event: frame.audit_event,
                measured_wire_bytes,
                payload: crate::contracts::IngressPayload::ArrowIpc(frame.payload),
            },
        )
        .await
    }

    /// Builds the immutable-retirement owner transferred with caller COMMIT attempts.
    fn commit_completion(&self) -> seal::ScribeCommitCompletion {
        seal::ScribeCommitCompletion {
            shards: Arc::clone(&self.shards),
            staging_file_publisher: self.staging_file_publisher.clone(),
            #[cfg(feature = "test-support")]
            publication_observer: self.publication_observer.clone(),
        }
    }
    /// Return the registered writer epoch used by this production Scribe.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub const fn writer_epoch_for_test(&self) -> i64 {
        self.writer_epoch
    }
    /// Construct a new `ScribeImpl` with empty memtable and provided dependencies.
    pub fn new_for_embedded_with_deps(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: &str,
        writer_epoch: i64,
    ) -> Self {
        Self::new_for_embedded_with_runtime(operator, wal, node_id, writer_epoch, Handle::current())
    }

    /// Construct an embedded Scribe that can resolve public logical ingress.
    ///
    /// The supplied catalog remains owned by Scribe so Gate only transports
    /// authenticated logical identity and bounded payload bytes.
    ///
    /// # Panics
    ///
    /// Panics when the node identifier is not a UUID or the embedded resource
    /// policy cannot satisfy Scribe's fixed ownership floors.
    pub fn new_for_embedded_with_deps_and_catalog(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: &str,
        writer_epoch: i64,
        catalog: Arc<BifrostCatalog>,
    ) -> Self {
        let admission = AdmissionConfig::default();
        let resources = embedded_scribe_resources(&admission);
        Self::new_for_embedded_with_runtime_config_and_admission_and_memory(
            operator,
            wal,
            node_id,
            writer_epoch,
            ScribeEmbeddedConfig {
                catalog: Some(catalog),
                lane_config: ScribeLaneConfig::default(),
                admission,
                coordination_runtime: Handle::current(),
                persistence: None,
                resources,
                staging_file_publisher: None,
                #[cfg(any(test, feature = "test-support"))]
                rotation_for_test: None,
            },
        )
    }

    /// Construct the embedded Scribe with a deterministic WAL sync delay.
    ///
    /// This seam is used only by real benchmark and failure-injection
    /// harnesses. Production boot supplies its own explicit pools through
    /// [`ScribeBuildConfig`] and always uses a zero WAL delay.
    ///
    /// # Errors
    ///
    /// Returns an error when a configured execution lane cannot be created or
    /// `node_id` is not a UUID accepted by the WAL stream identity.
    pub fn try_new_for_embedded_with_wal_sync_delay(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: &str,
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
    ///
    /// # Errors
    ///
    /// Returns an error when a configured execution lane cannot be created or
    /// `node_id` is not a UUID accepted by the WAL stream identity.
    pub fn try_new_for_embedded_with_wal_sync_delay_and_admission(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: &str,
        writer_epoch: i64,
        sync_delay: std::time::Duration,
        admission: AdmissionConfig,
    ) -> Result<Self, String> {
        let resources = embedded_scribe_resources(&admission);
        Self::try_new_for_embedded_with_wal_sync_delay_and_admission_and_memory(
            operator,
            wal,
            node_id,
            writer_epoch,
            sync_delay,
            ScribeEmbeddedConfig {
                catalog: None,
                lane_config: ScribeLaneConfig::resolved(),
                admission,
                coordination_runtime: Handle::current(),
                persistence: None,
                resources,
                staging_file_publisher: None,
                #[cfg(any(test, feature = "test-support"))]
                rotation_for_test: None,
            },
        )
    }

    /// Construct the embedded Scribe with deterministic sync delay, explicit
    /// admission limits, and an optional server-provisioned memory governor.
    ///
    /// # Errors
    ///
    /// Returns an error when a configured execution lane cannot be created or
    /// `node_id` is not a UUID accepted by the WAL stream identity.
    pub fn try_new_for_embedded_with_wal_sync_delay_and_admission_and_memory(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: &str,
        writer_epoch: i64,
        sync_delay: std::time::Duration,
        config: ScribeEmbeddedConfig,
    ) -> Result<Self, String> {
        let wal_rotation_bytes = config.wal_rotation_bytes(&wal);
        let memtable_rotation_bytes = config.memtable_rotation_bytes();
        let memtable_max_age = config.memtable_max_age();
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
        let stream = stream_identity::StreamIdentity::new(
            stream_identity::NodeId::new(
                uuid::Uuid::parse_str(node_id).map_err(|error| error.to_string())?,
            ),
            stream_identity::WriterEpoch::new(writer_epoch),
        );
        Ok(Self::new_with_execution_pools(ScribeBuildConfig {
            catalog: config.catalog,
            operator,
            wal,
            stream,
            admission: config.admission,
            coordination_runtime: config.coordination_runtime,
            execution_pools,
            persistence: config.persistence,
            resources: config.resources,
            ingest_limits: crate::gate::limits::IngestLimits::default(),
            wal_rotation_bytes,
            memtable_rotation_bytes,
            memtable_max_age,
            staging_file_publisher: None,
        }))
    }

    /// Construct a Scribe using an explicitly owned Tokio coordination runtime.
    ///
    /// Server deployments use [`Self::new_with_execution_pools`]. Tests and
    /// embedded callers may use this constructor on the current runtime.
    ///
    /// # Panics
    ///
    /// Panics when `node_id` is not a UUID accepted by the WAL stream identity
    /// or the fixed Scribe ownership graph cannot be initialized.
    pub fn new_for_embedded_with_runtime(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: &str,
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
    ///
    /// # Panics
    ///
    /// Panics when `node_id` is not a UUID accepted by the WAL stream identity
    /// or the fixed Scribe ownership graph cannot be initialized.
    pub fn new_for_embedded_with_runtime_config(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: &str,
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
    ///
    /// # Panics
    ///
    /// Panics when `node_id` is not a UUID accepted by the WAL stream identity
    /// or the fixed Scribe ownership graph cannot be initialized.
    pub fn new_for_embedded_with_runtime_config_and_admission(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: &str,
        writer_epoch: i64,
        lane_config: ScribeLaneConfig,
        admission: AdmissionConfig,
        coordination_runtime: Handle,
    ) -> Self {
        let resources = embedded_scribe_resources(&admission);
        Self::new_for_embedded_with_runtime_config_and_admission_and_memory(
            operator,
            wal,
            node_id,
            writer_epoch,
            ScribeEmbeddedConfig {
                catalog: None,
                lane_config,
                admission,
                coordination_runtime,
                persistence: None,
                resources,
                staging_file_publisher: None,
                #[cfg(any(test, feature = "test-support"))]
                rotation_for_test: None,
            },
        )
    }

    /// Construct Scribe with explicit execution lanes, admission bounds, and
    /// an optional server-provisioned memory governor.
    ///
    /// # Panics
    ///
    /// Panics when `node_id` is not a UUID accepted by the WAL stream identity
    /// or the fixed Scribe ownership graph cannot be initialized.
    pub fn new_for_embedded_with_runtime_config_and_admission_and_memory(
        operator: Arc<opendal::Operator>,
        wal: Arc<wal::WalWriter>,
        node_id: &str,
        writer_epoch: i64,
        config: ScribeEmbeddedConfig,
    ) -> Self {
        let wal_rotation_bytes = config.wal_rotation_bytes(&wal);
        let memtable_rotation_bytes = config.memtable_rotation_bytes();
        let memtable_max_age = config.memtable_max_age();
        let stream = stream_identity::StreamIdentity::new(
            stream_identity::NodeId::new(
                uuid::Uuid::parse_str(node_id).expect("embedded Scribe node_id must be a UUID"),
            ),
            stream_identity::WriterEpoch::new(writer_epoch),
        );
        Self::build(ScribeBuildConfig {
            catalog: config.catalog,
            operator,
            wal,
            stream,
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
            persistence: config.persistence,
            resources: config.resources,
            ingest_limits: crate::gate::limits::IngestLimits::default(),
            wal_rotation_bytes,
            memtable_rotation_bytes,
            memtable_max_age,
            staging_file_publisher: config.staging_file_publisher,
        })
    }

    /// Construct Scribe from execution lanes provisioned by server boot.
    pub fn new_with_execution_pools(config: ScribeBuildConfig) -> Self {
        Self::build(config)
    }

    /// Replaces the immutable ingest-limit snapshot for one test-support Scribe.
    ///
    /// Public journey fixtures use this builder before wrapping Scribe in an
    /// `Arc`, ensuring the server adapter, Gate, and Scribe observe one exact
    /// lowerable limits snapshot without changing production defaults. The
    /// in-crate unit tests use the same builder, so it is compiled for `test`
    /// as well as for the `test-support` feature; gating it on the feature
    /// alone breaks the crate's own default-feature test build.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn with_ingest_limits_for_test(
        mut self,
        ingest_limits: crate::gate::limits::IngestLimits,
    ) -> Self {
        self.ingest_limits = ingest_limits;
        self
    }

    /// Builds the complete Scribe ownership graph from server-provisioned dependencies.
    ///
    /// This is the single internal construction path used by production and
    /// embedded factories. It creates persistence before the fixed shard
    /// owners so every child receives the same lanes, WAL identity, memory
    /// ledger, and coordination runtime.
    ///
    /// # Panics
    ///
    /// Panics only if the configured fallback memory governor cannot represent
    /// the fixed one-gibibyte invariant or a shard WAL handle cannot be built.
    fn build(config: ScribeBuildConfig) -> Self {
        let memory = config.resources.clone();
        let memory_ownership =
            memory::ScribeOwnership::new(&memory).expect("zero-sized root ownership must be valid");
        let admission =
            AdmissionController::with_config_and_memory(config.admission, memory.clone());
        let ScribeBuildConfig {
            catalog,
            operator,
            wal,
            stream,
            coordination_runtime,
            execution_pools,
            persistence: persistence_config,
            admission: _,
            resources: _,
            ingest_limits,
            wal_rotation_bytes,
            memtable_rotation_bytes,
            memtable_max_age,
            staging_file_publisher,
        } = config;
        let (node_id, writer_epoch) = (stream.node_id.to_string(), stream.writer_epoch.as_i64());
        let ScribeExecutionPools {
            ingress_cpu,
            persistence_cpu,
            wal_io,
        } = execution_pools;
        let control_postgres = persistence_config
            .as_ref()
            .map(|config| Arc::clone(&config.postgres));
        let persistence = persistence_config.map(|config| {
            #[cfg(any(test, feature = "test-support"))]
            let faults = config.faults.clone();
            persistence::PersistenceRuntime::start(
                config,
                persistence::PersistenceRuntimeContext {
                    operator: Arc::clone(&operator),
                    wal: Arc::clone(&wal),
                    persistence_cpu: persistence_cpu.clone(),
                    wal_io: wal_io.clone(),
                    actor_stream: stream,
                    memory: memory.clone(),
                    staging_file_publisher: staging_file_publisher.clone(),
                    #[cfg(any(test, feature = "test-support"))]
                    faults,
                },
                &coordination_runtime,
            )
        });
        let shards = shards::ScribeShardRuntime::start(
            shards::ScribeShardStartConfig {
                admission: admission.clone(),
                wal_rotation_bytes,
                memtable_rotation_bytes,
                seal_max_age: memtable_max_age,
                wal: Arc::clone(&wal),
                persistence_cpu: persistence_cpu.clone(),
                wal_io: wal_io.clone(),
                persistence: persistence.clone(),
                control_postgres,
                stream,
                memory_ownership: memory_ownership.clone(),
            },
            &coordination_runtime,
        );
        let pressure_config = ScribePressureConfig::new(75, 50, memtable_max_age);
        Self::install_boot_metrics(wal.bytes_on_disk());
        Self {
            catalog,
            operator,
            wal,
            node_id,
            stream,
            writer_epoch,
            admission,
            memory,
            ingest_limits,
            pressure_config,
            memory_ownership,
            persistence_cpu,
            wal_io,
            ingress_cpu,
            shards,
            ingress_lifecycle: Arc::new(ScribeIngressLifecycle::default()),
            closed: AtomicBool::new(false),
            shutdown_state: std::sync::atomic::AtomicU8::new(SHUTDOWN_RUNNING),
            shutdown_notify: tokio::sync::Notify::new(),
            #[cfg(any(test, feature = "test-support"))]
            shutdown_draining_notify: tokio::sync::Notify::new(),
            recovery_ready: AtomicBool::new(true),
            recovery_cancelled: Arc::new(AtomicBool::new(false)),
            persistence,
            staging_file_publisher,
            #[cfg(any(test, feature = "test-support"))]
            ingest_stall: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(any(test, feature = "test-support"))]
            decoded_request_limit_for_test: AtomicUsize::new(0),
            #[cfg(feature = "test-support")]
            publication_observer: ScribePublicationObserver::default(),
        }
    }

    /// Publish the zero-initialized Scribe boot telemetry.
    ///
    /// Called once from [`Self::build`] so the ingress-active gauge, every named
    /// rejection counter, and the WAL disk-bytes gauge exist at value zero (or
    /// the current WAL residency) before the first request, giving scrapers a
    /// stable series set from process start. `wal_disk_bytes` is the current
    /// on-disk WAL byte count read from the freshly recovered writer.
    fn install_boot_metrics(wal_disk_bytes: u64) {
        metrics::gauge!("bifrost_scribe_ingress_active").set(0.0);
        for reason in ["in_flight", "memory", "wal", "queue", "closed", "invalid"] {
            metrics::counter!("bifrost_scribe_rejections_total", "reason" => reason).increment(0);
        }
        metrics::gauge!("bifrost_scribe_wal_disk_bytes")
            .set(wal_disk_bytes.to_f64().unwrap_or(f64::MAX));
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

        let resources = embedded_scribe_resources(&admission_config);
        let wal_rotation_bytes = wal.segment_bytes();
        Self::build(ScribeBuildConfig {
            catalog: None,
            operator,
            wal,
            stream: stream_identity::StreamIdentity::new(
                stream_identity::NodeId::new(
                    uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000")
                        .expect("static test Scribe node_id is a UUID"),
                ),
                stream_identity::WriterEpoch::new(1),
            ),
            admission: admission_config,
            coordination_runtime: Handle::current(),
            execution_pools: ScribeExecutionPools::new(
                ScribeIngressCpuPool::new(1),
                persistence_cpu,
                wal_io,
            ),
            persistence: None,
            resources,
            ingest_limits: crate::gate::limits::IngestLimits::default(),
            wal_rotation_bytes,
            memtable_rotation_bytes: memtable::MEMTABLE_ROTATION_BYTES,
            memtable_max_age: ScribePressureConfig::default().seal_max_age,
            staging_file_publisher: None,
        })
    }

    /// Stop accepting new shard work and drain execution lanes until `deadline`.
    ///
    /// Cancellation may leave durable accepted work for normal recovery. Every
    /// phase is first closed, then awaited only while the caller's process-wide
    /// shutdown budget remains. A cancellation guard synchronously aborts retained
    /// owners if the caller drops this future while it owns the draining state.
    pub async fn shutdown(&self, deadline: std::time::Instant) -> bool {
        if self
            .shutdown_state
            .compare_exchange(
                SHUTDOWN_RUNNING,
                SHUTDOWN_DRAINING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            while self.shutdown_state.load(Ordering::Acquire) != SHUTDOWN_STOPPED {
                let notified = self.shutdown_notify.notified();
                if self.shutdown_state.load(Ordering::Acquire) == SHUTDOWN_STOPPED {
                    break;
                }
                notified.await;
            }
            return true;
        }
        #[cfg(any(test, feature = "test-support"))]
        self.shutdown_draining_notify.notify_waiters();
        let mut cancellation_finalizer = ShutdownCancellationFinalizer {
            scribe: self,
            armed: true,
        };
        let started = std::time::Instant::now();
        self.begin_shutdown();
        let mut graceful = if tokio::time::Instant::now() < tokio::time::Instant::from_std(deadline)
        {
            matches!(
                tokio::time::timeout_at(
                    tokio::time::Instant::from_std(deadline),
                    self.shards.flush_all(),
                )
                .await,
                Ok(Ok(()))
            )
        } else {
            false
        };
        if graceful {
            graceful = await_shutdown_phase(deadline, self.shards.drain()).await;
        }
        // Stop new persistence submissions only after the final shard flush.
        // Keep the CPU and WAL lanes open while already-queued generations
        // finish encoding and publish their file-list rows.
        if graceful && let Some(persistence) = &self.persistence {
            persistence.close();
        }
        if graceful && let Some(persistence) = &self.persistence {
            graceful = await_shutdown_phase(deadline, persistence.drain()).await;
        }
        self.close_lanes();
        if graceful {
            graceful = await_shutdown_phase(deadline, self.shards.shutdown(deadline)).await;
        }
        if graceful {
            graceful = await_shutdown_phase(deadline, self.persistence_cpu.drain()).await;
        }
        if graceful {
            graceful = await_shutdown_phase(deadline, self.wal_io.drain()).await;
        }
        if graceful {
            graceful = await_shutdown_phase(deadline, self.ingress_cpu.drain()).await;
        }
        if self
            .shutdown_state
            .compare_exchange(
                SHUTDOWN_DRAINING,
                SHUTDOWN_FINALIZING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            self.finalize_shutdown_owners();
        } else {
            while self.shutdown_state.load(Ordering::Acquire) != SHUTDOWN_STOPPED {
                let notified = self.shutdown_notify.notified();
                if self.shutdown_state.load(Ordering::Acquire) == SHUTDOWN_STOPPED {
                    break;
                }
                notified.await;
            }
        }
        if !graceful {
            tracing::warn!("Scribe graceful cleanup was incomplete at shutdown deadline");
        }
        metrics::histogram!("bifrost_scribe_shutdown_seconds")
            .record(started.elapsed().as_secs_f64());
        cancellation_finalizer.disarm();
        graceful
    }

    /// Closes external admission and aborts every retained Tokio worker without waiting.
    ///
    /// This is the deadline-expiry path. It deliberately skips graceful flush,
    /// closes execution lanes, and leaves any unfinished durable work to WAL
    /// recovery. No external await or detached cleanup is started.
    pub fn abort_shutdown(&self) {
        loop {
            let state = self.shutdown_state.load(Ordering::Acquire);
            if matches!(state, SHUTDOWN_FINALIZING | SHUTDOWN_STOPPED) {
                return;
            }
            if self
                .shutdown_state
                .compare_exchange(
                    state,
                    SHUTDOWN_FINALIZING,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                break;
            }
        }
        self.begin_shutdown();
        self.close_lanes();
        self.finalize_shutdown_owners();
    }

    /// Abort retained async owners and publish the terminal stopped state.
    fn finalize_shutdown_owners(&self) {
        let aborted_shards = self.shards.abort_retained();
        self.shards.clear_retained_join_handles();
        let aborted_persistence = self
            .persistence
            .as_ref()
            .map_or(0, |persistence| persistence.abort_retained());
        if let Some(persistence) = &self.persistence {
            persistence.clear_retained_join_handles();
        }
        if let Err(error) = self.wal.close_all_streams() {
            tracing::error!(error = %error, "Scribe shutdown could not close every WAL stream owner");
        }
        tracing::debug!(
            aborted_shards,
            aborted_persistence,
            "Scribe shard and persistence owners finalized"
        );
        self.shutdown_state
            .store(SHUTDOWN_STOPPED, Ordering::Release);
        self.shutdown_notify.notify_waiters();
    }

    /// Closes external Scribe admission without cancelling internal flush lanes.
    ///
    /// Already accepted writable generations may still use the internal lanes
    /// during [`Self::shutdown`]'s bounded flush. Dropping shutdown after this
    /// transition is fail-closed; [`Self::abort_shutdown`] performs final abort.
    fn begin_shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.recovery_cancelled.store(true, Ordering::Release);
        self.shards.close();
    }

    /// Closes internal execution lanes after graceful flush has completed or timed out.
    ///
    /// Persistence remains available until this transition so accepted writable
    /// generations can be frozen and submitted during the bounded flush. Once
    /// closed, later drains can only finish work that was already admitted.
    fn close_lanes(&self) {
        if let Some(persistence) = &self.persistence {
            persistence.close();
        }
        self.persistence_cpu.close();
        self.wal_io.close();
        self.ingress_cpu.close();
    }

    /// Installs one retained never-completing shard task for shutdown-bound tests.
    ///
    /// The returned abort handle lets the production-owner test prove the real
    /// Scribe finalizer cancelled the task rather than merely dropping its wait.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn install_shutdown_stall_for_test(&self) -> tokio::task::AbortHandle {
        self.shards.install_shutdown_stall_for_test().await
    }

    /// Wait until graceful shutdown owns the draining state.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn wait_shutdown_draining_for_test(&self) {
        while self.shutdown_state.load(Ordering::Acquire) != SHUTDOWN_DRAINING {
            let notified = self.shutdown_draining_notify.notified();
            if self.shutdown_state.load(Ordering::Acquire) == SHUTDOWN_DRAINING {
                break;
            }
            notified.await;
        }
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
    #[cfg(any(test, feature = "test-support"))]
    pub async fn flush_writable_for_test(&self) -> Result<(), ScribeError> {
        self.shards.flush_all().await
    }

    /// Retire all committed generations through an acknowledged test-only retirement pass.
    ///
    /// Production lifecycle scheduling uses `Scribe::check_age` as a
    /// coalescing best-effort signal. Tests use this control when they need to
    /// observe the public read path after every eligible committed generation
    /// has been retired.
    ///
    /// Retirement is immediate: all `ImmutableState::Committed` generations across
    /// every shard are retired on this call with no grace period.
    ///
    /// # Errors
    ///
    /// Returns the owner error when a shard cannot run the retirement pass,
    /// including when a shard has stopped.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn retire_committed_for_test(&self) -> Result<(), ScribeError> {
        self.shards.retire_committed_for_test().await
    }

    /// Return the bounded persistence queue depth for test-tier drain checks.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn persistence_queue_depth_for_test(&self) -> usize {
        self.persistence
            .as_ref()
            .map_or(0, |persistence| persistence.queue_depth())
    }

    /// Return the exact admitted request count still owned by Scribe.
    ///
    /// This test-support inspection reads the production admission owner; it
    /// does not infer in-flight work from shard queue depth.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn inflight_items_for_test(&self) -> usize {
        self.admission.snapshot().items
    }

    /// Read a governor snapshot enriched with the runtime ingress watermarks.
    ///
    /// The governor fills `ingress_occupancy_bytes` and `ingress_limit_bytes`;
    /// this method layers the D83 high/low-water bytes from
    /// [`ScribeImpl::pressure_config`] so every watermark decision — admission
    /// and the periodic age scanner alike — reads one consistent snapshot.
    #[must_use]
    fn pressure_snapshot(&self) -> memory::MemorySnapshot {
        self.memory.memory_snapshot().with_ingress_watermarks(
            self.pressure_config.ingress_high_water_percent,
            self.pressure_config.ingress_low_water_percent,
        )
    }

    /// Request a coordinated pressure seal that drains ingress toward low-water.
    ///
    /// This is the single definition of the flush-first hysteresis, shared by
    /// the admission path (`Scribe::prepare_and_dispatch`) and the periodic age
    /// scanner (`Scribe::check_age`). It is a no-op below the high-water mark;
    /// at or above it, it selects the largest writable buckets aggregated across
    /// all shards (shard-count-invariant, via
    /// [`memtable::Memtable::select_pressure_victims`]) sufficient to release
    /// occupancy down to the low-water target, and fans a fire-and-forget flush
    /// signal to their owners. It never blocks or busy-waits: freezing and
    /// persistence proceed asynchronously, and callers retry the reservation
    /// once (admission) or wait for the next tick (scanner).
    fn request_pressure_seal_toward_low_water(&self) {
        let snapshot = self.pressure_snapshot();
        let Some(to_release) =
            snapshot.pressure_release_bytes(self.pressure_config.ingress_high_water_percent)
        else {
            return;
        };
        let candidates = self
            .shards
            .memtable_snapshots()
            .unwrap_or_default()
            .into_iter()
            .flat_map(|snapshot| snapshot.pressure_candidates)
            .collect::<Vec<_>>();
        let victims = memtable::Memtable::select_pressure_victims(candidates, to_release);
        tracing::debug!(
            trigger = SealTriggerReason::Pressure.as_label(),
            to_release,
            victims = victims.len(),
            "requesting coordinated ingress-pressure seal"
        );
        self.shards.request_pressure_flush(&victims);
    }

    /// Publish one coalescing lifecycle age tick to every shard owner.
    ///
    /// The ingress-pressure branch shares its hysteresis with admission through
    /// `Scribe::request_pressure_seal_toward_low_water`: crossing the ingress
    /// high-water mark seals down toward low-water; below low-water it no-ops.
    /// The independent WAL-disk soft-pressure branch is unchanged.
    ///
    /// As the production steady-state tick, it first exports the governor and
    /// memtable gauges (D84) so per-child occupancy, the D83 ingress watermarks,
    /// and the memtable seal-decision inputs are observable every tick. It also
    /// emits one debug-level governor snapshot line per tick — child totals,
    /// the derived parent-only remainder, and every per-category total — so a
    /// saturated ceiling can be attributed to its holder from logs alone
    /// (ceiling refusals collapse to one `IngestBusy` message and cannot name
    /// the holder themselves). All exports are emission-only and precede the
    /// flush requests; they never change the seal, age, or pressure decisions
    /// the rest of the method makes.
    pub fn check_age(&self, now: std::time::Instant) {
        let snapshot = self.pressure_snapshot();
        self.memory.emit_root_resource_gauges();
        snapshot.emit_ingress_watermark_gauges();
        tracing::debug!(
            scribe_total = snapshot.scribe_total_bytes,
            oracle_total = snapshot.oracle_total_bytes,
            bifrost_total = snapshot.bifrost_total_bytes,
            parent_only = snapshot.bifrost_total_bytes.saturating_sub(
                snapshot
                    .scribe_total_bytes
                    .saturating_add(snapshot.oracle_total_bytes)
            ),
            raw = snapshot.categories[memory::MemoryCategory::Raw as usize],
            decode = snapshot.categories[memory::MemoryCategory::Decode as usize],
            prepared = snapshot.categories[memory::MemoryCategory::Prepared as usize],
            queued = snapshot.categories[memory::MemoryCategory::Queued as usize],
            active = snapshot.categories[memory::MemoryCategory::Active as usize],
            immutable = snapshot.categories[memory::MemoryCategory::Immutable as usize],
            persistence = snapshot.categories[memory::MemoryCategory::Persistence as usize],
            metadata = snapshot.categories[memory::MemoryCategory::Metadata as usize],
            "governor tick snapshot"
        );
        if let Ok(stats) = self.aggregate_memtable_stats() {
            emit_memtable_gauges(&stats);
        }
        self.shards.request_expired_flush(now);
        self.request_pressure_seal_toward_low_water();
        if self.wal.disk_pressure().soft {
            let candidates = self
                .shards
                .memtable_snapshots()
                .unwrap_or_default()
                .into_iter()
                .flat_map(|snapshot| snapshot.pressure_candidates)
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
    /// Returns `ScribeCommitAttempt` handles whose post-commit tokens must be completed
    /// only after the caller commits the transaction. The caller owns commit/rollback.
    ///
    /// Repo rule (`check:from-pools-allowlist`): this signature MUST take
    /// `&mut vala_sql::TenantConn<'_>` and MUST NOT accept `sqlx::PgPool`.
    ///
    /// Under batch-spread routing a seal key may have buckets on several shards,
    /// so `ScribeShardRuntime::freeze_key` now returns
    /// every shard's frozen memtable. Each is pre-committed within the caller's
    /// transaction and returned as a separate [`seal::ScribeCommitAttempt`], so no frozen
    /// Arrow data is orphaned. If any pre-commit fails after earlier ones
    /// succeeded, the already-committed generations are aborted back to the
    /// active ledger before the error is returned, mirroring
    /// [`Self::force_seal`].
    ///
    /// # Errors
    /// Returns [`ScribeError`] if tenant validation, freezing, or any seal stage
    /// fails. On a mid-batch failure the partial post-commit state is rolled
    /// back before the error surfaces.
    pub async fn seal_one(
        &self,
        seal_key: &SealKey,
        conn: &mut TenantConn<'_>,
    ) -> Result<Vec<seal::ScribeCommitAttempt>, ScribeError> {
        let binding = TenantTableBinding::resolve((seal_key.tenant, seal_key.table.clone()))
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        binding
            .validate_authenticated_tenant(conn.data_tenant_id())
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        if self
            .persistence
            .as_ref()
            .and_then(|runtime| runtime.publication_reconciler())
            .is_none()
        {
            return Err(ScribeError::Internal {
                detail: "caller-owned Scribe seal requires a publication reconciler before encode"
                    .to_owned(),
            });
        }

        let driver = SealDriver::new_with_lane(
            self.operator.clone(),
            self.persistence_cpu.clone(),
            self.persistence
                .as_ref()
                .and_then(|runtime| runtime.output_scratch()),
            Some(self.memory.clone()),
        );
        let frozen_set = self.shards.freeze_key(seal_key.clone()).await?;
        let mut commits = Vec::with_capacity(frozen_set.len());
        for frozen in &frozen_set {
            match driver
                .pre_commit_frozen(
                    frozen,
                    seal_key,
                    &binding,
                    conn,
                    &self.node_id,
                    self.writer_epoch,
                )
                .await
            {
                Ok(mut commit) => {
                    commit.attach_completion(self.commit_completion());
                    commits.push(commit);
                }
                Err(error) => {
                    // Roll back the generations already pre-committed in this
                    // batch so no shard is left with orphaned immutable state.
                    let _ = self.abort_commit_attempts(commits).await;
                    return Err(error);
                }
            }
        }
        Ok(commits)
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
    /// Acquires the adapter-decode child from Scribe's ingress resources.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when another owner occupies the
    /// ingress envelope, or [`ScribeError::Internal`] for accounting, poison,
    /// or other resource-owner failures.
    fn reserve_otlp_decode(
        &self,
        bytes: usize,
    ) -> Result<crate::contracts::OtlpDecodeOwner, ScribeError> {
        let memory = self
            .memory
            .try_reserve_ingress(memory::MemoryCategory::Decode, bytes)?;
        Ok(crate::contracts::OtlpDecodeOwner { memory })
    }

    /// Reports whether recovery has opened the private durable ingress seam.
    fn is_ready(&self) -> bool {
        Self::is_ready(self)
    }

    /// Prepares one logical Gate frame and waits for durable completion.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when resolution, projection, admission,
    /// persistence, or durable acknowledgment fails.
    async fn ingest_frame(&self, frame: ScribeIngressFrame) -> Result<FrameAdmission, ScribeError> {
        let active = metrics::gauge!("bifrost_scribe_ingress_active");
        active.increment(1.0);
        let _active = ScribeIngressTelemetryGuard(active);
        if !self.is_ready() {
            record_scribe_rejection("closed");
            return Err(ScribeError::IngressClosed);
        }
        let admission = self
            .prepare_and_dispatch(frame)
            .await
            .inspect_err(|error| {
                let reason = match error {
                    ScribeError::UnsupportedWalVersion { .. } => Some("wal"),
                    ScribeError::PayloadTooLarge { .. }
                    | ScribeError::DecodedPayloadTooLarge { .. }
                    | ScribeError::TooManyRows { .. }
                    | ScribeError::InvalidFrame
                    | ScribeError::EventTimeOutOfRange { .. }
                    | ScribeError::FingerprintMismatch { .. }
                    | ScribeError::TableNotFound { .. }
                    | ScribeError::CardScopeDenied
                    | ScribeError::CardUnresolved
                    | ScribeError::StreamMismatch { .. } => Some("invalid"),
                    ScribeError::IngressClosed
                    | ScribeError::WalDiskFull
                    | ScribeError::IngestBusy { .. }
                    | ScribeError::ObjectStorePutFailed(_)
                    | ScribeError::Internal { .. } => None,
                };
                if let Some(reason) = reason {
                    record_scribe_rejection(reason);
                }
            })?;
        #[cfg(feature = "test-support")]
        self.publication_observer
            .record(ScribePublicationEvent::Acknowledged {
                batch_id: admission.batch_id,
                rows: admission.rows_accepted,
            });
        Ok(admission)
    }
}

/// Record one Scribe admission rejection using only closed owner labels.
fn record_scribe_rejection(reason: &'static str) {
    metrics::counter!("bifrost_scribe_rejections_total", "reason" => reason).increment(1);
}

/// Record one memory-ceiling rejection labelled by the ceiling that tripped (D84).
///
/// This shares the `bifrost_scribe_rejections_total` counter family with
/// [`record_scribe_rejection`] but attaches the closed
/// [`ScribeRejectionCeiling::as_metric_label`] value as the `reason`, so a
/// dashboard can tell a Scribe-child overflow apart from an ingress-sublimit,
/// parent-ceiling, or cgroup-breaker rejection instead of reading one collapsed
/// `reason="memory"`. The label set is closed to the five ceiling values and
/// carries no tenant, table, or request identity.
fn record_scribe_ceiling_rejection(ceiling: ScribeRejectionCeiling) {
    record_scribe_rejection(ceiling.as_metric_label());
}

/// Emit the three aggregate memtable gauges from a pod-level stats snapshot.
///
/// Emission-only: this sets the live gauges and never mutates admission state,
/// so it is safe to call from the periodic age scanner ([`ScribeImpl::check_age`])
/// as well as [`ScribeImpl::memtable_stats`]. The three gauges —
/// `bifrost_scribe_active_memtable_bytes`, `bifrost_scribe_immutable_memtable_bytes`,
/// and `bifrost_scribe_immutable_generation_count` — make the seal decision
/// inputs (writable pressure, immutable backlog, generation depth) observable
/// from production telemetry. The values are pod-global aggregates and carry no
/// identity labels.
fn emit_memtable_gauges(stats: &memtable::MemtableStats) {
    metrics::gauge!("bifrost_scribe_active_memtable_bytes")
        .set(stats.writable_bytes.to_f64().unwrap_or(f64::MAX));
    metrics::gauge!("bifrost_scribe_immutable_memtable_bytes")
        .set(stats.immutable_bytes.to_f64().unwrap_or(f64::MAX));
    metrics::gauge!("bifrost_scribe_immutable_generation_count")
        .set(stats.immutable_generations.to_f64().unwrap_or(f64::MAX));
}

/// Drop guard that drains the Scribe active-ingress gauge on every exit.
struct ScribeIngressTelemetryGuard(metrics::Gauge);

impl Drop for ScribeIngressTelemetryGuard {
    /// Release one active ingress ownership unit.
    fn drop(&mut self) {
        self.0.decrement(1.0);
    }
}

#[cfg(test)]
mod pressure_config_tests {
    use super::ScribePressureConfig;
    use std::time::Duration;

    /// The D83 defaults are the locked 75/50/600 s triple that T40 reads.
    #[test]
    fn default_is_the_locked_d83_triple() {
        let config = ScribePressureConfig::default();
        assert_eq!(config.ingress_high_water_percent, 75);
        assert_eq!(config.ingress_low_water_percent, 50);
        assert_eq!(config.seal_max_age, Duration::from_mins(10));
    }

    /// A low-water at or above high-water is clamped strictly below it, so the
    /// hysteresis invariant `low < high` can never be inverted by config.
    #[test]
    fn new_clamps_low_water_strictly_below_high_water() {
        let inverted = ScribePressureConfig::new(60, 90, Duration::from_secs(5));
        assert_eq!(inverted.ingress_high_water_percent, 60);
        assert_eq!(inverted.ingress_low_water_percent, 59);
        assert!(inverted.ingress_low_water_percent < inverted.ingress_high_water_percent);

        let equal = ScribePressureConfig::new(75, 75, Duration::from_secs(5));
        assert_eq!(equal.ingress_low_water_percent, 74);
    }
}

#[cfg(test)]
mod constructor_rotation_tests {
    use super::*;

    /// Embedded construction preserves production-selected WAL, memtable, and
    /// age thresholds when no test-only override is supplied.
    #[tokio::test]
    async fn embedded_constructor_preserves_selected_wal_rotation_target() {
        let segment_bytes = 37 * 1024 * 1024;
        let wal_root = tempfile::tempdir().expect("WAL root");
        let wal = Arc::new(
            wal::WalWriter::new(
                wal_root.path(),
                *uuid::Uuid::now_v7().as_bytes(),
                1,
                wal::WalConfig::new(segment_bytes).expect("nondefault WAL config"),
            )
            .expect("nondefault WAL writer"),
        );
        let operator = Arc::new(
            opendal::Operator::new(opendal::services::Memory::default())
                .expect("memory backend")
                .finish(),
        );
        let scribe = ScribeImpl::try_new_for_embedded_with_wal_sync_delay_and_admission(
            operator,
            Arc::clone(&wal),
            &uuid::Uuid::now_v7().to_string(),
            1,
            Duration::ZERO,
            AdmissionConfig::default(),
        )
        .expect("embedded Scribe");
        assert_eq!(wal.segment_bytes(), segment_bytes);
        assert_eq!(
            scribe.shards.rotation_thresholds_for_test(),
            (segment_bytes, memtable::MEMTABLE_ROTATION_BYTES)
        );
        assert_eq!(
            scribe.pressure_config.seal_max_age,
            ScribePressureConfig::default().seal_max_age
        );
        scribe
            .shutdown(std::time::Instant::now() + Duration::from_secs(1))
            .await;
    }

    /// Embedded construction applies every explicitly selected test-tier
    /// threshold to the same shard-owner graph used by production boot.
    #[tokio::test]
    async fn embedded_constructor_applies_rotation_for_test_override() {
        let segment_bytes = 41 * 1024 * 1024;
        let memtable_bytes = 23 * 1024 * 1024;
        let max_age = Duration::from_secs(7);
        let wal_root = tempfile::tempdir().expect("WAL root");
        let wal = Arc::new(
            wal::WalWriter::new(
                wal_root.path(),
                *uuid::Uuid::now_v7().as_bytes(),
                1,
                wal::WalConfig::new(segment_bytes).expect("nondefault WAL config"),
            )
            .expect("nondefault WAL writer"),
        );
        let operator = Arc::new(
            opendal::Operator::new(opendal::services::Memory::default())
                .expect("memory backend")
                .finish(),
        );
        let admission = AdmissionConfig::default();
        let configured_request_bytes = 201 * 1024 * 1024;
        let defaults = crate::gate::limits::IngestLimits::default();
        let ingest_limits = crate::gate::limits::IngestLimits {
            max_frame_bytes: configured_request_bytes,
            max_decoding_message_size: configured_request_bytes + 64 * 1024,
            otlp: crate::gate::limits::OtlpWireLimits {
                request_bytes: configured_request_bytes,
                ..defaults.otlp
            },
            ..defaults
        };
        let scribe = ScribeImpl::new_for_embedded_with_runtime_config_and_admission_and_memory(
            operator,
            wal,
            &uuid::Uuid::now_v7().to_string(),
            1,
            ScribeEmbeddedConfig {
                catalog: None,
                lane_config: ScribeLaneConfig::default(),
                admission,
                coordination_runtime: Handle::current(),
                persistence: None,
                resources: embedded_scribe_resources(&admission),
                staging_file_publisher: None,
                rotation_for_test: Some(ScribeRotationTestConfig {
                    wal_rotation_bytes: segment_bytes,
                    memtable_rotation_bytes: memtable_bytes,
                    memtable_max_age: max_age,
                }),
            },
        )
        .with_ingest_limits_for_test(ingest_limits);

        assert_eq!(
            scribe.shards.rotation_thresholds_for_test(),
            (segment_bytes, memtable_bytes)
        );
        assert_eq!(scribe.pressure_config.seal_max_age, max_age);
        assert_eq!(scribe.ingest_limits, ingest_limits);
        scribe
            .shutdown(std::time::Instant::now() + Duration::from_secs(1))
            .await;
    }
}

#[cfg(test)]
mod telemetry_tests {
    /// Decode scratch remains charged during construction and settles separately.
    ///
    /// # Panics
    ///
    /// Panics when the production-equivalent root cannot reserve, split, or
    /// release the exact decode capacities.
    #[tokio::test]
    async fn otlp_decode_scratch_split_settles_exactly_once() {
        let scribe = super::ScribeImpl::default();
        let baseline = scribe
            .memory
            .snapshot()
            .expect("baseline resource snapshot")
            .scribe_memory_used_bytes;
        let mut owner =
            <super::ScribeImpl as crate::contracts::Scribe>::reserve_otlp_decode(&scribe, 1024)
                .expect("decode owner reservation");
        assert_eq!(
            scribe
                .memory
                .snapshot()
                .expect("reserved resource snapshot")
                .scribe_memory_used_bytes,
            baseline + 1024
        );

        let scratch = owner.split_scratch(256).expect("scratch split");
        assert_eq!(
            scribe
                .memory
                .snapshot()
                .expect("split resource snapshot")
                .scribe_memory_used_bytes,
            baseline + 1024
        );
        drop(scratch);
        assert_eq!(
            scribe
                .memory
                .snapshot()
                .expect("scratch release snapshot")
                .scribe_memory_used_bytes,
            baseline + 768
        );
        let decoded = crate::contracts::DecodedOtlp::new((), 1, owner);
        assert_eq!(decoded.decode_bytes, 768);
        drop(decoded);
        assert_eq!(
            scribe
                .memory
                .snapshot()
                .expect("terminal resource snapshot")
                .scribe_memory_used_bytes,
            baseline
        );
    }

    /// The steady-state age path emits only root-owned capacity metric families.
    ///
    /// # Panics
    ///
    /// Panics when the embedded production-equivalent Scribe cannot emit its
    /// root snapshot or when a legacy competing capacity family is observed.
    #[tokio::test]
    async fn root_resource_metrics_replace_legacy_capacity_families() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            super::ScribeImpl::default().check_age(std::time::Instant::now());
        });
        let snapshot = recorder.snapshot();
        assert!(
            snapshot
                .gauges
                .keys()
                .any(|key| key.starts_with("bifrost_resource_memory_bytes{"))
        );
        assert!(
            snapshot
                .gauges
                .keys()
                .any(|key| key.starts_with("bifrost_resource_scratch_bytes{"))
        );
        assert!(snapshot.gauges.keys().all(|key| {
            !key.starts_with("bifrost_memory_reserved_bytes")
                && !key.starts_with("bifrost_memory_limit_bytes")
        }));
    }

    /// Dropping the ingress owner drains its exact active gauge without identity labels.
    #[test]
    fn ingress_telemetry_guard_cancellation_drains_active_zero() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let active = metrics::gauge!("bifrost_scribe_ingress_active");
            active.increment(1.0);
            drop(super::ScribeIngressTelemetryGuard(active));
        });
        let snapshot = recorder.snapshot();
        assert!(
            snapshot
                .gauges
                .get("bifrost_scribe_ingress_active")
                .is_some_and(|value| value.abs() <= f64::EPSILON)
        );
        assert!(!snapshot.gauges.keys().any(|key| {
            ["tenant", "table", "path", "request", "node", "error", "sql"]
                .iter()
                .any(|forbidden| key.contains(forbidden))
        }));
    }

    /// Every ceiling rejection reason is a closed `snake_case` label with no identity.
    ///
    /// Emitting all five D84 ceilings through
    /// [`super::record_scribe_ceiling_rejection`] proves the write-path rejection
    /// counter labels the ceiling that tripped and never widens the label set
    /// with tenant, table, path, request, node, error, or sql identity.
    #[test]
    fn write_path_metrics_carry_no_tenant_or_table_label() {
        use super::ScribeRejectionCeiling;
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            for ceiling in [
                ScribeRejectionCeiling::CgroupBreaker,
                ScribeRejectionCeiling::ScribeChild,
                ScribeRejectionCeiling::IngressSublimit,
                ScribeRejectionCeiling::BifrostParent,
                ScribeRejectionCeiling::CgroupParent,
            ] {
                super::record_scribe_ceiling_rejection(ceiling);
            }
        });
        let snapshot = recorder.snapshot();
        for reason in [
            "cgroup_breaker",
            "scribe_child",
            "ingress_sublimit",
            "bifrost_parent",
            "cgroup_parent",
        ] {
            let key = format!("bifrost_scribe_rejections_total{{reason=\"{reason}\"}}");
            assert_eq!(
                snapshot.counters.get(&key).copied(),
                Some(1),
                "missing ceiling rejection {key}"
            );
        }
        assert!(!snapshot.counters.keys().any(|key| {
            ["tenant", "table", "path", "request", "node", "error", "sql"]
                .iter()
                .any(|forbidden| key.contains(forbidden))
        }));
    }
}

impl ScribeImpl {
    /// Return the passive typed publication observer owned by this Scribe.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn publication_observer_for_test(&self) -> ScribePublicationObserver {
        self.publication_observer.clone()
    }
    /// Install a one-shot test barrier at the public write seam.
    #[cfg(any(test, feature = "test-support"))]
    pub fn stall_next_ingest_for_test(&self) -> Arc<IngestStall> {
        let stall = Arc::new(IngestStall::default());
        if let Ok(mut current) = self.ingest_stall.lock() {
            *current = Some(Arc::clone(&stall));
        }
        stall
    }
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

    /// Aggregate per-shard memtable state into one pod-level snapshot.
    ///
    /// Pure read: it sums every shard's writable and immutable counters without
    /// touching admission or emitting telemetry, so both the admission-syncing
    /// [`Self::memtable_stats`] and the emission-only age scanner
    /// (`Scribe::check_age`) can share one aggregation without one path forcing
    /// the other's side effects.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a shard memtable snapshot cannot be read.
    fn aggregate_memtable_stats(&self) -> Result<memtable::MemtableStats, ScribeError> {
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
        Ok(stats)
    }

    /// Return aggregate writable and immutable memtable state.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a shard memtable snapshot cannot be read.
    pub fn memtable_stats(&self) -> Result<memtable::MemtableStats, ScribeError> {
        let stats = self.aggregate_memtable_stats()?;
        self.admission
            .sync_memtable_bytes(stats.writable_bytes, stats.immutable_bytes);
        emit_memtable_gauges(&stats);
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
        self.memory.memory_snapshot()
    }

    /// Return the bounded setup and ownership snapshot used by test harnesses.
    ///
    /// Governor totals are authoritative. Bucket and transient-shard
    /// attribution is retried for coherence, but under uninterrupted writes the
    /// method returns the latest independently sampled attribution rather than
    /// failing an otherwise valid operational inspection.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when memtable statistics or per-shard owner
    /// snapshots cannot be collected.
    pub fn inspection_snapshot(&self) -> Result<ScribeInspectionSnapshot, ScribeError> {
        let stats = self.memtable_stats()?;
        let mut coherent = None;
        let mut latest = None;
        for _ in 0..64 {
            let before = self.memory.memory_snapshot();
            let owner_snapshots = self.shards.memtable_snapshots()?;
            let transient_by_shard = self.memory.shard_snapshot();
            let memory = self.memory.memory_snapshot().with_ingress_watermarks(
                self.pressure_config.ingress_high_water_percent,
                self.pressure_config.ingress_low_water_percent,
            );
            let bucket_total = owner_snapshots
                .iter()
                .flat_map(|snapshot| &snapshot.bucket_memory)
                .map(|bucket| bucket.writable_bytes.saturating_add(bucket.immutable_bytes))
                .sum::<usize>();
            let transient_total = transient_by_shard.into_iter().sum::<usize>();
            latest = Some((memory, owner_snapshots.clone(), transient_by_shard));
            if before.total_bytes() == memory.total_bytes()
                && bucket_total.saturating_add(transient_total) == memory.total_bytes()
            {
                coherent = Some((memory, owner_snapshots, transient_by_shard));
                break;
            }
            std::hint::spin_loop();
        }
        let (memory, owner_snapshots, transient_by_shard) =
            coherent.or(latest).ok_or_else(|| ScribeError::Internal {
                detail: "inspection could not read bucket and shard ownership".to_owned(),
            })?;
        let mut memory_by_shard = [0_usize; crate::scribe::routing::SCRIBE_SHARD_COUNT];
        let mut memory_by_bucket = Vec::new();
        for (shard, snapshot) in owner_snapshots.into_iter().enumerate() {
            for bucket in snapshot.bucket_memory {
                let bytes = bucket.writable_bytes.saturating_add(bucket.immutable_bytes);
                memory_by_shard[shard] = memory_by_shard[shard].saturating_add(bytes);
                memory_by_bucket.push(ScribeBucketMemorySnapshot {
                    seal_key: bucket.seal_key,
                    writable_bytes: bucket.writable_bytes,
                    immutable_bytes: bucket.immutable_bytes,
                });
            }
        }
        for (shard, bytes) in transient_by_shard.into_iter().enumerate() {
            memory_by_shard[shard] = memory_by_shard[shard].saturating_add(bytes);
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
            ingress_used_memory: memory.ingress_occupancy_bytes,
            ingress_memory_limit: memory.ingress_limit_bytes,
            ingress_high_water_memory: memory.ingress_high_water_bytes,
            ingress_low_water_memory: memory.ingress_low_water_bytes,
            wal_disk_bytes: self.wal.bytes_on_disk(),
            ingress_lifecycle: self.ingress_lifecycle.snapshot(),
            generation_lifecycle: self.memory_ownership.lifecycle_snapshot(),
        })
    }

    /// Trip the WAL availability breaker for deterministic test-tier probes.
    #[cfg(any(test, feature = "test-support"))]
    pub fn trip_wal_disk_full_for_test(&self) {
        self.admission.trip_wal_disk_full();
    }

    /// Returns the bounded CPU pool used by Scribe's ingest materialization.
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
    ) -> Result<Vec<seal::ScribeCommitAttempt>, ScribeError> {
        if self
            .persistence
            .as_ref()
            .and_then(|runtime| runtime.publication_reconciler())
            .is_none()
        {
            return Err(ScribeError::Internal {
                detail: "caller-owned Scribe seal requires a publication reconciler before encode"
                    .to_owned(),
            });
        }
        self.shards.drain().await;
        // A `TenantConn` is bound to exactly one tenant. Seal only the
        // memtable buckets whose seal-key belongs to that tenant; the harness
        // iterates tenants and opens a fresh `TenantConn` per tenant.
        //
        let tenant = conn.data_tenant_id();
        let frozen = self.shards.freeze_tenant(tenant).await?;
        let driver = seal::SealDriver::new_with_lane(
            self.operator.clone(),
            self.persistence_cpu.clone(),
            self.persistence
                .as_ref()
                .and_then(|runtime| runtime.output_scratch()),
            Some(self.memory.clone()),
        );

        let mut attempts = Vec::with_capacity(frozen.len());
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
                Ok(mut handle) => {
                    handle.attach_completion(self.commit_completion());
                    #[cfg(feature = "test-support")]
                    if let Some(token) = handle.token() {
                        self.publication_observer
                            .record(ScribePublicationEvent::Sealed {
                                seal_id: token.seal_id,
                                file_list_row_id: token.file_list_row_id,
                                batch_ids: token.batch_ids.clone(),
                                wal_lsn_min: token.wal_lsn_min.as_u64(),
                                wal_lsn_max: token.wal_lsn_max.as_u64(),
                            });
                    }
                    attempts.push(handle);
                }
                Err(error) => {
                    let _ = self.abort_commit_attempts(attempts).await;
                    return Err(error);
                }
            }
        }

        Ok(attempts)
    }

    /// Settles caller-owned Scribe publications from the exact COMMIT result.
    ///
    /// A confirmed commit completes immutable ownership normally. Every commit
    /// error is ambiguous: the same deterministic full-set transaction is
    /// retried through the operator capability before any object, scratch, or
    /// WAL authority is released.
    ///
    /// # Errors
    /// Returns a Scribe error when confirmed completion, exact scratch cleanup,
    /// or full-set reconciliation fails. An unresolved attempt poisons Scribe
    /// and retains its evidence for restart reconciliation.
    ///
    /// # Cancellation
    /// Dropping this future while reconciliation owns an attempt invokes the
    /// attempt's fail-stop drop path; it never deletes remote objects or
    /// advances the immutable generation.
    pub async fn settle_commit_attempts(
        &self,
        attempts: Vec<seal::ScribeCommitAttempt>,
        commit_result: &Result<(), vala_sql::SqlError>,
    ) -> Result<(), ScribeError> {
        if commit_result.is_ok() {
            let mut tokens = Vec::with_capacity(attempts.len());
            let mut artifact_sets = Vec::with_capacity(attempts.len());
            let mut parquet_owners = Vec::with_capacity(attempts.len());
            for attempt in attempts {
                let (token, artifacts, parquet_owner) = attempt.take_terminal();
                tokens.push(token);
                artifact_sets.push(artifacts);
                parquet_owners.push(parquet_owner);
            }
            self.complete_post_commit(seal::PostCommitBatch(tokens))
                .await?;
            for artifacts in artifact_sets {
                artifacts.cleanup()?;
            }
            drop(parquet_owners);
            return Ok(());
        }

        let runtime = self
            .persistence
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe publication reconciler disappeared after COMMIT ambiguity"
                    .to_owned(),
            })?;
        let mut completions = Vec::with_capacity(attempts.len());
        for attempt in attempts {
            completions.push(runtime.submit_reconciliation(attempt).map_err(|attempt| {
                drop(attempt);
                ScribeError::Internal {
                    detail: "Scribe publication reconciliation owner is unavailable".to_owned(),
                }
            })?);
        }
        for completion in completions {
            completion
                .await
                .map_err(|_| ScribeError::Internal {
                    detail: "Scribe publication reconciliation owner stopped".to_owned(),
                })?
                .map_err(|detail| ScribeError::Internal { detail })?;
        }
        Ok(())
    }

    /// Aborts attempts whose caller-owned transaction is known not committed.
    ///
    /// Remote bytes remain retained for deterministic retry before scratch and
    /// immutable ownership return to the active tier.
    ///
    /// # Errors
    /// Returns a Scribe error when scratch cleanup or immutable rollback fails.
    ///
    /// # Panics
    ///
    /// Panics only when an unsettled commit attempt has lost its required
    /// artifact owner, which violates the attempt state invariant.
    pub async fn abort_commit_attempts(
        &self,
        attempts: Vec<seal::ScribeCommitAttempt>,
    ) -> Result<(), ScribeError> {
        let mut tokens = Vec::with_capacity(attempts.len());
        let mut parquet_owners = Vec::with_capacity(attempts.len());
        for attempt in attempts {
            let (token, artifacts, parquet_owner) = attempt.take_terminal();
            artifacts.cleanup()?;
            tokens.push(token);
            parquet_owners.push(parquet_owner);
        }
        let result = self.abort_post_commit(seal::PostCommitBatch(tokens)).await;
        drop(parquet_owners);
        result
    }

    /// Complete one or more seal generations after the caller commits SQL.
    pub async fn complete_post_commit<T>(&self, post_commit: T) -> Result<(), ScribeError>
    where
        T: Into<seal::PostCommitBatch>,
    {
        for mut token in post_commit.into().0 {
            let result = async {
                let binding = TenantTableBinding::resolve((
                    token.seal_key.tenant,
                    token.seal_key.table.clone(),
                ))
                .map_err(|error| ScribeError::Internal {
                    detail: error.to_string(),
                })?;
                self.shards
                    .complete_post_commit(
                        token.seal_id,
                        token.shard_id,
                        &token.seal_key,
                        token.memtable_bytes,
                        token.file_list_key.clone(),
                    )
                    .await?;
                #[cfg(feature = "test-support")]
                self.publication_observer
                    .record(ScribePublicationEvent::Published {
                        seal_id: token.seal_id,
                        file_list_row_id: token.file_list_row_id,
                        batch_ids: token.batch_ids.clone(),
                        table: token.seal_key.table.fqn(),
                    });
                if let Some(publisher) = &self.staging_file_publisher {
                    let _ = publisher.try_publish(crate::maintenance::StagingFileCommitted::new(
                        binding,
                        token.seal_key.partition,
                    ));
                }
                Ok::<(), ScribeError>(())
            }
            .await;
            match result {
                Ok(()) => token.visibility.succeed(),
                Err(error) => {
                    token.visibility.fail();
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// Abort one or more post-commit capabilities after SQL rollback.
    pub async fn abort_post_commit<T>(&self, post_commit: T) -> Result<(), ScribeError>
    where
        T: Into<seal::PostCommitBatch>,
    {
        for mut token in post_commit.into().0 {
            let result = async {
                self.admission
                    .preflight_transfer_immutable_to_active(token.memtable_bytes)?;
                self.memory_ownership
                    .preflight_move_immutable_to_active(token.memtable_bytes)?;
                self.shards
                    .abort_post_commit(token.seal_id, token.shard_id, &token.seal_key)
                    .await?;
                self.admission
                    .transfer_immutable_to_active(token.memtable_bytes)?;
                self.memory_ownership
                    .move_immutable_to_active(token.memtable_bytes)
                    .map_err(|error| ScribeError::Internal {
                        detail: error.to_string(),
                    })?;
                Ok::<(), ScribeError>(())
            }
            .await;
            match result {
                Ok(()) => token.visibility.cancel(),
                Err(error) => {
                    token.visibility.fail();
                    return Err(error);
                }
            }
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
        mut token: seal::PostCommitToken,
        conn: &mut TenantConn<'_>,
    ) -> Result<bool, ScribeError> {
        let result = async {
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
                let binding = TenantTableBinding::resolve((
                    token.seal_key.tenant,
                    token.seal_key.table.clone(),
                ))
                .map_err(|error| ScribeError::Internal {
                    detail: error.to_string(),
                })?;
                self.shards
                    .complete_post_commit(
                        token.seal_id,
                        token.shard_id,
                        &token.seal_key,
                        token.memtable_bytes,
                        key.clone(),
                    )
                    .await?;
                if let Some(publisher) = &self.staging_file_publisher {
                    let _ = publisher.try_publish(crate::maintenance::StagingFileCommitted::new(
                        binding,
                        token.seal_key.partition,
                    ));
                }
                Ok(true)
            } else {
                self.shards
                    .abort_post_commit(token.seal_id, token.shard_id, &token.seal_key)
                    .await?;
                Ok(false)
            }
        }
        .await;
        match result {
            Ok(true) => {
                token.visibility.succeed();
                Ok(true)
            }
            Ok(false) => {
                token.visibility.cancel();
                Ok(false)
            }
            Err(error) => {
                token.visibility.fail();
                Err(error)
            }
        }
    }

    /// Replays eligible WAL sequentially through the bounded filesystem lane.
    ///
    /// Readiness remains false until every restored shard result, manifest
    /// advance, and WAL retirement settles. Shutdown requests cancellation
    /// between records; an already-started shard settlement completes first.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for bounded discovery or admission refusal,
    /// corrupt or contradictory WAL, cancellation, shard restoration failure,
    /// manifest advancement failure, or WAL retirement failure.
    pub async fn replay_wal_async(&self) -> Result<usize, ScribeError> {
        let started = std::time::Instant::now();
        self.recovery_ready.store(false, Ordering::Release);
        if self.recovery_cancelled.load(Ordering::Acquire) {
            return Err(ScribeError::Internal {
                detail: "WAL replay cancelled".to_owned(),
            });
        }
        if let Some(persistence) = &self.persistence {
            let recovered = persistence.recover_staged_publications().await?;
            tracing::info!(
                recovered,
                stream = %self.stream,
                "Scribe staged publications reconciled before WAL replay"
            );
        }
        let result = self
            .wal_io
            .submit(
                crate::scribe::execution_lanes::ScribeWalIoOp::ReplayDirectoryStream {
                    path: self.wal.base_dir().to_path_buf(),
                    wal: Arc::clone(&self.wal),
                    recovery_stream: self.stream,
                    shard_senders: self.shards.replay_senders(),
                    memory: self.memory.clone(),
                    cancelled: Arc::clone(&self.recovery_cancelled),
                },
            )
            .await;
        let result = match result {
            Ok(crate::scribe::execution_lanes::ScribeWalIoResult::ReplayStreamCompleted {
                restored,
                retirement_high_water,
            }) => {
                self.memtable_stats()?;
                tracing::info!(
                    restored,
                    retirement_high_water,
                    stream = %self.stream,
                    "Scribe WAL recovery completed"
                );
                Ok(restored)
            }
            Ok(_) => Err(ScribeError::Internal {
                detail: "WAL IO lane returned the wrong replay result".to_owned(),
            }),
            Err(error) => {
                let memory = self.memory.memory_snapshot();
                tracing::error!(
                    stage = "replay_stream",
                    purpose = "scribe_replay",
                    error = %error,
                    scribe_current_bytes = memory.scribe_total_bytes,
                    scribe_limit_bytes = memory.scribe_limit_bytes,
                    bifrost_current_bytes = memory.bifrost_total_bytes,
                    bifrost_limit_bytes = memory.bifrost_limit_bytes,
                    "Scribe WAL recovery failed"
                );
                Err(error)
            }
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
            self.memory.clone(),
        ))
    }

    /// Construct the bounded, fence-owning Scribe tail reader for new Oracle paths.
    ///
    /// The returned reader retains only shallow Arrow snapshots from this Scribe's
    /// shard runtime and owns the bounded fence protocol used by Oracle.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when this Scribe's node identity cannot produce a
    /// typed stream identity for the reader.
    pub fn tail_reader(&self) -> Result<ScribeTailReader, ScribeError> {
        Ok(ScribeTailReader::new(
            Arc::new(self.tail_service()?),
            TailFenceConfig::default(),
        ))
    }
}
