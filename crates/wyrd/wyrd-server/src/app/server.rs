//! `WyrdServer` — composable supervised server lifecycle.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use axum::Router;
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use wyrd_tonic::tonic::transport::server::Router as TonicRouter;
use wyrd_tonic::tonic_health::server::HealthReporter;

use crate::app::metrics::{install_recorder, metrics_router, serve_metrics};
use crate::app::serve::serve;
use crate::app::supervise::{TaskExit, TaskId, fallible_task, supervise, worker_task};
use crate::app::BootExit;
use crate::boot::{ServerBootError, spawn_storage_sweeper};
use crate::components::health::readiness_loop;
use crate::config::{ServeMode, WyrdServerConfig};
use crate::grpc::{
    GrpcRouterConfig, build_app_grpc, drive_health_status, publish_initial_health, serve_grpc,
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
    extra_workers: Vec<BoxWorker>,
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
        let (reporter, health_service) = wyrd_tonic::tonic_health::server::health_reporter();
        // Store this reporter in state so readiness drives THIS health service.
        let state = state.with_grpc_health(reporter.clone());

        let grpc_router = build_app_grpc(
            &state,
            health_service,
            GrpcRouterConfig { reflection_enabled: config.grpc.reflection_enabled },
        )?;
        let http_router = crate::http::build_router(state.clone());

        Ok(Self { config, state, http_router, grpc_router, reporter, extra_workers: Vec::new() })
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
        let protected =
            crate::http::router::apply_protected_edge(with_auth, &self.state);
        self.http_router = self.http_router.merge(protected);
        self
    }

    /// Transform the gRPC router — typically `|r| r.add_service(svc)`.
    #[must_use]
    pub fn with_grpc(mut self, f: impl FnOnce(TonicRouter) -> TonicRouter) -> Self {
        self.grpc_router = f(self.grpc_router);
        self
    }

    /// Register an additional background worker. It shares the server shutdown
    /// token via `self.state.shutdown_token`; the worker must observe it.
    #[must_use]
    pub fn spawn_worker<F>(mut self, worker: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.extra_workers.push(Box::pin(worker));
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
    #[must_use]
    pub fn into_http_router(self) -> Router {
        self.http_router
    }

    /// Bind listeners for `mode`, spawn all tasks, and drive the supervised
    /// lifecycle to completion.
    ///
    /// # Errors
    /// Returns [`BootExit::Other`] on listener bind failure or a terminal task
    /// error.
    pub async fn serve(mut self, mode: ServeMode) -> Result<(), BootExit> {
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
        if let Some(handle) = spawn_storage_sweeper(&self.state, shutdown.clone())
            .map_err(|e| BootExit::Other(Box::new(e)))?
        {
            set.spawn(worker_task(TaskId::Worker("storage_sweeper"), async move {
                if let Err(join_error) = handle.await {
                    if join_error.is_panic() {
                        std::panic::resume_unwind(join_error.into_panic());
                    }
                }
            }));
        }

        // Enterprise workers.
        for worker in self.extra_workers.drain(..) {
            set.spawn(worker_task(TaskId::Worker("custom"), worker));
        }

        // Transports (bind up front so bind errors are boot errors, not task errors).
        if mode.serves_http() {
            let listener = TcpListener::bind(self.config.http.bind)
                .await
                .map_err(|e| BootExit::Other(Box::new(e)))?;
            tracing::info!(bind = %self.config.http.bind, "HTTP server listening");
            set.spawn(fallible_task(
                TaskId::Http,
                serve(self.http_router, listener, shutdown.clone()),
            ));
        }
        if mode.serves_grpc() {
            let bind = self.config.grpc.bind;
            let router = self.grpc_router;
            let token = shutdown.clone();
            set.spawn(fallible_task(
                TaskId::Grpc,
                async move { serve_grpc(router, bind, token).await },
            ));
        }
        // Metrics is orthogonal to `mode`: it has its own listener and runs
        // whenever `metrics.enabled`, including in `ServeMode::Grpc` (HTTP-off).
        // The recorder is a process-global singleton installed here (once, at
        // serve, only when enabled) rather than in `new` — a constructed but
        // never-served server never touches the global recorder.
        if self.config.metrics.enabled {
            let handle = install_recorder().map_err(|e| BootExit::Other(Box::new(e)))?;
            let bind = self.config.metrics.resolved_bind(self.config.http.bind);
            let listener =
                TcpListener::bind(bind).await.map_err(|e| BootExit::Other(Box::new(e)))?;
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

        tracing::info!("wyrd-server shutdown complete");
        match terminal {
            Some(msg) => Err(BootExit::Other(Box::<dyn std::error::Error + Send + Sync>::from(msg))),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
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

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    async fn test_state_with_auth() -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let postgres = Arc::new(ServerPostgres::lazy_for_tests(app_pool.clone()));
        let root = tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let issuing_key = Arc::new(
            IssuingKey::from_ed_pem(
                secrecy::SecretString::from(PRIVATE_KEY_PEM),
                Kid::new("k1").expect("kid is valid"),
                "wyrd",
            )
            .expect("test issuing key loads"),
        );
        let mut keys = HashMap::new();
        keys.insert(
            Kid::new("k1").expect("kid is valid"),
            Arc::new(public_key_from_pem(PUBLIC_KEY_PEM).expect("public key loads")),
        );
        let verifier = Arc::new(TokenVerifier::new(
            keys,
            "wyrd",
            Arc::new(crate::auth::permission_resolver::SqlPermissionResolver::new(Arc::new(
                app_pool,
            ))),
            WyrdAuthVerifySettings::default(),
        ));
        AppState::new(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
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
        let server1 =
            WyrdServer::new(config.clone(), state1).expect("first WyrdServer construction succeeds");

        let state2 = test_state_with_auth().await;
        let server2 =
            WyrdServer::new(config, state2).expect("second WyrdServer construction succeeds");

        // Neither construction touched the global recorder — new() is side-effect-free.
        // Drop both without calling serve() to confirm no recorder was installed.
        drop(server1);
        drop(server2);
    }
}
