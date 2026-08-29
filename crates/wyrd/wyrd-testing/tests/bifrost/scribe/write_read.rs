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
use wyrd_testing::bifrost::{ScribeCacheMode, ScribeProductionWorkloadV1};

use super::support::{register_table, start_scribe_server, unique_table};

/// AC22 journey owner: a client writes, seals, and reads its own rows back.
///
/// This is the complete client-to-server path a real caller takes — public
/// gRPC ingest, the durable seal and publication, and the public query route —
/// driven by the canonical production workload record rather than a fixture
/// local to this file, so the cache task and Forge later run these exact bytes.
///
/// The record is serialized and deserialized before the run so a field that
/// only exists in this process cannot become part of the contract.
///
/// # Panics
///
/// Panics when the record does not survive its wire form, when the run does not
/// satisfy the record's checkpoints and digest, or when a published object's
/// promotion record cannot rebuild the Iceberg `DataFile` it claims.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_write_flush_read_user_journey() {
    let _tmp_guard = wyrd_telemetry::init(wyrd_telemetry::TelemetryConfig {
        filter: "vala_bifrost_redux=debug,wyrd_server=debug,datafusion=debug".to_owned(),
        ..wyrd_telemetry::TelemetryConfig::default()
    });
    let canonical = ScribeProductionWorkloadV1::canonical();
    let bytes = serde_json::to_vec(&canonical).expect("the canonical record serializes");
    let workload: ScribeProductionWorkloadV1 =
        serde_json::from_slice(&bytes).expect("the canonical record deserializes");
    assert_eq!(workload, canonical, "the handoff must be lossless");

    let server = start_scribe_server().await;
    let empty_tenant = server.data_tenant_id();
    let empty_table = register_table(
        &server,
        empty_tenant,
        BifrostNamespace::Datasets,
        &unique_table("empty_query"),
    )
    .await;
    let empty_client = tenant_client(&server, empty_tenant).await;
    let mut empty = vala_sdk::query::QueryClient::new(&empty_client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT value FROM {empty_table}"),
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

    let evidence = server
        .run_scribe_production_workload(&workload, ScribeCacheMode::Disabled)
        .await
        .expect("the production workload runs on public routes");
    evidence
        .assert_matches(&workload, ScribeCacheMode::Disabled)
        .expect("the run satisfies every normative field of the record");

    let published = evidence
        .checkpoints
        .last()
        .expect("the run reached its final checkpoint");
    let objects: usize = published
        .published
        .values()
        .map(|records| records.len())
        .sum();
    assert!(
        objects > 0,
        "a sealed journey must publish at least one hot object"
    );
    for records in published.published.values() {
        for record in records {
            record
                .data_file()
                .expect("every published record rebuilds its Iceberg data file");
        }
    }

    server.shutdown().await.expect("the server drains cleanly");
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
        Ok(mut stream) => loop {
            match stream.next_batch().await {
                Err(error) => break error,
                Ok(Some(_)) => panic!("undialable strict read emitted a successful batch"),
                Ok(None) => panic!("undialable strict read emitted a successful terminal"),
            }
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
