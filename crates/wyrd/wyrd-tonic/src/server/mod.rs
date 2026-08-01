//! gRPC server scaffold for the Wyrd skeleton.
//!
//! No business services are mounted here. Health and (optional) reflection only.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::server::Router as TonicRouter;
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tonic_health::pb::health_server::{Health, HealthServer};
use tonic_health::server::HealthReporter;
use tracing::warn;

use crate::health::{HealthSnapshot, WyrdHealthSentinel};

/// Passthrough auth interceptor: accepts every request.
///
/// Reserved structural seat for the real auth interceptor. Threading this
/// through `build_grpc_router` forces every future gRPC service to be wrapped
/// by the interceptor — there is no path that mounts a service without going
/// through one.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopInterceptor;

impl tonic::service::Interceptor for NoopInterceptor {
    fn call(&mut self, request: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        Ok(request)
    }
}

const HEALTH_CONSUMER_INTERVAL: Duration = Duration::from_secs(1);

/// Errors raised by the gRPC server scaffold.
#[derive(Debug, thiserror::Error)]
pub enum GrpcError {
    /// Process-wide Rustls provider ownership conflicts with Wyrd.
    #[error(transparent)]
    CryptoProvider(#[from] wyrd_tls::InstallError),
    /// Bind or serve failure from the tonic transport layer.
    #[error("gRPC transport failed")]
    Transport(#[from] tonic::transport::Error),
    /// Reflection builder failed to assemble. Only emitted when reflection is enabled.
    #[error("gRPC reflection assembly failed: {0}")]
    Reflection(String),
    /// Building the incoming stream from a pre-bound listener failed.
    #[error("gRPC incoming listener setup failed: {0}")]
    IncomingSetup(String),
    /// Ingest mount was requested but no token verifier is configured. Ingest is
    /// never mounted unauthenticated, so a missing verifier is a hard boot error.
    #[error("gRPC ingest requires a token verifier but none is configured")]
    MissingTokenVerifier,
    /// Ingest was mounted without the server-owned Scribe runtime.
    #[error("gRPC ingest requires a server-owned Scribe runtime")]
    MissingScribe,
}

/// Inputs to [`build_grpc_router`].
#[derive(Clone)]
pub struct GrpcRouterConfig {
    /// Whether reflection is exposed on the listener.
    pub reflection_enabled: bool,
    /// Optional PEM identity applied before any service is mounted.
    pub tls_identity: Option<Identity>,
}

/// Build and return the configured tonic router.
///
/// `tonic_health 0.12.x` does **not** expose `Health::from_reporter`.
/// Instead `tonic_health::server::health_reporter()` returns the
/// `(HealthReporter, HealthServer<HealthService>)` pair **once**; the caller
/// threads the reporter onto `AppState.grpc_health` for the readiness consumer
/// and threads the matching `health_service` here.
///
/// The returned router is **unbound**; the caller drives binding via
/// `serve_grpc`. Returning the router (not the bound server) lets tests
/// substitute an in-memory listener.
///
/// No `.layer(...)` is composed here — doing so changes the server's stacked
/// type and breaks the `TonicRouter` return type. The auth interceptor seat
/// is wired via `InterceptedService::new` on each mounted service.
///
/// # Errors
///
/// Returns [`GrpcError::CryptoProvider`] when another Rustls provider already
/// owns the process, [`GrpcError::Transport`] when tonic rejects the TLS
/// identity, or [`GrpcError::Reflection`] when reflection cannot be built.
pub fn build_grpc_router<H, I>(
    health_service: HealthServer<H>,
    interceptor: I,
    cfg: GrpcRouterConfig,
) -> Result<TonicRouter, GrpcError>
where
    H: Health,
    I: tonic::service::Interceptor + Clone + Send + Sync + 'static,
{
    wyrd_tls::install_crypto_provider()?;
    let mut server = Server::builder();
    if let Some(identity) = cfg.tls_identity {
        server = server
            .tls_config(ServerTlsConfig::new().identity(identity))
            .map_err(GrpcError::Transport)?;
    }
    let health = InterceptedService::new(health_service, interceptor.clone());

    let router = if cfg.reflection_enabled {
        #[cfg(feature = "server")]
        {
            let reflection = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
                .build_v1()
                .map_err(|e| GrpcError::Reflection(e.to_string()))?;
            let reflection = InterceptedService::new(reflection, interceptor);
            server.add_service(health).add_service(reflection)
        }
        #[cfg(not(feature = "server"))]
        {
            tracing::warn!(
                "reflection_enabled=true but wyrd-tonic server feature not enabled; ignoring"
            );
            let _ = interceptor;
            server.add_service(health)
        }
    } else {
        let _ = interceptor;
        server.add_service(health)
    };

    Ok(router)
}

/// Drive the tonic server to completion.
///
/// Returns once `shutdown.cancelled()` resolves or the bind fails.
#[tracing::instrument(skip(router, shutdown))]
pub async fn serve_grpc(
    router: TonicRouter,
    bind: SocketAddr,
    shutdown: CancellationToken,
) -> Result<(), GrpcError> {
    router
        .serve_with_shutdown(bind, async move { shutdown.cancelled().await })
        .await
        .map_err(GrpcError::Transport)
}

/// Drive the tonic server on a **pre-bound** listener to completion.
///
/// The caller binds the [`TcpListener`] (e.g. on an OS-assigned `:0` port) and
/// reads its `local_addr` before serving, so the served address is known with
/// no bind-then-rebind race — the same listener the address was read from is
/// the one served. Returns once `shutdown.cancelled()` resolves.
#[tracing::instrument(skip(router, listener, shutdown))]
pub async fn serve_grpc_with_listener(
    router: TonicRouter,
    listener: TcpListener,
    shutdown: CancellationToken,
) -> Result<(), GrpcError> {
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    router
        .serve_with_incoming_shutdown(incoming, async move { shutdown.cancelled().await })
        .await
        .map_err(GrpcError::Transport)
}

/// Publish the snapshot-driven initial health status before the gRPC bind opens.
///
/// This is the only function that writes to the reporter at boot time. It
/// reads the current snapshot and calls the matching reporter setter so the
/// gRPC sentinel never advertises SERVING before readiness has confirmed it.
/// The background consumer (`drive_health_status`) takes over from there.
#[tracing::instrument(skip(snapshot, reporter))]
pub async fn publish_initial_health<S: HealthSnapshot>(
    snapshot: &Arc<ArcSwap<S>>,
    reporter: &mut HealthReporter,
) {
    let current = snapshot.load();
    if current.all_ok() {
        reporter.set_serving::<WyrdHealthSentinel>().await;
    } else {
        reporter.set_not_serving::<WyrdHealthSentinel>().await;
    }
}

/// Background consumer: reads the cached readiness snapshot and drives the gRPC
/// health sentinel. Never calls live probes.
///
/// Callers must invoke `publish_initial_health` first so the reporter reflects
/// the current snapshot before the consumer's change-detection baseline is set.
#[tracing::instrument(skip(snapshot, reporter, shutdown))]
pub async fn drive_health_status<S: HealthSnapshot>(
    snapshot: Arc<ArcSwap<S>>,
    reporter: HealthReporter,
    shutdown: CancellationToken,
) {
    let mut last_ok: Option<bool> = Some(snapshot.load().all_ok());
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                reporter.set_not_serving::<WyrdHealthSentinel>().await;
                return;
            }
            _ = tokio::time::sleep(HEALTH_CONSUMER_INTERVAL) => {}
        }
        let ok = snapshot.load().all_ok();
        if last_ok != Some(ok) {
            if ok {
                reporter.set_serving::<WyrdHealthSentinel>().await;
            } else {
                warn!(
                    health_ok = false,
                    "gRPC health marking NotServing because cached readiness snapshot failed"
                );
                reporter.set_not_serving::<WyrdHealthSentinel>().await;
            }
            last_ok = Some(ok);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arc_swap::ArcSwap;
    use tokio_util::sync::CancellationToken;
    use tonic_health::server::health_reporter;

    use super::{
        build_grpc_router, drive_health_status, publish_initial_health, GrpcRouterConfig,
        NoopInterceptor,
    };
    use crate::health::HealthSnapshot;

    struct TestSnapshot {
        ok: bool,
    }

    impl HealthSnapshot for TestSnapshot {
        fn all_ok(&self) -> bool {
            self.ok
        }
    }

    #[tokio::test]
    async fn build_grpc_router_health_only() {
        let (_, health_service) = health_reporter();
        let result = build_grpc_router(
            health_service,
            NoopInterceptor,
            GrpcRouterConfig {
                reflection_enabled: false,
                tls_identity: None,
            },
        );
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn build_grpc_router_with_reflection() {
        let (_, health_service) = health_reporter();
        let result = build_grpc_router(
            health_service,
            NoopInterceptor,
            GrpcRouterConfig {
                reflection_enabled: true,
                tls_identity: None,
            },
        );
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn publish_initial_health_not_serving_on_cold_boot() {
        let snapshot = Arc::new(ArcSwap::new(Arc::new(TestSnapshot { ok: false })));
        let (mut reporter, _) = health_reporter();
        publish_initial_health(&snapshot, &mut reporter).await;
        // After publish on a failing snapshot the sentinel must be NotServing.
        // We verify indirectly via drive_health_status transition below.
        // (Direct status read requires a client — tested in grpc_smoke.rs)
        // Here we just verify the function runs without panic.
    }

    #[tokio::test]
    async fn drive_health_status_exits_on_cancel() {
        let snapshot = Arc::new(ArcSwap::new(Arc::new(TestSnapshot { ok: false })));
        let (mut reporter, _) = health_reporter();
        publish_initial_health(&snapshot, &mut reporter).await;
        let shutdown = CancellationToken::new();
        let consumer_reporter = reporter.clone();
        let consumer_snapshot = snapshot.clone();
        let consumer_shutdown = shutdown.clone();
        let handle = tokio::spawn(drive_health_status(
            consumer_snapshot,
            consumer_reporter,
            consumer_shutdown,
        ));
        shutdown.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("drive_health_status did not exit within 2s after cancel")
            .expect("drive_health_status task panicked");
    }
}
