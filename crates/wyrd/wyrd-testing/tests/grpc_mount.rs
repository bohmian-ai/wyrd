//! S3.C2c1 — the ingest gRPC service is mounted on a dedicated harness port via
//! the shared `build_app_grpc`, with auth completed in the handler.
//!
//! These tests exercise the auth seam without a committed write: an
//! unauthenticated stream is rejected, a valid token passes in-handler auth, and
//! `build_app_grpc` hard-errors when no verifier is configured. A full authed
//! round-trip is out of scope here (it needs a registered card scope; that lands
//! with the higher-level ingest journeys).

use std::time::{Duration, Instant};

use wyrd_server::grpc::{GrpcError, GrpcRouterConfig, build_app_grpc};
use wyrd_testing::{Bootstrap, WyrdTestServer};
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::tonic::{Code, Request};
use wyrd_tonic::tonic_health::server::health_reporter;
use wyrd_tonic::wyrd::v1::InsertBatchRequest;
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_client::BifrostIngestServiceClient;

async fn connect(url: &str) -> BifrostIngestServiceClient<Channel> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match Channel::from_shared(url.to_owned())
            .expect("endpoint parses")
            .connect()
            .await
        {
            Ok(channel) => return BifrostIngestServiceClient::new(channel),
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "gRPC channel never connected within 5s: {error}"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

fn empty_stream() -> tokio_stream::Iter<std::vec::IntoIter<InsertBatchRequest>> {
    tokio_stream::iter(Vec::<InsertBatchRequest>::new())
}

#[tokio::test]
async fn grpc_mount_binds_distinct_grpc_port() {
    let srv = WyrdTestServer::start_bound().await.expect("bound server");
    let base = srv.base_url().expect("base url").to_owned();
    let grpc = srv.grpc_url().expect("grpc url");
    assert_ne!(
        base, grpc,
        "gRPC must bind its own port distinct from the HTTP base_url"
    );
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn grpc_mount_unauthenticated_stream_rejected() {
    let srv = WyrdTestServer::start_bound().await.expect("bound server");
    let grpc = srv.grpc_url().expect("grpc url");
    let mut client = connect(&grpc).await;

    let status = client
        .insert_batch(Request::new(empty_stream()))
        .await
        .expect_err("unauthenticated ingest must be rejected");
    assert_eq!(
        status.code(),
        Code::Unauthenticated,
        "no token must yield Unauthenticated; got {status:?}"
    );

    srv.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn grpc_mount_valid_token_passes_in_handler_auth() {
    let srv = WyrdTestServer::start_bound().await.expect("bound server");
    let jwt = match srv
        .bootstrap_user("ingest-user", &[])
        .await
        .expect("bootstrap user")
    {
        Bootstrap::User { jwt, .. } => jwt,
        other => panic!("expected user bootstrap, got {other:?}"),
    };

    let grpc = srv.grpc_url().expect("grpc url");
    let mut client = connect(&grpc).await;

    let mut request = Request::new(empty_stream());
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {jwt}").parse().expect("metadata value"),
    );

    // Auth completes as the first step inside `insert_batch`. A valid token must
    // never yield Unauthenticated; the empty stream then fails downstream with a
    // different status (or succeeds trivially), proving auth passed in-handler.
    if let Err(status) = client.insert_batch(request).await {
        assert_ne!(
            status.code(),
            Code::Unauthenticated,
            "valid token must pass in-handler auth; got {status:?}"
        );
    }

    srv.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn grpc_mount_missing_verifier_hard_errors() {
    let srv = WyrdTestServer::start_bound().await.expect("bound server");

    let mut state = srv.state().clone();
    state.token_verifier = None;
    let (_reporter, health_service) = health_reporter();

    let result = build_app_grpc(
        &state,
        health_service,
        GrpcRouterConfig {
            reflection_enabled: false,
        },
    );
    assert!(
        matches!(result, Err(GrpcError::MissingTokenVerifier)),
        "build_app_grpc must hard-error when token_verifier is None"
    );

    srv.shutdown().await.expect("shutdown");
}
