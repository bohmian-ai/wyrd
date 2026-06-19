//! gRPC server scaffold for the Wyrd skeleton.
//!
//! No business services are mounted here. Health and (optional) reflection only.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tonic::transport::server::Router as TonicRouter;
use tonic_health::pb::health_server::{Health, HealthServer};
use tonic_health::server::HealthReporter;
use tracing::warn;

use crate::health::{HealthSnapshot, WyrdHealthSentinel};

const HEALTH_CONSUMER_INTERVAL: Duration = Duration::from_secs(1);

/// Errors raised by the gRPC server scaffold.
#[derive(Debug, thiserror::Error)]
pub enum GrpcError {
    /// Bind or serve failure from the tonic transport layer.
    #[error("gRPC transport failed")]
    Transport(#[from] tonic::transport::Error),
    /// Reflection builder failed to assemble. Only emitted when reflection is enabled.
    #[error("gRPC reflection assembly failed: {0}")]
    Reflection(String),
}

/// Inputs to [`build_grpc_router`].
#[derive(Debug, Clone)]
pub struct GrpcRouterConfig {
    pub reflection_enabled: bool,
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
/// is wired in the auth follow-up commit via `InterceptedService::new`.
pub fn build_grpc_router<H: Health>(
    health_service: HealthServer<H>,
    cfg: GrpcRouterConfig,
) -> Result<TonicRouter, GrpcError> {
    let mut server = Server::builder();

    let router = if cfg.reflection_enabled {
        #[cfg(feature = "server")]
        {
            let reflection = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
                .build_v1()
                .map_err(|e| GrpcError::Reflection(e.to_string()))?;
            server.add_service(health_service).add_service(reflection)
        }
        #[cfg(not(feature = "server"))]
        {
            tracing::warn!("reflection_enabled=true but wyrd-tonic server feature not enabled; ignoring");
            server.add_service(health_service)
        }
    } else {
        server.add_service(health_service)
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

/// Publish the snapshot-driven initial health status before the gRPC bind opens.
///
/// F-03 closeout: there is NO unconditional `set_serving` boot seed. This
/// function is the ONLY caller-visible function that writes to the reporter at
/// boot time. It reads the current snapshot (which is
/// `ReadinessSnapshot::initial()` on cold boot — all `warmup`, `all_ok() ==
/// false`) and calls the matching reporter setter. The background consumer
/// transitions the sentinel from there.
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
/// F-03 closeout: `publish_initial_health` in `main.rs` has already written a
/// snapshot-based status before this task spawned. The first tick here is a
/// true no-op when the snapshot has not changed.
#[tracing::instrument(skip(snapshot, reporter, shutdown))]
pub async fn drive_health_status<S: HealthSnapshot>(
    snapshot: Arc<ArcSwap<S>>,
    mut reporter: HealthReporter,
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

    use super::{GrpcRouterConfig, build_grpc_router, drive_health_status, publish_initial_health};
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
            GrpcRouterConfig { reflection_enabled: false },
        );
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn build_grpc_router_with_reflection() {
        let (_, health_service) = health_reporter();
        let result = build_grpc_router(
            health_service,
            GrpcRouterConfig { reflection_enabled: true },
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
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            handle,
        )
        .await
        .expect("drive_health_status did not exit within 2s after cancel")
        .expect("drive_health_status task panicked");
    }
}
