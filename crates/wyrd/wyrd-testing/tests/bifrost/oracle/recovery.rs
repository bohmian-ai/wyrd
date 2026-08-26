//! Oracle journeys — Terminal recovery and Scribe tail fencing across a server boundary.
//!
//! Module of the `oracle` binary; see `oracle.rs` for the capability it
//! proves and `support.rs` for the fixtures it shares.

use axum::body::Body;
use axum::http::{HeaderValue, Response, header};
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use secrecy::SecretString;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::file_list_writer::{FileListInsert, insert_and_audit};
use vala_sdk::{QueryClient, ValaSdkError};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest, FreshnessPolicy,
    VisibilityMode,
};
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::support::*;

/// Proves a two-Server write/query journey across query-scoped Scribe discovery.
///
/// Server A owns public writes and sealing; Server B performs strict and fused
/// reads through its Oracle/Scribe-tail path. The journey also checks tenant
/// isolation, transport outage fail-closed behavior, replacement after a
/// writer restart, post-seal tail visibility, and observation-only telemetry.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_two_server_scribe_tail_boundary_journey() {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::two_mixed())
        .await
        .expect("two-server boundary cluster");
    let writer_node = cluster
        .configured_node_ids()
        .first()
        .copied()
        .expect("writer node identity");
    let reader_node = cluster
        .configured_node_ids()
        .get(1)
        .copied()
        .expect("reader node identity");
    let table = unique_table("oracle_two_server_tail");
    {
        let writer = cluster
            .server_by_node(writer_node)
            .expect("writer Server A");
        register_table(writer, cluster.data_tenant_id(), &table)
            .await
            .expect("cross-server table registration");
        let writer_client = client(writer, "two-server-writer")
            .await
            .expect("writer client");
        ingest(&writer_client, &format!("vala.bifrost.{table}"), &[1, 2])
            .await
            .expect("public write on Server A");
    }
    // Exercise the supported server lifecycle flush instead of reaching into
    // the test-only Scribe handle; Server B must see these rows immediately.
    cluster
        .stop_node(writer_node)
        .await
        .expect("Server A shutdown flush");
    cluster
        .restart_node(writer_node)
        .await
        .expect("Server A restart after shutdown flush");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("post-flush writer snapshot");
    let writer_client = client(
        cluster
            .server_by_node(writer_node)
            .expect("restarted writer Server A"),
        "two-server-writer-restarted",
    )
    .await
    .expect("restarted writer client");
    let reader = cluster
        .server_by_node(reader_node)
        .expect("reader Server B");
    let reader_client = client(reader, "two-server-reader")
        .await
        .expect("reader client");
    assert_eq!(
        query_rows(&reader_client, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("strict query through Server B"),
        2
    );

    ingest(&writer_client, &format!("vala.bifrost.{table}"), &[3])
        .await
        .expect("post-seal write on Server A");
    let event_day = current_hour_partition();
    cluster
        .observe_live_tail(&format!("vala.bifrost.{table}"), event_day)
        .await
        .expect("observation-only live-tail discovery");
    assert_eq!(
        query_rows(&reader_client, &table, VisibilityMode::Fused)
            .await
            .expect("fused tail query through Server B"),
        3
    );

    let foreign_tenant = cluster
        .add_tenant("oracle-two-server-foreign")
        .await
        .expect("foreign tenant");
    let foreign_client = client_for_tenant(
        cluster
            .server_by_node(reader_node)
            .expect("reader remains available"),
        foreign_tenant,
        "two-server-foreign-reader",
    )
    .await
    .expect("foreign reader client");
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("two-server audit owner pool");
    let foreign_audit_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'bifrost.query.%'",
    )
    .bind(foreign_tenant.as_uuid())
    .fetch_one(&owner)
    .await
    .expect("foreign audit baseline");
    let system_audit_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'bifrost.query.%'",
    )
    .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
    .fetch_one(&owner)
    .await
    .expect("system audit baseline");
    let cross_tenant_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .expect("cross-tenant telemetry checkpoint");
    assert!(
        query_rows(&foreign_client, &table, VisibilityMode::Fused)
            .await
            .is_err(),
        "cross-tenant query must fail closed before tail access"
    );
    let foreign_audit_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'bifrost.query.%'",
    )
    .bind(foreign_tenant.as_uuid())
    .fetch_one(&owner)
    .await
    .expect("foreign audit attribution");
    let system_audit_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation LIKE 'bifrost.query.%'",
    )
    .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
    .fetch_one(&owner)
    .await
    .expect("system audit attribution");
    // The public tenant authorization boundary rejects this unregistered
    // table before Oracle planning, so it emits no Oracle read/security audit.
    // Verified-tenant security audit attribution is covered by the private
    // tonic authority journey; this path proves the public denial does not
    // fall through to the system chain.
    assert_eq!(foreign_audit_after, foreign_audit_before);
    assert_eq!(system_audit_after, system_audit_before);
    let cross_tenant_delta = cluster
        .telemetry()
        .delta_since(&cross_tenant_checkpoint)
        .expect("cross-tenant telemetry delta");
    assert!(
        cross_tenant_delta.metrics.iter().all(|sample| {
            (!(sample.family == "bifrost_oracle_tail_pages_total"
                || sample.family == "bifrost_oracle_tail_fences_total"))
                || sample.value <= 0.0
        }),
        "cross-tenant denial must preserve tail capacity before private access"
    );

    reader.set_tail_discovery_unavailable_for_test(true);
    assert!(
        query_rows(&reader_client, &table, VisibilityMode::Fused)
            .await
            .is_err(),
        "private Scribe discovery/auth outage must fail closed before first batch"
    );
    reader.set_tail_discovery_unavailable_for_test(false);
    assert_eq!(
        query_rows(&reader_client, &table, VisibilityMode::Fused)
            .await
            .expect("private Scribe discovery recovery"),
        3
    );

    let prior_stream = cluster
        .server_by_node(writer_node)
        .expect("prior writer")
        .state()
        .bifrost_tail_reader_for_test()
        .expect("prior Scribe tail reader")
        .stream_identity();
    cluster
        .stop_node(writer_node)
        .await
        .expect("writer restart stop");
    cluster
        .restart_node_at_new_address(writer_node)
        .await
        .expect("writer restart replacement");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("writer replacement snapshot");
    let replacement_writer = cluster
        .server_by_node(writer_node)
        .expect("replacement writer");
    let replacement_stream = replacement_writer
        .state()
        .bifrost_tail_reader_for_test()
        .expect("replacement Scribe tail reader")
        .stream_identity();
    assert_eq!(replacement_stream.node_id, prior_stream.node_id);
    assert!(
        replacement_stream.writer_epoch > prior_stream.writer_epoch,
        "replacement Scribe must reject the stale prior writer epoch"
    );
    let replacement_writer_client = client(replacement_writer, "two-server-replacement-writer")
        .await
        .expect("replacement writer client");
    ingest(
        &replacement_writer_client,
        &format!("vala.bifrost.{table}"),
        &[4, 5],
    )
    .await
    .expect("write after stale epoch replacement");
    cluster
        .observe_live_tail(&format!("vala.bifrost.{table}"), event_day)
        .await
        .expect("replacement live-tail discovery");
    let replacement_reader = cluster
        .server_by_node(reader_node)
        .expect("replacement reader");
    let replacement_reader_client = client(replacement_reader, "two-server-replacement-reader")
        .await
        .expect("replacement reader client");
    assert_eq!(
        query_rows(&replacement_reader_client, &table, VisibilityMode::Fused,)
            .await
            .expect("strict query succeeds after stale epoch replacement refresh"),
        5
    );

    let samples = cluster
        .telemetry()
        .snapshot()
        .expect("two-server observation telemetry");
    assert!(
        samples.iter().any(|sample| {
            sample.family == "bifrost_oracle_tail_pages_total" && sample.value > 0.0
        }),
        "tail query emitted no production observation"
    );
    cluster.shutdown().await.expect("two-server shutdown");
}

/// J7 proves stale replan, audit refusal, stale fence, SDK terminal rejection, and recovery.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_recovery_terminal_journey() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("J7 cluster");
    let server = cluster.server(0).expect("J7 server");
    let table = unique_table("oracle_j7");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("J7 table");
    let client = client(server, "oracle-j7").await.expect("J7 client");
    let missing_path = seed_missing_hot_row(&cluster, cluster.data_tenant_id(), &table)
        .await
        .expect("J7 missing hot row");
    let stale = QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT * FROM vala.bifrost.{table}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await;
    assert!(
        stale.is_err(),
        "stale source must fail before a public batch"
    );
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("J7 owner pool");
    let audit_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let retry_details: Vec<String> = loop {
        let details: Vec<String> = sqlx::query_scalar(
            "SELECT detail::text FROM vala.audit_outbox \
             WHERE data_tenant_id = $1 \
               AND operation = 'bifrost.query.read_decision' \
             ORDER BY seq",
        )
        .bind(cluster.data_tenant_id().as_uuid())
        .fetch_all(&owner)
        .await
        .expect("J7 retry audits");
        if details.len() >= 2 || tokio::time::Instant::now() >= audit_deadline {
            break details;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    assert_eq!(retry_details.len(), 2);
    assert!(retry_details[0].contains("\"retry_ordinal\":0"));
    assert!(retry_details[1].contains("\"retry_ordinal\":1"));
    sqlx::query("DELETE FROM vala.file_list WHERE file_path = $1")
        .bind(&missing_path)
        .execute(&owner)
        .await
        .expect("remove missing manifest row");

    prove_sdk_missing_terminal_rejected().await;
    ingest(&client, &format!("vala.bifrost.{table}"), &[7])
        .await
        .expect("J7 ingest");
    server.flush_bifrost().await.expect("J7 flush");
    assert_eq!(
        query_rows(&client, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("J7 recovered query"),
        1
    );
    let residual = cluster
        .oracle_inspection()
        .await
        .expect("J7 residual inspection");
    assert_eq!(residual.active_queries, 0);
    assert_eq!(residual.queued_queries, 0);
    assert_eq!(residual.reserved_memory_bytes, 0);
    assert_eq!(residual.reserved_spill_bytes, 0);
    assert_eq!(residual.peer_pending, 0);
    assert_eq!(residual.peer_running, 0);
    drop(owner);
    cluster.shutdown().await.expect("J7 shutdown");
}

/// Persist one manifest identity whose pinned object is intentionally absent.
///
/// # Errors
///
/// Returns a tenant-SQL or manifest persistence error.
async fn seed_missing_hot_row(
    cluster: &WyrdTestCluster,
    tenant: DataTenantId,
    table: &str,
) -> Result<String, JourneyError> {
    let binding =
        TenantTableBinding::resolve((tenant, TableRef::new(BifrostNamespace::Bifrost, table)))?;
    let path = format!("{}/j7-missing.parquet", binding.object_prefix);
    let event = AuditEvent::new(
        RequestId::now_v7(),
        None,
        "oracle.journey.missing_row".to_owned(),
        "bifrost.oracle.journey".to_owned(),
        None,
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKindTag::User,
        AuthMethod::Internal,
        "bifrost_query:read".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "stale replan fixture".to_owned(),
    );
    let mut conn = cluster.pg_fixture().tenant_conn_for(tenant).await?;
    insert_and_audit(
        &mut conn,
        &FileListInsert {
            id: uuid::Uuid::now_v7(),
            data_tenant_id: tenant,
            namespace: &binding.logical_namespace,
            table_name: &binding.table_name,
            file_path: &path,
            file_size: 128,
            row_count: 1,
            min_event_time: Utc::now(),
            max_event_time: Utc::now(),
            partition: vala_bifrost_redux::catalog::layout::TimePartition::new(
                vala_bifrost_redux::catalog::layout::TimeGranularity::Day,
                chrono::DateTime::UNIX_EPOCH,
            )?,
            node_id: uuid::Uuid::now_v7(),
            writer_epoch: 1,
            wal_lsn_min: 9_002,
            wal_lsn_max: 9_002,
        },
        &[event],
    )
    .await?;
    conn.commit().await?;
    Ok(path)
}

/// Drive the real Rust SDK against an EOF-before-terminal HTTP response.
async fn prove_sdk_missing_terminal_rejected() {
    let app = Router::new()
        .route(
            "/auth/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token": "missing-terminal-access-token",
                    "token_type": "Bearer",
                    "expires_at": "2099-01-01T00:00:00Z"
                }))
            }),
        )
        .route(
            "/v1/query",
            post(|| async {
                let mut response = Response::new(Body::empty());
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/vnd.wyrd.bifrost-query-stream"),
                );
                response
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("missing-terminal listener");
    let address = listener.local_addr().expect("missing-terminal address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("missing-terminal server");
    });
    let client = WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: format!("http://{address}"),
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: format!("http://{address}"),
            ..HttpConfig::default()
        },
        api_key: Some(SecretString::from("missing-terminal-test")),
        ..ClientConfig::default()
    })
    .expect("missing-terminal SDK client");
    let mut stream = QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: "SELECT 1".to_owned(),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await
        .expect("missing-terminal response stream");
    assert!(matches!(
        stream.next_batch().await,
        Err(ValaSdkError::IncompleteQueryStream)
    ));
    server.abort();
}
