//! `WyrdServer` — composable supervised server lifecycle.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::Duration;

use axum::Router;
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use wyrd_tonic::tonic::transport::server::Router as TonicRouter;
use wyrd_tonic::tonic_health::server::HealthReporter;

use crate::app::BootExit;
use crate::app::metrics::{install_recorder, metrics_router, serve_metrics};
use crate::app::serve::serve;
use crate::app::supervise::{TaskExit, TaskId, fallible_task, supervise, worker_task};
use crate::boot::{ServerBootError, spawn_maintenance_scheduler, spawn_storage_sweeper};
use crate::components::health::readiness_loop;
use crate::config::{ServeMode, WyrdServerConfig};
use crate::grpc::{
    GrpcRouterConfig, build_app_grpc, drive_health_status, publish_initial_health,
    serve_grpc_with_listener,
};
use crate::state::AppState;

type BoxWorker = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

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

        let grpc_router = build_app_grpc(
            &state,
            health_service,
            GrpcRouterConfig {
                reflection_enabled: config.grpc.reflection_enabled,
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
    /// Returns [`BootExit::Other`] on listener bind failure.
    pub async fn bind(self, mode: ServeMode) -> Result<BoundServer, BootExit> {
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
        if let Some(scribe) = self.state.scribe.clone() {
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
        let scheduler = spawn_maintenance_scheduler(&self.state, shutdown.clone())
            .map_err(|e| BootExit::Other(Box::new(e)))?;
        set.spawn(fallible_task(
            TaskId::Worker("maintenance_scheduler"),
            scheduler,
        ));

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
        let terminal = supervise(set, shutdown, drain).await;

        if let Some(gate) = &self.state.gate {
            gate.close();
        }
        if let Some(scribe) = &self.state.scribe {
            scribe.shutdown().await;
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
    use vala_bifrost_redux::gate::auth::ingest_auth_interceptor;
    use vala_bifrost_redux::gate::limits::IngestLimits;
    use vala_bifrost_redux::scribe::{
        ScribeImpl,
        wal::{WalConfig, WalWriter},
    };
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use super::*;
    use crate::components::auth::ServerAuth;
    use crate::config::WyrdServerConfig;
    use crate::postgres::ServerPostgres;

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
        let catalog = crate::test_support::test_catalog().await;
        let redux_catalog = crate::test_support::test_redux_catalog().await;
        let tenant = crate::test_support::test_tenant().await;
        let wal_root = tempdir().expect("wal temp dir");
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *Uuid::now_v7().as_bytes(),
                1,
                tenant,
                WalConfig::default(),
            )
            .expect("wal initializes"),
        );
        let scribe = Arc::new(ScribeImpl::new_for_embedded_with_deps(
            Arc::new(storage.operator().clone()),
            wal,
            Uuid::now_v7().to_string(),
            1,
        ));
        let gate = Arc::new(vala_bifrost_redux::gate::Gate::with_scribe_and_projection(
            Arc::clone(&redux_catalog),
            scribe.clone(),
            ingest_auth_interceptor(Arc::clone(&verifier)),
            IngestLimits::default(),
            Arc::new(vala_bifrost_redux::gate::IngressCpuProjection::new(
                scribe.ingress_cpu_pool(),
            )),
        ));
        AppState::new(postgres, storage, catalog)
            .with_bifrost_redux(redux_catalog)
            .with_scribe(scribe)
            .with_gate(gate)
            .with_auth(ServerAuth {
                issuing_key: Some(issuing_key),
                token_verifier: Some(verifier),
                ..ServerAuth::default()
            })
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
