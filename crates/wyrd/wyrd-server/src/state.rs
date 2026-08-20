//! Shared axum application state.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
#[cfg(feature = "test-support")]
use std::time::Duration;
use std::time::Instant;

use arc_swap::ArcSwap;
use secrecy::SecretString;
use tokio::sync::Mutex;
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::timeout_at;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::cluster::{ClusterRegistry, RegisteredRole};
use vala_bifrost_redux::contracts::{
    DecodedOtlp, IngressPayload, Scribe as ScribeContract, ScribeIngressFrame, ScribeOtlpOutcome,
};
use vala_bifrost_redux::forge::Forge as ForgeCoordinator;
use vala_bifrost_redux::forge::ForgeWorker;
use vala_bifrost_redux::gate::limits::IngestLimits;
use vala_bifrost_redux::oracle::Oracle as OracleEngine;
use vala_bifrost_redux::oracle::dispatcher::FragmentDispatcher;
use vala_bifrost_redux::oracle::dispatcher::{OraclePeerCredentials, OraclePeerTls};
use vala_bifrost_redux::oracle::follower::{PhysicalPlanFollower, ScribeTailResolver};
use vala_bifrost_redux::oracle::peer::{PeerSecurityAudit, PeerTicketVerifier};
use vala_bifrost_redux::oracle::{
    AuthorizedQueryContext, OracleQueryStream, QueryOptions, RunningQueryRegistry,
};
use vala_bifrost_redux::resources::{BifrostRoleResources, OracleResources, ScribeResources};
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::scribe::tail_rpc::{
    FetchLiveTailService, ScribeTailReader, TailFenceConfig,
};
use wyrd_auth_verify::TokenVerifier;
use wyrd_runtime::PermissionCheck;
use wyrd_storage::StorageHandle;
use wyrd_telemetry::TelemetryGuard;
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::tonic_health::server::HealthReporter;
use wyrd_tonic::wyrd::v1::InsertBatchRequest;

use crate::auth::permission_resolver::SqlPermissionResolver;
use crate::auth::pg_resolvers::PgIssuerResolver;
use crate::components::auth::{ServerAuth, ServerAuthz};
use crate::components::eval::{EvalAuditWriter, EvalRuns, TracingEvalAuditWriter, new_run_map};
use crate::components::health::ReadinessSnapshot;
use crate::config::{BifrostRuntimeConfig, BifrostTarget, DeploymentProfile, ForgeRuntimeConfig};
use crate::postgres::ServerPostgres;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_spec::vala::api::BifrostQueryRequest;

/// Production [`TokenVerifier`] specialization: SQL-backed permission resolution
/// (`SqlPermissionResolver`) plus Postgres-backed issuer resolution
/// (`PgIssuerResolver`). Aliased so the nested handle type stays readable across
/// `AppState`, the boot path, and the test harness.
pub type WyrdTokenVerifier = TokenVerifier<SqlPermissionResolver, PgIssuerResolver>;

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
    /// Shared tenant-qualified Bifrost catalog.
    pub catalog: Arc<BifrostCatalog>,
    /// One root-derived resource graph for every selected local role.
    pub resources: BifrostRoleResources,
    /// One shared current-ready cluster registry.
    pub cluster: Arc<ClusterRegistry>,
    /// Exact public and private request token verifier.
    pub token_verifier: Arc<WyrdTokenVerifier>,
    /// Existing outbound peer bearer owner.
    pub peer_credentials: Arc<dyn OraclePeerCredentials>,
    /// Immutable peer TLS trust policy validated before composition.
    pub peer_tls: OraclePeerTls,
    /// Existing boot-loaded signing authority used to mint peer and tail tickets.
    pub signing_key: SecretString,
    /// Immutable role configuration snapshot.
    pub config: BifrostRuntimeConfig,
    /// Immutable Forge configuration snapshot.
    pub forge_config: ForgeRuntimeConfig,
    /// Stable physical node identity.
    pub node_id: wyrd_spec::vala::api::NodeId,
    /// Private endpoint advertised by selected fenced roles.
    pub advertise_addr: String,
    /// Durable Scribe WAL root from which role scratch paths are derived.
    pub wal_dir: PathBuf,
    /// Process shutdown signal injected into every selected owner.
    pub shutdown: CancellationToken,
    /// Focused production-control overrides consumed only by the shared test composer.
    #[cfg(feature = "test-support")]
    pub test_controls: Option<BifrostTestControls>,
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
    /// Optional projection of the production Scribe rotation thresholds.
    pub scribe_rotation: Option<vala_bifrost_redux::scribe::ScribeRotationTestConfig>,
    /// Existing Scribe persistence fault controls.
    pub scribe_persistence_faults: vala_bifrost_redux::scribe::persistence::PersistenceFaults,
    /// Optional override of the existing Scribe admission configuration.
    pub scribe_admission: Option<vala_bifrost_redux::scribe::admission::AdmissionConfig>,
    /// Optional accelerated role cadence already installed on the shared registry.
    pub role_timing: Option<vala_bifrost_redux::cluster::RoleTiming>,
}
/// Production Gate specialization used by AppState.
/// Focused public-ingest admission state retained by the server Gate.
#[derive(Clone)]
pub struct IngestAdmission {
    /// Immutable transport and typed-ingress limits.
    limits: IngestLimits,
    /// Shared bounded encoded-transport capacity from the process resource graph.
    transport: vala_bifrost_redux::gate::limits::BifrostTransportAdmission,
    /// Shared admission closure observed by every public protocol.
    closed: Arc<AtomicBool>,
}

/// Focused public-query admission policy retained by the server Gate.
#[derive(Clone)]
pub struct QueryAdmission {
    /// Shared admission closure observed by every public protocol.
    closed: Arc<AtomicBool>,
}

/// One server-owned public authentication and admission boundary.
#[derive(Clone)]
pub struct Gate {
    /// Exact production token verifier shared with every Bifrost public transport.
    token_verifier: Arc<WyrdTokenVerifier>,
    /// Existing bounded ingest admission policy.
    ingest_admission: IngestAdmission,
    /// Existing bounded query admission policy.
    query_admission: QueryAdmission,
}

impl Gate {
    /// Creates the one server Gate from boot-validated dependencies.
    #[must_use]
    pub(crate) fn new(
        token_verifier: Arc<WyrdTokenVerifier>,
        limits: IngestLimits,
        transport: vala_bifrost_redux::gate::limits::BifrostTransportAdmission,
    ) -> Self {
        let closed = Arc::new(AtomicBool::new(false));
        Self {
            token_verifier,
            ingest_admission: IngestAdmission {
                limits,
                transport,
                closed: Arc::clone(&closed),
            },
            query_admission: QueryAdmission { closed },
        }
    }

    /// Authenticates one private peer request through the same token verifier as public Gate work.
    ///
    /// # Errors
    /// Returns the stable authentication refusal without consulting a role provider.
    pub async fn authenticate_peer(
        &self,
        metadata: &wyrd_tonic::tonic::metadata::MetadataMap,
    ) -> Result<vala_bifrost_redux::gate::AuthContext, vala_bifrost_redux::gate::IngestError> {
        vala_bifrost_redux::gate::auth::authenticate(self.token_verifier.as_ref(), metadata).await
    }

    /// Authenticates one public ingest request after checking Gate admission.
    ///
    /// # Errors
    /// Returns the stable authentication or closed-admission refusal.
    pub async fn authenticate_ingest(
        &self,
        metadata: &wyrd_tonic::tonic::metadata::MetadataMap,
    ) -> Result<vala_bifrost_redux::gate::AuthContext, vala_bifrost_redux::gate::IngestError> {
        self.ensure_ingest_open()?;
        self.authenticate_peer(metadata).await
    }

    /// Returns the immutable OTLP wire limits.
    #[must_use]
    pub const fn otlp_wire_limits(&self) -> vala_bifrost_redux::gate::limits::OtlpWireLimits {
        self.ingest_admission.limits.otlp
    }

    /// Returns the maximum gRPC decoding message size.
    #[must_use]
    pub const fn otlp_decoding_message_size(&self) -> usize {
        self.ingest_admission.limits.max_decoding_message_size
    }

    /// Borrows the immutable typed-ingest limits.
    #[must_use]
    pub const fn ingest_limits(&self) -> &IngestLimits {
        &self.ingest_admission.limits
    }

    /// Rejects ingest after the one Gate begins shutdown.
    ///
    /// # Errors
    /// Returns the stable closed-ingress error after shutdown begins.
    pub fn ensure_ingest_open(&self) -> Result<(), vala_bifrost_redux::gate::IngestError> {
        if self.ingest_admission.closed.load(Ordering::Acquire) {
            Err(vala_bifrost_redux::gate::IngestError::IngressClosed)
        } else {
            Ok(())
        }
    }

    /// Rejects query work after the one Gate begins shutdown.
    ///
    /// # Errors
    /// Returns the stable unavailable error after shutdown begins.
    pub fn ensure_query_open(&self) -> Result<(), wyrd_spec::vala::error::BifrostError> {
        if self.query_admission.closed.load(Ordering::Acquire) {
            Err(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable)
        } else {
            Ok(())
        }
    }

    /// Closes all admission represented by this Gate.
    pub fn close(&self) {
        self.ingest_admission.closed.store(true, Ordering::Release);
        self.query_admission.closed.store(true, Ordering::Release);
    }
}

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
    /// Optional dedicated runtime that owns Scribe coordination tasks in production.
    coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
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
    /// Distributed physical-fragment dispatcher shared with the engine.
    dispatcher: Arc<FragmentDispatcher>,
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
    /// Readiness value preserved by heartbeats during transport drain.
    advertise_ready: Arc<AtomicBool>,
    /// Optional server-owned executor retained for Oracle coordination tasks.
    coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Local WAL publisher retained for the complete Oracle lifecycle.
    audit: Arc<crate::oracle::OracleAuditPublisher>,
    /// Canonical authenticated transport for owner-local lifecycle fanout.
    lifecycle_transport: Arc<crate::oracle::OracleLifecycleTransport>,
}

impl Oracle {
    /// Cancels Oracle work and explicitly aborts retained role tasks without awaiting.
    ///
    /// Used only after the process deadline is exhausted. It closes role
    /// activity synchronously, aborts the heartbeat and snapshot poller through
    /// handles that do not require their async owner locks, starts no registry
    /// operation, and leaves incomplete durable cleanup to existing recovery.
    pub(crate) fn abort_shutdown(&self) {
        self.lifecycle.begin_stopping();
        self.role_shutdown.cancel();
        self.heartbeat_abort.abort();
        self.snapshot_poller_abort.abort();
        self.engine.begin_shutdown();
    }
    /// Creates the retained query lifecycle after role registration succeeds.
    #[must_use]
    pub fn new(
        engine: Arc<OracleEngine>,
        catalog: Arc<BifrostCatalog>,
        registered_role: RegisteredRole,
        cluster: Arc<ClusterRegistry>,
        coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
        audit: Arc<crate::oracle::OracleAuditPublisher>,
        lifecycle_transport: Arc<crate::oracle::OracleLifecycleTransport>,
        resources: OracleResources,
        peer: Arc<crate::oracle::OraclePeerRuntime>,
    ) -> Self {
        let role_shutdown = CancellationToken::new();
        let advertise_ready = Arc::new(AtomicBool::new(true));
        let heartbeat = Arc::clone(&cluster).start_readiness_heartbeat(
            registered_role.clone(),
            Arc::clone(&advertise_ready),
            role_shutdown.clone(),
        );
        let snapshot_poller = Arc::clone(&cluster).start_snapshot_poller(role_shutdown.clone());
        let heartbeat_abort = heartbeat.abort_handle();
        let snapshot_poller_abort = snapshot_poller.abort_handle();
        let running_queries = Arc::clone(engine.running_queries());
        let dispatcher = engine
            .fragment_dispatcher()
            .expect("production Oracle composition requires its distributed dispatcher");
        let query_controls = crate::oracle::RunningQueryControls::new(
            Arc::clone(&running_queries),
            Arc::clone(&lifecycle_transport),
            Arc::clone(&cluster),
        );
        Self {
            engine,
            catalog,
            registered_role,
            cluster,
            role_shutdown,
            heartbeat: Arc::new(Mutex::new(Some(heartbeat))),
            heartbeat_abort,
            snapshot_poller: Arc::new(Mutex::new(Some(snapshot_poller))),
            snapshot_poller_abort,
            lifecycle: RoleLifecycle::serving(),
            advertise_ready,
            coordination_runtime,
            audit,
            running_queries,
            lifecycle_transport,
            query_controls,
            peer,
            dispatcher,
            resources,
        }
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

    /// Captures production-owned Oracle resource reservations and WAL backlog.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn oracle_runtime_inspection(
        &self,
    ) -> (
        vala_bifrost_redux::oracle::OracleRuntimeInspection,
        (u64, u64, Option<Duration>),
    ) {
        (self.engine.runtime_inspection(), self.audit.wal_snapshot())
    }

    /// Pauses relay SQL at the production fault-injection seam.
    #[cfg(feature = "test-support")]
    pub fn pause_audit_relay_for_test(&self) -> crate::oracle::AuditRelayPauseGuard {
        self.audit.pause_relay_before_postgres()
    }

    /// Injects the documented commit-before-checkpoint replay window.
    #[cfg(feature = "test-support")]
    pub fn fail_audit_after_commit_for_test(&self) {
        self.audit
            .fail_after_next_postgres_commit_before_checkpoint();
    }

    /// Aborts production audit tasks so an abrupt test restart releases WAL locks.
    #[cfg(feature = "test-support")]
    pub async fn abort_audit_tasks_for_test(&self) {
        self.audit.abort_for_test().await;
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
        self.shutdown_owner(deadline).await;
        self.shutdown_registry(deadline).await;
    }

    /// Stops Oracle-owned work and retained role tasks within `deadline`.
    ///
    /// Cancellation first closes new work. Maintenance and role tasks then
    /// drain against the unchanged process deadline; partial progress is
    /// retained for recovery when the future reaches or is dropped at expiry.
    pub(crate) async fn shutdown_owner(&self, deadline: Instant) {
        self.lifecycle.begin_stopping();
        self.role_shutdown.cancel();
        self.engine.shutdown(deadline).await;
        let _ = self.audit.shutdown(deadline).await;
        await_role_task(&self.heartbeat, deadline, "oracle heartbeat").await;
        await_role_task(&self.snapshot_poller, deadline, "oracle snapshot poller").await;
    }

    /// Unregisters the exact Oracle fence within `deadline`.
    ///
    /// No registry await starts after expiry. Failure or timeout leaves the
    /// lifecycle unfinished and observable in logs rather than claiming cleanup.
    pub(crate) async fn shutdown_registry(&self, deadline: Instant) {
        if tokio::time::Instant::now() < tokio::time::Instant::from_std(deadline) {
            match timeout_at(
                tokio::time::Instant::from_std(deadline),
                self.cluster.shutdown_role(self.registered_role.clone()),
            )
            .await
            {
                Ok(Ok(())) => self.lifecycle.finish(),
                Ok(Err(error)) => {
                    tracing::warn!(%error, "failed to unregister the Oracle role during shutdown")
                }
                Err(_) => tracing::warn!("Oracle role unregister exceeded shutdown deadline"),
            }
        } else {
            tracing::warn!("skipping Oracle role unregister after shutdown deadline");
        }
    }

    /// Returns whether the runtime retains a dedicated coordination executor.
    #[must_use]
    pub fn has_dedicated_coordination_runtime(&self) -> bool {
        self.coordination_runtime.is_some()
    }

    /// Returns whether both retained Oracle role tasks have reached termination.
    #[cfg(all(test, feature = "test-support"))]
    #[must_use]
    pub(crate) fn role_tasks_finished_for_test(&self) -> (bool, bool) {
        (
            self.heartbeat_abort.is_finished(),
            self.snapshot_poller_abort.is_finished(),
        )
    }
}

impl Scribe {
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
    pub fn new(
        ingest: Arc<ScribeImpl>,
        catalog: Arc<BifrostCatalog>,
        resources: ScribeResources,
        cluster: Arc<ClusterRegistry>,
        registered_role: RegisteredRole,
        fragment_verifier: Arc<dyn PeerTicketVerifier>,
        fragment_security_audit: Arc<dyn PeerSecurityAudit>,
        coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
    ) -> Self {
        let tail_service = Arc::new(
            ingest
                .tail_service()
                .expect("constructed Scribe must retain a valid UUID stream identity"),
        );
        let fragment_follower = Arc::new(PhysicalPlanFollower::new(ScribeTailResolver::new(
            Arc::clone(&tail_service),
            Arc::clone(&catalog),
        )));
        let tail_reader = Arc::new(ScribeTailReader::new(
            Arc::clone(&tail_service),
            TailFenceConfig::default(),
        ));
        let role_shutdown = CancellationToken::new();
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
            resources,
            cluster,
            registered_role,
            lifecycle: RoleLifecycle::serving(),
            catalog,
            tail_reader,
            tail_authority: None,
            fragment_verifier,
            fragment_security_audit,
            coordination_runtime,
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
        self.shutdown_owner(deadline).await;
        self.shutdown_registry(deadline).await;
    }

    /// Closes and drains Scribe-owned work and retained role tasks within `deadline`.
    ///
    /// The composite closes Gate first; Scribe may flush work accepted before
    /// that transition while budget remains. Every later join shares `deadline`, and
    /// timed-out retained handles are aborted before this method returns.
    pub(crate) async fn shutdown_owner(&self, deadline: Instant) {
        self.lifecycle.begin_stopping();
        self.role_shutdown.cancel();
        if let Some(authority) = &self.tail_authority {
            authority.clear_replay_state();
        }
        self.ingest.shutdown(deadline).await;
        await_role_task(&self.heartbeat, deadline, "scribe heartbeat").await;
        await_role_task(&self.snapshot_poller, deadline, "scribe snapshot poller").await;
    }

    /// Unregisters the exact Scribe fence within `deadline`.
    ///
    /// The exact retained fence is removed only while budget remains. Timeout,
    /// cancellation, or registry failure leaves lifecycle completion unset and
    /// starts no post-deadline retry.
    pub(crate) async fn shutdown_registry(&self, deadline: Instant) {
        if tokio::time::Instant::now() < tokio::time::Instant::from_std(deadline) {
            match timeout_at(
                tokio::time::Instant::from_std(deadline),
                self.cluster.shutdown_role(self.registered_role.clone()),
            )
            .await
            {
                Ok(Ok(())) => self.lifecycle.finish(),
                Ok(Err(error)) => {
                    tracing::warn!(%error, "failed to unregister the Scribe role during shutdown")
                }
                Err(_) => tracing::warn!("Scribe role unregister exceeded shutdown deadline"),
            }
        } else {
            tracing::warn!("skipping Scribe role unregister after shutdown deadline");
        }
    }

    /// Returns whether this runtime retains a dedicated coordination executor.
    #[must_use]
    pub fn has_dedicated_coordination_runtime(&self) -> bool {
        self.coordination_runtime.is_some()
    }

    /// Returns whether both retained Scribe role tasks have reached termination.
    #[cfg(all(test, feature = "test-support"))]
    #[must_use]
    pub(crate) fn role_tasks_finished_for_test(&self) -> (bool, bool) {
        (
            self.heartbeat_abort.is_finished(),
            self.snapshot_poller_abort.is_finished(),
        )
    }
}

/// Wait for one role-owned task until the process shutdown deadline.
async fn await_role_task(
    task: &Mutex<Option<JoinHandle<()>>>,
    deadline: Instant,
    task_name: &'static str,
) {
    let Ok(mut guard) = timeout_at(tokio::time::Instant::from_std(deadline), task.lock()).await
    else {
        tracing::warn!(
            task = task_name,
            "role task lock exceeded shutdown deadline"
        );
        return;
    };
    let Some(mut task) = guard.take() else {
        return;
    };
    drop(guard);
    if timeout_at(tokio::time::Instant::from_std(deadline), &mut task)
        .await
        .is_err()
    {
        tracing::warn!(task = task_name, "role task exceeded shutdown deadline");
        task.abort();
    }
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

    /// Claim and clear the one-shot fault atomically.
    #[must_use]
    pub fn claim(&self) -> Option<QueryStreamFault> {
        match self.next.swap(0, Ordering::AcqRel) {
            1 => Some(QueryStreamFault::EofAfterSchema),
            2 => Some(QueryStreamFault::EofAfterBatch),
            3 => Some(QueryStreamFault::StallAfterSchema),
            _ => None,
        }
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
    /// Root-derived Forge resource capability.
    resources: vala_bifrost_redux::resources::ForgeResources,
    /// Process lifecycle signal shared by both selected capabilities.
    shutdown: CancellationToken,
    /// Stable physical node identity retained by the selected Forge owner.
    node_id: wyrd_spec::vala::api::NodeId,
}

impl Forge {
    /// Composes exactly the Forge capabilities selected by the process target.
    #[must_use]
    pub fn new(
        coordinator: Option<Arc<ForgeCoordinator>>,
        worker: Option<Arc<ForgeWorker>>,
        resources: vala_bifrost_redux::resources::ForgeResources,
        shutdown: CancellationToken,
        node_id: wyrd_spec::vala::api::NodeId,
    ) -> Self {
        Self {
            coordinator,
            worker,
            resources,
            shutdown,
            node_id,
        }
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

    /// Returns the root-derived Forge resource capability.
    #[must_use]
    pub fn resources(&self) -> vala_bifrost_redux::resources::ForgeResources {
        self.resources.clone()
    }

    /// Signals selected Forge capabilities to stop accepting work.
    pub fn begin_shutdown(&self) {
        self.shutdown.cancel();
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
    /// One public authentication and admission owner.
    gate: Gate,
    /// Selected Scribe runtime, when this process owns the role.
    scribe: Option<Arc<Scribe>>,
    /// Selected Forge runtime, when this process owns a coordinator or worker.
    forge: Option<Arc<Forge>>,
    /// Selected Oracle runtime, when this process owns the role.
    oracle: Option<Arc<Oracle>>,
}

impl Bifrost {
    /// Assembles the complete immutable process composition before publication.
    pub(crate) fn assembled(
        gate: Gate,
        scribe: Option<Arc<Scribe>>,
        forge: Option<Arc<Forge>>,
        oracle: Option<Arc<Oracle>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            gate,
            scribe,
            forge,
            oracle,
        })
    }

    /// Borrows the selected Scribe runtime.
    #[must_use]
    pub const fn scribe(&self) -> Option<&Arc<Scribe>> {
        self.scribe.as_ref()
    }

    /// Borrows the one public Gate.
    #[must_use]
    pub const fn gate(&self) -> &Gate {
        &self.gate
    }

    /// Borrows the selected Oracle runtime.
    #[must_use]
    pub const fn oracle(&self) -> Option<&Arc<Oracle>> {
        self.oracle.as_ref()
    }

    /// Borrows the tenant-authorized lifecycle facade from the selected Oracle runtime.
    #[must_use]
    pub fn query_controls(&self) -> Option<&crate::oracle::RunningQueryControls> {
        self.oracle.as_ref().map(|runtime| runtime.query_controls())
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
        self.gate.ingest_admission.transport.clone()
    }

    /// Returns the shared root-health signal through one selected role capability.
    #[must_use]
    pub fn resource_health(&self) -> Option<vala_bifrost_redux::resources::BifrostResourceHealth> {
        self.scribe
            .as_ref()
            .map(|scribe| scribe.resources.health())
            .or_else(|| self.oracle.as_ref().map(|oracle| oracle.resources.health()))
            .or_else(|| self.forge.as_ref().map(|forge| forge.resources.health()))
    }

    /// Borrows the shared catalog through one selected serving subsystem.
    pub(crate) fn catalog(&self) -> Option<&Arc<BifrostCatalog>> {
        self.scribe
            .as_ref()
            .map(|scribe| &scribe.catalog)
            .or_else(|| self.oracle.as_ref().map(|oracle| &oracle.catalog))
    }

    /// Reserves Scribe-owned memory for one preflighted OTLP decode.
    ///
    /// # Errors
    /// Returns the stable closed or bounded-resource refusal before decoding.
    pub fn reserve_otlp_decode(
        &self,
        bytes: usize,
    ) -> Result<vala_bifrost_redux::contracts::OtlpDecodeOwner, vala_bifrost_redux::gate::IngestError>
    {
        self.gate().ensure_ingest_open()?;
        self.scribe
            .as_ref()
            .ok_or(vala_bifrost_redux::gate::IngestError::IngressClosed)?
            .ingest
            .reserve_otlp_decode(bytes)
            .map_err(vala_bifrost_redux::gate::IngestError::from_scribe)
    }

    /// Routes one authenticated trace export through Gate into the selected Scribe.
    ///
    /// # Errors
    /// Returns the stable Gate, authorization, or Scribe error.
    pub async fn ingest_decoded_resource_spans(
        &self,
        auth: &vala_bifrost_redux::gate::AuthContext,
        decoded: DecodedOtlp<ExportTraceServiceRequest>,
    ) -> Result<vala_bifrost_redux::gate::IngestOutcome, vala_bifrost_redux::gate::IngestError>
    {
        match self
            .dispatch_otlp(
                auth,
                TableRef::new(BifrostNamespace::Traces, "spans"),
                decoded.wire_bytes,
                IngressPayload::OtlpTraces(decoded),
            )
            .await?
        {
            ScribeOtlpOutcome::Traces(outcome) => Ok(outcome),
            _ => Err(vala_bifrost_redux::gate::IngestError::Internal(
                "Scribe returned the wrong OTLP outcome".to_owned(),
            )),
        }
    }

    /// Routes one authenticated metrics export through Gate into the selected Scribe.
    ///
    /// # Errors
    /// Returns the stable Gate, authorization, or Scribe error.
    pub async fn ingest_decoded_resource_metrics(
        &self,
        auth: &vala_bifrost_redux::gate::AuthContext,
        decoded: DecodedOtlp<ExportMetricsServiceRequest>,
    ) -> Result<vala_bifrost_redux::gate::MetricsOutcome, vala_bifrost_redux::gate::IngestError>
    {
        match self
            .dispatch_otlp(
                auth,
                TableRef::new(BifrostNamespace::Metrics, "points"),
                decoded.wire_bytes,
                IngressPayload::OtlpMetrics(decoded),
            )
            .await?
        {
            ScribeOtlpOutcome::Metrics(outcome) => Ok(outcome),
            _ => Err(vala_bifrost_redux::gate::IngestError::Internal(
                "Scribe returned the wrong OTLP outcome".to_owned(),
            )),
        }
    }

    /// Routes one authenticated log export through Gate into the selected Scribe.
    ///
    /// # Errors
    /// Returns the stable Gate, authorization, or Scribe error.
    pub async fn ingest_decoded_resource_logs(
        &self,
        auth: &vala_bifrost_redux::gate::AuthContext,
        decoded: DecodedOtlp<ExportLogsServiceRequest>,
    ) -> Result<vala_bifrost_redux::gate::LogsOutcome, vala_bifrost_redux::gate::IngestError> {
        match self
            .dispatch_otlp(
                auth,
                TableRef::new(BifrostNamespace::Logs, "records"),
                decoded.wire_bytes,
                IngressPayload::OtlpLogs(decoded),
            )
            .await?
        {
            ScribeOtlpOutcome::Logs(outcome) => Ok(outcome),
            _ => Err(vala_bifrost_redux::gate::IngestError::Internal(
                "Scribe returned the wrong OTLP outcome".to_owned(),
            )),
        }
    }

    /// Routes one authenticated OTLP payload through the exact Scribe frame contract.
    async fn dispatch_otlp(
        &self,
        auth: &vala_bifrost_redux::gate::AuthContext,
        table: TableRef,
        measured_wire_bytes: usize,
        payload: IngressPayload,
    ) -> Result<ScribeOtlpOutcome, vala_bifrost_redux::gate::IngestError> {
        self.gate().ensure_ingest_open()?;
        if measured_wire_bytes > self.gate().ingest_limits().max_frame_bytes {
            return Err(vala_bifrost_redux::gate::IngestError::PayloadTooLarge {
                bytes: u64::try_from(measured_wire_bytes).unwrap_or(u64::MAX),
                limit: u64::try_from(self.gate().ingest_limits().max_frame_bytes)
                    .unwrap_or(u64::MAX),
            });
        }
        wyrd_runtime::RbacCheck
            .check(
                &auth.principal,
                &wyrd_runtime::Permission::bifrost_record_write(),
            )
            .into_result()
            .map_err(vala_bifrost_redux::gate::IngestError::from_rbac)?;
        let audit_event = wyrd_spec::vala::api::AuditEvent {
            request_id: auth.request_id.clone(),
            trace_id: None,
            operation: "bifrost.otlp".to_owned(),
            resource: table.fqn(),
            card_ref: auth.principal.card_ref().cloned(),
            principal_id: auth.principal.id,
            principal_kind: auth.principal.kind.tag(),
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "bifrost:record:write".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "one bounded OTLP frame".to_owned(),
            detail: None,
        };
        self.scribe
            .as_ref()
            .ok_or(vala_bifrost_redux::gate::IngestError::IngressClosed)?
            .ingest
            .ingest_frame(ScribeIngressFrame {
                principal: auth.principal.clone(),
                authenticated_tenant: auth.tenant,
                table,
                expected_schema_fingerprint: None,
                request_id: auth.request_id.clone(),
                batch_id: uuid::Uuid::now_v7(),
                audit_event,
                measured_wire_bytes,
                payload,
            })
            .await
            .map_err(vala_bifrost_redux::gate::IngestError::from_scribe)?
            .otlp_outcome
            .ok_or_else(|| {
                vala_bifrost_redux::gate::IngestError::Internal(
                    "Scribe omitted the OTLP outcome".to_owned(),
                )
            })
    }

    /// Routes one authenticated native Arrow frame through Gate into the selected Scribe.
    ///
    /// Once admitted, the durable append is detached from transport cancellation so a
    /// disconnected caller can retry the same batch identity against Scribe deduplication.
    ///
    /// # Errors
    /// Returns the stable Gate validation, authorization, role, or Scribe refusal.
    pub async fn ingest_native_frame(
        &self,
        auth: &vala_bifrost_redux::gate::AuthContext,
        frame: InsertBatchRequest,
    ) -> Result<u64, vala_bifrost_redux::gate::IngestError> {
        self.gate().ensure_ingest_open()?;
        vala_bifrost_redux::gate::validate_batch(&frame, self.gate().ingest_limits())?;
        wyrd_runtime::RbacCheck
            .check(
                &auth.principal,
                &wyrd_runtime::Permission::bifrost_record_write(),
            )
            .into_result()
            .map_err(vala_bifrost_redux::gate::IngestError::from_rbac)?;
        let (namespace, name) = vala_bifrost_redux::gate::resolve_fqn(&frame.table)?;
        if namespace == BifrostNamespace::Audit {
            return Err(
                vala_bifrost_redux::gate::IngestError::ReservedBuiltinWriteDenied {
                    table: frame.table,
                },
            );
        }
        let batch_id =
            uuid::Uuid::from_bytes(frame.wyrd_batch_id.as_ref().try_into().map_err(|_| {
                vala_bifrost_redux::gate::IngestError::RequestValidation(
                    "invalid batch id".to_owned(),
                )
            })?);
        let table = TableRef::new(namespace, name);
        let audit_event = wyrd_spec::vala::api::AuditEvent {
            request_id: auth.request_id.clone(),
            trace_id: None,
            operation: "bifrost.ingest_batch".to_owned(),
            resource: table.fqn(),
            card_ref: auth.principal.card_ref().cloned(),
            principal_id: auth.principal.id,
            principal_kind: auth.principal.kind.tag(),
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "bifrost:record:write".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "one bounded native batch".to_owned(),
            detail: None,
        };
        let scribe = Arc::clone(
            &self
                .scribe
                .as_ref()
                .ok_or(vala_bifrost_redux::gate::IngestError::IngressClosed)?
                .ingest,
        );
        let principal = auth.principal.clone();
        let authenticated_tenant = auth.tenant;
        let request_id = auth.request_id.clone();
        let admission = tokio::spawn(async move {
            scribe
                .ingest_frame(ScribeIngressFrame {
                    principal,
                    authenticated_tenant,
                    table,
                    expected_schema_fingerprint: None,
                    request_id,
                    batch_id,
                    audit_event,
                    measured_wire_bytes: frame.arrow_ipc.len(),
                    payload: IngressPayload::ArrowIpc(frame.arrow_ipc),
                })
                .await
        })
        .await
        .map_err(|error| {
            vala_bifrost_redux::gate::IngestError::Internal(format!(
                "durable Scribe task failed: {error}"
            ))
        })?
        .map_err(vala_bifrost_redux::gate::IngestError::from_scribe)?;
        Ok(admission.rows_accepted)
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
        self.gate().ensure_query_open()?;
        let oracle = self
            .oracle
            .as_ref()
            .filter(|oracle| oracle.is_ready())
            .ok_or(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable)?;
        oracle.engine.query_sql(context, request).await
    }

    /// Dispatches one authorized logical plan through Gate into the selected Oracle.
    ///
    /// # Errors
    /// Returns role-unavailable, admission, planning, or execution errors.
    pub async fn query_plan(
        &self,
        context: AuthorizedQueryContext,
        plan: datafusion::logical_expr::LogicalPlan,
        options: QueryOptions,
    ) -> Result<OracleQueryStream, wyrd_spec::vala::error::BifrostError> {
        self.gate().ensure_query_open()?;
        let oracle = self
            .oracle
            .as_ref()
            .filter(|oracle| oracle.is_ready())
            .ok_or(wyrd_spec::vala::error::BifrostError::OracleRoleUnavailable)?;
        oracle.engine.query_plan(context, plan, options).await
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

    /// Drains every selected subsystem against one absolute deadline.
    ///
    /// # Errors
    /// Returns a stable lifecycle failure when a selected role cannot remove readiness.
    pub async fn shutdown(
        &self,
        deadline: Instant,
    ) -> Result<BifrostShutdownReport, wyrd_spec::vala::error::BifrostError> {
        self.begin_shutdown();
        if let Some(oracle) = &self.oracle {
            oracle.begin_shutdown().await.map_err(|_| {
                wyrd_spec::vala::error::BifrostError::RunningQueryControlUnavailable
            })?;
            oracle.shutdown_owner(deadline).await;
            oracle.shutdown_registry(deadline).await;
        }
        if let Some(scribe) = &self.scribe {
            scribe
                .begin_shutdown()
                .await
                .map_err(|_| wyrd_spec::vala::error::BifrostError::ScribeRoleUnavailable)?;
            scribe.shutdown_owner(deadline).await;
            scribe.shutdown_registry(deadline).await;
        }
        Ok(BifrostShutdownReport {
            scribe_drained: self.scribe.is_some(),
            forge_drained: self.forge.is_some(),
            oracle_drained: self.oracle.is_some(),
        })
    }

    /// Closes public admission and aborts selected role owners without claiming a flush.
    pub fn abort(&self) {
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
    }
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
    /// Telemetry guard (holds the tracer provider).
    pub telemetry: Arc<TelemetryGuard>,
    /// Request-shaping limits for the router middleware stack.
    pub limits: LimitsConfig,
    /// gRPC health reporter shared between HTTP readiness and gRPC health service.
    pub grpc_health: HealthReporter,
    /// Cached readiness snapshot from the background readiness_loop task.
    pub readiness: Arc<ArcSwap<ReadinessSnapshot>>,
    /// Tenant-keyed in-memory eval run/lease/session map. Ephemeral, single-replica.
    pub eval_runs: EvalRuns,
    /// Audit sink for eval run open/complete events.
    pub eval_audit: Arc<dyn EvalAuditWriter>,
    /// Optional deterministic stream truncation controller for test servers.
    #[cfg(feature = "test-support")]
    pub query_stream_fault: Option<QueryStreamFaultController>,
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
            telemetry: Arc::new(wyrd_telemetry::init_test_only_no_global(
                wyrd_telemetry::TelemetryConfig::default(),
            )),
            limits: LimitsConfig::default(),
            grpc_health: reporter,
            readiness: Arc::new(ArcSwap::from_pointee(ReadinessSnapshot::initial())),
            eval_runs: new_run_map(),
            eval_audit: Arc::new(TracingEvalAuditWriter),
            #[cfg(feature = "test-support")]
            query_stream_fault: None,
        }
    }

    /// Replace the eval audit writer, primarily for tests.
    #[must_use]
    pub fn with_eval_audit(mut self, eval_audit: Arc<dyn EvalAuditWriter>) -> Self {
        self.eval_audit = eval_audit;
        self
    }

    /// Attach a test-tier one-shot query stream fault controller.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_query_stream_fault(mut self, controller: QueryStreamFaultController) -> Self {
        self.query_stream_fault = Some(controller);
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
        mut conn: vala_sql::TenantConn<'_>,
    ) -> Result<(), vala_bifrost_redux::contracts::ScribeError> {
        let Some(runtime) = self.bifrost.scribe() else {
            return Err(vala_bifrost_redux::contracts::ScribeError::Internal {
                detail: "Scribe is not configured".to_owned(),
            });
        };
        let attempts = runtime.scribe().force_seal(&mut conn).await?;
        let commit_result = conn.commit().await;
        runtime
            .scribe()
            .settle_commit_attempts(attempts, &commit_result)
            .await
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
mod pg_tests {
    use std::sync::Arc;

    use wyrd_tonic::wyrd::v1::oracle_lifecycle_service_client::OracleLifecycleServiceClient;

    use super::{AppState, LimitsConfig, ProductionValidationError};

    /// The runtime accessor and mounted lifecycle adapter share one allocation.
    ///
    /// # Panics
    ///
    /// Panics when production-shaped Oracle composition, TCP serving, JWT
    /// authentication, registry observation, cancellation, or shutdown fails.
    #[test]
    fn bifrost_query_runtime_owns_one_running_registry() {
        wyrd_runtime::runtime().block_on(async {
            let (state, _) = crate::oracle::pg_tests::real_api_serving_state(
                crate::config::BifrostTarget::Server,
            )
            .await;
            let runtime = state.bifrost_query().expect("query runtime is published");
            let first = runtime.running_queries();
            let second = runtime.running_queries();
            assert!(Arc::ptr_eq(first, second));
            assert!(Arc::ptr_eq(
                &state.bifrost.oracle_peer_service().expect("published peer"),
                runtime.peer()
            ));
            let tenant = wyrd_spec::DataTenantId::new_v7();
            let lifecycle_bearer = crate::oracle::pg_tests::tenant_bearer(&state, tenant);
            let request_id = wyrd_spec::request_id::RequestId::now_v7();
            assert!(runtime.running_queries().insert(
                crate::oracle::lifecycle_service::pg_tests::running_entry(
                    tenant,
                    request_id.clone(),
                )
            ));
            let (channel, shutdown) = crate::oracle::pg_tests::serve(&state).await;
            let lookup = wyrd_tonic::wyrd::v1::ListOracleLifecyclesRequest {
                tenant_id: tenant.as_uuid().to_string(),
            };
            let listed = OracleLifecycleServiceClient::new(channel.clone())
                .list_lifecycles(crate::oracle::pg_tests::peer_request(
                    lookup,
                    &lifecycle_bearer,
                ))
                .await
                .expect("mounted lifecycle adapter observes runtime registry")
                .into_inner();
            assert_eq!(listed.queries.len(), 1);
            assert_eq!(listed.queries[0].request_id, request_id.to_string());
            let cancel = wyrd_tonic::wyrd::v1::CancelOracleLifecycleRequest {
                tenant_id: tenant.as_uuid().to_string(),
                request_id: request_id.to_string(),
            };
            let cancelled = OracleLifecycleServiceClient::new(channel)
                .cancel_lifecycle(crate::oracle::pg_tests::peer_request(
                    cancel,
                    &lifecycle_bearer,
                ))
                .await
                .expect("mounted lifecycle adapter mutates runtime registry")
                .into_inner();
            assert!(cancelled.cancellation_started);
            assert!(
                runtime
                    .running_queries()
                    .get(tenant, &request_id)
                    .expect("runtime entry remains observable")
                    .cancellation_token()
                    .is_cancelled()
            );
            shutdown.cancel();
            runtime
                .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(2))
                .await;
        });
    }

    /// The published application state retains one composite, catalog, and Gate allocation.
    ///
    /// # Panics
    /// Panics when production-shaped composition duplicates a Bifrost owner.
    #[test]
    fn app_state_owns_one_bifrost_composite_and_gate() {
        wyrd_runtime::runtime().block_on(async {
            let (state, _) = crate::oracle::pg_tests::real_api_serving_state(
                crate::config::BifrostTarget::Server,
            )
            .await;
            let composite = Arc::clone(&state.bifrost);
            assert!(Arc::ptr_eq(composite.catalog(), state.bifrost_catalog()));
            assert!(Arc::ptr_eq(
                composite.gate(),
                state.bifrost_gate().expect("route Gate").as_ref(),
            ));
            assert!(Arc::ptr_eq(
                composite.oracle().expect("composite Oracle"),
                state.bifrost_query().expect("route Oracle"),
            ));
            composite
                .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(2))
                .await;
        });
    }

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

    #[tokio::test]
    async fn with_shutdown_token_replaces_field() {
        let state = test_state().await;
        let token = tokio_util::sync::CancellationToken::new();
        let state = state.with_shutdown_token(token.clone());
        token.cancel();
        assert!(state.shutdown_token.is_cancelled());
    }

    #[tokio::test]
    async fn production_validate_passes_development_profile() {
        let state = test_state().await;
        assert!(state.production_validate().is_ok());
    }

    #[tokio::test]
    async fn production_validate_rejects_stub_on_production() {
        let state = test_state()
            .await
            .with_deployment_profile(crate::config::DeploymentProfile::Production);
        let err = state.production_validate().unwrap_err();
        assert!(matches!(err, ProductionValidationError::StubPolicyHook));
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
        AppState::new(
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
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    fn principal(name: &str) -> Principal {
        Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::Service {
                card_ref: card_ref(name),
                card_ref_scope: wyrd_spec::reference::CardRefScope::own(&card_ref(name)),
            },
            tenant_id: DataTenantId::new_v7(),
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
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
            iat: chrono::Utc::now(),
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
