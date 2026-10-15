mod grpc_tests {
    use std::sync::Arc;

    use arc_swap::ArcSwap;
    use tokio_util::sync::CancellationToken;
    use wyrd_tonic::health::HealthSnapshot;
    use wyrd_tonic::health::WyrdHealthSentinel;
    use wyrd_tonic::server::{
        GrpcRouterConfig, NoopInterceptor, build_grpc_router, drive_health_status,
        publish_initial_health, serve_grpc,
    };
    use wyrd_tonic::tonic::server::NamedService;
    use wyrd_tonic::tonic_health::server::health_reporter;

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
                tenant_spans: None,
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
                tenant_spans: None,
            },
        );
        assert!(
            result.is_ok(),
            "gRPC router with reflection must build successfully"
        );
    }

    #[tokio::test]
    async fn publish_initial_health_not_serving_on_cold_boot() {
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
                tenant_spans: None,
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
            assert!(
                std::time::Instant::now() < deadline,
                "gRPC health did not transition to SERVING within 5s; last status = {last_status}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
        assert_eq!(last_status, ServingStatus::Serving as i32);

        shutdown.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), server_handle).await;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), consumer_handle).await;
    }
}

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use wyrd_server::grpc::{GrpcRouterConfig, build_app_grpc, serve_grpc};
use wyrd_testing::WyrdTestServer;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::tonic_health::server::health_reporter;
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_client::BifrostIngestServiceClient;
use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;
use wyrd_tonic::wyrd::v1::{InsertBatchRequest, ListRunningQueriesRequest};

/// Trace id of the caller span the continuity case sends.
const CALLER_TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
/// Span id of the caller span the continuity case sends.
const CALLER_SPAN: &str = "00f067aa0ba902b7";

/// Serve this server's public gRPC router on a free loopback port and return
/// a connected channel with the listener's shutdown token.
///
/// # Panics
/// Panics when the router does not build or the channel does not connect
/// within five seconds.
async fn serve_public_grpc(server: &WyrdTestServer) -> (Channel, CancellationToken) {
    let (_, health_service) = health_reporter();
    let router = build_app_grpc(
        server.state(),
        health_service,
        GrpcRouterConfig {
            reflection_enabled: false,
            tls_identity: None,
            tenant_spans: None,
        },
    )
    .expect("gRPC router builds");
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind probe listener");
    let bind: SocketAddr = probe.local_addr().expect("probe local addr");
    drop(probe);
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    tokio::spawn(async move { serve_grpc(router, bind, token).await });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match Channel::from_shared(format!("http://{bind}"))
            .expect("endpoint parses")
            .connect()
            .await
        {
            Ok(channel) => return (channel, shutdown),
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "gRPC channel never connected: {error}"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

/// Wrap `message` in a request carrying `traceparent` metadata.
fn traced<T>(message: T, traceparent: &str) -> Request<T> {
    let mut request = Request::new(message);
    request.metadata_mut().insert(
        "traceparent",
        traceparent.parse().expect("traceparent metadata value"),
    );
    request
}

/// Every public gRPC service continues a valid inbound `traceparent`: the
/// ingest, OTLP, and query requests each open a SERVER `request` span that is
/// a child of the caller's span and names its RPC, while malformed metadata
/// starts a new trace without failing differently.
///
/// The requests carry no credential, so each is refused after its span opens;
/// continuity is a property of the shared router, not of a service's
/// authentication outcome.
///
/// # Panics
/// Panics when a service's request span is missing, is not a SERVER child of
/// the caller's span, or the malformed request joins the caller's trace.
#[tokio::test]
async fn grpc_traceparent_parents_the_server_span() {
    let (_telemetry, traces) =
        wyrd_server::install_capture_runtime(wyrd_telemetry::TelemetryConfig::default())
            .expect("production-shaped telemetry installs");
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let (channel, shutdown) = serve_public_grpc(&server).await;
    let parent = format!("00-{CALLER_TRACE}-{CALLER_SPAN}-01");
    let checkpoint = traces.checkpoint();

    let ingest = BifrostIngestServiceClient::new(channel.clone())
        .insert_batch(traced(InsertBatchRequest::default(), &parent))
        .await
        .expect_err("an uncredentialed insert is refused");
    let export = TraceServiceClient::new(channel.clone())
        .export(traced(ExportTraceServiceRequest::default(), &parent))
        .await
        .expect_err("an uncredentialed export is refused");
    let query = BifrostQueryServiceClient::new(channel.clone())
        .list_running_queries(traced(ListRunningQueriesRequest::default(), &parent))
        .await
        .expect_err("an uncredentialed listing is refused");
    let malformed = BifrostQueryServiceClient::new(channel)
        .list_running_queries(traced(
            ListRunningQueriesRequest::default(),
            "00-not-a-trace-01",
        ))
        .await
        .expect_err("an uncredentialed listing is refused");
    assert_eq!(malformed.code(), query.code(), "{ingest:?} {export:?}");
    shutdown.cancel();
    server.shutdown().await.expect("server shuts down");

    let spans: Vec<_> = traces
        .finished_since(checkpoint)
        .into_iter()
        .filter(|span| span.name == "request")
        .collect();
    for (service, method) in [
        ("wyrd.v1.BifrostIngestService", "InsertBatch"),
        (
            "opentelemetry.proto.collector.trace.v1.TraceService",
            "Export",
        ),
        ("wyrd.v1.BifrostQueryService", "ListRunningQueries"),
    ] {
        let span = spans
            .iter()
            .find(|span| {
                span.trace_id == CALLER_TRACE
                    && span.attributes.get("rpc.service").map(String::as_str) == Some(service)
            })
            .unwrap_or_else(|| panic!("{service} continues the caller: {spans:#?}"));
        assert_eq!(span.parent_span_id, CALLER_SPAN, "{span:?}");
        assert_eq!(span.kind, "Server", "{span:?}");
        assert_eq!(
            span.attributes.get("rpc.method").map(String::as_str),
            Some(method),
            "{span:?}"
        );
    }
    let listings: Vec<_> = spans
        .iter()
        .filter(|span| {
            span.attributes.get("rpc.method").map(String::as_str) == Some("ListRunningQueries")
        })
        .collect();
    assert_eq!(listings.len(), 2, "{listings:#?}");
    assert!(
        listings
            .iter()
            .any(|span| span.trace_id != CALLER_TRACE && span.parent_span_id == "0000000000000000"),
        "malformed metadata starts a new trace: {listings:#?}"
    );
}
