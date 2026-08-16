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
use crate::boot::{ServerBootError, spawn_maintenance_scheduler, spawn_storage_sweeper};
use crate::components::health::readiness_loop;
use crate::config::{ForgeProcessRole, ServeMode, WyrdServerConfig};
use crate::grpc::{
    GrpcRouterConfig, build_app_grpc, drive_health_status, publish_initial_health,
    serve_grpc_with_listener,
};
use crate::state::AppState;

type BoxWorker = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Reports whether Tokio's runtime clock still has shutdown budget remaining.
fn shutdown_deadline_active(deadline: std::time::Instant) -> bool {
    tokio::time::Instant::now() < tokio::time::Instant::from_std(deadline)
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
        let http_router = crate::http::build_router(state.clone());

        Ok(Self {
            config,
            state,
            http_router,
            grpc_router,
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
    /// For enterprise write routes, prefer [`merge_http_protected`].
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
        self.bind(mode).await?.run().await
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

        Ok(BoundServer {
            config: self.config,
            state: self.state,
            http_router: self.http_router,
            grpc_router: self.grpc_router,
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

    /// Read-only access to core state.
    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Spawn all tasks on the pre-bound listeners and drive the supervised
    /// lifecycle to completion.
    ///
    /// # Errors
    /// Returns [`BootExit::Other`] on a terminal task error or if the process-
    /// global metrics recorder fails to install.
    pub async fn run(mut self) -> Result<(), BootExit> {
        let shutdown = self.state.shutdown_token.clone();
        let mut set: JoinSet<TaskExit> = JoinSet::new();
        if let Some(resources) = self.state.bifrost_resources.as_ref() {
            let health = resources.health();
            set.spawn(fallible_task(
                TaskId::Worker("bifrost_resource_health"),
                async move { health.wait_for_poison().await },
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
            .bifrost_ingest
            .as_ref()
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
        if self.config.role == ForgeProcessRole::All {
            let worker = crate::boot::spawn_forge_worker(
                &self.state,
                shutdown.clone(),
                self.config.forge.worker_concurrency,
                self.config.forge.resolved_per_tenant_active_cap(),
            )
            .map_err(|e| BootExit::Other(Box::new(e)))?;
            set.spawn(fallible_task(TaskId::Worker("forge_worker"), worker));
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
        let query = self.state.bifrost_query();
        let ingest = self.state.bifrost_ingest.clone();
        #[cfg(feature = "test-support")]
        let shutdown_probe = self.shutdown_probe.clone();
        let terminal = classify_first_exit_with_shutdown(&mut set, &shutdown).await;
        let deadline = tokio::time::Instant::now() + drain;
        drain_with_shutdown_hooks(set, shutdown, deadline, || {
            let ingest = ingest.clone();
            #[cfg(feature = "test-support")]
            let shutdown_probe = shutdown_probe.clone();
            async move {
                #[cfg(feature = "test-support")]
                if let Some(probe) = shutdown_probe {
                    probe.enter(ShutdownTestPhase::Readiness, deadline).await;
                }
                if let Some(runtime) = query
                    && let Err(error) = runtime.begin_shutdown().await
                {
                    tracing::warn!(%error, "failed to remove Oracle readiness before transport drain");
                }
                if let Some(runtime) = ingest
                    && let Err(error) = runtime.begin_shutdown().await
                {
                    tracing::warn!(%error, "failed to remove Scribe readiness before transport drain");
                }
            }
        }, || {
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
        }).await;

        let deadline = deadline.into_std();
        if shutdown_deadline_active(deadline)
            && let Some(runtime) = self.state.bifrost_query()
        {
            runtime.shutdown_owner(deadline).await;
        } else if let Some(runtime) = self.state.bifrost_query() {
            runtime.abort_shutdown();
        }
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
        if shutdown_deadline_active(deadline)
            && let Some(runtime) = self.state.bifrost_query()
        {
            runtime.shutdown_registry(deadline).await;
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
        if shutdown_deadline_active(deadline)
            && let Some(runtime) = &self.state.bifrost_ingest
        {
            runtime.shutdown_owner(deadline).await;
        } else if let Some(runtime) = &self.state.bifrost_ingest {
            runtime.abort_shutdown();
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
        if shutdown_deadline_active(deadline)
            && let Some(runtime) = &self.state.bifrost_ingest
        {
            runtime.shutdown_registry(deadline).await;
        }

        tracing::info!("wyrd-server shutdown complete");
        match terminal {
            Some(msg) => Err(BootExit::Other(
                Box::<dyn std::error::Error + Send + Sync>::from(msg),
            )),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use tempfile::tempdir;
    use uuid::Uuid;
    use vala_bifrost_redux::cluster::ClusterRegistry;
    use vala_bifrost_redux::oracle::dispatcher::OraclePeerCredentials;
    use vala_bifrost_redux::scribe::{
        ScribeImpl,
        wal::{WalConfig, WalWriter},
    };
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_spec::vala::api::{NodeId as ClusterNodeId, ScribeCapabilitiesV1};
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use super::*;
    use crate::components::auth::ServerAuth;
    use crate::config::WyrdServerConfig;
    use crate::postgres::ServerPostgres;
    use crate::state::BifrostIngestRuntime;

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
            Arc::new(
                crate::auth::permission_resolver::SqlPermissionResolver::new(Arc::new(app_pool)),
            ),
            WyrdAuthVerifySettings::default(),
        ));
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(signer)));
        let redux_catalog = crate::test_support::test_catalog().await;
        let wal_root = tempdir().expect("wal temp dir");
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *Uuid::now_v7().as_bytes(),
                1,
                WalConfig::default(),
            )
            .expect("wal initializes"),
        );
        let scribe = Arc::new(ScribeImpl::new_for_embedded_with_deps(
            Arc::new(storage.operator().clone()),
            wal,
            &Uuid::now_v7().to_string(),
            1,
        ));
        let ingest = Arc::new(BifrostIngestRuntime::new(
            scribe,
            Arc::clone(&redux_catalog),
            Arc::clone(&verifier),
            vala_bifrost_redux::gate::limits::IngestLimits::default(),
            None,
        ));
        let gate = ingest.gate();
        AppState::new(postgres, storage, redux_catalog)
            .with_bifrost_ingest(ingest)
            .with_bifrost_gate(gate)
            .with_auth(ServerAuth {
                issuing_key: Some(issuing_key),
                token_verifier: Some(verifier),
                ..ServerAuth::default()
            })
    }

    /// Builds production-shaped Oracle and Scribe role task ownership for shutdown tests.
    #[cfg(feature = "test-support")]
    async fn test_state_with_role_tasks() -> AppState {
        let mut state = test_state_with_auth().await;
        state.postgres = crate::test_support::test_server_postgres().await;

        let node_id = ClusterNodeId::new(Uuid::now_v7());
        let cluster = Arc::new(ClusterRegistry::new(state.postgres.vala().clone(), node_id));
        let scribe_role = cluster
            .register_scribe(
                "127.0.0.1:0",
                ScribeCapabilitiesV1 {
                    tail_protocol_version: 1,
                },
            )
            .await
            .expect("test Scribe role registers");
        let ingest = Arc::new(
            state
                .bifrost_ingest
                .as_ref()
                .expect("test state owns Scribe")
                .clone_with_scribe_role_for_test(cluster, scribe_role),
        );
        let gate = ingest.gate();
        state = state.with_bifrost_ingest(ingest).with_bifrost_gate(gate);
        let mut oracle_config = crate::config::WyrdServerConfig::default();
        let oracle_signing_key =
            IssuingKey::generate_ephemeral_pem().expect("test Oracle signing key");
        oracle_config.auth.signing_key = Some(oracle_signing_key.clone());
        let (state, credentials): (AppState, Arc<dyn OraclePeerCredentials>) =
            crate::boot::pg_tests::with_test_oracle_peer_credentials(
                state,
                &oracle_config,
                &oracle_signing_key,
            )
            .await;
        crate::boot::attach_test_oracle_runtime_for_node_at_with_credentials(
            state,
            wyrd_spec::vala::api::NodeId::new(Uuid::now_v7()),
            oracle_signing_key,
            "http://127.0.0.1:0".to_owned(),
            credentials,
        )
        .await
        .expect("test Oracle role attaches")
    }

    /// Proves the production server owner does not grant a stalled Scribe a second budget.
    #[cfg(feature = "test-support")]
    #[tokio::test(start_paused = true)]
    async fn bound_server_run_aborts_stalled_scribe_at_original_deadline() {
        tokio::time::resume();
        let mut config = WyrdServerConfig::default();
        config.http.bind = "127.0.0.1:0".parse().expect("static bind is valid");
        config.metrics.enabled = false;
        config.shutdown.drain_ms = 1_000;
        let state = test_state_with_role_tasks().await;
        let query = Arc::clone(state.bifrost_query().expect("test state owns Oracle"));
        let ingest = Arc::clone(
            state
                .bifrost_ingest
                .as_ref()
                .expect("test state owns Scribe role"),
        );
        let scribe = Arc::clone(
            state
                .bifrost_ingest
                .as_ref()
                .expect("test state owns Scribe")
                .scribe(),
        );
        let stalled = scribe.install_shutdown_stall_for_test().await;
        tokio::time::pause();
        let server = WyrdServer::new(config, state)
            .expect("test server builds")
            .spawn_worker("shutdown_trigger", async {});
        let bound = server.bind(ServeMode::Http).await.expect("HTTP binds");
        let started = tokio::time::Instant::now();

        let result = bound.run().await;
        tokio::task::yield_now().await;

        assert!(result.is_err(), "early worker exit remains terminal");
        assert!(
            tokio::time::Instant::now() - started <= Duration::from_millis(1_001),
            "role task timers must not add a second shutdown budget"
        );
        assert!(
            stalled.is_finished(),
            "Scribe retained task must be aborted"
        );
        assert!(!scribe.is_ready(), "Scribe admission must be closed");
        assert_eq!(
            query.role_tasks_finished_for_test(),
            (true, true),
            "Oracle heartbeat and snapshot poller cannot survive server completion"
        );
        assert_eq!(
            ingest.role_tasks_finished_for_test(),
            Some((true, true)),
            "Scribe heartbeat and snapshot poller cannot survive server completion"
        );
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
}
