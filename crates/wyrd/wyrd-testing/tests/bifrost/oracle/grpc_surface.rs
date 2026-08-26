//! Oracle journeys — The public gRPC query surface: frame parity with HTTP, unavailability
//! without Oracle, and resource release on client drop.
//!
//! Module of the `oracle` binary; see `oracle.rs` for the capability it
//! proves and `support.rs` for the fixtures it shares.

use vala_sdk::{CollectedQueryLimits, QueryClient};
use wyrd_client::WyrdClient;
use wyrd_server::config::BifrostRuntimeRole;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
use wyrd_tonic::frame_codec::FrameDecoder;
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::wyrd::v1 as proto;
use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;

use crate::support::*;

/// Proves public gRPC query frames match the HTTP stream for one seeded table.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn public_grpc_matches_http_frames() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("gRPC parity cluster");
    let server = cluster.server(0).expect("gRPC parity server");
    let table = unique_table("oracle_grpc_parity");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("register parity table");
    let client = client(server, "grpc-parity").await.expect("parity client");
    ingest(&client, &format!("vala.bifrost.{table}"), &[11, 22])
        .await
        .expect("parity ingest");
    server.flush_bifrost().await.expect("parity flush");
    let request = BifrostQueryRequest {
        sql: format!("SELECT id, value FROM vala.bifrost.{table} ORDER BY id"),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: None,
    };
    let http = http_query_frames(server.base_url().expect("HTTP URL"), &client, &request)
        .await
        .expect("HTTP frames");
    let grpc = grpc_query_frames(&client, &request)
        .await
        .expect("gRPC frames");
    assert_eq!(http, grpc, "HTTP and gRPC logical frames must be identical");
    let terminal = http
        .iter()
        .find_map(|frame| match frame.frame.as_ref() {
            Some(proto::query_stream_frame::Frame::Terminal(terminal)) => Some(terminal),
            _ => None,
        })
        .expect("parity terminal");
    assert_eq!(terminal.row_count, 2);
    assert_eq!(
        terminal.outcome,
        proto::QueryTerminalOutcome::Success as i32
    );
    assert_eq!(terminal.source_completion.len(), 2);
    cluster.shutdown().await.expect("parity shutdown");
}

/// Rejects the retired component-partial process topology.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn public_grpc_without_oracle_is_unavailable() {
    let mut spec = BifrostClusterSpec::one_mixed();
    spec.nodes = vec![wyrd_testing::bifrost::BifrostNodeSpec {
        node_id: wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7()),
        roles: [
            BifrostRuntimeRole::Scribe,
            BifrostRuntimeRole::ForgeCoordinator,
        ]
        .into_iter()
        .collect(),
        oracle: None,
        role_timing: None,
    }];
    assert!(WyrdTestCluster::start_spec(spec).await.is_err());
}

/// Proves production shutdown drains a dropped stream's queued durable release.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn public_grpc_drop_releases_query_resources() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("drop cluster");
    let server = cluster.server(0).expect("drop server");
    let table = unique_table("oracle_grpc_drop");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("register drop table");
    let client = client(server, "grpc-drop").await.expect("drop client");
    ingest(&client, &format!("vala.bifrost.{table}"), &[1, 2])
        .await
        .expect("drop ingest");
    server.flush_bifrost().await.expect("drop flush");
    let request = BifrostQueryRequest {
        sql: format!("SELECT id, value FROM vala.bifrost.{table} ORDER BY id"),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: None,
    };
    server.stall_next_query_after_schema();
    let query = QueryClient::new(&client);
    let query_task = tokio::spawn(async move {
        query
            .collect_bounded(
                &request,
                CollectedQueryLimits {
                    max_rows: 16,
                    max_encoded_bytes: 1024 * 1024,
                },
            )
            .await
    });
    let _query_id = server
        .wait_query_schema_stall()
        .await
        .expect("query reaches schema stall");
    query_task.abort();
    let _ = query_task.await;
    let _ = cluster.shutdown_and_inspect().await.expect("drop shutdown");
}

/// Collect one HTTP query response into canonical protobuf frames.
///
/// # Errors
///
/// Returns a transport, status, framing, or protobuf error when the HTTP
/// stream cannot be decoded to the public frame contract.
async fn http_query_frames(
    base_url: &str,
    client: &WyrdClient,
    request: &BifrostQueryRequest,
) -> Result<Vec<proto::QueryStreamFrame>, JourneyError> {
    let bearer = client.auth().bearer().await?;
    let base_url = base_url.trim_end_matches('/');
    let response = reqwest::Client::new()
        .post(format!("{base_url}/v1/query"))
        .header("x-wyrd-access-token", format!("Bearer {}", bearer.expose()))
        .json(request)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!("HTTP query failed with {}", response.status()).into());
    }
    let body = response.bytes().await?;
    let mut decoder = FrameDecoder::new(32 * 1024 * 1024);
    let frames = decoder.push::<proto::QueryStreamFrame>(&body)?;
    decoder.finish()?;
    Ok(frames)
}

/// Open one authenticated public gRPC query stream.
///
/// # Errors
///
/// Returns a connection, metadata, request validation, or typed tonic status
/// error before the first stream frame.
async fn grpc_query_stream(
    client: &WyrdClient,
    request: &BifrostQueryRequest,
) -> Result<wyrd_tonic::tonic::codec::Streaming<proto::QueryStreamFrame>, JourneyError> {
    let connection = client.connect_grpc().await?;
    let bearer = connection.auth().bearer().await?;
    let mut rpc = BifrostQueryServiceClient::new(connection.channel());
    let mut rpc_request =
        wyrd_tonic::tonic::Request::new(proto::BifrostQueryRequest::from(request.clone()));
    rpc_request.metadata_mut().insert(
        "x-wyrd-access-token",
        MetadataValue::try_from(format!("Bearer {}", bearer.expose()))?,
    );
    Ok(rpc.query(rpc_request).await?.into_inner())
}

/// Collect one public gRPC query response into canonical protobuf frames.
///
/// # Errors
///
/// Returns a connection, typed tonic status, or stream decode error.
async fn grpc_query_frames(
    client: &WyrdClient,
    request: &BifrostQueryRequest,
) -> Result<Vec<proto::QueryStreamFrame>, JourneyError> {
    let mut stream = grpc_query_stream(client, request).await?;
    let mut frames = Vec::new();
    while let Some(frame) = stream.message().await? {
        frames.push(frame);
    }
    Ok(frames)
}
