//! Shared axum application state.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use tokio::sync::Mutex;
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::timeout_at;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::cluster::{ClusterRegistry, RegisteredRole};
use vala_bifrost_redux::forge::Forge as ForgeCoordinator;
use vala_bifrost_redux::forge::ForgeWorker;
use vala_bifrost_redux::gate::Gate;
use vala_bifrost_redux::gate::limits::IngestLimits;
use vala_bifrost_redux::oracle::Oracle as OracleEngine;
use vala_bifrost_redux::oracle::dispatcher::{BifrostPeerTls, OraclePeerCredentials};
use vala_bifrost_redux::oracle::follower::{PhysicalPlanFollower, ScribeTailResolver};
use vala_bifrost_redux::oracle::peer::{PeerSecurityAudit, PeerTicketVerifier};
use vala_bifrost_redux::oracle::{AuthorizedQueryContext, OracleQueryStream, RunningQueryRegistry};
use vala_bifrost_redux::resources::{BifrostRoleResources, OracleResources, ScribeResources};
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::scribe::tail_rpc::{
    FetchLiveTailService, ScribeTailReader, TailFenceConfig,
};
use wyrd_auth_verify::TokenVerifier;
use wyrd_storage::StorageHandle;
use wyrd_telemetry::TelemetryGuard;
use wyrd_tonic::tonic_health::server::HealthReporter;

use crate::bifrost::gate_audit::PostgresGateAudit;
use crate::boot::data_root::BifrostDataRoot;
use crate::components::auth::{ServerAuth, ServerAuthz};
use crate::components::eval::{EvalRuns, new_run_map};
use crate::components::health::ReadinessSnapshot;
use crate::config::{BifrostRuntimeConfig, BifrostTarget, DeploymentProfile, ForgeRuntimeConfig};
#[cfg(feature = "test-support")]
use crate::oracle::SilentForwardPeer;
use crate::postgres::ServerPostgres;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::BifrostQueryRequest;

/// Redact database failures at the public registry boundary.
///
/// Card registration surfaces a stable `WYRD_REGISTRY_*` unavailability error
/// rather than leaking driver or schema detail to the caller; the original
/// failure stays in the server log.
pub(crate) fn registry_db_error(error: impl std::fmt::Display) -> WyrdError {
    tracing::error!(%error, "card registration database operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}

/// External dependency graph consumed exactly once by production Bifrost composition.
pub struct BifrostBuildInputs {
    /// Closed process target controlling the selected subsystem graph.
    pub target: BifrostTarget,
    /// Deployment posture used for fail-closed peer transport validation.
    pub deployment_profile: DeploymentProfile,
    /// Shared production PostgreSQL handles.
    pub postgres: ServerPostgres,
    /// Shared object-storage handle used by Scribe and Forge construction.
    pub storage: Arc<StorageHandle>,
    /// This node's one Bifrost storage owner.
    ///
    /// Retained so every co-located role shares one pointer-identical owner,
    /// one request ceiling, and one shutdown, rather than each role composing
    /// its own view of the same backend.
    pub bifrost_storage: Arc<vala_bifrost_redux::storage::BifrostStorage>,
    /// Shared tenant-qualified Bifrost catalog.
    pub catalog: Arc<BifrostCatalog>,
    /// One root-derived resource graph for every selected local role.
    pub resources: BifrostRoleResources,
    /// One shared current-ready cluster registry.
    pub cluster: Arc<ClusterRegistry>,
    /// Exact public and private request token verifier.
    pub token_verifier: Arc<TokenVerifier>,
    /// Existing outbound peer bearer owner.
    pub peer_credentials: Arc<dyn OraclePeerCredentials>,
    /// Immutable peer TLS trust policy validated before composition.
    ///
    /// `Some` exactly when peer CA material is configured. This is the single
    /// predicate deciding whether outbound peer transports use TLS, matching
    /// what `TonicOraclePeerTransport` already requires of advertised
    /// addresses; it is deliberately not keyed on the deployment profile.
    pub peer_tls: Option<BifrostPeerTls>,
    /// Independent Bifrost peer ticket keyring used to mint and verify peer
    /// and tail tickets.
    ///
    /// This is deliberately not the north-south workload signing key: a user
    /// or API token must never validate as a peer-purpose ticket, and the peer
    /// keyring rotates on its own schedule.
    pub peer_keyring: Arc<crate::oracle::PeerTicketKeyring>,
    /// Immutable role configuration snapshot.
    pub config: BifrostRuntimeConfig,
    /// Immutable Forge configuration snapshot.
    pub forge_config: ForgeRuntimeConfig,
    /// Stable physical node identity.
    pub node_id: wyrd_spec::vala::api::NodeId,
    /// Private endpoint advertised by selected fenced roles.
    pub advertise_addr: String,
    /// Exclusively owned local root from which every role path is derived.
    pub data_root: BifrostDataRoot,
    /// Process shutdown signal injected into every selected owner.
    pub shutdown: CancellationToken,
    /// Focused production-control overrides consumed only by the shared test composer.
    #[cfg(feature = "test-support")]
    pub test_controls: Option<BifrostTestControls>,
}

/// Composed inputs for one selected Oracle query role.
///
/// `compose_bifrost` derives every field from the process-wide resource graph
/// before the role exists, so they travel together as one value rather than as
/// a long positional argument list whose order carries no meaning.
pub struct OracleBuildInputs {
    /// Distributed query engine this role wraps with lifecycle ownership.
    pub engine: Arc<OracleEngine>,
    /// Shared tenant-qualified Bifrost catalog.
    pub catalog: Arc<BifrostCatalog>,
    /// Cluster registration this role heartbeats against.
    pub registered_role: RegisteredRole,
    /// Shared current-ready cluster registry.
    pub cluster: Arc<ClusterRegistry>,
    /// Audit outbox writer for Oracle read decisions and tenant tripwires.
    pub audit: Arc<crate::oracle::OracleQueryAudit>,
    /// Transport carrying lifecycle control to peer participants.
    pub lifecycle_transport: Arc<crate::oracle::OracleLifecycleTransport>,
    /// Root-derived resource capability for the Oracle role.
    pub resources: OracleResources,
    /// Peer runtime owning outbound fragment dispatch.
    pub peer: Arc<crate::oracle::OraclePeerRuntime>,
    /// Role-scoped cancellation signal.
    pub role_shutdown: CancellationToken,
}

/// Composed inputs for one selected Scribe ingest role.
///
/// Mirrors [`OracleBuildInputs`]: the composer resolves each dependency from
/// the shared resource graph, then hands the role one cohesive value.
pub struct ScribeBuildInputs {
    /// Embedded Scribe implementation this role wraps.
    pub ingest: Arc<ScribeImpl>,
    /// Shared tenant-qualified Bifrost catalog.
    pub catalog: Arc<BifrostCatalog>,
    /// Root-derived resource capability for the Scribe role.
    pub resources: ScribeResources,
    /// Shared current-ready cluster registry.
    pub cluster: Arc<ClusterRegistry>,
    /// Cluster registration this role heartbeats against.
    pub registered_role: RegisteredRole,
    /// Verifier admitting inbound peer fragment tickets.
    pub fragment_verifier: Arc<dyn PeerTicketVerifier>,
    /// Audit sink for refused inbound fragment requests.
    pub fragment_security_audit: Arc<dyn PeerSecurityAudit>,
    /// Audit outbox writer for follower tenant tripwires.
    pub fragment_query_audit: Arc<crate::oracle::OracleQueryAudit>,
    /// Whether this role owns `fragment_query_audit`'s shutdown, which it does
    /// only when no local Oracle role shares the publisher.
    pub owns_fragment_query_audit: bool,
    /// Role-scoped cancellation signal.
    pub role_shutdown: CancellationToken,
}

/// Sole owner of the dedicated Tokio runtime that hosts Scribe coordination tasks.
///
/// The coordination runtime executes the sixteen `ShardOwner::run` lanes plus
/// the Scribe reconciliation and persistence loops. Every consumer of that
/// executor holds a [`tokio::runtime::Handle`], never the [`Runtime`] value, so
/// this owner is the only place in the process where the runtime itself is
/// reachable. It is deliberately **not** `Clone` and is never stored on
/// [`AppState`], [`Bifrost`], [`Scribe`], or [`Oracle`]: a runtime reachable
/// from a per-request-cloned graph would be dropped on whichever clone happened
/// to die last, and [`Runtime::drop`] blocks to join its workers, which panics
/// when it runs on an async frame.
///
/// [`Runtime`]: tokio::runtime::Runtime
/// [`Runtime::drop`]: tokio::runtime::Runtime
pub struct ScribeCoordinationRuntime {
    /// The dedicated executor, present only when this process selected Scribe.
    runtime: Option<tokio::runtime::Runtime>,
}

impl ScribeCoordinationRuntime {
    /// Takes sole ownership of a composed coordination runtime.
    ///
    /// `runtime` is `None` for every process that did not select the Scribe
    /// role; those targets never build a dedicated executor and their owner is
    /// an inert value whose drop does nothing.
    pub(crate) const fn new(runtime: Option<tokio::runtime::Runtime>) -> Self {
        Self { runtime }
    }
}

impl Drop for ScribeCoordinationRuntime {
    /// Releases the coordination runtime without blocking the dropping thread.
    ///
    /// [`tokio::runtime::Runtime::shutdown_background`] detaches the worker
    /// threads instead of joining them, so it is legal from any context —
    /// including the async frame that `async fn main` forces on the final drop
    /// of the composed server. Because it does not wait, drop ordering is
    /// load-bearing: this owner must outlive Scribe role drain, otherwise a
    /// shard owner still holding WAL segments or post-`COMMIT` memtable state is
    /// abandoned mid-flight. Both production (`app::run` drops it after
    /// `BoundServer::run` returns) and the test harness (which drops it after
    /// the serve task joins) satisfy that ordering.
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

/// Sole owner of the dedicated Tokio runtime that hosts Forge compaction runners.
///
/// Only admitted compaction plan runners execute here. Everything else the
/// Forge worker does — durable claims, maintenance strategies, reconciliation,
/// settlement — stays on the ambient server runtime, so a saturated rewrite
/// cannot starve the loop that would otherwise settle it.
///
/// Ownership follows [`ScribeCoordinationRuntime`] exactly and for the same
/// reason: consumers hold a [`tokio::runtime::Handle`], this owner holds the
/// only [`Runtime`], it is deliberately **not** `Clone`, and it is never stored
/// on a per-request-cloned graph whose last surviving clone would run blocking
/// [`Runtime::drop`] glue on an async frame.
///
/// [`Runtime`]: tokio::runtime::Runtime
/// [`Runtime::drop`]: tokio::runtime::Runtime
pub struct ForgeCompactionRuntime {
    /// The dedicated executor, present only when this process selected the
    /// Forge worker role.
    runtime: Option<tokio::runtime::Runtime>,
}

impl ForgeCompactionRuntime {
    /// Takes sole ownership of a composed compaction runtime.
    ///
    /// `runtime` is `None` for every process that did not select the Forge
    /// worker role; those targets never build a dedicated executor and their
    /// owner is an inert value whose drop does nothing.
    pub(crate) const fn new(runtime: Option<tokio::runtime::Runtime>) -> Self {
        Self { runtime }
    }

    /// Returns the handle admitted compaction runners are spawned on.
    ///
    /// `None` for a process without the Forge worker role, which never spawns a
    /// runner in the first place. The handle is cloned out of the owner rather
    /// than out of the raw runtime so that composition cannot hand a worker an
    /// executor this owner does not hold, and therefore does not release.
    pub(crate) fn handle(&self) -> Option<tokio::runtime::Handle> {
        self.runtime
            .as_ref()
            .map(|runtime| runtime.handle().clone())
    }
}

impl Drop for ForgeCompactionRuntime {
    /// Releases the compaction runtime without blocking the dropping thread.
    ///
    /// [`tokio::runtime::Runtime::shutdown_background`] detaches the worker
    /// threads rather than joining them, so it is legal from the async frame
    /// that `async fn main` forces on the final drop of the composed server.
    /// Because it does not wait, drop ordering is load-bearing: this owner must
    /// outlive Forge worker supervision, otherwise a runner that has already
    /// written output objects and is about to Prepare its operation is
    /// abandoned mid-flight and leaves durable ambiguity behind. Both
    /// production (`app::run` drops it after the supervised worker returns) and
    /// the test harness satisfy that ordering.
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

/// One composed Bifrost graph paired with the coordination runtime that hosts it.
///
/// `compose_bifrost` produces two values with different ownership rules: the
/// shareable [`Bifrost`] graph, and the single non-`Clone`
/// [`ScribeCoordinationRuntime`] owner. They travel together as one named value
/// so callers cannot silently drop the owner and leave the shard lanes running
/// on a runtime nothing holds.
pub struct ComposedBifrost {
    /// Shareable composition root published into [`AppState`].
    pub bifrost: Arc<Bifrost>,
    /// Sole owner of the executor backing the composed Scribe role.
    pub coordination_runtime: ScribeCoordinationRuntime,
    /// Sole owner of the executor backing admitted Forge compaction runners.
    pub compaction_runtime: ForgeCompactionRuntime,
    /// Exclusive local data-root owner the caller retains while the graph runs.
    pub data_root: BifrostDataRoot,
}

/// Existing concrete production controls injected by server journeys.
///
/// The shared composer consumes these values while building the same owners as
/// production. The completed [`Bifrost`] never retains this test-only DTO.
#[cfg(feature = "test-support")]
pub struct BifrostTestControls {
    /// Deterministic Forge wall clock.
    pub forge_clock: vala_bifrost_redux::forge::ForgeClock,
    /// Explicit wake/observation handle for the production Forge scheduler.
    pub forge_scheduler_trigger: vala_bifrost_redux::forge::ForgeSchedulerTrigger,
    /// Optional observer of completed Forge worker tasks.
    pub forge_completion_observer: Option<vala_bifrost_redux::forge::ForgeWorkerCompletionObserver>,
    /// Optional production Forge configuration override used by focused journeys.
    pub forge_config: Option<vala_bifrost_redux::forge::ForgeConfig>,
    /// Optional catalog wrapper used to inject catalog uncertainty.
    pub forge_catalog: Option<Arc<dyn iceberg::Catalog>>,
    /// Optional object-store wrapper used to inject object-store behavior.
    pub forge_object_store: Option<Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>>,
    /// Deterministic delay in the existing Scribe WAL IO lane.
    pub scribe_wal_sync_delay: Duration,
    /// Optional complete override of the production Scribe geometry.
    ///
    /// Scaled production journeys need the assembled-object target and the
    /// rotation limits moved together; a validated geometry is the one value
    /// that carries both without introducing a second policy.
    pub scribe_geometry: Option<vala_bifrost_redux::scribe::geometry::ScribeGeometry>,
    /// Existing Scribe persistence fault controls.
    pub scribe_persistence_faults: vala_bifrost_redux::scribe::persistence::PersistenceFaults,
    /// Optional override of the existing Scribe admission configuration.
    pub scribe_admission: Option<vala_bifrost_redux::scribe::admission::AdmissionConfig>,
    /// Optional accelerated role cadence already installed on the shared registry.
    pub role_timing: Option<vala_bifrost_redux::cluster::RoleTiming>,
}
/// The one monomorphic Bifrost Gate specialization served by this process.
///
/// Fixing the audit sink keeps [`Bifrost`] and [`AppState`] non-generic while
/// the Gate itself stays generic over its sink.
pub type ServerGate = Gate<PostgresGateAudit>;

/// Ordered local lifecycle states for one independently fenced role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum RoleLifecycleState {
    /// The role may advertise readiness and accept new work.
    Serving = 0,
    /// Durable readiness is being removed while accepted transports drain.
    Draining = 1,
    /// Heartbeats are stopped and the owner is rejecting or cancelling work.
    Stopping = 2,
    /// The retained fence has been unregistered.
    Stopped = 3,
}

/// Lock-free owner for monotonic role shutdown transitions.
#[derive(Debug, Clone)]
pub struct RoleLifecycle {
    /// Current [`RoleLifecycleState`] encoded for request-path reads.
    state: Arc<AtomicU8>,
}

impl RoleLifecycle {
    /// Creates a role lifecycle in its serving state.
    fn serving() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(RoleLifecycleState::Serving as u8)),
        }
    }

    /// Reports whether new work and readiness advertisement remain permitted.
    fn is_serving(&self) -> bool {
        self.state.load(Ordering::Acquire) == RoleLifecycleState::Serving as u8
    }

    /// Starts readiness removal exactly once.
    fn begin_draining(&self) -> bool {
        self.state
            .compare_exchange(
                RoleLifecycleState::Serving as u8,
                RoleLifecycleState::Draining as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Advances the owner to heartbeat-stop and work-rejection.
    fn begin_stopping(&self) {
        self.state
            .fetch_max(RoleLifecycleState::Stopping as u8, Ordering::AcqRel);
    }

    /// Records that the exact role fence has completed teardown.
    fn finish(&self) {
        self.state
            .store(RoleLifecycleState::Stopped as u8, Ordering::Release);
    }
}

/// Owns one completely wired Bifrost ingest subsystem for a server process.
///
/// The runtime retains the exact Scribe allocation provided to Gate, its
/// coordination runtime, and the resulting Gate. Keeping the three values
/// together prevents a request path, a test seam, and shutdown from selecting
/// different writers.
#[derive(Clone)]
pub struct Scribe {
    /// Durable Scribe implementation used by every Gate dispatch and lifecycle path.
    ingest: Arc<ScribeImpl>,
    /// Exact role-local source used by private tail reads.
    tail_service: Arc<FetchLiveTailService>,
    /// Request-local governed physical-plan follower for live Scribe assignments.
    fragment_follower: Arc<PhysicalPlanFollower<ScribeTailResolver>>,
    /// Completed request-local Scribe fragment executions.
    fragment_executions: Arc<AtomicU64>,
    /// Footer frames emitted by the Scribe follower owner.
    fragment_footers: Arc<AtomicU64>,
    /// Root-derived Scribe resource capability.
    resources: ScribeResources,
    /// Shared current-ready cluster registry.
    cluster: Arc<ClusterRegistry>,
    /// Exact Scribe fence registered after recovery completed.
    registered_role: RegisteredRole,
    /// Monotonic registered-role lifecycle.
    lifecycle: RoleLifecycle,
    /// Shared tenant-qualified catalog retained by the selected data subsystem.
    catalog: Arc<BifrostCatalog>,
    /// One fence reader shared by every local and authenticated tonic tail read.
    tail_reader: Arc<ScribeTailReader>,
    /// Optional domain-separated authority for private tail RPCs.
    tail_authority: Option<Arc<crate::oracle::ScribeTailAuthority>>,
    /// Raw-ticket verifier for Scribe-targeted physical fragments.
    fragment_verifier: Arc<dyn PeerTicketVerifier>,
    /// Durable security audit for rejected Scribe fragment authority.
    fragment_security_audit: Arc<dyn PeerSecurityAudit>,
    /// Process-owned query audit required by decoded tenant tripwires.
    fragment_query_audit: Arc<crate::oracle::OracleQueryAudit>,
    /// Retains audit shutdown ownership only when this process has no Oracle owner.
    owns_fragment_query_audit: bool,
    /// Cancels the recurring heartbeat and snapshot tasks before role removal.
    role_shutdown: CancellationToken,
    /// Retains the heartbeat task so teardown can prove it stopped before unregister.
    heartbeat: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Synchronously aborts the heartbeat without acquiring its async owner lock.
    heartbeat_abort: AbortHandle,
    /// Retains the snapshot poller so role-owned background work is drained.
    snapshot_poller: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Synchronously aborts the snapshot poller without acquiring its async owner lock.
    snapshot_poller_abort: AbortHandle,
    /// Readiness value preserved by heartbeats during transport drain.
    advertise_ready: Arc<AtomicBool>,
}

/// Owns one retained Oracle and its independent fenced server lifecycle.
#[derive(Clone)]
pub struct Oracle {
    /// Retained leader/worker query engine used by every Gate query dispatch.
    engine: Arc<OracleEngine>,
    /// Oracle-owned private peer execution service.
    peer: Arc<crate::oracle::OraclePeerRuntime>,
    /// Process-wide owner-local active-query registry.
    running_queries: Arc<RunningQueryRegistry>,
    /// Tenant-scoped controls over the exact running-query registry.
    query_controls: crate::oracle::RunningQueryControls,
    /// Root-derived Oracle resource capability.
    resources: OracleResources,
    /// Shared current-ready cluster registry.
    cluster: Arc<ClusterRegistry>,
    /// Exact Oracle fence registered after worker dependencies became ready.
    registered_role: RegisteredRole,
    /// Monotonic registered-role lifecycle.
    lifecycle: RoleLifecycle,
    /// Shared tenant-qualified catalog retained by the selected data subsystem.
    catalog: Arc<BifrostCatalog>,
    /// Cancels the Oracle heartbeat and membership snapshot tasks.
    role_shutdown: CancellationToken,
    /// Retains the heartbeat task until ordered shutdown stops it.
    heartbeat: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Synchronously aborts the heartbeat without acquiring its async owner lock.
    heartbeat_abort: AbortHandle,
    /// Retains the membership poller until ordered shutdown stops it.
    snapshot_poller: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Synchronously aborts the snapshot poller without acquiring its async owner lock.
    snapshot_poller_abort: AbortHandle,
    /// Retains the delegated-continuity monitor until role shutdown.
    continuity_monitor: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Synchronously aborts the continuity monitor after a shutdown deadline.
    continuity_monitor_abort: AbortHandle,
    /// Readiness value preserved by heartbeats during transport drain.
    advertise_ready: Arc<AtomicBool>,
    /// Audit outbox writer retained for the complete Oracle lifecycle.
    audit: Arc<crate::oracle::OracleQueryAudit>,
    /// Canonical authenticated transport for owner-local lifecycle fanout.
    lifecycle_transport: Arc<crate::oracle::OracleLifecycleTransport>,
}

impl Oracle {
    /// Reports whether the injected process shutdown token reached Oracle lifecycle work.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn process_shutdown_observed_for_test(&self) -> bool {
        self.role_shutdown.is_cancelled() && self.engine.process_shutdown_observed_for_test()
    }

    /// Returns the exact registered Oracle role identity and fence.
    #[must_use]
    pub(crate) const fn registered_role(&self) -> &RegisteredRole {
        &self.registered_role
    }

    /// Cancels Oracle work and explicitly aborts retained role tasks without awaiting.
    ///
    /// Used only after the process deadline is exhausted. It closes role
    /// activity synchronously, aborts the heartbeat, snapshot poller, and
    /// continuity monitor through handles that do not require their async owner
    /// locks, starts no registry operation, and leaves incomplete durable cleanup
    /// to existing recovery.
    pub(crate) fn abort_shutdown(&self) {
        self.lifecycle.begin_stopping();
        self.role_shutdown.cancel();
        self.heartbeat_abort.abort();
        self.snapshot_poller_abort.abort();
        self.continuity_monitor_abort.abort();
        self.engine.begin_shutdown();
    }
    /// Creates the retained query lifecycle and its reader-epoch loss monitor.
    ///
    /// # Errors
    ///
    /// Returns [`wyrd_spec::vala::error::BifrostError`] when the retained role
    /// composition cannot be completed.
    pub fn new(inputs: OracleBuildInputs) -> Result<Self, wyrd_spec::vala::error::BifrostError> {
        let OracleBuildInputs {
            engine,
            catalog,
            registered_role,
            cluster,
            audit,
            lifecycle_transport,
            resources,
            peer,
            role_shutdown,
        } = inputs;
        let advertise_ready = Arc::new(AtomicBool::new(true));
        let lifecycle = RoleLifecycle::serving();
        let heartbeat = Arc::clone(&cluster).start_readiness_heartbeat(
            registered_role.clone(),
            Arc::clone(&advertise_ready),
            role_shutdown.clone(),
        );
        let snapshot_poller = Arc::clone(&cluster).start_snapshot_poller(role_shutdown.clone());
        let heartbeat_abort = heartbeat.abort_handle();
        let snapshot_poller_abort = snapshot_poller.abort_handle();
        let continuity_monitor = tokio::spawn(run_reader_epoch_loss_monitor(
            Arc::clone(&engine),
            Arc::clone(&cluster),
            registered_role.clone(),
            lifecycle.clone(),
            Arc::clone(&advertise_ready),
            role_shutdown.clone(),
        ));
        let continuity_monitor_abort = continuity_monitor.abort_handle();
        let running_queries = Arc::clone(engine.running_queries());
        // The engine already owns the distributed dispatcher, so assert the
        // composition invariant here rather than retaining a second handle that
        // nothing reads.
        assert!(
            engine.fragment_dispatcher().is_some(),
            "production Oracle composition requires its distributed dispatcher"
        );
        let query_controls = crate::oracle::RunningQueryControls::new(
            Some(Arc::clone(&running_queries)),
            Arc::clone(&lifecycle_transport),
            Arc::clone(&cluster),
        );
        Ok(Self {
            engine,
            catalog,
            registered_role,
            cluster,
            role_shutdown,
            heartbeat: Arc::new(Mutex::new(Some(heartbeat))),
            heartbeat_abort,
            snapshot_poller: Arc::new(Mutex::new(Some(snapshot_poller))),
            snapshot_poller_abort,
            continuity_monitor: Arc::new(Mutex::new(Some(continuity_monitor))),
            continuity_monitor_abort,
            lifecycle,
            advertise_ready,
            audit,
            running_queries,
            lifecycle_transport,
            query_controls,
            peer,
            resources,
        })
    }

    /// Borrows the Oracle-owned private peer service.
    #[must_use]
    pub const fn peer(&self) -> &Arc<crate::oracle::OraclePeerRuntime> {
        &self.peer
    }

    /// Borrows the retained Oracle used by local Gate dispatch.
    #[must_use]
    pub fn engine(&self) -> &Arc<OracleEngine> {
        &self.engine
    }

    /// Borrows the retained query engine for existing route adapters.
    #[must_use]
    pub fn oracle(&self) -> &Arc<OracleEngine> {
        &self.engine
    }

    /// Borrows the one process-wide owner-local active-query registry.
    #[must_use]
    pub fn running_queries(&self) -> &Arc<RunningQueryRegistry> {
        &self.running_queries
    }

    /// Borrows the one boot-constructed authenticated lifecycle transport.
    #[must_use]
    pub fn lifecycle_transport(&self) -> &Arc<crate::oracle::OracleLifecycleTransport> {
        &self.lifecycle_transport
    }

    /// Returns the shared cluster registry retained by Oracle.
    #[must_use]
    pub fn cluster(&self) -> Arc<ClusterRegistry> {
        Arc::clone(&self.cluster)
    }

    /// Borrows the one tenant-authorized logical lifecycle facade.
    #[must_use]
    pub const fn query_controls(&self) -> &crate::oracle::RunningQueryControls {
        &self.query_controls
    }

    /// Captures Oracle resource reservations and in-flight audit outbox commits.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn oracle_runtime_inspection(
        &self,
    ) -> (vala_bifrost_redux::oracle::OracleRuntimeInspection, usize) {
        (self.engine.runtime_inspection(), self.audit.pending())
    }

    /// Reports startup reconciliation and lifecycle readiness, excluding saturation.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.lifecycle.is_serving() && self.engine.is_ready()
    }

    /// Synchronously closes local Oracle readiness before transport cancellation.
    pub(crate) fn start_draining(&self) {
        if self.lifecycle.begin_draining() {
            metrics::gauge!("bifrost_role_ready", "role" => "oracle").set(0.0);
            self.advertise_ready.store(false, Ordering::Release);
        }
    }

    /// Removes durable readiness before transport draining begins.
    ///
    /// # Errors
    ///
    /// Returns the registry error when the exact Oracle fence cannot be marked
    /// unready. Local readiness still closes so the process fails closed.
    pub async fn begin_shutdown(&self) -> Result<(), vala_bifrost_redux::cluster::ClusterError> {
        self.start_draining();
        self.cluster.deactivate(&self.registered_role).await
    }

    /// Stops heartbeat, rejects/cancels Oracle work, drains, and unregisters.
    ///
    /// This method mutates only the exact Oracle fence retained at construction;
    /// a colocated Scribe role is never advanced or removed.
    ///
    /// # Cancellation
    ///
    /// Cancellation after readiness removal can leave accepted Oracle work
    /// for durable recovery. Cleanup is best-effort and never extends the
    /// caller-owned process deadline.
    pub async fn shutdown(&self, deadline: Instant) {
        let _ = self.shutdown_owner(deadline).await;
        let _ = self.shutdown_registry(deadline).await;
    }

    /// Stops Oracle-owned work and retained role tasks within `deadline`.
    ///
    /// Cancellation first closes new work. Maintenance and role tasks then
    /// drain against the unchanged process deadline; partial progress is
    /// retained for recovery when the future reaches or is dropped at expiry.
    pub(crate) async fn shutdown_owner(
        &self,
        deadline: Instant,
    ) -> Result<(), wyrd_spec::vala::error::BifrostError> {
        self.lifecycle.begin_stopping();
        self.role_shutdown.cancel();
        let report = self.engine.shutdown(deadline).await;
        let audit = self.audit.shutdown(deadline).await;
        await_role_task(
            &self.continuity_monitor,
            deadline,
            "oracle continuity monitor",
        )
        .await?;
        await_role_task(&self.heartbeat, deadline, "oracle heartbeat").await?;
        await_role_task(&self.snapshot_poller, deadline, "oracle snapshot poller").await?;
        if report.active_queries != 0
            || report.queued_queries != 0
            || report.peer_pending != 0
            || report.peer_running != 0
            || report.reserved_memory_bytes != 0
            || report.reserved_spill_bytes != 0
            || audit != 0
        {
            return Err(wyrd_spec::vala::error::BifrostError::Internal {
                detail: "Oracle shutdown retained admission, resource, peer, or audit state"
                    .to_owned(),
            });
        }
        Ok(())
    }

    /// Unregisters the exact Oracle fence within `deadline`.
    ///
    /// No registry await starts after expiry. Failure or timeout leaves the
    /// lifecycle unfinished and observable in logs rather than claiming cleanup.
    pub(crate) async fn shutdown_registry(
        &self,
        deadline: Instant,
    ) -> Result<(), wyrd_spec::vala::error::BifrostError> {
        if tokio::time::Instant::now() < tokio::time::Instant::from_std(deadline) {
            match timeout_at(
                tokio::time::Instant::from_std(deadline),
                self.cluster.shutdown_role(self.registered_role.clone()),
            )
            .await
            {
                Ok(Ok(())) => {
                    self.lifecycle.finish();
                    Ok(())
                }
                Ok(Err(_)) | Err(_) => Err(wyrd_spec::vala::error::BifrostError::Internal {
                    detail: "Oracle role registry shutdown failed".to_owned(),
                }),
            }
        } else {
            Err(wyrd_spec::vala::error::BifrostError::Internal {
                detail: "Oracle role registry shutdown exceeded its deadline".to_owned(),
            })
        }
    }
}

/// Closes this Oracle fence once its reader epoch selects its own loss.
///
/// The local readiness bit and engine cancellation close synchronously before
/// the exact durable role row is marked unready. Refreshing the local immutable
/// snapshot then prevents both local Gate dispatch and remote forwarding from
/// selecting the dead owner. Query streams observe engine cancellation and
/// retain their existing terminal-settlement guards.
async fn run_reader_epoch_loss_monitor(
    engine: Arc<OracleEngine>,
    cluster: Arc<ClusterRegistry>,
    registered_role: RegisteredRole,
    lifecycle: RoleLifecycle,
    advertise_ready: Arc<AtomicBool>,
    shutdown: CancellationToken,
) {
    // The reader epoch's own loss is selected before its audited edge commits,
    // so this monitor observes the epoch directly rather than waiting for a
    // report an epoch fencing itself never produces.
    tokio::select! {
        () = shutdown.cancelled() => return,
        () = engine.reader_authority().loss_selected_notify().cancelled() => {}
    }
    crate::oracle::close_local_reader_epoch(advertise_ready.as_ref(), &shutdown);
    lifecycle.begin_draining();
    metrics::gauge!("bifrost_role_ready", "role" => "oracle").set(0.0);
    engine.begin_shutdown();
    let deactivation =
        crate::oracle::deactivate_lost_reader_epoch(&cluster, &registered_role).await;
    let report = await_reader_epoch_loss_settlement(
        deactivation,
        engine.shutdown(Instant::now() + Duration::from_secs(5)),
    )
    .await;
    if report.active_queries != 0 || report.queued_queries != 0 {
        tracing::error!(
            active_queries = report.active_queries,
            queued_queries = report.queued_queries,
            "Oracle reader-epoch shutdown retained unsettled query admission"
        );
    }
}

/// Reports durable routing failure without skipping local query settlement.
async fn await_reader_epoch_loss_settlement<F, T>(
    deactivation: Result<(), vala_bifrost_redux::cluster::ClusterError>,
    settlement: F,
) -> T
where
    F: std::future::Future<Output = T>,
{
    if let Err(error) = deactivation {
        tracing::error!(%error, "failed to deactivate Oracle after reader-epoch loss");
    }
    settlement.await
}

impl Scribe {
    /// Borrows the durable Scribe implementation handed to the one Gate.
    ///
    /// Boot needs the exact allocation so the Gate and the lifecycle owner
    /// never select different writers; the field itself stays private so no
    /// request path can reach around the runtime that owns its lifecycle.
    #[must_use]
    pub(crate) const fn ingest(&self) -> &Arc<ScribeImpl> {
        &self.ingest
    }

    /// Reports whether the injected process shutdown token reached Scribe lifecycle work.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn process_shutdown_observed_for_test(&self) -> bool {
        self.role_shutdown.is_cancelled()
    }

    /// Explicitly aborts all retained Scribe role work without awaiting.
    ///
    /// This deadline-expiry path signals role cancellation, explicitly aborts
    /// heartbeat and snapshot tasks without acquiring their async owner locks,
    /// and aborts retained Scribe workers. It performs no flush or external await.
    pub(crate) fn abort_shutdown(&self) {
        self.lifecycle.begin_stopping();
        self.role_shutdown.cancel();
        self.heartbeat_abort.abort();
        self.snapshot_poller_abort.abort();
        self.ingest.abort_shutdown();
    }
    /// Builds the Scribe runtime retained by the process composition owner.
    ///
    /// The projection shares Scribe's bounded ingress CPU lane, while an
    /// optional runtime keeps server-created coordination consumers alive.
    #[must_use]
    pub fn new(inputs: ScribeBuildInputs) -> Self {
        let ScribeBuildInputs {
            ingest,
            catalog,
            resources,
            cluster,
            registered_role,
            fragment_verifier,
            fragment_security_audit,
            fragment_query_audit,
            owns_fragment_query_audit,
            role_shutdown,
        } = inputs;
        let tail_service = Arc::new(
            ingest
                .tail_service()
                .expect("constructed Scribe must retain a valid UUID stream identity"),
        );
        let fragment_follower = Arc::new(
            PhysicalPlanFollower::new(ScribeTailResolver::new(
                Arc::clone(&tail_service),
                Arc::clone(&catalog),
            ))
            .with_audit(fragment_query_audit.clone()),
        );
        let tail_reader = Arc::new(ScribeTailReader::new(
            Arc::clone(&tail_service),
            TailFenceConfig::default(),
        ));
        let advertise_ready = Arc::new(AtomicBool::new(true));
        let heartbeat = Arc::clone(&cluster).start_readiness_heartbeat(
            registered_role.clone(),
            Arc::clone(&advertise_ready),
            role_shutdown.clone(),
        );
        let snapshot_poller = Arc::clone(&cluster).start_snapshot_poller(role_shutdown.clone());
        let heartbeat_abort = heartbeat.abort_handle();
        let snapshot_poller_abort = snapshot_poller.abort_handle();
        Self {
            ingest,
            tail_service,
            fragment_follower,
            fragment_executions: Arc::new(AtomicU64::new(0)),
            fragment_footers: Arc::new(AtomicU64::new(0)),
            resources,
            cluster,
            registered_role,
            lifecycle: RoleLifecycle::serving(),
            catalog,
            tail_reader,
            tail_authority: None,
            fragment_verifier,
            fragment_security_audit,
            fragment_query_audit,
            owns_fragment_query_audit,
            role_shutdown,
            heartbeat: Arc::new(Mutex::new(Some(heartbeat))),
            heartbeat_abort,
            snapshot_poller: Arc::new(Mutex::new(Some(snapshot_poller))),
            snapshot_poller_abort,
            advertise_ready,
        }
    }

    /// Injects the server-owned private Scribe-tail authority.
    #[must_use]
    pub fn with_tail_authority(
        mut self,
        authority: Arc<crate::oracle::ScribeTailAuthority>,
    ) -> Self {
        self.tail_authority = Some(authority);
        self
    }

    /// Returns the private tail authority used by the gRPC adapter.
    #[must_use]
    pub fn tail_authority(&self) -> Option<Arc<crate::oracle::ScribeTailAuthority>> {
        self.tail_authority.clone()
    }

    /// Borrows the exact Scribe role fence retained by this runtime.
    #[must_use]
    pub const fn scribe_registered_role(&self) -> &RegisteredRole {
        &self.registered_role
    }

    /// Borrows the Scribe used by the Gate and lifecycle paths.
    #[must_use]
    pub fn scribe(&self) -> &Arc<ScribeImpl> {
        &self.ingest
    }

    /// Borrows the request-local governed Scribe fragment follower.
    #[must_use]
    pub const fn fragment_follower(&self) -> &Arc<PhysicalPlanFollower<ScribeTailResolver>> {
        &self.fragment_follower
    }

    /// Records one Scribe-owned fragment execution after authenticated resolution.
    pub(crate) fn record_fragment_execution(&self) {
        self.fragment_executions.fetch_add(1, Ordering::AcqRel);
    }

    /// Records one footer emitted by the Scribe-owned fragment stream.
    pub(crate) fn record_fragment_footer(&self) {
        self.fragment_footers.fetch_add(1, Ordering::AcqRel);
    }

    /// Returns exact Scribe follower activity for production-shaped journeys.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn fragment_inspection(&self) -> (u64, u64) {
        (
            self.fragment_executions.load(Ordering::Acquire),
            self.fragment_footers.load(Ordering::Acquire),
        )
    }

    /// Borrows the root-derived Scribe resource capability.
    #[must_use]
    pub const fn resources(&self) -> &ScribeResources {
        &self.resources
    }

    /// Returns the shared current-ready cluster registry retained by Scribe.
    #[must_use]
    pub fn cluster(&self) -> Arc<ClusterRegistry> {
        Arc::clone(&self.cluster)
    }

    /// Borrows the raw-ticket verifier for Scribe fragment dispatch.
    #[must_use]
    pub fn fragment_verifier(&self) -> Arc<dyn PeerTicketVerifier> {
        Arc::clone(&self.fragment_verifier)
    }

    /// Borrows the durable audit sink for Scribe fragment denials.
    #[must_use]
    pub fn fragment_security_audit(&self) -> Arc<dyn PeerSecurityAudit> {
        Arc::clone(&self.fragment_security_audit)
    }

    /// Returns the shared Scribe-owned tail reader mounted only on private paths.
    #[must_use]
    pub fn tail_reader(&self) -> Arc<ScribeTailReader> {
        Arc::clone(&self.tail_reader)
    }

    /// Returns the exact role-local source used by the retained tail reader.
    #[must_use]
    pub(crate) fn tail_service(&self) -> Arc<FetchLiveTailService> {
        Arc::clone(&self.tail_service)
    }

    /// Reports whether the ingest writer has completed recovery and can accept work.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.lifecycle.is_serving() && self.ingest.is_ready()
    }

    /// Synchronously closes local Scribe readiness before transport cancellation.
    pub(crate) fn start_draining(&self) {
        if self.lifecycle.begin_draining() {
            metrics::gauge!("bifrost_role_ready", "role" => "scribe").set(0.0);
            self.advertise_ready.store(false, Ordering::Release);
        }
    }

    /// Removes Scribe readiness before transport draining begins.
    ///
    /// # Errors
    ///
    /// Returns the registry error when the exact Scribe fence cannot be marked
    /// unready. Local readiness still closes so the process fails closed.
    pub async fn begin_shutdown(&self) -> Result<(), vala_bifrost_redux::cluster::ClusterError> {
        self.start_draining();
        self.cluster.deactivate(&self.registered_role).await
    }

    /// Stops heartbeat before Scribe drain and unregister.
    ///
    /// # Cancellation
    ///
    /// Cancellation after Gate closes can leave accepted Scribe work for
    /// durable recovery. Cleanup is best-effort and never extends the
    /// caller-owned process deadline.
    pub async fn shutdown(&self, deadline: Instant) {
        let _ = self.shutdown_owner(deadline).await;
        let _ = self.shutdown_registry(deadline).await;
    }

    /// Closes and drains Scribe-owned work and retained role tasks within `deadline`.
    ///
    /// The composite closes Gate first; Scribe may flush work accepted before
    /// that transition while budget remains. Every later join shares `deadline`, and
    /// timed-out retained handles are aborted before this method returns.
    pub(crate) async fn shutdown_owner(
        &self,
        deadline: Instant,
    ) -> Result<(), wyrd_spec::vala::error::BifrostError> {
        self.lifecycle.begin_stopping();
        self.role_shutdown.cancel();
        if let Some(authority) = &self.tail_authority {
            authority.clear_replay_state();
        }
        if !self.ingest.shutdown(deadline).await {
            return Err(wyrd_spec::vala::error::BifrostError::Internal {
                detail: "Scribe shutdown did not flush every retained owner".to_owned(),
            });
        }
        if self.owns_fragment_query_audit && self.fragment_query_audit.shutdown(deadline).await != 0
        {
            return Err(wyrd_spec::vala::error::BifrostError::Internal {
                detail: "Scribe shutdown retained tenant-tripwire audit state".to_owned(),
            });
        }
        await_role_task(&self.heartbeat, deadline, "scribe heartbeat").await?;
        await_role_task(&self.snapshot_poller, deadline, "scribe snapshot poller").await?;
        Ok(())
    }

    /// Unregisters the exact Scribe fence within `deadline`.
    ///
    /// The exact retained fence is removed only while budget remains. Timeout,
    /// cancellation, or registry failure leaves lifecycle completion unset and
    /// starts no post-deadline retry.
    pub(crate) async fn shutdown_registry(
        &self,
        deadline: Instant,
    ) -> Result<(), wyrd_spec::vala::error::BifrostError> {
        if tokio::time::Instant::now() < tokio::time::Instant::from_std(deadline) {
            match timeout_at(
                tokio::time::Instant::from_std(deadline),
                self.cluster.shutdown_role(self.registered_role.clone()),
            )
            .await
            {
                Ok(Ok(())) => {
                    self.lifecycle.finish();
                    Ok(())
                }
                Ok(Err(_)) | Err(_) => Err(wyrd_spec::vala::error::BifrostError::Internal {
                    detail: "Scribe role registry shutdown failed".to_owned(),
                }),
            }
        } else {
            Err(wyrd_spec::vala::error::BifrostError::Internal {
                detail: "Scribe role registry shutdown exceeded its deadline".to_owned(),
            })
        }
    }
}

/// Wait for one role-owned task until the process shutdown deadline.
async fn await_role_task(
    task: &Mutex<Option<JoinHandle<()>>>,
    deadline: Instant,
    task_name: &'static str,
) -> Result<(), wyrd_spec::vala::error::BifrostError> {
    let Ok(mut guard) = timeout_at(tokio::time::Instant::from_std(deadline), task.lock()).await
    else {
        tracing::warn!(
            task = task_name,
            "role task lock exceeded shutdown deadline"
        );
        return Err(wyrd_spec::vala::error::BifrostError::Internal {
            detail: format!("{task_name} lock exceeded shutdown deadline"),
        });
    };
    let Some(mut task) = guard.take() else {
        return Ok(());
    };
    drop(guard);
    if timeout_at(tokio::time::Instant::from_std(deadline), &mut task)
        .await
        .is_err()
    {
        tracing::warn!(task = task_name, "role task exceeded shutdown deadline");
        task.abort();
        return Err(wyrd_spec::vala::error::BifrostError::Internal {
            detail: format!("{task_name} exceeded shutdown deadline"),
        });
    }
    Ok(())
}

/// Runtime-ready limits derived from config.
#[derive(Debug, Clone, Copy)]
pub struct LimitsConfig {
    /// Maximum allowed request body size in bytes.
    pub body_bytes: usize,
    /// Per-request processing timeout.
    pub timeout: std::time::Duration,
    /// Maximum in-flight concurrent requests.
    pub concurrency: usize,
}

/// One-shot truncation requested by test-tier language journeys.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryStreamFault {
    /// End the next query immediately after its schema frame.
    EofAfterSchema = 1,
    /// End the next query immediately after its first batch frame.
    EofAfterBatch = 2,
    /// Stall the next query after its schema until the HTTP body is cancelled.
    StallAfterSchema = 3,
    /// Bind the next query's resource probe to a waiting observer and change nothing else.
    CaptureProbe = 4,
}

/// Notification-backed one-shot binding of one query's resource probe.
///
/// This is deliberately not a fault. `StallAfterSchema` exists to hold a body
/// open so a cancellation can be observed, and it therefore owns and eventually
/// cancels the stream. A journey that needs the *successful* path's resource
/// evidence cannot use it, so this observes the same probe and leaves the
/// stream entirely to its real caller.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub struct QueryStreamProbeCapture {
    /// The probe of the query that claimed this capture.
    probe: std::sync::Mutex<Option<Arc<vala_bifrost_redux::oracle::QueryResourceProbe>>>,
    /// Wakes a waiter once the route has bound the probe.
    bound: tokio::sync::Notify,
}

#[cfg(feature = "test-support")]
impl QueryStreamProbeCapture {
    /// Binds the claiming query's probe and wakes any waiter.
    pub fn bind(&self, probe: Arc<vala_bifrost_redux::oracle::QueryResourceProbe>) {
        if let Ok(mut current) = self.probe.lock() {
            *current = Some(probe);
        }
        self.bound.notify_waiters();
    }

    /// Returns the bound probe, waiting without polling until one arrives.
    ///
    /// The waiter is registered before the current binding is observed, because
    /// [`tokio::sync::Notify::notify_waiters`] wakes only already-registered
    /// waiters; observing first would lose a binding that lands in between and
    /// park this caller forever.
    pub async fn wait_resource_probe(&self) -> Arc<vala_bifrost_redux::oracle::QueryResourceProbe> {
        loop {
            let notified = self.bound.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(probe) = self.probe.lock().ok().and_then(|probe| probe.clone()) {
                return probe;
            }
            notified.await;
        }
    }
}

/// Notification-backed lifecycle state for one stalled test-tier query body.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub struct QueryStreamStall {
    /// Records whether the response body reached the deterministic stall.
    entered: AtomicBool,
    /// Wakes a waiter when the response body reaches the deterministic stall.
    entered_notify: tokio::sync::Notify,
    /// Records whether client cancellation dropped the stalled response body.
    dropped: AtomicBool,
    /// Wakes a waiter after stream ownership and response-body state drop.
    dropped_notify: tokio::sync::Notify,
    /// Exact lifecycle observation for the query that claimed this stall.
    resource_probe: std::sync::Mutex<Option<Arc<vala_bifrost_redux::oracle::QueryResourceProbe>>>,
}

#[cfg(feature = "test-support")]
impl QueryStreamStall {
    /// Binds the stall to the existing Oracle query identity before schema emission.
    pub fn bind_resource_probe(&self, probe: Arc<vala_bifrost_redux::oracle::QueryResourceProbe>) {
        if let Ok(mut current) = self.resource_probe.lock() {
            *current = Some(probe);
        }
    }

    /// Returns the exact lifecycle probe for the query that reached this stall.
    ///
    /// # Errors
    ///
    /// Returns an error when the stall has not been claimed or its lock is poisoned.
    pub fn resource_probe(
        &self,
    ) -> Result<Arc<vala_bifrost_redux::oracle::QueryResourceProbe>, String> {
        self.resource_probe
            .lock()
            .map_err(|_| "query resource-probe lock poisoned".to_owned())?
            .clone()
            .ok_or_else(|| "query resource probe is not bound".to_owned())
    }

    /// Publishes that the body is blocked immediately after its schema frame.
    pub fn mark_entered(&self) {
        self.entered.store(true, Ordering::Release);
        self.entered_notify.notify_waiters();
    }

    /// Waits without polling until the response reaches its schema stall.
    pub async fn wait_entered(&self) {
        while !self.entered.load(Ordering::Acquire) {
            self.entered_notify.notified().await;
        }
    }

    /// Publishes that cancellation dropped the stalled body and its stream owner.
    pub fn mark_dropped(&self) {
        self.dropped.store(true, Ordering::Release);
        self.dropped_notify.notify_waiters();
    }

    /// Waits without polling until cancellation drops the stalled response body.
    pub async fn wait_dropped(&self) {
        while !self.dropped.load(Ordering::Acquire) {
            self.dropped_notify.notified().await;
        }
    }
}

/// Atomic next-query fault controller owned by a test server instance.
#[cfg(feature = "test-support")]
#[derive(Debug, Default, Clone)]
pub struct QueryStreamFaultController {
    next: Arc<AtomicU8>,
    /// Lifecycle state retained for the one scheduled stall probe.
    stall: Arc<std::sync::Mutex<Option<Arc<QueryStreamStall>>>>,
    /// Observer retained for the one armed passive probe capture.
    capture: Arc<std::sync::Mutex<Option<Arc<QueryStreamProbeCapture>>>>,
}

/// Atomic control-audit fault owned by a test server instance.
#[cfg(feature = "test-support")]
#[derive(Debug, Default, Clone)]
pub struct QueryControlAuditFaultController {
    fail_cancel_attempts: Arc<AtomicBool>,
    /// Forces the authoritative object-denial audit append to fail.
    fail_object_denials: Arc<AtomicBool>,
}

#[cfg(feature = "test-support")]
impl QueryControlAuditFaultController {
    /// Enables failure of cancellation pre-dispatch audit appends.
    pub fn fail_cancel_attempts(&self) {
        self.fail_cancel_attempts.store(true, Ordering::Release);
    }

    /// Restores cancellation pre-dispatch audit appends.
    pub fn restore_cancel_attempts(&self) {
        self.fail_cancel_attempts.store(false, Ordering::Release);
    }

    /// Reports whether cancellation pre-dispatch audit appends must fail.
    #[must_use]
    pub fn cancel_attempts_fail(&self) -> bool {
        self.fail_cancel_attempts.load(Ordering::Acquire)
    }

    /// Enables failure of the authoritative object-denial audit append.
    pub fn fail_object_denials(&self) {
        self.fail_object_denials.store(true, Ordering::Release);
    }

    /// Restores the authoritative object-denial audit append.
    pub fn restore_object_denials(&self) {
        self.fail_object_denials.store(false, Ordering::Release);
    }

    /// Reports whether object-denial audit appends must fail.
    #[must_use]
    pub fn object_denials_fail(&self) -> bool {
        self.fail_object_denials.load(Ordering::Acquire)
    }
}

#[cfg(feature = "test-support")]
impl QueryStreamFaultController {
    /// Schedule one truncation and replace any previously scheduled fault.
    pub fn set_next(&self, fault: QueryStreamFault) {
        self.next.store(fault as u8, Ordering::Release);
    }

    /// Schedules one schema stall and returns its notification-backed lifecycle.
    #[must_use]
    pub fn stall_next_after_schema(&self) -> Arc<QueryStreamStall> {
        let stall = Arc::new(QueryStreamStall::default());
        if let Ok(mut current) = self.stall.lock() {
            *current = Some(Arc::clone(&stall));
        }
        self.set_next(QueryStreamFault::StallAfterSchema);
        stall
    }

    /// Arms one passive capture of the next query's resource probe.
    ///
    /// Stored beside the existing atomic next-query fault, so arming a capture
    /// and scheduling a truncation remain the same one-shot slot: a journey
    /// cannot accidentally observe a query it also truncated.
    #[must_use]
    pub fn capture_next_probe(&self) -> Arc<QueryStreamProbeCapture> {
        let capture = Arc::new(QueryStreamProbeCapture::default());
        if let Ok(mut current) = self.capture.lock() {
            *current = Some(Arc::clone(&capture));
        }
        self.set_next(QueryStreamFault::CaptureProbe);
        capture
    }

    /// Claim and clear the one-shot fault atomically.
    #[must_use]
    pub fn claim(&self) -> Option<QueryStreamFault> {
        match self.next.swap(0, Ordering::AcqRel) {
            1 => Some(QueryStreamFault::EofAfterSchema),
            2 => Some(QueryStreamFault::EofAfterBatch),
            3 => Some(QueryStreamFault::StallAfterSchema),
            4 => Some(QueryStreamFault::CaptureProbe),
            _ => None,
        }
    }

    /// Takes the observer paired with a claimed probe capture.
    #[must_use]
    pub fn claim_capture(&self) -> Option<Arc<QueryStreamProbeCapture>> {
        self.capture
            .lock()
            .ok()
            .and_then(|mut capture| capture.take())
    }

    /// Takes the lifecycle state paired with a claimed schema stall.
    #[must_use]
    pub fn claim_stall(&self) -> Option<Arc<QueryStreamStall>> {
        self.stall.lock().ok().and_then(|mut stall| stall.take())
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            body_bytes: 1_048_576,
            timeout: std::time::Duration::from_secs(30),
            concurrency: 1024,
        }
    }
}

/// Bifrost-owned Forge coordinator and worker composition.
#[derive(Clone)]
pub struct Forge {
    /// Selected durable maintenance coordinator.
    coordinator: Option<Arc<ForgeCoordinator>>,
    /// Selected bounded task worker.
    worker: Option<Arc<ForgeWorker>>,
    /// Process lifecycle signal shared by both selected capabilities.
    shutdown: CancellationToken,
    /// Stable physical node identity retained by the selected Forge owner.
    node_id: wyrd_spec::vala::api::NodeId,
    /// Set only after every supervised Forge task joins before the process deadline.
    supervision_drained: Arc<AtomicBool>,
    /// Coordinator readiness published by the supervised planning loop.
    coordinator_ready: vala_bifrost_redux::forge::ForgeRoleReadiness,
    /// Worker readiness published only after durable recovery completes.
    worker_ready: vala_bifrost_redux::forge::ForgeRoleReadiness,
}

impl Forge {
    /// Composes exactly the Forge capabilities selected by the process target.
    #[must_use]
    pub fn new(
        coordinator: Option<Arc<ForgeCoordinator>>,
        worker: Option<Arc<ForgeWorker>>,
        shutdown: CancellationToken,
        node_id: wyrd_spec::vala::api::NodeId,
    ) -> Self {
        Self {
            coordinator,
            worker,
            shutdown,
            node_id,
            supervision_drained: Arc::new(AtomicBool::new(false)),
            coordinator_ready: vala_bifrost_redux::forge::ForgeRoleReadiness::detached(),
            worker_ready: vala_bifrost_redux::forge::ForgeRoleReadiness::detached(),
        }
    }

    /// Returns the handle the supervised coordinator loop publishes into.
    #[must_use]
    pub fn coordinator_readiness(&self) -> vala_bifrost_redux::forge::ForgeRoleReadiness {
        self.coordinator_ready.clone()
    }

    /// Returns the handle the supervised worker loop publishes into.
    #[must_use]
    pub fn worker_readiness(&self) -> vala_bifrost_redux::forge::ForgeRoleReadiness {
        self.worker_ready.clone()
    }

    /// Borrows the selected coordinator.
    #[must_use]
    pub const fn coordinator(&self) -> Option<&Arc<ForgeCoordinator>> {
        self.coordinator.as_ref()
    }

    /// Borrows the selected worker.
    #[must_use]
    pub const fn worker(&self) -> Option<&Arc<ForgeWorker>> {
        self.worker.as_ref()
    }

    /// Borrows the token every selected Forge capability stops on.
    #[must_use]
    pub const fn shutdown_token(&self) -> &CancellationToken {
        &self.shutdown
    }

    /// Signals selected Forge capabilities to stop accepting work.
    ///
    /// Routing closes before the token is cancelled: a role that is losing its
    /// authority must stop being advertised on `/readyz` ahead of the loops
    /// noticing, and the close is terminal so a loop still running cannot
    /// republish readiness on its way out.
    pub fn begin_shutdown(&self) {
        self.coordinator_ready.close();
        self.worker_ready.close();
        self.shutdown.cancel();
    }

    /// Records the production supervisor's bounded join result exactly once.
    pub(crate) fn mark_supervision_drained(&self, drained: bool) {
        self.supervision_drained.store(drained, Ordering::Release);
    }

    /// Returns whether every selected Forge supervisor joined without deadline abort.
    fn supervision_drained(&self) -> bool {
        self.supervision_drained.load(Ordering::Acquire)
    }
}

/// Process-wide composition root for every Bifrost capability.
///
/// This owner is the only place where the catalog, resource graph, Gate, and
/// selected Scribe, Forge, and Oracle runtimes are retained together. Routes
/// and lifecycle owners borrow narrow capabilities from this graph; they never
/// reconstruct sibling state from [`AppState`].
#[derive(Clone)]
pub struct Bifrost {
    /// The one public authentication, admission, and dispatch owner.
    gate: ServerGate,
    /// Selected Scribe runtime, when this process owns the role.
    scribe: Option<Arc<Scribe>>,
    /// Selected Forge runtime, when this process owns a coordinator or worker.
    forge: Option<Arc<Forge>>,
    /// Selected Oracle runtime, when this process owns the role.
    oracle: Option<Arc<Oracle>>,
    /// This node's one Bifrost storage owner, shared by every co-located role.
    ///
    /// `None` only for the ownerless unit-test shell, which composes no role and
    /// therefore performs no object I/O at all.
    bifrost_storage: Option<Arc<vala_bifrost_redux::storage::BifrostStorage>>,
    /// Process-wide encoded-body admission shared by every transport edge.
    transport: vala_bifrost_redux::gate::limits::BifrostTransportAdmission,
    /// Verifier shared by public Gate work and the private peer service.
    token_verifier: Arc<TokenVerifier>,
    /// The one peer Service principal this process admits, when it serves the
    /// private plane. Absent for targets that open no peer listener.
    peer_identity: Option<crate::grpc::PeerWorkloadIdentity>,
    /// Retained forwarder, reachable by the private peer inbound handler.
    query_forwarder: Option<Arc<crate::oracle::ReadyOracleForwarder>>,
    /// Lifecycle routing retained by every query ingress, regardless of local role.
    query_controls: Option<crate::oracle::RunningQueryControls>,
    /// Read-only proof handle for test-tier inspection of the production graph.
    #[cfg(feature = "test-support")]
    test_resources: Option<BifrostRoleResources>,
    /// Catalog reachable by an ownerless unit-test shell that selects no role.
    ///
    /// A production process only ever reaches the catalog through its selected
    /// Scribe or Oracle runtime. An in-crate unit test exercises the catalog
    /// service functions without composing either role, so the shell retains the
    /// already-built test catalog here and [`Bifrost::catalog`] falls back to it
    /// when no role owns one.
    #[cfg(feature = "test-support")]
    test_catalog: Option<Arc<BifrostCatalog>>,
}

/// Complete immutable composition retained by one published [`Bifrost`].
///
/// The values travel together because they are derived once, in boot order,
/// from the same resource graph; passing them positionally would let a future
/// caller silently transpose the two `Arc`-shaped role runtimes.
pub(crate) struct BifrostComposition {
    /// The one public Gate composed from the boot-validated dependency graph.
    pub(crate) gate: ServerGate,
    /// Selected Scribe runtime, when this process owns the role.
    pub(crate) scribe: Option<Arc<Scribe>>,
    /// Selected Forge runtime, when this process owns a coordinator or worker.
    pub(crate) forge: Option<Arc<Forge>>,
    /// Selected Oracle runtime, when this process owns the role.
    pub(crate) oracle: Option<Arc<Oracle>>,
    /// This node's one Bifrost storage owner, shared by every co-located role.
    pub(crate) bifrost_storage: Arc<vala_bifrost_redux::storage::BifrostStorage>,
    /// Process-wide encoded-body admission shared by every transport edge.
    pub(crate) transport: vala_bifrost_redux::gate::limits::BifrostTransportAdmission,
    /// Verifier shared by public Gate work and the private peer service.
    pub(crate) token_verifier: Arc<TokenVerifier>,
    /// The one peer Service principal this process admits, when it serves the
    /// private plane.
    pub(crate) peer_identity: Option<crate::grpc::PeerWorkloadIdentity>,
    /// Canonical ready-Oracle selector and authenticated private forwarder.
    pub(crate) query_forwarder: Option<Arc<crate::oracle::ReadyOracleForwarder>>,
    /// Shared local controls or remote-only routing for a forwarding ingress.
    pub(crate) query_controls: Option<crate::oracle::RunningQueryControls>,
    /// Already-composed production resources exposed only to the test tier.
    #[cfg(feature = "test-support")]
    pub(crate) resources: Option<BifrostRoleResources>,
}

impl Bifrost {
    /// Assembles the complete immutable process composition before publication.
    pub(crate) fn assembled(composition: BifrostComposition) -> Arc<Self> {
        let BifrostComposition {
            gate,
            scribe,
            forge,
            oracle,
            bifrost_storage,
            transport,
            token_verifier,
            peer_identity,
            query_forwarder,
            query_controls,
            #[cfg(feature = "test-support")]
            resources,
        } = composition;
        Arc::new(Self {
            gate,
            scribe,
            forge,
            oracle,
            bifrost_storage: Some(bifrost_storage),
            transport,
            token_verifier,
            peer_identity,
            query_forwarder,
            query_controls,
            #[cfg(feature = "test-support")]
            test_resources: resources,
            #[cfg(feature = "test-support")]
            test_catalog: None,
        })
    }

    /// Builds an ownerless unit-test shell for non-Bifrost route fixtures.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn test_shell(token_verifier: Arc<TokenVerifier>) -> Arc<Self> {
        Arc::new(Self {
            bifrost_storage: None,
            gate: ServerGate::without_scribe(
                vala_bifrost_redux::gate::auth::ingest_auth_interceptor(Arc::clone(
                    &token_verifier,
                )),
                IngestLimits::default(),
            ),
            scribe: None,
            forge: None,
            oracle: None,
            transport: vala_bifrost_redux::gate::limits::BifrostTransportAdmission::for_tests(),
            token_verifier,
            peer_identity: None,
            query_forwarder: None,
            query_controls: None,
            test_resources: None,
            test_catalog: None,
        })
    }

    /// Builds an ownerless unit-test shell that can still reach one catalog.
    ///
    /// The in-crate `bifrost::service` tests call the catalog service functions
    /// directly against the shared embedded-Postgres catalog. They compose no
    /// Scribe or Oracle runtime, so the shell retains the catalog itself and
    /// [`Self::catalog`] resolves to it. Everything else matches
    /// [`Self::test_shell`].
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn test_shell_with_catalog(
        token_verifier: Arc<TokenVerifier>,
        catalog: Arc<BifrostCatalog>,
    ) -> Arc<Self> {
        Arc::new(Self {
            bifrost_storage: None,
            gate: ServerGate::without_scribe(
                vala_bifrost_redux::gate::auth::ingest_auth_interceptor(Arc::clone(
                    &token_verifier,
                )),
                IngestLimits::default(),
            ),
            scribe: None,
            forge: None,
            oracle: None,
            transport: vala_bifrost_redux::gate::limits::BifrostTransportAdmission::for_tests(),
            token_verifier,
            peer_identity: None,
            query_forwarder: None,
            query_controls: None,
            test_resources: None,
            test_catalog: Some(catalog),
        })
    }

    /// Borrows the selected Scribe runtime.
    #[must_use]
    pub const fn scribe(&self) -> Option<&Arc<Scribe>> {
        self.scribe.as_ref()
    }

    /// Borrows the one public Gate.
    #[must_use]
    pub const fn gate(&self) -> &ServerGate {
        &self.gate
    }

    /// Borrows the verifier shared by public Gate work and the peer service.
    ///
    /// The private peer inbound handler authenticates through the same engine
    /// as public ingest; exposing the verifier here keeps that from becoming a
    /// third authentication path.
    #[must_use]
    pub fn token_verifier(&self) -> &TokenVerifier {
        &self.token_verifier
    }

    /// Borrows the shared verifier as an owner the peer boundary can retain.
    #[must_use]
    pub fn shared_token_verifier(&self) -> Arc<TokenVerifier> {
        Arc::clone(&self.token_verifier)
    }

    /// Borrows the one peer Service principal this process admits.
    ///
    /// `None` on a target that opens no peer listener, which is why composing
    /// a peer router without it is a boot error rather than a silent default.
    #[must_use]
    pub fn peer_identity(&self) -> Option<&crate::grpc::PeerWorkloadIdentity> {
        self.peer_identity.as_ref()
    }

    /// Borrows the retained ready-Oracle forwarder, when this process has one.
    ///
    /// Public SQL reaches the forwarder through the Gate dispatch seam. This
    /// accessor exists for the private peer inbound path, which executes an
    /// already-signed envelope rather than beginning a public request.
    #[must_use]
    pub(crate) fn query_forwarder(&self) -> Option<&Arc<crate::oracle::ReadyOracleForwarder>> {
        self.query_forwarder.as_ref()
    }

    /// Exercises signed local forwarding with an ingress budget shorter than the request.
    ///
    /// The test seam changes only the signed input instant; the retained forwarder
    /// performs normal signature, context, replay and fence checks before execution.
    ///
    /// # Errors
    /// Returns role-unavailable for ownerless shells, or the normal acceptance error.
    #[cfg(feature = "test-support")]
    pub async fn query_with_signed_deadline_for_test(
        &self,
        context: vala_bifrost_redux::oracle::AuthorizedQueryContext,
        request: wyrd_spec::vala::api::BifrostQueryRequest,
        deadline_ms: i64,
    ) -> Result<vala_bifrost_redux::oracle::OracleQueryStream, wyrd_spec::vala::BifrostError> {
        self.query_forwarder
            .as_ref()
            .ok_or(wyrd_spec::vala::BifrostError::OracleRoleUnavailable)?
            .accept_with_deadline_for_test(context, request, deadline_ms)
            .await
    }

    /// Borrows the test-only silent forwarding-peer switch of this replica.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn silent_forward_peer_for_test(&self) -> Option<&SilentForwardPeer> {
        self.query_forwarder
            .as_ref()
            .map(|forwarder| forwarder.silent_peer_for_test())
    }

    /// Borrows the selected Oracle runtime.
    #[must_use]
    pub const fn oracle(&self) -> Option<&Arc<Oracle>> {
        self.oracle.as_ref()
    }

    /// Borrows tenant-authorized lifecycle routing on local or forwarding-only ingress.
    #[must_use]
    pub fn query_controls(&self) -> Option<&crate::oracle::RunningQueryControls> {
        self.query_controls.as_ref()
    }

    /// Returns Scribe's exact tail service when this process owns Scribe.
    #[must_use]
    pub fn scribe_tail_service(&self) -> Option<Arc<FetchLiveTailService>> {
        self.scribe.as_ref().map(|runtime| runtime.tail_service())
    }

    /// Returns Oracle's private peer runtime when this process owns Oracle.
    #[must_use]
    pub fn oracle_peer_service(&self) -> Option<Arc<crate::oracle::OraclePeerRuntime>> {
        self.oracle
            .as_ref()
            .map(|runtime| Arc::clone(runtime.peer()))
    }

    /// Borrows the shared catalog retained by the selected data owners.
    #[must_use]
    pub(crate) fn catalog(&self) -> Option<&Arc<BifrostCatalog>> {
        let selected = self
            .scribe
            .as_ref()
            .map(|scribe| &scribe.catalog)
            .or_else(|| self.oracle.as_ref().map(|oracle| &oracle.catalog));
        #[cfg(feature = "test-support")]
        {
            selected.or(self.test_catalog.as_ref())
        }
        #[cfg(not(feature = "test-support"))]
        {
            selected
        }
    }

    /// Borrows this node's one Bifrost storage owner.
    ///
    /// `None` only for the ownerless unit-test shell. Every co-located role in a
    /// composed process observes the identical pointer, which is what makes the
    /// node-wide request ceiling and the single shutdown real rather than
    /// per-role.
    #[must_use]
    pub fn bifrost_storage(&self) -> Option<&Arc<vala_bifrost_redux::storage::BifrostStorage>> {
        self.bifrost_storage.as_ref()
    }

    /// Borrows the shared role-resource graph retained by the selected owners.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub(crate) fn resources(&self) -> Option<&BifrostRoleResources> {
        self.test_resources.as_ref()
    }

    /// Borrows the selected Forge runtime.
    #[must_use]
    pub(crate) fn forge(&self) -> Option<&Forge> {
        self.forge.as_deref()
    }

    /// Reports whether this process exposes a public serving capability.
    #[must_use]
    pub fn serves_api(&self) -> bool {
        self.scribe.is_some() || self.oracle.is_some()
    }

    /// Returns the physical node identity retained by one selected subsystem owner.
    #[must_use]
    pub fn node_id(&self) -> Option<wyrd_spec::vala::api::NodeId> {
        self.scribe
            .as_ref()
            .map(|scribe| scribe.registered_role.key.node_id)
            .or_else(|| {
                self.oracle
                    .as_ref()
                    .map(|oracle| oracle.registered_role.key.node_id)
            })
            .or_else(|| self.forge.as_ref().map(|forge| forge.node_id))
    }

    /// Returns the shared transport admission owner retained by Gate.
    #[must_use]
    pub fn transport_admission(
        &self,
    ) -> vala_bifrost_redux::gate::limits::BifrostTransportAdmission {
        self.transport.clone()
    }

    /// Returns the shared root-health signal through one selected role capability.
    ///
    /// Forge is not a source here: it holds no memory or scratch capability, so
    /// a Forge-only target has no root ledger to report on.
    #[must_use]
    pub fn resource_health(&self) -> Option<vala_bifrost_redux::resources::BifrostResourceHealth> {
        self.scribe
            .as_ref()
            .map(|scribe| scribe.resources.health())
            .or_else(|| self.oracle.as_ref().map(|oracle| oracle.resources.health()))
    }

    /// Dispatches one authorized SQL request through Gate into the selected Oracle.
    ///
    /// # Errors
    /// Returns role-unavailable, admission, planning, or execution errors.
    pub async fn query_sql(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryRequest,
    ) -> Result<OracleQueryStream, wyrd_spec::vala::error::BifrostError> {
        self.gate().query_sql(context, request).await
    }

    /// Closes public admission and synchronously removes local readiness advertisement.
    pub fn begin_shutdown(&self) {
        self.gate.close();
        if let Some(oracle) = &self.oracle {
            oracle.start_draining();
        }
        if let Some(scribe) = &self.scribe {
            scribe.start_draining();
        }
        if let Some(forge) = &self.forge {
            forge.begin_shutdown();
        }
    }

    /// Drains every selected subsystem and this node's storage owner against
    /// one absolute deadline.
    ///
    /// The order is fixed and load-bearing: readiness and public admission
    /// close first, Forge supervision must already be quiesced, the Oracle and
    /// then the Scribe owner and registry drain, and only then is the storage
    /// owner closed. Storage is last because work a role already admitted may
    /// still need object I/O to finish; closing it first would fail that work
    /// rather than let it complete.
    ///
    /// Any failure along the way — unquiesced Forge supervision, a role drain
    /// error, or a storage owner that does not settle by the deadline — runs
    /// the abort path for both roles *and* storage before returning, so a
    /// failed shutdown never leaves the metadata cache open, decoded entries
    /// retained, or object I/O admissible. A successful report is therefore
    /// only ever produced when storage is closed and settled.
    ///
    /// # Errors
    /// Returns a stable lifecycle failure when a selected role cannot remove
    /// readiness, when Forge supervision has not joined, or
    /// [`BifrostError::Internal`](wyrd_spec::vala::error::BifrostError::Internal)
    /// with detail `"Bifrost storage did not settle before shutdown deadline"`
    /// when the storage owner does not settle.
    pub async fn shutdown(
        &self,
        deadline: Instant,
    ) -> Result<BifrostShutdownReport, wyrd_spec::vala::error::BifrostError> {
        self.begin_shutdown();
        match self.drain_selected_owners(deadline).await {
            Ok(report) => Ok(report),
            Err(error) => {
                self.abort_selected_owners().await;
                Err(error)
            }
        }
    }

    /// Runs the ordered drain without owning the abort-on-failure decision.
    ///
    /// Split from [`Self::shutdown`] so every failure path leaves through one
    /// abort rather than repeating it at each `?`.
    ///
    /// # Errors
    /// Returns the first stable lifecycle failure the ordered drain reached.
    async fn drain_selected_owners(
        &self,
        deadline: Instant,
    ) -> Result<BifrostShutdownReport, wyrd_spec::vala::error::BifrostError> {
        let forge_drained = if let Some(forge) = &self.forge {
            if !forge.supervision_drained() {
                return Err(wyrd_spec::vala::error::BifrostError::Internal {
                    detail: "Forge supervision did not join before shutdown".to_owned(),
                });
            }
            true
        } else {
            false
        };
        let oracle_drained = if let Some(oracle) = &self.oracle {
            selected_owner_completion(oracle.begin_shutdown().await.map_err(|_| {
                wyrd_spec::vala::error::BifrostError::RunningQueryControlUnavailable
            }))?;
            oracle.shutdown_owner(deadline).await?;
            oracle.shutdown_registry(deadline).await?;
            true
        } else {
            false
        };
        let scribe_drained = if let Some(scribe) = &self.scribe {
            selected_owner_completion(
                scribe
                    .begin_shutdown()
                    .await
                    .map_err(|_| wyrd_spec::vala::error::BifrostError::ScribeRoleUnavailable),
            )?;
            scribe.shutdown_owner(deadline).await?;
            scribe.shutdown_registry(deadline).await?;
            true
        } else {
            false
        };
        if let Some(storage) = &self.bifrost_storage
            && !storage.close(deadline).await
        {
            return Err(wyrd_spec::vala::error::BifrostError::Internal {
                detail: "Bifrost storage did not settle before shutdown deadline".to_owned(),
            });
        }
        Ok(BifrostShutdownReport {
            scribe_drained,
            forge_drained,
            oracle_drained,
        })
    }

    /// Closes public admission and aborts selected role owners without claiming a flush.
    ///
    /// Awaits the storage owner's abort, so once this returns no retained
    /// loader and no governed operation is still running against the node's
    /// backend. Repeated calls remain safe.
    pub async fn abort(&self) {
        self.abort_selected_owners().await;
    }

    /// Cancels every selected owner and settles the storage owner.
    ///
    /// The one abort path, shared by the public seam and by every failed
    /// shutdown, so a failure can never take a cleanup route that skips
    /// storage.
    async fn abort_selected_owners(&self) {
        self.gate.close();
        if let Some(query) = &self.oracle {
            query.abort_shutdown();
        }
        if let Some(ingest) = &self.scribe {
            ingest.abort_shutdown();
        }
        if let Some(forge) = &self.forge {
            forge.begin_shutdown();
        }
        if let Some(storage) = &self.bifrost_storage {
            storage.abort().await;
        }
    }
}

/// Preserves a selected owner's concrete lifecycle failure without translating it to success.
fn selected_owner_completion(
    result: Result<(), wyrd_spec::vala::error::BifrostError>,
) -> Result<(), wyrd_spec::vala::error::BifrostError> {
    result
}

/// Structured completion summary for the selected Bifrost subsystem graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BifrostShutdownReport {
    /// A selected Scribe owner completed its bounded shutdown path.
    pub scribe_drained: bool,
    /// A selected Forge owner completed its bounded shutdown path.
    pub forge_drained: bool,
    /// A selected Oracle owner completed its bounded shutdown path.
    pub oracle_drained: bool,
}

impl BifrostShutdownReport {
    /// Returns the report for a teardown in which no subsystem drained.
    ///
    /// Two paths legitimately produce it. [`Bifrost::abort`] closes admission
    /// and cancels role owners without claiming a flush, so an aborted teardown
    /// drained nothing. A process topology that never runs the bounded Bifrost
    /// drain at all — a dedicated Forge worker, for instance — reports the same
    /// thing. Naming the value keeps those paths from open-coding an all-`false`
    /// literal whose meaning depends on the reader.
    #[must_use]
    pub const fn none_drained() -> Self {
        Self {
            scribe_drained: false,
            forge_drained: false,
            oracle_drained: false,
        }
    }
}

/// Process-wide handle registry. One instance is shared by all HTTP handlers.
///
/// Foundation commits append their owned handles here, for example storage,
/// auth verification, and policy evaluation. This skeleton ships only the
/// runtime database pools that already survive boot.
#[derive(Clone)]
pub struct AppState {
    /// Composed production Postgres handle. Single DB access path for all routes.
    pub postgres: Arc<ServerPostgres>,
    /// Process-wide artifact storage handle.
    pub storage: Arc<StorageHandle>,
    /// One composite owner for the complete Bifrost runtime graph.
    pub bifrost: Arc<Bifrost>,
    /// Authentication handles: token issuance + verification + issuer/binding resolution.
    pub auth: ServerAuth,
    /// Authorization handles: policy decision + RBAC evaluation + decision audit.
    pub authz: ServerAuthz,
    /// Deployment posture (Development / Production) locked at boot.
    pub deployment_profile: DeploymentProfile,
    /// Shared cancellation token for cooperative shutdown.
    pub shutdown_token: CancellationToken,
    /// In-flight MCP tool work for this process.
    ///
    /// Every `/mcp` tool invocation holds an RAII token for the life of its
    /// work, including cancellation cleanup. After the supervised transport
    /// drain stops admission, `BoundServer::run` closes this tracker and waits
    /// for it inside the existing shutdown deadline, so a pod cannot report a
    /// clean drain while an MCP handler is still settling.
    pub mcp_tasks: tokio_util::task::TaskTracker,
    /// Register the test-support MCP context probe in the `/mcp` tool catalog.
    ///
    /// Default `false`: only a test server that explicitly opted in through
    /// `WyrdTestServerBuilder::with_mcp_context_probe_for_test` exposes it, so
    /// merely compiling `test-support` never adds a capability to the catalog.
    #[cfg(feature = "test-support")]
    pub mcp_context_probe: bool,
    /// Telemetry guard (holds the tracer provider).
    pub telemetry: Arc<TelemetryGuard>,
    /// Request-shaping limits for the router middleware stack.
    pub limits: LimitsConfig,
    /// gRPC health reporter shared between HTTP readiness and gRPC health service.
    pub grpc_health: HealthReporter,
    /// Cached readiness snapshot from the background readiness_loop task.
    pub readiness: Arc<ArcSwap<ReadinessSnapshot>>,
    /// Retained status of this process's private Bifrost peer listener.
    pub peer_plane: Arc<crate::app::peer_plane::PeerPlaneStatus>,
    /// Tenant-keyed in-memory eval run/lease/session map. Ephemeral, single-replica.
    pub eval_runs: EvalRuns,
    /// Optional deterministic stream truncation controller for test servers.
    #[cfg(feature = "test-support")]
    pub query_stream_fault: Option<QueryStreamFaultController>,
    /// Optional deterministic lifecycle-audit fault controller for test servers.
    #[cfg(feature = "test-support")]
    pub query_control_audit_fault: Option<QueryControlAuditFaultController>,
}

impl AppState {
    /// Build runtime state from production-ready Postgres handles.
    #[must_use]
    pub fn new(
        postgres: Arc<ServerPostgres>,
        storage: Arc<StorageHandle>,
        bifrost: Arc<Bifrost>,
        shutdown_token: CancellationToken,
    ) -> Self {
        let (reporter, _service) = wyrd_tonic::tonic_health::server::health_reporter();
        Self {
            postgres,
            storage,
            bifrost,
            auth: ServerAuth::default(),
            authz: ServerAuthz::default(),
            deployment_profile: DeploymentProfile::Development,
            shutdown_token,
            mcp_tasks: tokio_util::task::TaskTracker::new(),
            #[cfg(feature = "test-support")]
            mcp_context_probe: false,
            telemetry: Arc::new(wyrd_telemetry::init_test_only_no_global(
                wyrd_telemetry::TelemetryConfig::default(),
            )),
            limits: LimitsConfig::default(),
            grpc_health: reporter,
            readiness: Arc::new(ArcSwap::from_pointee(ReadinessSnapshot::initial())),
            peer_plane: Arc::new(crate::app::peer_plane::PeerPlaneStatus::default()),
            eval_runs: new_run_map(),
            #[cfg(feature = "test-support")]
            query_stream_fault: None,
            #[cfg(feature = "test-support")]
            query_control_audit_fault: None,
        }
    }

    /// Register the test-support MCP context probe on this state.
    ///
    /// Connectivity journeys call this through
    /// `WyrdTestServerBuilder::with_mcp_context_probe_for_test`; ordinary test
    /// servers leave the catalog empty.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_mcp_context_probe(mut self, enabled: bool) -> Self {
        self.mcp_context_probe = enabled;
        self
    }

    /// Attach a test-tier one-shot query stream fault controller.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_query_stream_fault(mut self, controller: QueryStreamFaultController) -> Self {
        self.query_stream_fault = Some(controller);
        self
    }

    /// Attach a test-tier lifecycle-audit fault controller.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_query_control_audit_fault(
        mut self,
        controller: QueryControlAuditFaultController,
    ) -> Self {
        self.query_control_audit_fault = Some(controller);
        self
    }

    /// Attach authentication handles (issuance, verification, resolvers).
    #[must_use]
    pub fn with_auth(mut self, auth: ServerAuth) -> Self {
        self.auth = auth;
        self
    }

    /// Attach authorization handles (policy, RBAC, audit).
    #[must_use]
    pub fn with_authz(mut self, authz: ServerAuthz) -> Self {
        self.authz = authz;
        self
    }

    /// Set the deployment posture.
    #[must_use]
    pub fn with_deployment_profile(mut self, profile: DeploymentProfile) -> Self {
        self.deployment_profile = profile;
        self
    }

    /// Borrows the selected Scribe runtime.
    #[must_use]
    pub fn bifrost_ingest(&self) -> Option<&Arc<Scribe>> {
        self.bifrost.scribe()
    }

    /// Attach the live telemetry guard.
    #[must_use]
    pub fn with_telemetry(mut self, telemetry: Arc<TelemetryGuard>) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Attach request-shaping limits from WyrdServerConfig.
    #[must_use]
    pub fn with_limits(mut self, limits: LimitsConfig) -> Self {
        self.limits = limits;
        self
    }

    /// Attach the gRPC health reporter.
    #[must_use]
    pub fn with_grpc_health(mut self, reporter: HealthReporter) -> Self {
        self.grpc_health = reporter;
        self
    }

    /// Attach the cached readiness publisher.
    #[must_use]
    pub fn with_readiness(mut self, readiness: Arc<ArcSwap<ReadinessSnapshot>>) -> Self {
        self.readiness = readiness;
        self
    }

    /// Replace the storage handle.
    #[must_use]
    pub fn with_storage(mut self, storage: Arc<StorageHandle>) -> Self {
        self.storage = storage;
        self
    }

    /// Get a tenant-scoped Postgres connection for registry operations.
    ///
    /// # Errors
    ///
    /// Returns a redacted `WYRD_REGISTRY_*` unavailability error when the
    /// tenant-scoped connection cannot be acquired.
    pub async fn registry_tenant_conn(
        &self,
        tenant_id: DataTenantId,
    ) -> Result<wyrd_sql::TenantConn<'_>, WyrdError> {
        self.postgres
            .tenant_conn(tenant_id)
            .await
            .map_err(registry_db_error)
    }

    /// Returns the independently optional retained Oracle query subsystem.
    #[must_use]
    pub fn bifrost_query(&self) -> Option<&Arc<Oracle>> {
        self.bifrost.oracle()
    }

    /// Borrow the retained Forge handle for server-owned supervision.
    #[must_use]
    pub(crate) fn forge_handle(&self) -> Option<&Arc<ForgeCoordinator>> {
        self.bifrost.forge().and_then(Forge::coordinator)
    }

    /// Borrow the retained Forge owner from test-tier callers.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn forge(&self) -> Option<&Forge> {
        self.bifrost.forge()
    }

    /// Borrow the retained Redux Forge coordinator for test inspection.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn forge_coordinator(&self) -> Option<&Arc<ForgeCoordinator>> {
        self.bifrost.forge().and_then(Forge::coordinator)
    }

    /// Borrow the shared Bifrost catalog for test-tier fixtures.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn bifrost_catalog(&self) -> Option<&Arc<BifrostCatalog>> {
        self.bifrost.catalog()
    }

    /// Borrow the shared Bifrost role resources for test-tier fixtures.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn bifrost_resources(&self) -> Option<&BifrostRoleResources> {
        self.bifrost.resources()
    }

    /// Borrow this node's one Bifrost storage owner for test-tier fixtures.
    ///
    /// Exposed so a journey can assert node identity directly: co-located roles
    /// must observe one pointer, and two simulated nodes must not, even when
    /// they share the same test backend.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn bifrost_storage(&self) -> Option<&Arc<vala_bifrost_redux::storage::BifrostStorage>> {
        self.bifrost.bifrost_storage()
    }

    /// Borrow the Oracle private peer runtime for test-tier transport fixtures.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn oracle_peer(&self) -> Option<Arc<crate::oracle::OraclePeerRuntime>> {
        self.bifrost.oracle_peer_service()
    }

    /// Borrow the Oracle owner's shared membership registry for test transport setup.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn oracle_cluster(&self) -> Option<Arc<ClusterRegistry>> {
        self.bifrost
            .oracle()
            .map(|owner| Arc::clone(&owner.cluster))
    }

    /// Borrow whichever fenced role owns this node's membership registry.
    ///
    /// A peer-bearing node registers under Oracle, Scribe, or both, and every
    /// co-located role shares one registry, so either owner answers the same
    /// membership. Returns `None` only when this process selected neither
    /// fenced role.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn bifrost_cluster_for_test(&self) -> Option<Arc<ClusterRegistry>> {
        self.oracle_cluster()
            .or_else(|| self.bifrost.scribe().map(|owner| owner.cluster()))
    }

    /// Borrow the retained Bifrost Scribe for test-tier harness inspection.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn bifrost_scribe_for_test(&self) -> Option<&Arc<ScribeImpl>> {
        self.bifrost.scribe().map(|runtime| runtime.scribe())
    }

    /// Borrow the private Scribe tail reader for observation-only journey checks.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn bifrost_tail_reader_for_test(&self) -> Option<Arc<ScribeTailReader>> {
        self.bifrost.scribe().map(|runtime| runtime.tail_reader())
    }

    /// Flush the private Scribe runtime for the test harness only.
    #[cfg(feature = "test-support")]
    pub async fn flush_scribe_for_test(
        &self,
    ) -> Result<(), vala_bifrost_redux::contracts::ScribeError> {
        let Some(runtime) = self.bifrost.scribe() else {
            return Err(vala_bifrost_redux::contracts::ScribeError::Internal {
                detail: "Scribe is not configured".to_owned(),
            });
        };
        runtime.scribe().flush_staged().await.map(|_| ())
    }

    /// Trip the Scribe WAL breaker for a deterministic test-tier probe.
    #[cfg(feature = "test-support")]
    pub fn trip_scribe_wal_disk_full_for_test(&self) -> Result<(), String> {
        let Some(runtime) = self.bifrost.scribe() else {
            return Err("Scribe is not configured".to_owned());
        };
        runtime.scribe().trip_wal_disk_full_for_test();
        Ok(())
    }

    /// Release the held Scribe WAL refusal so retirement clears the breaker.
    ///
    /// # Errors
    /// Returns an error when this server hosts no Scribe.
    #[cfg(feature = "test-support")]
    pub fn clear_scribe_wal_disk_full_injection_for_test(&self) -> Result<(), String> {
        let Some(runtime) = self.bifrost.scribe() else {
            return Err("Scribe is not configured".to_owned());
        };
        runtime.scribe().clear_wal_disk_full_injection_for_test();
        Ok(())
    }

    /// Return the bounded Scribe ownership snapshot for test-tier inspection.
    #[cfg(feature = "test-support")]
    pub fn scribe_inspection_snapshot_for_test(
        &self,
    ) -> Result<vala_bifrost_redux::scribe::telemetry::ScribeInspectionSnapshot, String> {
        let Some(runtime) = self.bifrost.scribe() else {
            return Err("Scribe is not configured".to_owned());
        };
        runtime
            .scribe()
            .inspection_snapshot()
            .map_err(|error| error.to_string())
    }

    /// Return the exact active fence count from this server's Scribe owner.
    ///
    /// # Errors
    /// Returns an error when Scribe is absent or its fence registry cannot be read.
    #[cfg(feature = "test-support")]
    pub fn active_scribe_tail_fences_for_test(&self) -> Result<u64, String> {
        let Some(runtime) = self.bifrost.scribe() else {
            return Err("Scribe is not configured".to_owned());
        };
        runtime
            .tail_reader()
            .active_fence_count_for_test()
            .map_err(|error| error.to_string())
    }
}

/// Errors raised by [`AppState::production_validate`].
#[derive(Debug, thiserror::Error)]
pub enum ProductionValidationError {
    /// Stub allow policy hook is mounted in a production build.
    #[error(
        "authz.policy_hook is StubAllowPolicyHook in a production build; install a real PolicyHook"
    )]
    StubPolicyHook,
    /// Noop audit writer is mounted in a production build.
    #[error(
        "authz.audit_writer is NoopAuthzAuditWriter in a production build; install a real AuthzAuditWriter"
    )]
    NoopAuditWriter,
    /// Token verifier is absent in a production build.
    #[error("auth.token_verifier is None in a production build; auth-plan boot must install it")]
    MissingTokenVerifier,
    /// Preview auth is still enabled in a production build.
    #[error("auth.allow_preview is true in a production build; clear WYRD_AUTH_ALLOW_PREVIEW")]
    PreviewAuthEnabled,
    /// The private Oracle peer was not constructed from verified production dependencies.
    #[error("oracle_peer is None in a production build; Oracle role boot must complete")]
    MissingOraclePeer,
}

impl AppState {
    /// Reject production-profile boots where stubs survived assembly.
    ///
    /// Development-profile boots short-circuit to `Ok(())` so unit tests work.
    pub fn production_validate(&self) -> Result<(), ProductionValidationError> {
        if !self.deployment_profile.is_production() {
            return Ok(());
        }
        // Dedicated Forge workers do not expose the public serving surface and
        // therefore do not construct Gate/authentication or Oracle peers. The
        // closed role topology still records Forge ownership in
        // `bifrost_roles`; only the server roles carry Scribe/Oracle markers.
        let serves_api = self.bifrost.serves_api();
        if !serves_api {
            return Ok(());
        }
        if self.authz.policy_hook.is_stub_default() {
            return Err(ProductionValidationError::StubPolicyHook);
        }
        if self.authz.audit_writer.is_stub_default() {
            return Err(ProductionValidationError::NoopAuditWriter);
        }
        if self.auth.token_verifier.is_none() {
            return Err(ProductionValidationError::MissingTokenVerifier);
        }
        if self.auth.allow_preview {
            return Err(ProductionValidationError::PreviewAuthEnabled);
        }
        Ok(())
    }
}

/// PostgreSQL-backed application-state ownership proofs.
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{AppState, LimitsConfig};

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use wyrd_auth_check::AuthzCheckContext;
    use wyrd_auth_verify::VerifiedToken;
    use wyrd_runtime::{
        DelegationStep, PermissionSet, Principal, PrincipalId, PrincipalKind, PrincipalRef,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::card::policy::PolicyDecision;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::request_id::RequestId;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::postgres::ServerPostgres;

    /// The compaction runtime is a single non-`Clone` owner with a nonblocking drop.
    ///
    /// Both halves are load-bearing. Non-`Clone` is what keeps the executor out
    /// of the per-request-cloned state graph, where the last surviving clone
    /// would decide when it dies. A nonblocking drop is what makes that death
    /// legal from the async frame `async fn main` forces on final teardown,
    /// where a joining `Runtime::drop` would panic instead.
    ///
    /// # Panics
    ///
    /// Panics when dropping the owner blocks, or when the inert Forge-absent
    /// owner is not equally safe to drop.
    #[test]
    fn forge_compaction_runtime_shutdowns_background_on_drop() {
        /// Compile-time proof the owner cannot be cloned into the state graph.
        const fn assert_not_clone<T>() {}
        assert_not_clone::<super::ForgeCompactionRuntime>();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("dedicated compaction runtime builds");
        let owner = super::ForgeCompactionRuntime::new(Some(runtime));
        assert!(
            owner.handle().is_some(),
            "a composed owner must hand runners a handle"
        );
        // A worker thread parked in a long blocking call is exactly the state a
        // joining drop would wait on, so it is the state this drop must not.
        owner
            .handle()
            .expect("composed handle")
            .spawn_blocking(|| std::thread::sleep(std::time::Duration::from_secs(30)));
        let started = std::time::Instant::now();
        drop(owner);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "dropping the compaction runtime must detach its threads, not join them"
        );

        let absent = super::ForgeCompactionRuntime::new(None);
        assert!(
            absent.handle().is_none(),
            "a process without the Forge worker role has no executor to hand out"
        );
        drop(absent);
    }

    /// Durable registry failure cannot skip local reader-epoch settlement.
    #[tokio::test]
    async fn reader_epoch_loss_registry_failure_still_settles_local_work() {
        let settled = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&settled);
        super::await_reader_epoch_loss_settlement(
            Err(vala_bifrost_redux::cluster::ClusterError::StaleFence),
            async move {
                observed.store(true, Ordering::Release);
            },
        )
        .await;

        assert!(settled.load(Ordering::Acquire));
    }

    /// Builds one verifier with a real decoding key for shell composition.
    ///
    /// [`super::Bifrost::test_shell`] hands the verifier straight to the Gate
    /// auth interceptor, and `TokenVerifier::new` requires at least one key, so
    /// the shell cannot be composed from an empty key map.
    fn shell_token_verifier() -> Arc<super::TokenVerifier> {
        let mut keys = std::collections::HashMap::new();
        keys.insert(
            wyrd_auth_verify::Kid::new("test").expect("static kid is valid"),
            Arc::new(
                wyrd_auth_verify::public_key_from_pem(
                    b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n",
                )
                .expect("static test key is valid"),
            ),
        );
        Arc::new(wyrd_auth_verify::TokenVerifier::new(
            keys,
            "wyrd",
            wyrd_auth_verify::WyrdAuthVerifySettings::default(),
        ))
    }

    /// The process composition retains exactly one Gate and its shared owners.
    ///
    /// # Panics
    ///
    /// Panics when a migrated owner is unreachable from [`super::Bifrost`], or
    /// when lifecycle shutdown leaves query admission open — which would let a
    /// draining replica keep admitting reads.
    #[tokio::test]
    async fn bifrost_holds_exactly_one_gate_owner() {
        let bifrost = super::Bifrost::test_shell(shell_token_verifier());

        assert!(bifrost.gate().ensure_query_open().is_ok());
        let _transport = bifrost.transport_admission();
        let _verifier = bifrost.token_verifier();
        assert!(
            bifrost.query_forwarder().is_none(),
            "an ownerless shell composes no forwarder"
        );

        bifrost.begin_shutdown();
        assert!(matches!(
            bifrost.gate().ensure_query_open(),
            Err(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable)
        ));
    }

    #[tokio::test]
    async fn defaults_for_test_safe() {
        let state = test_state().await;

        assert_eq!(
            state.authz.policy_hook.evaluate(&context()).await,
            PolicyDecision::Allow
        );
        assert!(state.authz.audit_writer.is_stub_default());
    }

    #[tokio::test]
    async fn new_state_has_fresh_cancellation_token() {
        let state = test_state().await;
        assert!(!state.shutdown_token.is_cancelled());
        state.shutdown_token.cancel();
        assert!(state.shutdown_token.is_cancelled());
    }

    /// A selected owner's lifecycle error remains the composite shutdown result.
    #[test]
    fn bifrost_shutdown_propagates_selected_owner_failures() {
        let failure = wyrd_spec::vala::error::BifrostError::ScribeRoleUnavailable;
        let result = super::selected_owner_completion(Err(failure.clone()));

        assert_eq!(result, Err(failure));
    }

    #[tokio::test]
    async fn production_validate_passes_development_profile() {
        let state = test_state().await;
        assert!(state.production_validate().is_ok());
    }

    /// The production stub rule applies exactly to processes that serve the API.
    ///
    /// The shell fixture carries the stub policy hook the rule refuses, so the
    /// only reason it validates on a production profile is the dedicated-worker
    /// carve-out: a process with no Scribe and no Oracle exposes no public
    /// surface for a stub hook to decide anything on. Asserting both halves is
    /// what keeps the carve-out from silently becoming a hole — if `serves_api`
    /// ever reports true for this shell, the stub hook must start failing it.
    ///
    /// The serving half of the rule needs a state with a real Scribe or Oracle
    /// owner and is proven where such a state exists, not in this unit tier.
    #[tokio::test]
    async fn production_validate_applies_the_stub_rule_only_to_serving_processes() {
        let state = test_state()
            .await
            .with_deployment_profile(crate::config::DeploymentProfile::Production);
        assert!(
            state.authz.policy_hook.is_stub_default(),
            "the shell fixture carries the stub hook the production rule refuses"
        );
        assert!(
            !state.bifrost.serves_api(),
            "the shell composes no Scribe and no Oracle, so it serves no public surface"
        );
        assert!(
            state.production_validate().is_ok(),
            "a process that serves no public surface is not refused for a stub hook"
        );
    }

    #[tokio::test]
    async fn with_limits_updates_all_fields() {
        let state = test_state().await;
        let limits = LimitsConfig {
            body_bytes: 2048,
            timeout: std::time::Duration::from_millis(1000),
            concurrency: 10,
        };
        let state = state.with_limits(limits);
        assert_eq!(state.limits.body_bytes, 2048);
        assert_eq!(state.limits.concurrency, 10);
    }

    async fn test_state() -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None);
        let vala = vala_sql::ValaPostgres::from_pool(app_pool);
        let postgres = Arc::new(ServerPostgres::from_parts(wyrd, vala));
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        crate::test_support::test_app_state(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
    }

    fn card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    fn principal(name: &str) -> Principal {
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: Some(card_ref(name)),
                card_ref_scope: wyrd_spec::reference::CardRefScope::own(&card_ref(name)),
            },
            tenant_id: DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
            credential_id: None,
        }
    }

    fn context() -> AuthzCheckContext {
        let caller = principal("caller");
        let callee = principal("callee");
        let verified = VerifiedToken {
            principal: callee,
            delegation_chain: vec![DelegationStep {
                principal: PrincipalRef::from_principal(&caller),
            }],
            exp: chrono::Utc::now(),
        };
        let request = wyrd_auth_check::AuthzCheckRequest {
            target: card_ref("callee"),
            action: "card_write".to_owned(),
            context: serde_json::json!({}),
        };
        let request_id = RequestId::parse(&uuid::Uuid::now_v7().to_string())
            .expect("generated UUIDv7 is a valid request id");

        AuthzCheckContext::from_verified(&verified, request, None, request_id)
            .expect("test context is delegated")
    }
}
