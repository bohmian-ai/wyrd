//! The Scribe write/flush/read user journey.

use std::sync::Arc;

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::query::ValaSdkError;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryTerminalErrorCode, VisibilityMode};
use wyrd_testing::bifrost::{
    BifrostClusterSpec, ScribeCacheMode, ScribeProductionWorkloadV1, WyrdTestCluster,
};

use super::support::{register_table, start_scribe_server, unique_table};

/// The complete client-to-server Scribe contract, end to end.
///
/// This is the whole path a real caller takes, in the order a real caller takes
/// it: public registration, public append with a durable acknowledgment, the
/// freeze and the publication that move those rows between authoritative
/// sources, a strict public read at each of them, a replayed identical batch
/// that must not duplicate a row, a pod restart that must recover the same rows
/// under the same identities, and a terminal drain that must leak no ownership.
///
/// The record drives all of it and the comparator judges all of it. Everything
/// this journey asserts is a normative field of the canonical record, so a
/// consumer that runs the same bytes is held to the same contract; a hand
/// written epilogue here would be a second, weaker contract that only this file
/// enforces.
///
/// The record is serialized once, hashed, and re-read from those exact bytes,
/// so a field that only exists in this process cannot become part of the
/// contract and the bytes Forge and the cache task later run are provably the
/// bytes this journey ran.
///
/// The pod runs inside a one-node cluster because the last two boundaries need
/// a pod that can be stopped and started again on its retained roots, and the
/// runner takes the cluster because the record's terminal operation destroys
/// it.
///
/// # Panics
///
/// Panics when the record does not survive its wire form, or when the run does
/// not satisfy every normative field of the record: its six lifecycle
/// boundaries in order, its acknowledged-row counts, its read-back digest, the
/// publication each boundary is defined by, the recovery evidence at the
/// restart boundary, and the drain evidence at the terminal one.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_write_flush_read_user_journey() {
    let canonical = ScribeProductionWorkloadV1::canonical();
    let bytes = serde_json::to_vec(&canonical).expect("the canonical record serializes");
    let digest = wyrd_testing::bifrost::scribe_workload_digest(&bytes);
    let workload: ScribeProductionWorkloadV1 =
        serde_json::from_slice(&bytes).expect("the canonical record deserializes");
    assert_eq!(workload, canonical, "the handoff must be lossless");
    assert_eq!(
        wyrd_testing::bifrost::scribe_workload_digest(
            &serde_json::to_vec(&workload).expect("the round-tripped record re-serializes")
        ),
        digest,
        "the record's wire bytes must be stable across the round trip that consumers repeat"
    );

    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("the one-pod mixed cluster starts");
    assert_empty_table_reads_cleanly(cluster.server(0).expect("the mixed pod is running")).await;

    let run = cluster
        .run_scribe_production_workload(&workload, ScribeCacheMode::Disabled)
        .await
        .expect("the production workload runs on public routes");
    run.evidence
        .assert_matches(&workload, ScribeCacheMode::Disabled)
        .expect("the run satisfies every normative field of the record");
}

/// Asserts an unwritten table's strict read reaches a clean empty terminal.
async fn assert_empty_table_reads_cleanly(server: &wyrd_testing::WyrdTestServer) {
    let tenant = server.data_tenant_id();
    let table = register_table(
        server,
        tenant,
        BifrostNamespace::Datasets,
        &unique_table("empty_query"),
    )
    .await;
    let client = tenant_client(server, tenant).await;
    let mut empty = vala_sdk::query::QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT value FROM {table}"),
            visibility: VisibilityMode::Fused,
            freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            deadline_ms: Some(60_000),
        })
        .await
        .expect("empty public query starts");
    assert!(
        empty
            .next_batch()
            .await
            .expect("empty public query reaches its successful terminal")
            .is_none(),
        "empty logical results must not emit zero-row batch frames"
    );
}

/// An undialable ready Scribe peer fails a strict fused read with its typed 503.
///
/// This drives the public SDK against an active, unflushed generation so Oracle
/// must use the private Scribe RPC. The test then replaces only the durable
/// membership address with a concrete closed loopback endpoint and refreshes
/// the production registry snapshot. No successful terminal may be returned.
///
/// # Panics
///
/// Panics when setup fails, membership advertises `:0`, the private endpoint is
/// not initially reachable, or the public SDK flattens the terminal failure.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_undialable_private_peer_returns_typed_visibility_failure() {
    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let table = register_table(
        &server,
        tenant,
        BifrostNamespace::Datasets,
        &unique_table("undialable_peer"),
    )
    .await;
    let client = tenant_client(&server, tenant).await;
    append_active_row(&client, &table).await;

    let cluster = server
        .state()
        .oracle_cluster()
        .expect("mixed server owns the Oracle cluster registry");
    cluster
        .refresh_snapshot()
        .await
        .expect("membership snapshot refreshes");
    let snapshot = cluster.snapshot();
    let scribe = snapshot
        .live_scribes()
        .into_iter()
        .next()
        .expect("one ready Scribe membership");
    assert!(!scribe.address.ends_with(":0"));
    let reachable = scribe
        .address
        .strip_prefix("http://")
        .expect("test peer uses plaintext loopback");
    tokio::net::TcpStream::connect(reachable)
        .await
        .expect("advertised private Scribe endpoint is reachable before readiness");

    let closed = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("reserve closed endpoint")
        .local_addr()
        .expect("closed endpoint address");
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("system membership pool");
    sqlx::query(
        "UPDATE vala.cluster_nodes SET advertise_addr=$1, heartbeat_at=now(), ready=true \
         WHERE data_tenant_id=$2 AND role='scribe'",
    )
    .bind(format!("http://{closed}"))
    .bind(uuid::Uuid::from(wyrd_spec::DataTenantId::SYSTEM_OWNER))
    .execute(&pool)
    .await
    .expect("inject undialable private membership");
    cluster
        .refresh_snapshot()
        .await
        .expect("Oracle observes the undialable membership");

    let started = vala_sdk::query::QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT value FROM {table}"),
            visibility: VisibilityMode::Fused,
            freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            deadline_ms: Some(5_000),
        })
        .await;
    let error = match started {
        Err(error) => error,
        Ok(mut stream) => match stream.next_batch().await {
            Err(error) => error,
            Ok(Some(_)) => panic!("undialable strict read emitted a successful batch"),
            Ok(None) => panic!("undialable strict read emitted a successful terminal"),
        },
    };
    match &error {
        ValaSdkError::FailedTerminal { terminal } => {
            assert_eq!(
                terminal.outcome,
                wyrd_spec::vala::api::QueryTerminalOutcome::Failed
            );
            assert_eq!(
                terminal.error.as_ref().expect("failed terminal error").code,
                QueryTerminalErrorCode::QueryVisibilityUnavailable
            );
        }
        other => panic!("expected typed failed terminal, got {other:?}"),
    }
    assert_eq!(
        error_code(&error),
        "WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Builds one public SDK client scoped to the supplied workload tenant.
async fn tenant_client(
    server: &wyrd_testing::WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
) -> wyrd_client::WyrdClient {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, &unique_table("peer_client"), &["admin"])
        .await
        .expect("tenant service bootstrap");
    let api_key = bootstrap.api_key().expect("service API key");
    wyrd_client::WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().expect("bound gRPC URL"),
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().expect("bound HTTP URL").to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key.clone()),
        ..ClientConfig::default()
    })
    .expect("tenant SDK client")
}

/// Appends one row without flushing so strict fused visibility requires Scribe.
async fn append_active_row(client: &wyrd_client::WyrdClient, table: &str) {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(Int64Array::from(vec![7_i64]))],
    )
    .expect("active row batch");
    let mut ipc = Vec::new();
    let mut writer =
        arrow::ipc::writer::StreamWriter::try_new(&mut ipc, schema.as_ref()).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC terminal");
    vala_sdk::grpc::BifrostGrpcTransport::connect(client)
        .await
        .expect("public ingest transport")
        .insert_batch(table, uuid::Uuid::now_v7().into_bytes(), ipc)
        .await
        .expect("active row append");
}

/// Returns the public stable code without consuming the typed SDK error.
fn error_code(error: &ValaSdkError) -> &'static str {
    error.code()
}
