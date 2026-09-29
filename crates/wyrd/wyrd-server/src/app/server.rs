//! `WyrdServer` — composable supervised server lifecycle.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use secrecy::{ExposeSecret, SecretString};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use wyrd_tonic::tonic::transport::Identity;
use wyrd_tonic::tonic::transport::server::Router as TonicRouter;
use wyrd_tonic::tonic_health::server::HealthReporter;

use crate::app::BootExit;
use crate::app::metrics::{install_recorder, metrics_router, serve_metrics};
use crate::app::serve::serve;
use crate::app::supervise::{
    TaskExit, TaskId, classify_first_exit_with_shutdown, drain_with_shutdown_hooks, fallible_task,
    worker_task,
};
use crate::boot::{
    ServerBootError, check_card_recovery_pool, spawn_maintenance_scheduler, spawn_storage_sweeper,
};
use crate::components::cards::reconciler;
use crate::components::health::readiness_loop;
use crate::config::{BifrostTarget, ServeMode, WyrdServerConfig};
use crate::grpc::{
    GrpcRouterConfig, build_app_grpc, build_peer_grpc, drive_health_status, publish_initial_health,
    serve_grpc_with_listener,
};
use crate::state::{AppState, BifrostShutdownReport};
use crate::verification::{RuntimeLimits, VerificationRuntime};

type BoxWorker = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Reports whether Tokio's runtime clock still has shutdown budget remaining.
fn shutdown_deadline_active(deadline: std::time::Instant) -> bool {
    tokio::time::Instant::now() < tokio::time::Instant::from_std(deadline)
}

/// Resolves supervisor and Bifrost terminal state without discarding lifecycle failures.
///
/// `report` is the drain outcome the caller already computed; it is returned
/// only on the clean path so a lifecycle failure can never be mistaken for a
/// completed drain. A supervisor terminal message and a Bifrost lifecycle error
/// both remain terminal, with the Bifrost error taking precedence because it
/// describes the state the process is actually leaving behind.
fn server_shutdown_result(
    terminal: Option<String>,
    bifrost_error: Option<wyrd_spec::vala::error::BifrostError>,
    report: BifrostShutdownReport,
) -> Result<BifrostShutdownReport, BootExit> {
    match (terminal, bifrost_error) {
        (_, Some(error)) => Err(BootExit::Other(Box::new(error))),
        (Some(message), None) => Err(BootExit::Other(
            Box::<dyn std::error::Error + Send + Sync>::from(message),
        )),
        (None, None) => Ok(report),
    }
}

/// Boot-loaded gRPC identity whose private key remains redacted until tonic consumes it.
struct GrpcIdentityMaterial {
    /// Public certificate-chain PEM bytes.
    certificate: Vec<u8>,
    /// Private-key PEM retained in a redacting secret owner.
    private_key: SecretString,
}

impl std::fmt::Debug for GrpcIdentityMaterial {
    /// Formats only the public certificate size and never exposes key material.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GrpcIdentityMaterial")
            .field("certificate_bytes", &self.certificate.len())
            .field("private_key", &"[REDACTED]")
            .finish()
    }
}

impl GrpcIdentityMaterial {
    /// Moves the public certificate and an explicitly exposed key into tonic's identity boundary.
    fn into_identity(self) -> Identity {
        Identity::from_pem(
            self.certificate,
            self.private_key.expose_secret().as_bytes(),
        )
    }
}

/// Composable server: seeds core HTTP/gRPC + workers from `AppState`, accepts
/// enterprise extensions, and drives a supervised lifecycle.
pub struct WyrdServer {
    config: WyrdServerConfig,
    state: AppState,
    http_router: Router,
    grpc_router: TonicRouter,
    /// Private Bifrost peer router, present only for a peer-bearing target.
    peer_router: Option<TonicRouter>,
    reporter: HealthReporter,
    metrics_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
    extra_workers: Vec<(&'static str, BoxWorker)>,
    #[cfg(feature = "test-support")]
    shutdown_probe: Option<ShutdownTestProbe>,
}

impl WyrdServer {
    /// Assemble the server around a fully-built core `AppState`.
    ///
    /// Creates the gRPC health reporter/service pair here so the reporter stored
    /// in `AppState.grpc_health` and the mounted health service come from the
    /// same `health_reporter()` call.
    ///
    /// Does **not** install the Prometheus recorder — that is a process-global
    /// side effect and is deferred to [`serve`], which installs it exactly once
    /// and only when `config.metrics.enabled`. This keeps `new` free of global
    /// side effects, so constructing (or double-constructing) a `WyrdServer`
    /// never touches the global recorder.
    ///
    /// # Errors
    /// Returns [`ServerBootError`] on gRPC router assembly (e.g. missing token
    /// verifier).
    pub fn new(config: WyrdServerConfig, state: AppState) -> Result<Self, ServerBootError> {
        Self::new_with_metrics_handle(config, state, None)
    }

    /// Assemble a server when the process recorder was installed before boot.
    pub fn new_with_metrics_handle(
        config: WyrdServerConfig,
        state: AppState,
        metrics_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
    ) -> Result<Self, ServerBootError> {
        let (reporter, health_service) = wyrd_tonic::tonic_health::server::health_reporter();
        // Store this reporter in state so readiness drives THIS health service.
        let state = state.with_grpc_health(reporter.clone());

        let tls_identity = match (
            &config.grpc.certificate_chain_path,
            &config.grpc.private_key_path,
        ) {
            (Some(certificate_path), Some(key_path)) => {
                let certificate = std::fs::read(certificate_path).map_err(|_| {
                    ServerBootError::OraclePeer(format!(
                        "failed to read gRPC certificate chain {}",
                        certificate_path.display()
                    ))
                })?;
                let key = std::fs::read_to_string(key_path).map_err(|_| {
                    ServerBootError::OraclePeer(format!(
                        "failed to read gRPC private key {}",
                        key_path.display()
                    ))
                })?;
                Some(
                    GrpcIdentityMaterial {
                        certificate,
                        private_key: SecretString::from(key),
                    }
                    .into_identity(),
                )
            }
            (None, None) => None,
            _ => {
                return Err(ServerBootError::OraclePeer(
                    "gRPC TLS certificate and private key must be configured together".to_owned(),
                ));
            }
        };
        let grpc_router = build_app_grpc(
            &state,
            health_service,
            GrpcRouterConfig {
                reflection_enabled: config.grpc.reflection_enabled,
                tls_identity,
            },
        )?;
        let peer_router = match load_peer_tls(&config)? {
            Some(peer_tls) => build_peer_grpc(
                &state,
                peer_tls,
                config.bifrost.peer.denial_audit_concurrency,
            )?,
            None => None,
        };
        // Declared from the composed router rather than from the target alone,
        // so a target that should serve the peer plane but composed no private
        // router is reported unready instead of quietly serving nothing.
        if config.role.serves_peer() {
            state.peer_plane.require();
        }
        let http_router = crate::http::build_router(state.clone());

        Ok(Self {
            config,
            state,
            http_router,
            grpc_router,
            peer_router,
            reporter,
            metrics_handle,
            extra_workers: Vec::new(),
            #[cfg(feature = "test-support")]
            shutdown_probe: None,
        })
    }

    /// Merge extension routes that **carry their own edge stack** onto core.
    ///
    /// A router merged here is served with tracing + metrics but **without**
    /// request-id, panic-to-500 mapping, request limits, or authentication
    /// unless it brought them itself. Use only for deliberately unprotected
    /// routes or routes that finalized their own edge stack.
    ///
    /// For enterprise write routes, prefer [`Self::merge_http_protected`].
    #[must_use]
    pub fn merge_http(mut self, extra: Router) -> Self {
        self.http_router = self.http_router.merge(extra);
        self
    }

    /// Merge extension routes wrapped in the **core protective edge stack**
    /// (request-id propagation, panic→`WyrdError` 500, error mapping, load-shed,
    /// concurrency, timeout, body limit) plus core `require_authenticated`.
    ///
    /// Guarantees enterprise write routes get the same request-id spine, panic
    /// handling, request limits, and authenticated `Principal` that core `/v1`
    /// routes get. ABAC/policy decisions still run through the injected
    /// `ServerAuthz.policy_hook` seam — the authenticated principal established
    /// here is what the policy hook authorizes.
    #[must_use]
    pub fn merge_http_protected(mut self, extra: Router) -> Self {
        // extra is Router<()> (caller-finalized). Apply the edge stack to it
        // (generic over S, so Router<()> is accepted), then finalize to Router<()>
        // with the auth middleware state so it can be merged into http_router.
        let with_auth = extra.layer(axum::middleware::from_fn_with_state(
            self.state.clone(),
            crate::http::middleware::authenticate::require_authenticated,
        ));
        let protected = crate::http::router::apply_protected_edge(with_auth, &self.state);
        self.http_router = self.http_router.merge(protected);
        self
    }

    /// Transform the gRPC router — typically `|r| r.add_service(svc)`.
    #[must_use]
    pub fn with_grpc(mut self, f: impl FnOnce(TonicRouter) -> TonicRouter) -> Self {
        self.grpc_router = f(self.grpc_router);
        self
    }

    /// Register an additional background worker with a diagnostic `name`.
    ///
    /// The name is used in supervisor logs and terminal-error messages so
    /// multiple workers can be distinguished from one another.
    /// The worker shares the server shutdown token via `self.state.shutdown_token`
    /// and must observe it to participate in cooperative shutdown.
    #[must_use]
    pub fn spawn_worker<F>(mut self, name: &'static str, worker: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.extra_workers.push((name, Box::pin(worker)));
        self
    }

    /// Installs the test-only production shutdown probe.
    #[cfg(all(feature = "test-support", test))]
    #[must_use]
    fn with_shutdown_probe(mut self, probe: ShutdownTestProbe) -> Self {
        self.shutdown_probe = Some(probe);
        self
    }

    /// Read-only access to core state (enterprise wraps it into its own state
    /// before/after construction as needed).
    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Consume the server and return the finalized HTTP router.
    ///
    /// Useful for integration tests that drive the router via
    /// `tower::ServiceExt::oneshot` without binding a listener. Not intended
    /// for production use — call [`serve`] for the full supervised lifecycle.
    pub fn into_http_router(self) -> Router {
        self.http_router
    }

    /// Bind listeners for `mode`, spawn all tasks, and drive the supervised
    /// lifecycle to completion.
    ///
    /// This is the production entry point: it is exactly [`bind`](Self::bind)
    /// followed by [`BoundServer::run`]. Callers that need the bound addresses
    /// before the server runs to completion (e.g. a test harness binding on an
    /// OS-assigned `:0` port) should call [`bind`](Self::bind) directly, read
    /// the addresses off the returned [`BoundServer`], then spawn
    /// [`BoundServer::run`].
    ///
    /// # Errors
    /// Returns [`BootExit::Other`] on listener bind failure or a terminal task
    /// error.
    pub async fn serve(self, mode: ServeMode) -> Result<(), BootExit> {
        // The drain report is a result of *running* the server, which the
        // process entry point has no use for; absorbing it here keeps
        // `serve`'s public signature and every caller below it unchanged.
        self.bind(mode).await?.run().await.map(|_report| ())
    }

    /// Bind every listener the `mode` requires **now**, returning a
    /// [`BoundServer`] that exposes the concrete bound addresses.
    ///
    /// Binding up front means bind failures surface here as boot errors (not
    /// task errors), and — critically for `:0` binds — the resolved port is
    /// readable from the [`BoundServer`] before serving begins, with no
    /// bind-then-rebind race. The process-global metrics recorder is **not**
    /// installed here; that is deferred to [`BoundServer::run`] so a bound but
    /// never-run server never touches the global recorder.
    ///
    /// # Errors
    /// Returns [`BootExit::Other`] when another Rustls provider already owns the
    /// process, listener binding fails, or configured gRPC identity material
    /// cannot be loaded. Cancellation may leave already-bound listeners open
    /// until the returned future and its owned server state are dropped.
    pub async fn bind(self, mode: ServeMode) -> Result<BoundServer, BootExit> {
        wyrd_tls::install_crypto_provider().map_err(|error| BootExit::Other(Box::new(error)))?;
        let (http_listener, http_addr) = if mode.serves_http() {
            let bind = self.config.http.bind;
            let listener = TcpListener::bind(bind).await.map_err(|e| {
                BootExit::Other(format!("HTTP listener failed to bind {bind}: {e}").into())
            })?;
            let addr = listener
                .local_addr()
                .map_err(|e| BootExit::Other(Box::new(e)))?;
            (Some(listener), Some(addr))
        } else {
            (None, None)
        };
        let (grpc_listener, grpc_addr) = if mode.serves_grpc() {
            let bind = self.config.grpc.bind;
            let listener = TcpListener::bind(bind).await.map_err(|e| {
                BootExit::Other(format!("gRPC listener failed to bind {bind}: {e}").into())
            })?;
            let addr = listener
                .local_addr()
                .map_err(|e| BootExit::Other(Box::new(e)))?;
            (Some(listener), Some(addr))
        } else {
            (None, None)
        };
        // Metrics is orthogonal to `mode`: its own listener, enabled whenever
        // `metrics.enabled`. Resolve against the *actually bound* HTTP addr when
        // present so a `:0` HTTP bind yields `bound_port + 1`, not `config + 1`.
        let (metrics_listener, metrics_addr) = if self.config.metrics.enabled {
            let base = http_addr.unwrap_or(self.config.http.bind);
            let bind = self.config.metrics.resolved_bind(base).ok_or_else(|| {
                BootExit::Other(
                    "metrics port arithmetic overflow: HTTP port 65535 leaves no room for the \
                     metrics listener (would wrap to 65535)"
                        .into(),
                )
            })?;
            let listener = TcpListener::bind(bind).await.map_err(|e| {
                BootExit::Other(format!("metrics listener failed to bind {bind}: {e}").into())
            })?;
            let addr = listener
                .local_addr()
                .map_err(|e| BootExit::Other(Box::new(e)))?;
            (Some(listener), Some(addr))
        } else {
            (None, None)
        };

        // The peer socket is bound by the same owner, in the same call, as the
        // public sockets. Binding it here is what lets a bind failure surface
        // as a boot error and what lets readiness wait on the address the
        // listener actually holds rather than the configured one.
        let (peer_listener, peer_addr) = match self.peer_router.is_some() {
            true => {
                let bind = self.config.bifrost.peer.bind;
                let listener = TcpListener::bind(bind).await.map_err(|e| {
                    BootExit::Other(
                        format!("Bifrost peer listener failed to bind {bind}: {e}").into(),
                    )
                })?;
                let addr = listener
                    .local_addr()
                    .map_err(|e| BootExit::Other(Box::new(e)))?;
                (Some(listener), Some(addr))
            }
            false => (None, None),
        };

        Ok(BoundServer {
            config: self.config,
            state: self.state,
            http_router: self.http_router,
            grpc_router: self.grpc_router,
            peer_router: self.peer_router,
            peer_listener,
            peer_addr,
            reporter: self.reporter,
            metrics_handle: self.metrics_handle,
            extra_workers: self.extra_workers,
            #[cfg(feature = "test-support")]
            shutdown_probe: self.shutdown_probe,
            http_listener,
            grpc_listener,
            metrics_listener,
            http_addr,
            grpc_addr,
            metrics_addr,
        })
    }
}

/// A [`WyrdServer`] whose listeners are already bound.
///
/// Produced by [`WyrdServer::bind`]. The bound addresses are readable before
/// the supervised lifecycle starts, so a caller can learn an OS-assigned `:0`
/// port and hand it to clients, then drive the exact production supervision via
/// [`run`](Self::run).
pub struct BoundServer {
    config: WyrdServerConfig,
    state: AppState,
    http_router: Router,
    grpc_router: TonicRouter,
    /// Private Bifrost peer router, present only for a peer-bearing target.
    peer_router: Option<TonicRouter>,
    /// Pre-bound private peer listener paired with `peer_router`.
    peer_listener: Option<TcpListener>,
    /// Exact address the private peer listener holds.
    peer_addr: Option<SocketAddr>,
    reporter: HealthReporter,
    metrics_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
    extra_workers: Vec<(&'static str, BoxWorker)>,
    http_listener: Option<TcpListener>,
    grpc_listener: Option<TcpListener>,
    metrics_listener: Option<TcpListener>,
    http_addr: Option<SocketAddr>,
    grpc_addr: Option<SocketAddr>,
    metrics_addr: Option<SocketAddr>,
    #[cfg(feature = "test-support")]
    shutdown_probe: Option<ShutdownTestProbe>,
}

/// Test-only shutdown boundaries exercised through [`BoundServer::run`].
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShutdownTestPhase {
    Readiness,
    Transport,
    OracleRegistry,
    ScribeShutdown,
    ScribeRegistry,
}

/// Test-only recorder and deterministic stall injected into the production owner.
#[cfg(feature = "test-support")]
#[derive(Clone)]
struct ShutdownTestProbe {
    stall: ShutdownTestPhase,
    calls: Arc<std::sync::Mutex<Vec<(ShutdownTestPhase, tokio::time::Instant)>>>,
}

#[cfg(feature = "test-support")]
impl ShutdownTestProbe {
    /// Creates a probe that stalls exactly one production shutdown phase.
    #[cfg(test)]
    fn new(stall: ShutdownTestPhase) -> Self {
        Self {
            stall,
            calls: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Records the deadline by value and stalls the selected phase forever.
    async fn enter(&self, phase: ShutdownTestPhase, deadline: tokio::time::Instant) {
        self.calls
            .lock()
            .expect("shutdown probe call lock remains available")
            .push((phase, deadline));
        if self.stall == phase {
            std::future::pending::<()>().await;
        }
    }

    /// Returns the ordered production phases reached before deadline exhaustion.
    #[cfg(test)]
    fn calls(&self) -> Vec<(ShutdownTestPhase, tokio::time::Instant)> {
        self.calls
            .lock()
            .expect("shutdown probe call lock remains available")
            .clone()
    }
}

impl BoundServer {
    /// The bound HTTP address, or `None` when the mode does not serve HTTP.
    #[must_use]
    pub fn http_addr(&self) -> Option<SocketAddr> {
        self.http_addr
    }

    /// Compose this process's verification runtime from its configuration.
    ///
    /// Results publish through `verification.ingest_endpoint` when set, and
    /// otherwise through this process's own plaintext gRPC listener when it
    /// hosts a Scribe. The drain grace is clipped inside the server's shutdown
    /// budget so released leases settle before teardown aborts the task.
    fn verification_runtime(&self) -> Option<VerificationRuntime> {
        let limits = RuntimeLimits::default()
            .within_server_drain(Duration::from_millis(self.config.shutdown.drain_ms));
        let mut builder = VerificationRuntime::builder(&self.state).limits(limits);
        if let Some(endpoint) = &self.config.verification.ingest_endpoint {
            builder = builder.ingest_endpoint(endpoint.clone());
        }
        builder
            .local_ingest(
                self.grpc_addr,
                self.config.grpc.certificate_chain_path.is_some(),
            )
            .build()
    }

    /// The bound gRPC address, or `None` when the mode does not serve gRPC.
    #[must_use]
    pub fn grpc_addr(&self) -> Option<SocketAddr> {
        self.grpc_addr
    }

    /// The bound `/metrics` address, or `None` when metrics are disabled.
    #[must_use]
    pub fn metrics_addr(&self) -> Option<SocketAddr> {
        self.metrics_addr
    }

    /// The bound private Bifrost peer address, or `None` for a non-peer target.
    ///
    /// This is the address a peer-bearing role advertises into membership: a
    /// selected `NodeId` and fence must reach this exact replica, so callers
    /// publish it rather than the public gRPC address.
    #[must_use]
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.peer_addr
    }

    /// Read-only access to core state.
    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Spawn all tasks on the pre-bound listeners and drive the supervised
    /// lifecycle to completion.
    ///
    /// The [`BifrostShutdownReport`] returned on the clean path is the outcome
    /// of the bounded Bifrost drain this method performs. It is a first-class
    /// result rather than an internal detail because the drain ordering it
    /// records — in particular `scribe_drained` — is what a caller holding the
    /// Scribe coordination-runtime owner needs in order to know that releasing
    /// that executor cannot abandon a live shard owner.
    ///
    /// When the shutdown deadline is already exhausted the server aborts
    /// instead of draining, and the returned report is all-`false`: that is the
    /// accurate description of an aborted teardown, not a placeholder.
    ///
    /// # Errors
    /// Returns [`BootExit::Other`] on a terminal task error, a Bifrost
    /// lifecycle failure, or if the process-global metrics recorder fails to
    /// install.
    pub async fn run(mut self) -> Result<BifrostShutdownReport, BootExit> {
        // A production deployment without the Wyrd operator pool cannot run the
        // Card recovery sweep, so stale precommits would leak indefinitely.
        check_card_recovery_pool(&self.state).map_err(|e| BootExit::Other(Box::new(e)))?;

        let shutdown = self.state.shutdown_token.clone();
        let mut set: JoinSet<TaskExit> = JoinSet::new();
        if let Some(health) = self.state.bifrost.resource_health() {
            set.spawn(fallible_task(
                TaskId::Worker("bifrost_resource_health"),
                // Poison is a terminal the supervisor reacts to, but a healthy
                // server never publishes one, so this wait must also end on a
                // clean shutdown. Without the cancellation arm the task can
                // never join and every shutdown burns the full drain deadline
                // before aborting it.
                {
                    let shutdown = shutdown.clone();
                    async move {
                        tokio::select! {
                            poisoned = health.wait_for_poison() => poisoned,
                            () = shutdown.cancelled() => Ok(()),
                        }
                    }
                },
            ));
        }

        // Health: publish the initial snapshot before driving it.
        publish_initial_health(&self.state.readiness, &mut self.reporter).await;

        // Core workers.
        set.spawn(worker_task(
            TaskId::Worker("readiness"),
            readiness_loop(
                self.state.clone(),
                Duration::from_millis(self.config.readiness.tick_ms),
                Duration::from_millis(self.config.readiness.probe_timeout_ms),
                shutdown.clone(),
            ),
        ));
        set.spawn(worker_task(
            TaskId::Worker("health_status"),
            drive_health_status(
                self.state.readiness.clone(),
                self.reporter.clone(),
                shutdown.clone(),
            ),
        ));
        if let Some(scribe) = self
            .state
            .bifrost_ingest()
            .map(|runtime| Arc::clone(runtime.scribe()))
        {
            let shutdown = shutdown.clone();
            set.spawn(worker_task(
                TaskId::Worker("scribe_age_scanner"),
                async move {
                    let mut ticks = tokio::time::interval(Duration::from_secs(1));
                    loop {
                        tokio::select! {
                            _ = shutdown.cancelled() => break,
                            _ = ticks.tick() => scribe.check_age(std::time::Instant::now()),
                        }
                    }
                },
            ));
        }
        // Retained audit history: one bounded sweep per interval moves each
        // tenant's oldest contiguous run of audit events out of the
        // transactional staging and retires it only once Scribe has it durably.
        #[cfg(feature = "test-support")]
        let publication = (!self.state.audit_publication_disabled)
            .then(|| crate::audit::publication::AuditPublisher::from_state(&self.state))
            .flatten();
        #[cfg(not(feature = "test-support"))]
        let publication = crate::audit::publication::AuditPublisher::from_state(&self.state);
        if let Some(publisher) = publication {
            set.spawn(worker_task(
                TaskId::Worker("audit_publisher"),
                publisher.run(shutdown.clone()),
            ));
        }
        if let Some(handle) = spawn_storage_sweeper(&self.state, shutdown.clone())
            .map_err(|e| BootExit::Other(Box::new(e)))?
        {
            set.spawn(worker_task(TaskId::Worker("storage_sweeper"), async move {
                if let Err(join_error) = handle.await
                    && join_error.is_panic()
                {
                    std::panic::resume_unwind(join_error.into_panic());
                }
            }));
        }
        // One supervised Redux Forge worker owns compaction, expiry, reconciliation,
        // live-set rebuild, and orphan GC for this process.
        if let Some(scheduler) = spawn_maintenance_scheduler(&self.state, shutdown.clone())
            .map_err(|e| BootExit::Other(Box::new(e)))?
        {
            set.spawn(fallible_task(
                TaskId::Worker("maintenance_scheduler"),
                scheduler,
            ));
        }
        // `All` owns one bounded Forge worker in addition to the scheduler;
        // `Server` intentionally schedules maintenance without executing it.
        // The dedicated `ForgeWorker` process is composed by
        // `run_forge_worker_process` and never reaches this serving owner.
        if self.config.role == BifrostTarget::All {
            let worker = crate::boot::spawn_forge_worker(&self.state, shutdown.clone())
                .map_err(|e| BootExit::Other(Box::new(e)))?;
            set.spawn(fallible_task(TaskId::Worker("forge_worker"), worker));
        }

        if let Some(operator) = self.state.postgres.operator_pool() {
            set.spawn(worker_task(
                TaskId::Worker("card_reconciler"),
                reconciler::run(self.state.clone(), operator, shutdown.clone()),
            ));
        }

        // One supervised verification runtime per API-serving process: the
        // durable queue coordinates every replica, so no leader is elected.
        if self.config.verification.enabled
            && self.config.role.serves_api()
            && let Some(runtime) = self.verification_runtime()
        {
            set.spawn(worker_task(
                TaskId::Worker("verification_runtime"),
                runtime.run(shutdown.clone()),
            ));
        }

        // Enterprise workers.
        for (name, worker) in self.extra_workers.drain(..) {
            set.spawn(worker_task(TaskId::Worker(name), worker));
        }

        // Transports — drive the listeners bound in `WyrdServer::bind`.
        if let Some(listener) = self.http_listener.take() {
            tracing::info!(addr = ?self.http_addr, "HTTP server listening");
            set.spawn(fallible_task(
                TaskId::Http,
                serve(self.http_router, listener, shutdown.clone()),
            ));
        }
        if let Some(listener) = self.grpc_listener.take() {
            tracing::info!(addr = ?self.grpc_addr, "gRPC server listening");
            let router = self.grpc_router;
            let token = shutdown.clone();
            set.spawn(fallible_task(TaskId::Grpc, async move {
                serve_grpc_with_listener(router, listener, token).await
            }));
        }
        if let Some(listener) = self.peer_listener.take() {
            tracing::info!(addr = ?self.peer_addr, "Bifrost peer server listening");
            let router = self
                .peer_router
                .take()
                .expect("a bound peer listener always carries its peer router");
            let token = shutdown.clone();
            // Published here rather than at bind time, so readiness reports the
            // plane as up only while the serving task actually holds it.
            let peer_plane = Arc::clone(&self.state.peer_plane);
            peer_plane.mark_serving();
            set.spawn(fallible_task(TaskId::BifrostPeer, async move {
                let served = serve_grpc_with_listener(router, listener, token).await;
                peer_plane.mark_stopped();
                served
            }));
        }
        if let Some(listener) = self.metrics_listener.take() {
            let handle = match self.metrics_handle.take() {
                Some(handle) => handle,
                None => install_recorder().map_err(|e| BootExit::Other(Box::new(e)))?,
            };
            let router = metrics_router(handle);
            set.spawn(fallible_task(
                TaskId::Metrics,
                serve_metrics(router, listener, shutdown.clone()),
            ));
        }

        // Signal watcher — completing this is the normal shutdown trigger.
        set.spawn(worker_task(
            TaskId::Signal,
            crate::app::shutdown::signal_watcher(shutdown.clone()),
        ));

        let drain = Duration::from_millis(self.config.shutdown.drain_ms);
        let bifrost = Arc::clone(&self.state.bifrost);
        #[cfg(feature = "test-support")]
        let shutdown_probe = self.shutdown_probe.clone();
        let terminal = classify_first_exit_with_shutdown(&mut set, &shutdown).await;
        let deadline = tokio::time::Instant::now() + drain;
        let supervised_drained = drain_with_shutdown_hooks(
            set,
            shutdown,
            deadline,
            || {
                let bifrost = Arc::clone(&bifrost);
                #[cfg(feature = "test-support")]
                let shutdown_probe = shutdown_probe.clone();
                async move {
                    #[cfg(feature = "test-support")]
                    if let Some(probe) = shutdown_probe {
                        probe.enter(ShutdownTestPhase::Readiness, deadline).await;
                    }
                    bifrost.begin_shutdown();
                }
            },
            || {
                #[cfg(feature = "test-support")]
                let shutdown_probe = shutdown_probe.clone();
                async move {
                    #[cfg(feature = "test-support")]
                    if let Some(probe) = shutdown_probe {
                        probe.enter(ShutdownTestPhase::Transport, deadline).await;
                        return true;
                    }
                    false
                }
            },
        )
        .await;
        if let Some(forge) = bifrost.forge() {
            forge.mark_supervision_drained(supervised_drained);
        }

        // Transport admission has stopped, so no further MCP request can start.
        // Close the tracker and wait for the work already in flight inside the
        // unchanged process deadline. `timeout_at` polls the tracker before the
        // deadline, so an already-empty tracker returns immediately even when
        // the supervised drain consumed the whole budget. A tracker still
        // holding tokens at the deadline is a lifecycle failure: reporting a
        // clean drain there would claim a settlement that never happened.
        self.state.mcp_tasks.close();
        let mcp_drained = tokio::time::timeout_at(deadline, self.state.mcp_tasks.wait())
            .await
            .is_ok();
        // Gateway calls refuse admission once shutdown begins; accounting of
        // the calls already accepted, including streams that settle after
        // their response, drains within the same deadline.
        self.state.gateway_tasks.close();
        let gateway_drained = tokio::time::timeout_at(deadline, self.state.gateway_tasks.wait())
            .await
            .is_ok();
        // Drained calls have enqueued their capture; publish it within the
        // same deadline. Evidence still buffered at the deadline may be lost,
        // which capture permits before Scribe acknowledgement.
        if tokio::time::timeout_at(deadline, self.state.gateway_capture.shutdown())
            .await
            .is_err()
        {
            tracing::warn!("gateway capture did not drain before the shutdown deadline");
        }
        let terminal = match terminal {
            Some(message) => Some(message),
            None if !mcp_drained => {
                Some("MCP in-flight work did not drain before the shutdown deadline".to_owned())
            }
            None if !gateway_drained => Some(
                "gateway call accounting did not drain before the shutdown deadline".to_owned(),
            ),
            None => None,
        };

        let deadline = deadline.into_std();
        #[cfg(feature = "test-support")]
        if shutdown_deadline_active(deadline)
            && let Some(probe) = &self.shutdown_probe
        {
            let _ = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                probe.enter(
                    ShutdownTestPhase::OracleRegistry,
                    tokio::time::Instant::from_std(deadline),
                ),
            )
            .await;
        }
        #[cfg(feature = "test-support")]
        if shutdown_deadline_active(deadline)
            && let Some(probe) = &self.shutdown_probe
        {
            let _ = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                probe.enter(
                    ShutdownTestPhase::ScribeShutdown,
                    tokio::time::Instant::from_std(deadline),
                ),
            )
            .await;
        }
        #[cfg(feature = "test-support")]
        if shutdown_deadline_active(deadline)
            && let Some(probe) = &self.shutdown_probe
        {
            let _ = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                probe.enter(
                    ShutdownTestPhase::ScribeRegistry,
                    tokio::time::Instant::from_std(deadline),
                ),
            )
            .await;
        }
        let (report, bifrost_shutdown_error) = if shutdown_deadline_active(deadline) {
            match bifrost.shutdown(deadline).await {
                Ok(report) => (report, None),
                Err(error) => {
                    tracing::warn!(%error, "Bifrost shutdown did not complete cleanly");
                    bifrost.abort().await;
                    (BifrostShutdownReport::none_drained(), Some(error))
                }
            }
        } else {
            // The supervisor drain consumed the whole budget. That is a process
            // lifecycle failure, not a clean teardown: nothing drained and the
            // storage owner is only settled by the awaited abort below, so
            // reporting success here would let a pod exit claiming a drain it
            // never ran.
            bifrost.abort().await;
            (
                BifrostShutdownReport::none_drained(),
                Some(wyrd_spec::vala::error::BifrostError::Internal {
                    detail: "Bifrost shutdown deadline elapsed before role drain".to_owned(),
                }),
            )
        };

        tracing::info!("wyrd-server shutdown complete");
        server_shutdown_result(terminal, bifrost_shutdown_error, report)
    }
}

/// Loads this process's private peer TLS material, when it serves the peer plane.
///
/// Returns `None` for a target that mounts no private service, so a Forge
/// worker neither reads certificate files nor opens a peer socket.
///
/// # Errors
///
/// Returns [`ServerBootError::OraclePeer`] when a peer-bearing target has an
/// incomplete peer configuration or a PEM file cannot be read.
fn load_peer_tls(
    config: &WyrdServerConfig,
) -> Result<Option<wyrd_tonic::server::MutualTlsServerConfig>, ServerBootError> {
    let peer = &config.bifrost.peer;
    if !peer.is_complete() {
        if config.role.serves_peer() {
            return Err(ServerBootError::OraclePeer(
                "Scribe- and Oracle-bearing targets require the complete bifrost.peer identity"
                    .to_owned(),
            ));
        }
        return Ok(None);
    }
    let read = |path: &std::path::Path, label: &str| -> Result<Vec<u8>, ServerBootError> {
        std::fs::read(path).map_err(|error| {
            ServerBootError::OraclePeer(format!(
                "failed to read Bifrost peer {label} {}: {error}",
                path.display()
            ))
        })
    };
    let certificate = read(
        peer.certificate_chain_path
            .as_ref()
            .expect("peer completeness guarantees a certificate chain path"),
        "certificate chain",
    )?;
    let key = read(
        peer.private_key_path
            .as_ref()
            .expect("peer completeness guarantees a private key path"),
        "private key",
    )?;
    let ca = read(
        peer.ca_certificate_path
            .as_ref()
            .expect("peer completeness guarantees a CA path"),
        "CA certificate",
    )?;
    Ok(Some(wyrd_tonic::server::MutualTlsServerConfig::from_pem(
        &certificate,
        &key,
        &ca,
    )))
}

#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use tempfile::tempdir;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use super::*;
    use crate::components::auth::ServerAuth;
    use crate::config::WyrdServerConfig;
    use crate::postgres::ServerPostgres;

    /// Private gRPC key material is absent from diagnostic formatting.
    #[test]
    fn grpc_identity_material_debug_redacts_private_key() {
        let material = GrpcIdentityMaterial {
            certificate: b"public certificate".to_vec(),
            private_key: SecretString::from("private-key-sentinel"),
        };
        let debug = format!("{material:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("private-key-sentinel"));
    }

    async fn test_state_with_auth() -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let postgres = Arc::new(ServerPostgres::from_parts(
            wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None),
            vala_sql::ValaPostgres::from_pool(app_pool.clone()),
        ));
        let root = tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let kid = Kid::new("k1").expect("kid is valid");
        let pem = IssuingKey::generate_ephemeral_pem().expect("ephemeral key generates");
        let raw_issuing_key =
            IssuingKey::from_ed_pem(pem, kid.clone(), "wyrd").expect("ephemeral issuing key loads");
        let pub_pem = raw_issuing_key
            .verifying_key_pem()
            .expect("public key derives from ephemeral key");
        let issuing_key = Arc::new(raw_issuing_key);
        let mut keys = HashMap::new();
        keys.insert(
            kid,
            Arc::new(public_key_from_pem(pub_pem.as_bytes()).expect("public key loads")),
        );
        let verifier = Arc::new(TokenVerifier::new(
            keys,
            "wyrd",
            WyrdAuthVerifySettings::default(),
        ));
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(signer)));
        let shutdown = tokio_util::sync::CancellationToken::new();
        let bifrost = crate::state::Bifrost::test_shell(Arc::clone(&verifier));
        AppState::new(postgres, storage, bifrost, shutdown).with_auth(ServerAuth {
            issuing_key: Some(issuing_key),
            token_verifier: Some(verifier),
            ..ServerAuth::default()
        })
    }

    /// Proves every production shutdown phase shares one deadline and later phases are skipped.
    #[cfg(feature = "test-support")]
    #[tokio::test(start_paused = true)]
    async fn bound_server_run_uses_one_deadline_for_each_stalled_phase() {
        let phases = [
            ShutdownTestPhase::Readiness,
            ShutdownTestPhase::Transport,
            ShutdownTestPhase::OracleRegistry,
            ShutdownTestPhase::ScribeShutdown,
            ShutdownTestPhase::ScribeRegistry,
        ];
        for (stall_index, stall) in phases.into_iter().enumerate() {
            let mut config = WyrdServerConfig::default();
            config.http.bind = "127.0.0.1:0".parse().expect("static bind is valid");
            config.metrics.enabled = false;
            config.shutdown.drain_ms = 1_000;
            config.bifrost.peer = crate::test_support::test_peer_config();
            // The unit shell composes no Forge, and the default `All` target
            // refuses to run without a retained Forge worker before shutdown
            // is ever reached. `Server` is the serving target that schedules
            // maintenance without executing it; it reaches the identical
            // readiness, transport, Oracle, and Scribe shutdown phases.
            config.role = BifrostTarget::Server;
            let probe = ShutdownTestProbe::new(stall);
            let server = WyrdServer::new(config, test_state_with_auth().await)
                .expect("test server builds")
                .with_shutdown_probe(probe.clone())
                .spawn_worker("shutdown_trigger", async {});
            let bound = server.bind(ServeMode::Http).await.expect("HTTP binds");
            let started = tokio::time::Instant::now();

            let result = bound.run().await;

            assert!(result.is_err(), "early worker exit remains terminal");
            assert_eq!(
                tokio::time::Instant::now() - started,
                Duration::from_secs(1),
                "{stall:?} must return at the original bound"
            );
            let calls = probe.calls();
            assert_eq!(
                calls.iter().map(|(phase, _)| *phase).collect::<Vec<_>>(),
                phases[..=stall_index],
                "no phase may start after {stall:?} exhausts the deadline"
            );
            let recorded_deadline = calls[0].1;
            assert!(
                calls
                    .iter()
                    .all(|(_, deadline)| *deadline == recorded_deadline),
                "every phase must receive the identical absolute deadline"
            );
        }
    }

    #[tokio::test]
    async fn metrics_disabled_construction_has_no_global_recorder() {
        let mut config = WyrdServerConfig::default();
        config.metrics.enabled = false;
        config.bifrost.peer = crate::test_support::test_peer_config();

        let state1 = test_state_with_auth().await;
        let server1 = WyrdServer::new(config.clone(), state1)
            .expect("first WyrdServer construction succeeds");

        let state2 = test_state_with_auth().await;
        let server2 =
            WyrdServer::new(config, state2).expect("second WyrdServer construction succeeds");

        // Neither construction touched the global recorder — new() is side-effect-free.
        // Drop both without calling serve() to confirm no recorder was installed.
        drop(server1);
        drop(server2);
    }

    /// Preserves an observable Bifrost lifecycle failure after the required abort.
    #[tokio::test]
    async fn bound_server_run_preserves_bifrost_shutdown_failure_after_abort() {
        let result = server_shutdown_result(
            Some("worker exited".to_owned()),
            Some(wyrd_spec::vala::error::BifrostError::ScribeRoleUnavailable),
            BifrostShutdownReport::none_drained(),
        );

        let BootExit::Other(error) =
            result.expect_err("Bifrost failure remains terminal after abort")
        else {
            panic!("Bifrost lifecycle failure must use the runtime error channel");
        };
        assert_eq!(error.to_string(), "Scribe role unavailable");
    }
}
