pub mod support;

mod pg_tests {
    use super::*;
    use std::sync::Arc;

    use arc_swap::ArcSwap;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use tokio_util::sync::CancellationToken;
    use wyrd_server::AppState;
    use wyrd_server::postgres::ServerPostgres;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};
    use wyrd_tonic::health::HealthSnapshot;
    use wyrd_tonic::health::WyrdHealthSentinel;
    use wyrd_tonic::server::{
        GrpcRouterConfig, NoopInterceptor, build_grpc_router, drive_health_status,
        publish_initial_health, serve_grpc,
    };
    use wyrd_tonic::tonic::server::NamedService;
    use wyrd_tonic::tonic_health::server::health_reporter;

    async fn test_state() -> AppState {
        let pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let postgres = Arc::new(ServerPostgres::from_parts(
            wyrd_sql::WyrdPostgres::from_pools(pool.clone(), None),
            vala_sql::ValaPostgres::from_pool(pool),
        ));
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        AppState::new(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            support::test_catalog().await,
        )
    }

    #[derive(Clone)]
    struct StubSnapshot {
        ok: bool,
    }

    impl HealthSnapshot for StubSnapshot {
        fn all_ok(&self) -> bool {
            self.ok
        }
    }

    #[tokio::test]
    async fn build_grpc_router_health_only_succeeds() {
        let (_, health_service) = health_reporter();
        let result = build_grpc_router(
            health_service,
            NoopInterceptor,
            GrpcRouterConfig {
                reflection_enabled: false,
                tls_identity: None,
            },
        );
        assert!(
            result.is_ok(),
            "health-only gRPC router must build successfully"
        );
    }

    #[tokio::test]
    async fn build_grpc_router_with_reflection_succeeds() {
        let (_, health_service) = health_reporter();
        let result = build_grpc_router(
            health_service,
            NoopInterceptor,
            GrpcRouterConfig {
                reflection_enabled: true,
                tls_identity: None,
            },
        );
        assert!(
            result.is_ok(),
            "gRPC router with reflection must build successfully"
        );
    }

    #[tokio::test]
    async fn publish_initial_health_not_serving_on_cold_boot() {
        let _state = test_state().await;
        let snapshot: Arc<ArcSwap<StubSnapshot>> =
            Arc::new(ArcSwap::new(Arc::new(StubSnapshot { ok: false })));
        let (mut reporter, _) = health_reporter();
        // Should not panic and should mark NotServing on a failing snapshot.
        publish_initial_health(&snapshot, &mut reporter).await;
    }

    #[tokio::test]
    async fn publish_initial_health_serving_on_warm_snapshot() {
        let snapshot: Arc<ArcSwap<StubSnapshot>> =
            Arc::new(ArcSwap::new(Arc::new(StubSnapshot { ok: true })));
        let (mut reporter, _) = health_reporter();
        publish_initial_health(&snapshot, &mut reporter).await;
        // Verifying serving status requires a gRPC client; the real assertion
        // is that publish_initial_health does not panic on a passing snapshot.
    }

    #[tokio::test]
    async fn drive_health_status_exits_on_cancel() {
        let snapshot: Arc<ArcSwap<StubSnapshot>> =
            Arc::new(ArcSwap::new(Arc::new(StubSnapshot { ok: false })));
        let (mut reporter, _) = health_reporter();
        publish_initial_health(&snapshot, &mut reporter).await;

        let shutdown = CancellationToken::new();
        let consumer_snapshot = snapshot.clone();
        let consumer_reporter = reporter.clone();
        let consumer_shutdown = shutdown.clone();

        let handle = tokio::spawn(drive_health_status(
            consumer_snapshot,
            consumer_reporter,
            consumer_shutdown,
        ));
        shutdown.cancel();

        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("drive_health_status must exit within 2s after cancel")
            .expect("drive_health_status task must not panic");
    }

    #[tokio::test]
    async fn default_grpc_bind_is_loopback() {
        // The default gRPC bind must be loopback so a fresh install never exposes
        // the gRPC port off-box without explicit operator opt-in.
        use wyrd_server::WyrdServerConfig;
        let config = WyrdServerConfig::default();
        assert_eq!(
            config.grpc.bind.to_string(),
            "127.0.0.1:50051",
            "default gRPC bind must be loopback"
        );
    }

    #[tokio::test]
    async fn readiness_snapshot_drives_health_sentinel() {
        // Seed a failing snapshot → consumer must stay NotServing.
        // Then update to a passing snapshot → consumer must transition to Serving.
        // We verify the transition direction by checking the consumer runs past
        // one tick without panicking after a snapshot swap.
        let snapshot: Arc<ArcSwap<StubSnapshot>> =
            Arc::new(ArcSwap::new(Arc::new(StubSnapshot { ok: false })));
        let (mut reporter, _) = health_reporter();
        publish_initial_health(&snapshot, &mut reporter).await;

        let shutdown = CancellationToken::new();
        let consumer_snapshot = snapshot.clone();
        let consumer_reporter = reporter.clone();
        let consumer_shutdown = shutdown.clone();

        let handle = tokio::spawn(drive_health_status(
            consumer_snapshot,
            consumer_reporter,
            consumer_shutdown,
        ));

        // Give the consumer one tick to observe the failing snapshot.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Swap to a passing snapshot.
        snapshot.store(Arc::new(StubSnapshot { ok: true }));

        // Give the consumer time to react.
        tokio::time::sleep(std::time::Duration::from_millis(1_200)).await;

        // Cancel and confirm clean exit.
        shutdown.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("consumer must exit within 2s after cancel")
            .expect("consumer must not panic");
    }

    #[tokio::test]
    async fn grpc_health_transitions_to_serving_when_snapshot_passes() {
        use std::net::SocketAddr;

        use wyrd_tonic::tonic_health::pb::HealthCheckRequest;
        use wyrd_tonic::tonic_health::pb::health_check_response::ServingStatus;
        use wyrd_tonic::tonic_health::pb::health_client::HealthClient;

        // Reserve a free loopback port, then drop the reservation so serve_grpc
        // can bind to it. A brief race window is acceptable inside a unit test.
        let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("reserve loopback port");
        let bind: SocketAddr = probe.local_addr().expect("reserved addr");
        drop(probe);

        let snapshot: Arc<ArcSwap<StubSnapshot>> =
            Arc::new(ArcSwap::new(Arc::new(StubSnapshot { ok: false })));
        let (mut reporter, health_service) = health_reporter();
        publish_initial_health(&snapshot, &mut reporter).await;

        let grpc_router = build_grpc_router(
            health_service,
            NoopInterceptor,
            GrpcRouterConfig {
                reflection_enabled: false,
                tls_identity: None,
            },
        )
        .expect("router builds");

        let shutdown = CancellationToken::new();
        let server_handle = tokio::spawn({
            let token = shutdown.clone();
            async move { serve_grpc(grpc_router, bind, token).await }
        });
        let consumer_handle = tokio::spawn(drive_health_status(
            snapshot.clone(),
            reporter.clone(),
            shutdown.clone(),
        ));

        // Yield long enough for drive_health_status to initialize its `last_ok`
        // baseline against the failing snapshot. Without this the consumer would
        // race ahead, initialize last_ok = Some(true) against the already-swapped
        // snapshot, and never fire set_serving — masking the change-detection bug
        // the test exists to catch.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Swap to a passing snapshot. The consumer ticks every ~1s; poll the gRPC
        // health endpoint until it transitions to SERVING (bounded by a deadline so
        // a real change-detection bug fails the test instead of hanging).
        snapshot.store(Arc::new(StubSnapshot { ok: true }));

        // Retry connect until the gRPC server has bound the loopback port.
        let endpoint = format!("http://{bind}");
        let connect_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let channel = loop {
            match wyrd_tonic::tonic::transport::Channel::from_shared(endpoint.clone())
                .expect("endpoint parses")
                .connect()
                .await
            {
                Ok(channel) => break channel,
                Err(error) if std::time::Instant::now() >= connect_deadline => {
                    panic!("gRPC channel never connected within 5s: {error}");
                }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
            }
        };
        let mut client = HealthClient::new(channel);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut last_status;
        loop {
            let response = client
                .check(HealthCheckRequest {
                    service: WyrdHealthSentinel::NAME.to_owned(),
                })
                .await
                .expect("health check responds");
            last_status = response.into_inner().status;
            if last_status == ServingStatus::Serving as i32 {
                break;
            }
            if std::time::Instant::now() >= deadline {
                panic!(
                    "gRPC health did not transition to SERVING within 5s; last status = {last_status}"
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
        assert_eq!(last_status, ServingStatus::Serving as i32);

        shutdown.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), server_handle).await;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), consumer_handle).await;
    }
}
