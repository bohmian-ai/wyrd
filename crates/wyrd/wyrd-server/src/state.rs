//! Shared axum application state.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Instant;

use arc_swap::ArcSwap;
use datafusion::execution::memory_pool::MemoryPool;
use tokio::sync::Mutex;
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::timeout_at;
use tokio_util::sync::CancellationToken;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost_redux::catalog::BifrostCatalog;
use vala_bifrost_redux::cluster::{ClusterRegistry, RegisteredRole};
use vala_bifrost_redux::contracts::Scribe;
use vala_bifrost_redux::forge::Forge;
use vala_bifrost_redux::gate::IngressCpuProjection;
use vala_bifrost_redux::gate::auth::ingest_auth_interceptor;
use vala_bifrost_redux::gate::limits::IngestLimits;
use vala_bifrost_redux::oracle::Oracle;
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::scribe::memory::BifrostMemoryGovernor;
use vala_bifrost_redux::scribe::tail_rpc::ScribeTailReader;
use wyrd_auth_verify::TokenVerifier;
use wyrd_storage::StorageHandle;
use wyrd_telemetry::TelemetryGuard;
use wyrd_tonic::tonic_health::server::HealthReporter;

use crate::auth::permission_resolver::SqlPermissionResolver;
use crate::auth::pg_resolvers::PgIssuerResolver;
use crate::components::auth::{ServerAuth, ServerAuthz};
use crate::components::eval::{EvalAuditWriter, EvalRuns, TracingEvalAuditWriter, new_run_map};
use crate::components::health::ReadinessSnapshot;
use crate::config::{BifrostRuntimeRole, DeploymentProfile};
use crate::postgres::ServerPostgres;

/// Production [`TokenVerifier`] specialization: SQL-backed permission resolution
/// (`SqlPermissionResolver`) plus Postgres-backed issuer resolution
/// (`PgIssuerResolver`). Aliased so the nested handle type stays readable across
/// `AppState`, the boot path, and the test harness.
pub type WyrdTokenVerifier = TokenVerifier<SqlPermissionResolver, PgIssuerResolver>;
/// Production Gate specialization used by AppState.
pub type ServerGate =
    vala_bifrost_redux::gate::Gate<BifrostCatalog, SqlPermissionResolver, PgIssuerResolver>;

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
#[derive(Debug)]
struct RoleLifecycle {
    /// Current [`RoleLifecycleState`] encoded for request-path reads.
    state: AtomicU8,
}

impl RoleLifecycle {
    /// Creates a role lifecycle in its serving state.
    fn serving() -> Self {
        Self {
            state: AtomicU8::new(RoleLifecycleState::Serving as u8),
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
pub struct BifrostIngestRuntime {
    /// Durable Scribe implementation used by every Gate dispatch and lifecycle path.
    scribe: Arc<ScribeImpl>,
    /// One fence registry shared by every local and authenticated tonic tail read.
    tail_reader: Arc<ScribeTailReader>,
    /// Optional domain-separated authority for private tail RPCs.
    tail_authority: Option<Arc<crate::oracle::ScribeTailAuthority>>,
    /// Protocol and policy boundary built around [`Self::scribe`].
    gate: Arc<ServerGate>,
    /// Optional dedicated runtime that owns Scribe coordination tasks in production.
    coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Optional production Scribe role lifecycle; embedded/test writers need no SQL membership.
    scribe_role: Option<ScribeRoleRuntime>,
}

/// Owns the independently fenced Scribe membership lifecycle for one server process.
#[derive(Clone)]
struct ScribeRoleRuntime {
    /// Shared durable role registry used by heartbeat, discovery, and shutdown.
    registry: Arc<ClusterRegistry>,
    /// Exact Scribe fence registered during this boot.
    registered: RegisteredRole,
    /// Cancels the recurring heartbeat and snapshot tasks before role removal.
    shutdown: CancellationToken,
    /// Retains the heartbeat task so teardown can prove it stopped before unregister.
    heartbeat: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Synchronously aborts the heartbeat without acquiring its async owner lock.
    heartbeat_abort: AbortHandle,
    /// Retains the snapshot poller so role-owned background work is drained.
    snapshot_poller: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Synchronously aborts the snapshot poller without acquiring its async owner lock.
    snapshot_poller_abort: AbortHandle,
    /// Enforces readiness removal before transport drain and role teardown.
    lifecycle: Arc<RoleLifecycle>,
    /// Readiness value preserved by heartbeats during transport drain.
    advertise_ready: Arc<AtomicBool>,
}

/// Owns one retained Oracle and its independent fenced server lifecycle.
#[derive(Clone)]
pub struct BifrostQueryRuntime {
    /// Retained leader/worker query engine used by every Gate query dispatch.
    oracle: Arc<Oracle>,
    /// Exact Oracle fence registered after worker dependencies became ready.
    registered_role: RegisteredRole,
    /// Private peer service owner mounted on the shared gRPC listener.
    peer: Arc<crate::oracle::OraclePeerRuntime>,
    /// Shared durable registry used by heartbeat, discovery, and unregister.
    registry: Arc<ClusterRegistry>,
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
    /// Enforces readiness removal before transport drain and role teardown.
    lifecycle: Arc<RoleLifecycle>,
    /// Readiness value preserved by heartbeats during transport drain.
    advertise_ready: Arc<AtomicBool>,
    /// Optional server-owned executor retained for Oracle coordination tasks.
    coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
}

impl BifrostQueryRuntime {
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
        self.oracle.begin_shutdown();
    }
    /// Creates the retained query lifecycle after role registration succeeds.
    #[must_use]
    pub fn new(
        oracle: Arc<Oracle>,
        registered_role: RegisteredRole,
        peer: Arc<crate::oracle::OraclePeerRuntime>,
        registry: Arc<ClusterRegistry>,
        coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
    ) -> Self {
        let role_shutdown = CancellationToken::new();
        let advertise_ready = Arc::new(AtomicBool::new(true));
        let heartbeat = Arc::clone(&registry).start_readiness_heartbeat(
            registered_role.clone(),
            Arc::clone(&advertise_ready),
            role_shutdown.clone(),
        );
        let snapshot_poller = Arc::clone(&registry).start_snapshot_poller(role_shutdown.clone());
        let heartbeat_abort = heartbeat.abort_handle();
        let snapshot_poller_abort = snapshot_poller.abort_handle();
        Self {
            oracle,
            registered_role,
            peer,
            registry,
            role_shutdown,
            heartbeat: Arc::new(Mutex::new(Some(heartbeat))),
            heartbeat_abort,
            snapshot_poller: Arc::new(Mutex::new(Some(snapshot_poller))),
            snapshot_poller_abort,
            lifecycle: Arc::new(RoleLifecycle::serving()),
            advertise_ready,
            coordination_runtime,
        }
    }

    /// Borrows the retained Oracle used by local Gate dispatch.
    #[must_use]
    pub fn oracle(&self) -> &Arc<Oracle> {
        &self.oracle
    }

    /// Returns the private peer service owner mounted for this Oracle fence.
    #[must_use]
    pub fn peer(&self) -> Arc<crate::oracle::OraclePeerRuntime> {
        Arc::clone(&self.peer)
    }

    /// Reports startup reconciliation and lifecycle readiness, excluding saturation.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.lifecycle.is_serving() && self.oracle.is_ready()
    }

    /// Removes durable readiness before transport draining begins.
    ///
    /// # Errors
    ///
    /// Returns the registry error when the exact Oracle fence cannot be marked
    /// unready. Local readiness still closes so the process fails closed.
    pub async fn begin_shutdown(&self) -> Result<(), vala_bifrost_redux::cluster::ClusterError> {
        if !self.lifecycle.begin_draining() {
            return Ok(());
        }
        metrics::gauge!("bifrost_role_ready", "role" => "oracle").set(0.0);
        self.advertise_ready.store(false, Ordering::Release);
        self.registry.deactivate(&self.registered_role).await
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
        self.oracle.shutdown(deadline).await;
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
                self.registry.shutdown_role(self.registered_role.clone()),
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

impl BifrostIngestRuntime {
    /// Closes Gate and explicitly aborts all retained Scribe role work without awaiting.
    ///
    /// This deadline-expiry path signals role cancellation, explicitly aborts
    /// heartbeat and snapshot tasks without acquiring their async owner locks,
    /// and aborts retained Scribe workers. It performs no flush or external await.
    pub(crate) fn abort_shutdown(&self) {
        self.gate.close();
        if let Some(role) = &self.scribe_role {
            role.lifecycle.begin_stopping();
            role.shutdown.cancel();
            role.heartbeat_abort.abort();
            role.snapshot_poller_abort.abort();
        }
        self.scribe.abort_shutdown();
    }
    /// Builds Gate around the exact Scribe allocation retained by this runtime.
    ///
    /// The projection shares Scribe's bounded ingress CPU lane, while an
    /// optional runtime keeps server-created coordination consumers alive.
    #[must_use]
    pub fn new(
        scribe: Arc<ScribeImpl>,
        catalog: Arc<BifrostCatalog>,
        verifier: Arc<WyrdTokenVerifier>,
        limits: IngestLimits,
        coordination_runtime: Option<Arc<tokio::runtime::Runtime>>,
    ) -> Self {
        let tail_reader = Arc::new(
            scribe
                .tail_reader()
                .expect("constructed Scribe must retain a valid UUID stream identity"),
        );
        let gate_scribe: Arc<dyn Scribe> = scribe.clone();
        let gate = Arc::new(ServerGate::with_scribe_and_projection(
            catalog,
            gate_scribe,
            ingest_auth_interceptor(verifier),
            limits,
            Arc::new(IngressCpuProjection::new(scribe.ingress_cpu_pool())),
        ));
        Self {
            scribe,
            tail_reader,
            tail_authority: None,
            gate,
            coordination_runtime,
            scribe_role: None,
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

    /// Starts the independently fenced Scribe role lifecycle on the active server runtime.
    ///
    /// The registry owns durable role mutations. The runtime retains only the
    /// cancellation edge so shutdown can stop both recurring tasks before it
    /// removes exactly the role fence created at boot.
    #[must_use]
    pub fn with_scribe_role(
        mut self,
        registry: Arc<ClusterRegistry>,
        registered: RegisteredRole,
    ) -> Self {
        let shutdown = CancellationToken::new();
        let advertise_ready = Arc::new(AtomicBool::new(true));
        let heartbeat = Arc::clone(&registry).start_readiness_heartbeat(
            registered.clone(),
            Arc::clone(&advertise_ready),
            shutdown.clone(),
        );
        let snapshot_poller = Arc::clone(&registry).start_snapshot_poller(shutdown.clone());
        let heartbeat_abort = heartbeat.abort_handle();
        let snapshot_poller_abort = snapshot_poller.abort_handle();
        self.scribe_role = Some(ScribeRoleRuntime {
            registry,
            registered,
            shutdown,
            heartbeat: Arc::new(Mutex::new(Some(heartbeat))),
            heartbeat_abort,
            snapshot_poller: Arc::new(Mutex::new(Some(snapshot_poller))),
            snapshot_poller_abort,
            lifecycle: Arc::new(RoleLifecycle::serving()),
            advertise_ready,
        });
        self
    }

    /// Clones this test runtime while attaching a production-shaped Scribe role lifecycle.
    #[cfg(all(test, feature = "test-support"))]
    #[must_use]
    pub(crate) fn clone_with_scribe_role_for_test(
        &self,
        registry: Arc<ClusterRegistry>,
        registered: RegisteredRole,
    ) -> Self {
        let mut runtime = self.clone();
        runtime.scribe_role = None;
        runtime.with_scribe_role(registry, registered)
    }

    /// Returns the Gate mounted by gRPC and HTTP ingest routes.
    #[must_use]
    pub fn gate(&self) -> Arc<ServerGate> {
        Arc::clone(&self.gate)
    }

    /// Borrows the Scribe used by the Gate and lifecycle paths.
    #[must_use]
    pub fn scribe(&self) -> &Arc<ScribeImpl> {
        &self.scribe
    }

    /// Returns the shared Scribe-owned tail reader mounted only on private paths.
    #[must_use]
    pub fn tail_reader(&self) -> Arc<ScribeTailReader> {
        Arc::clone(&self.tail_reader)
    }

    /// Reports whether the ingest writer has completed recovery and can accept work.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.scribe_role
            .as_ref()
            .is_none_or(|role| role.lifecycle.is_serving())
            && self.scribe.is_ready()
    }

    /// Removes Scribe readiness before transport draining begins.
    ///
    /// # Errors
    ///
    /// Returns the registry error when the exact Scribe fence cannot be marked
    /// unready. Local readiness still closes so the process fails closed.
    pub async fn begin_shutdown(&self) -> Result<(), vala_bifrost_redux::cluster::ClusterError> {
        let Some(role) = &self.scribe_role else {
            return Ok(());
        };
        if !role.lifecycle.begin_draining() {
            return Ok(());
        }
        metrics::gauge!("bifrost_role_ready", "role" => "scribe").set(0.0);
        role.advertise_ready.store(false, Ordering::Release);
        role.registry.deactivate(&role.registered).await
    }

    /// Stops heartbeat before Gate rejection, Scribe drain, and unregister.
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
    /// Gate admission closes first; Scribe may flush work accepted before that
    /// transition while budget remains. Every later join shares `deadline`, and
    /// timed-out retained handles are aborted before this method returns.
    pub(crate) async fn shutdown_owner(&self, deadline: Instant) {
        self.gate.close();
        if let Some(role) = &self.scribe_role {
            role.lifecycle.begin_stopping();
            role.shutdown.cancel();
        }
        if let Some(authority) = &self.tail_authority {
            authority.clear_replay_state();
        }
        self.scribe.shutdown(deadline).await;
        if let Some(role) = &self.scribe_role {
            await_role_task(&role.heartbeat, deadline, "scribe heartbeat").await;
            await_role_task(&role.snapshot_poller, deadline, "scribe snapshot poller").await;
        }
    }

    /// Unregisters the exact Scribe fence within `deadline`.
    ///
    /// The exact retained fence is removed only while budget remains. Timeout,
    /// cancellation, or registry failure leaves lifecycle completion unset and
    /// starts no post-deadline retry.
    pub(crate) async fn shutdown_registry(&self, deadline: Instant) {
        if let Some(role) = &self.scribe_role {
            if tokio::time::Instant::now() < tokio::time::Instant::from_std(deadline) {
                match timeout_at(
                    tokio::time::Instant::from_std(deadline),
                    role.registry.shutdown_role(role.registered.clone()),
                )
                .await
                {
                    Ok(Ok(())) => role.lifecycle.finish(),
                    Ok(Err(error)) => {
                        tracing::warn!(%error, "failed to unregister the Scribe role during shutdown")
                    }
                    Err(_) => tracing::warn!("Scribe role unregister exceeded shutdown deadline"),
                }
            } else {
                tracing::warn!("skipping Scribe role unregister after shutdown deadline");
            }
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
    pub(crate) fn role_tasks_finished_for_test(&self) -> Option<(bool, bool)> {
        self.scribe_role.as_ref().map(|role| {
            (
                role.heartbeat_abort.is_finished(),
                role.snapshot_poller_abort.is_finished(),
            )
        })
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
    /// Process-wide Bifrost OLAP catalog.
    pub bifrost: Arc<WyrdCatalog>,
    /// Tenant-qualified Redux catalog used by Gate, Scribe, Forge, and Oracle
    /// query paths.
    pub bifrost_redux: Option<Arc<BifrostCatalog>>,
    /// Shared parent memory governor used by Scribe, Forge, and Oracle reads.
    pub bifrost_memory: Option<BifrostMemoryGovernor>,
    /// Shared DataFusion pool bounded by the Bifrost parent ceiling.
    pub bifrost_query_memory: Option<Arc<dyn MemoryPool>>,
    /// Complete Gate/Scribe ingest subsystem, absent only when Bifrost ingest is disabled.
    pub bifrost_ingest: Option<Arc<BifrostIngestRuntime>>,
    /// Stable Gate mounted for every role configuration.
    pub bifrost_gate: Option<Arc<ServerGate>>,
    /// Independently optional retained Oracle query subsystem.
    pub bifrost_query: Option<Arc<BifrostQueryRuntime>>,
    /// Optional readiness-qualified Oracle peer runtime mounted on private gRPC.
    pub oracle_peer: Option<Arc<crate::oracle::OraclePeerRuntime>>,
    /// Shared Redux Forge owner used by supervision and maintenance tests.
    ///
    /// This is private so every server path observes the one supervised Forge
    /// graph assembled at boot rather than replacing it after construction.
    forge: Option<Arc<Forge>>,
    /// Authentication handles: token issuance + verification + issuer/binding resolution.
    pub auth: ServerAuth,
    /// Authorization handles: policy decision + RBAC evaluation + decision audit.
    pub authz: ServerAuthz,
    /// Deployment posture (Development / Production) locked at boot.
    pub deployment_profile: DeploymentProfile,
    /// Closed role set selected for this server process.
    pub bifrost_roles: BTreeSet<BifrostRuntimeRole>,
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
        bifrost: Arc<WyrdCatalog>,
    ) -> Self {
        let (reporter, _service) = wyrd_tonic::tonic_health::server::health_reporter();
        Self {
            postgres,
            storage,
            bifrost,
            bifrost_redux: None,
            bifrost_memory: None,
            bifrost_query_memory: None,
            bifrost_ingest: None,
            bifrost_gate: None,
            bifrost_query: None,
            oracle_peer: None,
            forge: None,
            auth: ServerAuth::default(),
            authz: ServerAuthz::default(),
            deployment_profile: DeploymentProfile::Development,
            bifrost_roles: BTreeSet::new(),
            shutdown_token: CancellationToken::new(),
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

    /// Attaches the closed role set selected during config validation.
    #[must_use]
    pub fn with_bifrost_roles(mut self, roles: BTreeSet<BifrostRuntimeRole>) -> Self {
        self.bifrost_roles = roles;
        self
    }

    /// Set the shared shutdown cancellation token.
    #[must_use]
    pub fn with_shutdown_token(mut self, shutdown_token: CancellationToken) -> Self {
        self.shutdown_token = shutdown_token;
        self
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

    /// Attach the Redux-owned tenant-qualified Bifrost catalog.
    #[must_use]
    pub fn with_bifrost_redux(mut self, catalog: Arc<BifrostCatalog>) -> Self {
        self.bifrost_redux = Some(catalog);
        self
    }

    /// Attach the process-wide Bifrost memory governor and its shared DataFusion pool.
    ///
    /// The supplied pool is retained verbatim and must also be passed to Forge;
    /// this preserves one bounded DataFusion allocation graph across query,
    /// rewrite, and ingest paths. Construction remains synchronous.
    #[must_use]
    pub fn with_bifrost_memory_pool(
        mut self,
        memory: BifrostMemoryGovernor,
        query_memory: Arc<dyn MemoryPool>,
    ) -> Self {
        self.bifrost_query_memory = Some(query_memory);
        self.bifrost_memory = Some(memory);
        self
    }

    /// Attach one complete, internally consistent Bifrost ingest subsystem.
    ///
    /// No separate Gate or Scribe setters exist, so callers cannot mount a Gate
    /// around a writer that lifecycle and test controls do not also own.
    #[must_use]
    pub fn with_bifrost_ingest(mut self, runtime: Arc<BifrostIngestRuntime>) -> Self {
        self.bifrost_ingest = Some(runtime);
        self
    }

    /// Attaches the stable Gate used by every public Bifrost transport.
    #[must_use]
    pub fn with_bifrost_gate(mut self, gate: Arc<ServerGate>) -> Self {
        self.bifrost_gate = Some(gate);
        self
    }

    /// Attaches one retained Oracle query subsystem.
    #[must_use]
    pub fn with_bifrost_query(mut self, runtime: Arc<BifrostQueryRuntime>) -> Self {
        self.oracle_peer = Some(runtime.peer());
        self.bifrost_query = Some(runtime);
        self
    }

    /// Returns the independently optional retained Oracle query subsystem.
    #[must_use]
    pub fn bifrost_query(&self) -> Option<&Arc<BifrostQueryRuntime>> {
        self.bifrost_query.as_ref()
    }

    /// Attach one sentinel-verified Oracle peer runtime for local and tonic execution.
    #[must_use]
    pub fn with_oracle_peer(mut self, peer: Arc<crate::oracle::OraclePeerRuntime>) -> Self {
        self.oracle_peer = Some(peer);
        self
    }

    /// Attach the single production Forge owner used by the supervised worker.
    ///
    /// The caller must pass the Forge built from the same memory-pool Arc stored
    /// by [`Self::with_bifrost_memory_pool`].
    #[must_use]
    pub fn with_forge(mut self, forge: Arc<Forge>) -> Self {
        self.forge = Some(forge);
        self
    }

    /// Borrow the retained Forge handle for server-owned supervision.
    #[must_use]
    pub(crate) fn forge_handle(&self) -> Option<&Arc<Forge>> {
        self.forge.as_ref()
    }

    /// Borrow the retained Forge owner from test-tier callers.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn forge(&self) -> Option<&Arc<Forge>> {
        self.forge.as_ref()
    }

    /// Borrow the retained Bifrost Scribe for test-tier harness inspection.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn bifrost_scribe_for_test(&self) -> Option<&Arc<ScribeImpl>> {
        self.bifrost_ingest.as_ref().map(|runtime| runtime.scribe())
    }

    /// Borrow the private Scribe tail reader for observation-only journey checks.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn bifrost_tail_reader_for_test(&self) -> Option<Arc<ScribeTailReader>> {
        self.bifrost_ingest
            .as_ref()
            .map(|runtime| runtime.tail_reader())
    }

    /// Flush the private Scribe runtime for the test harness only.
    #[cfg(feature = "test-support")]
    pub async fn flush_scribe_for_test(
        &self,
        mut conn: vala_sql::TenantConn<'_>,
    ) -> Result<(), vala_bifrost_redux::contracts::ScribeError> {
        let Some(runtime) = &self.bifrost_ingest else {
            return Err(vala_bifrost_redux::contracts::ScribeError::Internal {
                detail: "Scribe is not configured".to_owned(),
            });
        };
        let post_commit = runtime.scribe().force_seal(&mut conn).await?;
        conn.commit().await.map_err(|error| {
            vala_bifrost_redux::contracts::ScribeError::Internal {
                detail: error.to_string(),
            }
        })?;
        runtime.scribe().complete_post_commit(post_commit).await
    }

    /// Trip the Scribe WAL breaker for a deterministic test-tier probe.
    #[cfg(feature = "test-support")]
    pub fn trip_scribe_wal_disk_full_for_test(&self) -> Result<(), String> {
        let Some(runtime) = &self.bifrost_ingest else {
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
        let Some(runtime) = &self.bifrost_ingest else {
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
        let Some(runtime) = &self.bifrost_ingest else {
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
        let serves_api = self.bifrost_roles.contains(&BifrostRuntimeRole::Scribe)
            || self.bifrost_roles.contains(&BifrostRuntimeRole::Oracle);
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
        if self.bifrost_roles.contains(&BifrostRuntimeRole::Oracle)
            && (self.oracle_peer.is_none() || self.bifrost_query.is_none())
        {
            return Err(ProductionValidationError::MissingOraclePeer);
        }
        Ok(())
    }
}

#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;

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

    use super::{AppState, LimitsConfig, ProductionValidationError};

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
