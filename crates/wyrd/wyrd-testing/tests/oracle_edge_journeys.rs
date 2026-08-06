//! Public-client Oracle operational journeys.

use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, FixedSizeBinaryArray, FixedSizeBinaryBuilder, Int32Array, Int64Array,
    StringArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use axum::body::Body;
use axum::http::{HeaderValue, Response, header};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{NaiveDate, Utc};
use opendal::Buffer;
use parquet::arrow::ArrowWriter;
use secrecy::SecretString;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::with_managed_columns;
use vala_bifrost_redux::scribe::file_list_writer::{FileListInsert, insert_and_audit};
use vala_sdk::{
    BifrostGrpcTransport, CollectedQueryLimits, IngestTransport, QueryClient, ValaSdkError,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_server::config::BifrostRuntimeRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest, EventDay,
    FreshnessPolicy, QueryTerminalErrorCode, QueryTerminalOutcome, QueryWarning, VisibilityMode,
};
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
use wyrd_tonic::frame_codec::FrameDecoder;
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
use wyrd_tonic::otlp::trace::v1::{
    ResourceSpans, ScopeSpans, Span as OtlpSpan, Status as OtlpStatus, span, status::StatusCode,
};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::wyrd::v1 as proto;
use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;
use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;
use wyrd_tonic::wyrd::v1::{QueryTracesRequest, QueryWindow};

type JourneyError = Box<dyn std::error::Error + Send + Sync>;

/// Complete normative production metric inventory and exact label-key sets.
const ORACLE_METRIC_LABELS: &[(&str, &[&str])] = &[
    ("oracle_queries_active", &["class"]),
    ("oracle_queries_queued", &["class"]),
    ("oracle_admission_total", &["class", "outcome", "reason"]),
    ("oracle_admission_queue_duration_seconds", &["class"]),
    ("oracle_tenant_budget_pressure", &["class"]),
    ("oracle_query_duration_seconds", &["class", "outcome"]),
    ("oracle_query_time_to_first_batch_seconds", &["class"]),
    ("oracle_query_rows_total", &["class"]),
    ("oracle_query_logical_bytes_selected_total", &["class"]),
    ("oracle_query_bytes_scanned_total", &["class"]),
    ("oracle_query_bytes_returned_total", &["class"]),
    ("oracle_query_files_scanned_total", &["class"]),
    ("oracle_query_partitions_scanned_total", &["class"]),
    ("oracle_query_spill_bytes_total", &["class"]),
    ("oracle_fragments_active", &[]),
    ("oracle_fragment_duration_seconds", &["outcome"]),
    ("oracle_query_cancellations_total", &["reason"]),
    ("oracle_audit_wal_records", &[]),
    ("oracle_audit_wal_bytes", &[]),
    ("oracle_audit_oldest_record_age_seconds", &[]),
    ("oracle_audit_append_duration_seconds", &[]),
    ("oracle_audit_relay_total", &["outcome"]),
    ("oracle_audit_relay_lag_seconds", &[]),
    ("oracle_audit_relay_batch_size", &[]),
    ("oracle_audit_relay_failures_total", &["reason"]),
    ("bifrost_role_ready", &["role"]),
];

/// Consequential production spans required by the Oracle operational contract.
const ORACLE_SPANS: &[&str] = &[
    "bifrost.gate.role_dispatch",
    "bifrost.oracle.query",
    "bifrost.oracle.plan",
    "bifrost.oracle.audit",
    "bifrost.oracle.source",
    "bifrost.oracle.admission",
    "bifrost.oracle.tail",
    "bifrost.oracle.tail_fence",
    "bifrost.oracle.fragment",
    "bifrost.oracle.slot_reservation",
    "bifrost.oracle.reconcile",
    "bifrost.oracle.stream",
];

/// J1 proves exact PublishedOnly rows and a validated terminal through the public SDK.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_published_journey() {
    public_roundtrip(
        BifrostClusterSpec::one_mixed(),
        VisibilityMode::PublishedOnly,
        true,
        0,
        None,
        false,
    )
    .await
    .expect("J1 PublishedOnly journey");
}

/// J2 proves a Fused query drains a live tonic tail without a Scribe flush.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_fused_reconcile_journey() {
    public_roundtrip(
        BifrostClusterSpec::one_mixed(),
        VisibilityMode::Fused,
        false,
        0,
        Some("bifrost_oracle_tail_pages_total"),
        false,
    )
    .await
    .expect("J2 Fused journey");
}

/// J3 proves ingest-only gRPC write and query-only HTTP/Arrow read routing.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_role_separated_journey() {
    public_roundtrip(
        BifrostClusterSpec::role_separated(),
        VisibilityMode::Fused,
        false,
        2,
        Some("bifrost_oracle_tail_pages_total"),
        false,
    )
    .await
    .expect("J3 role-separated journey");
}

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
    let event_day =
        EventDay::new(chrono::Utc::now().format("%Y-%m-%d").to_string()).expect("event day");
    cluster
        .observe_live_tail(&format!("vala.bifrost.{table}"), event_day.clone())
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

/// J4 enters the final mixed node so its frozen membership includes remote workers.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_distributed_journey() {
    public_roundtrip(
        BifrostClusterSpec::three_mixed(),
        VisibilityMode::PublishedOnly,
        true,
        2,
        Some("oracle_query_rows_total"),
        true,
    )
    .await
    .expect("J4 distributed journey");
}

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
    let spec = BifrostClusterSpec {
        nodes: vec![wyrd_testing::bifrost::BifrostNodeSpec {
            node_id: wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7()),
            roles: [BifrostRuntimeRole::Scribe, BifrostRuntimeRole::Forge]
                .into_iter()
                .collect(),
            oracle: None,
        }],
    };
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

/// J5 proves tenant isolation, local fairness, durable audit, and audit refusal.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("J5 cluster");
    let server = cluster.server(0).expect("J5 server");
    let tenant_a = cluster.data_tenant_id();
    let tenant_b = cluster.add_tenant("oracle-j5-b").await.expect("tenant B");
    let table_name = unique_table("oracle_j5");
    for tenant in [tenant_a, tenant_b] {
        register_table(server, tenant, &table_name)
            .await
            .expect("tenant table registration");
        let client = client_for_tenant(server, tenant, &format!("j5-{tenant}"))
            .await
            .expect("tenant client");
        ingest(
            &client,
            &format!("vala.bifrost.{table_name}"),
            &[tenant_marker(tenant)],
        )
        .await
        .expect("tenant ingest");
        server
            .flush_bifrost_for_tenant(tenant)
            .await
            .expect("tenant flush");
        assert_eq!(
            query_rows(&client, &table_name, VisibilityMode::PublishedOnly)
                .await
                .expect("tenant query"),
            1
        );
    }
    seed_foreign_hot_row(&cluster, tenant_a, &table_name, tenant_b, "j5-foreign")
        .await
        .expect("foreign physical row");
    let tenant_a_client = client_for_tenant(server, tenant_a, "j5-tripwire")
        .await
        .expect("tripwire client");
    let table_fqn = format!("vala.bifrost.{table_name}");
    for sql in [
        format!("SELECT count(*) AS total FROM {table_fqn}"),
        format!("SELECT a.id FROM {table_fqn} a JOIN {table_fqn} b ON a.id = b.id"),
    ] {
        let (rows, outcome, error) = query_statement(&tenant_a_client, sql)
            .await
            .expect("tripwire terminal");
        assert_eq!(rows, 0, "foreign row reached a SQL operator");
        assert_eq!(outcome, QueryTerminalOutcome::Failed);
        assert_eq!(error, Some(QueryTerminalErrorCode::QueryTenantInvariant));
    }
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("J5 table-owner pool");
    let audit_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.audit_outbox \
             WHERE data_tenant_id = $1 \
               AND operation = 'bifrost.query.security_violation'",
        )
        .bind(tenant_a.as_uuid())
        .fetch_one(&owner)
        .await
        .expect("security audit count");
        if count >= 2 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < audit_deadline,
            "security audit relay did not drain"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let security_details: Vec<String> = sqlx::query_scalar(
        "SELECT detail::text FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 \
           AND operation = 'bifrost.query.security_violation' \
         ORDER BY seq",
    )
    .bind(tenant_a.as_uuid())
    .fetch_all(&owner)
    .await
    .expect("security audit rows");
    assert!(
        security_details.len() >= 2,
        "COUNT and JOIN each require a durable security audit"
    );
    assert!(security_details.iter().all(|detail| {
        detail.contains("\"violation\":\"tenant_row\"") && detail.contains("\"phase\":\"source\"")
    }));

    let inspection = cluster.oracle_inspection().await.expect("J5 inspection");
    assert!(inspection.audit_rows >= 4);
    assert_eq!(inspection.active_queries, 0);
    assert_eq!(inspection.queued_queries, 0);
    assert_eq!(inspection.reserved_memory_bytes, 0);
    assert_eq!(inspection.reserved_spill_bytes, 0);
    assert_eq!(inspection.peer_pending, 0);
    assert_eq!(inspection.peer_running, 0);
    drop(owner);
    cluster.shutdown().await.expect("J5 shutdown");
}

/// J6 proves durable-before-read acceptance, bounded relay backlog, and replay windows.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_audit_relay_journey() {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("J6 cluster");
    let server = cluster.server(0).expect("J6 server");
    let node_id = server.node_id();
    let table = unique_table("oracle_audit_relay");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("J6 table");
    let primary_bootstrap = server
        .bootstrap_service_in_tenant(cluster.data_tenant_id(), "oracle-audit-relay", &["admin"])
        .await
        .expect("J6 primary bootstrap");
    let primary_principal = primary_bootstrap.id().as_uuid();
    let query_client = client_from_bootstrap(server, primary_bootstrap)
        .await
        .expect("J6 client");
    let secondary_bootstrap = server
        .bootstrap_service_in_tenant(
            cluster.data_tenant_id(),
            "oracle-audit-relay-second",
            &["admin"],
        )
        .await
        .expect("J6 secondary bootstrap");
    let secondary_principal = secondary_bootstrap.id().as_uuid();
    let secondary_client = client_from_bootstrap(server, secondary_bootstrap)
        .await
        .expect("J6 second client");
    ingest(&query_client, &format!("vala.bifrost.{table}"), &[1])
        .await
        .expect("J6 ingest");
    server.flush_bifrost().await.expect("J6 flush");
    let pause = server.pause_audit_relay_for_test().expect("J6 pause relay");
    let audit_pool = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("J6 audit owner");
    let primary_boundary: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM vala.audit_outbox WHERE data_tenant_id = $1",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_one(&audit_pool)
    .await
    .expect("J6 primary sequence boundary");
    assert_eq!(
        query_rows(&query_client, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("J6 query"),
        1
    );
    let secondary_boundary: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM vala.audit_outbox WHERE data_tenant_id = $1",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_one(&audit_pool)
    .await
    .expect("J6 secondary sequence boundary");
    assert_eq!(
        query_rows(&secondary_client, &table, VisibilityMode::PublishedOnly,)
            .await
            .expect("J6 second query"),
        1
    );
    let blocked = server.oracle_runtime_inspection().expect("J6 inspection");
    assert!(
        blocked.audit_wal_records >= 1,
        "accepted audit must be durable before relay"
    );
    drop(pause);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if server
            .oracle_runtime_inspection()
            .expect("J6 relay inspection")
            .audit_wal_records
            == 0
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        assert!(
            tokio::time::Instant::now() < deadline,
            "relay did not drain"
        );
    }
    let primary_rows: Vec<(uuid::Uuid, String, String, uuid::Uuid, String)> = sqlx::query_as(
        "SELECT data_tenant_id, request_id, resource, principal_id, operation FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND resource = $2 AND principal_id = $3 \
           AND operation = $4 AND seq > $5 ORDER BY seq",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .bind("bifrost.query")
    .bind(primary_principal)
    .bind("bifrost.query.read_decision")
    .bind(primary_boundary)
    .fetch_all(&audit_pool)
    .await
    .expect("J6 primary correlated audit row");
    let secondary_rows: Vec<(uuid::Uuid, String, String, uuid::Uuid, String)> = sqlx::query_as(
        "SELECT data_tenant_id, request_id, resource, principal_id, operation FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND resource = $2 AND principal_id = $3 \
           AND operation = $4 AND seq > $5 ORDER BY seq",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .bind("bifrost.query")
    .bind(secondary_principal)
    .bind("bifrost.query.read_decision")
    .bind(secondary_boundary)
    .fetch_all(&audit_pool)
    .await
    .expect("J6 secondary correlated audit row");
    assert_eq!(primary_rows.len(), 1);
    assert_eq!(secondary_rows.len(), 1);
    assert_ne!(primary_rows[0].1, secondary_rows[0].1);
    let primary = &primary_rows[0];
    let primary_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND request_id = $2 AND resource = $3 \
           AND principal_id = $4 AND operation = $5",
    )
    .bind(primary.0)
    .bind(&primary.1)
    .bind(&primary.2)
    .bind(primary.3)
    .bind(&primary.4)
    .fetch_one(
        &cluster
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("J6 exact tuple owner"),
    )
    .await
    .expect("J6 exact tuple count");
    assert_eq!(primary_count, 1);
    let roots = cluster
        .terminate_node_abruptly_for_test(node_id)
        .await
        .expect("J6 terminate");
    cluster
        .restart_terminated_node_at_new_address(node_id, roots)
        .await
        .expect("J6 restart");
    let restarted = cluster.server(0).expect("J6 restarted");
    let current: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND request_id = $2 AND resource = $3 \
           AND principal_id = $4 AND operation = $5",
    )
    .bind(primary.0)
    .bind(&primary.1)
    .bind(&primary.2)
    .bind(primary.3)
    .bind(&primary.4)
    .fetch_one(
        &cluster
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("J6 owner restart"),
    )
    .await
    .expect("J6 restart count");
    assert_eq!(current, 1, "checkpointed relay must not replay on restart");
    let replay_bootstrap = restarted
        .bootstrap_service_in_tenant(cluster.data_tenant_id(), "oracle-audit-replay", &["admin"])
        .await
        .expect("J6 replay bootstrap");
    let replay_principal = replay_bootstrap.id().as_uuid();
    let replay_client = client_from_bootstrap(restarted, replay_bootstrap)
        .await
        .expect("J6 replay client");
    let replay_boundary: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM vala.audit_outbox WHERE data_tenant_id = $1",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_one(&audit_pool)
    .await
    .expect("J6 replay sequence boundary");
    restarted
        .fail_audit_after_commit_for_test()
        .expect("J6 crash seam");
    assert_eq!(
        query_rows(&replay_client, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("J6 replay query"),
        1
    );
    let replay_owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("J6 replay owner");
    let replay_tuple: (uuid::Uuid, String, String, uuid::Uuid, String) = loop {
        let tuples: Vec<(uuid::Uuid, String, String, uuid::Uuid, String)> = sqlx::query_as(
            "SELECT data_tenant_id, request_id, resource, principal_id, operation \
             FROM vala.audit_outbox WHERE data_tenant_id = $1 AND resource = $2 \
               AND principal_id = $3 AND operation = $4 AND seq > $5 ORDER BY seq",
        )
        .bind(cluster.data_tenant_id().as_uuid())
        .bind("bifrost.query")
        .bind(replay_principal)
        .bind("bifrost.query.read_decision")
        .bind(replay_boundary)
        .fetch_all(&replay_owner)
        .await
        .expect("J6 replay tuple");
        if !tuples.is_empty() {
            assert_eq!(tuples.len(), 1, "J6 replay initial tuple must be unique");
            break tuples.into_iter().next().expect("J6 replay tuple exists");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    let commit_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let committed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.audit_outbox \
             WHERE data_tenant_id = $1 AND request_id = $2 AND resource = $3 \
               AND principal_id = $4 AND operation = $5",
        )
        .bind(replay_tuple.0)
        .bind(&replay_tuple.1)
        .bind(&replay_tuple.2)
        .bind(replay_tuple.3)
        .bind(&replay_tuple.4)
        .fetch_one(&replay_owner)
        .await
        .expect("J6 committed replay count");
        let inspection = restarted
            .oracle_runtime_inspection()
            .expect("J6 replay inspection");
        if committed == 1 && inspection.audit_wal_records >= 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < commit_deadline,
            "commit-before-checkpoint seam did not expose a durable uncheckpointed record"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let replay_roots = cluster
        .terminate_node_abruptly_for_test(node_id)
        .await
        .expect("J6 replay terminate");
    cluster
        .restart_terminated_node_at_new_address(node_id, replay_roots)
        .await
        .expect("J6 replay restart");
    let replay_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let replayed: i64 = loop {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.audit_outbox \
             WHERE data_tenant_id = $1 AND request_id = $2 AND resource = $3 \
               AND principal_id = $4 AND operation = $5",
        )
        .bind(replay_tuple.0)
        .bind(&replay_tuple.1)
        .bind(&replay_tuple.2)
        .bind(replay_tuple.3)
        .bind(&replay_tuple.4)
        .fetch_one(&replay_owner)
        .await
        .expect("J6 replay count");
        if count >= 2 || tokio::time::Instant::now() >= replay_deadline {
            break count;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    assert_eq!(replayed, 2, "commit-before-checkpoint is at-least-once");
    let chain: Vec<(i64, Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT seq, prev_hash, entry_hash FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND request_id = $2 AND resource = $3 \
           AND principal_id = $4 AND operation = $5 ORDER BY seq DESC LIMIT 2",
    )
    .bind(replay_tuple.0)
    .bind(&replay_tuple.1)
    .bind(&replay_tuple.2)
    .bind(replay_tuple.3)
    .bind(&replay_tuple.4)
    .fetch_all(
        &cluster
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("J6 chain owner"),
    )
    .await
    .expect("J6 chain rows");
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].0, chain[1].0 + 1);
    assert_eq!(chain[0].1, chain[1].2);
    cluster.shutdown().await.expect("J6 shutdown");
}

/// J6 proves real-tonic ticket, payload, permission, replay, footer, and cleanup invariants.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_peer_security_journey() {
    crate::oracle_peer::prove_oracle_peer_security_journey().await;
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

    crate::oracle_peer::prove_oracle_peer_restart_rejects_old_fence_and_releases_reservation()
        .await;
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

/// A real SDK query replans once when its selected worker restarts before dispatch.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_live_topology_replans_through_boot_directory() {
    let mut cluster = WyrdTestCluster::start_spec_with_oracle_peer_tls_delayed_last(
        BifrostClusterSpec::three_mixed(),
    )
    .await
    .expect("leader-first TLS topology");
    let delayed = *cluster
        .configured_node_ids()
        .last()
        .expect("delayed worker identity");
    cluster
        .restart_node_at_new_address(delayed)
        .await
        .expect("worker joins after leader boot");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("late worker snapshot");
    let server = cluster.server(0).expect("query leader");
    let table = unique_table("oracle_topology_replan");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("topology table");
    let client = client(server, "oracle-topology-replan")
        .await
        .expect("topology client");
    // The production planner closes at most 16 files into one fragment. Four
    // deterministic fragments exercise the real portable assignment rather
    // than the single-fragment leader-only fast path.
    for ordinal in 0..64 {
        seed_foreign_hot_row(
            &cluster,
            cluster.data_tenant_id(),
            &table,
            cluster.data_tenant_id(),
            &format!("topology-{ordinal}"),
        )
        .await
        .expect("independent sealed file");
    }
    let probe = Arc::new(vala_bifrost_redux::oracle::OracleTopologyProbe::default());
    server
        .state()
        .bifrost_query()
        .expect("boot query runtime")
        .oracle()
        .bind_topology_probe_for_test(Arc::clone(&probe));
    let mut query = tokio::spawn(async move {
        let mut stream = QueryClient::new(&client)
            .query(&BifrostQueryRequest {
                sql: format!("SELECT id FROM vala.bifrost.{table} ORDER BY id"),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(20_000),
            })
            .await?;
        let mut rows = 0_u64;
        while let Some(batch) = stream.next_batch().await? {
            rows = rows.saturating_add(u64::try_from(batch.num_rows())?);
        }
        let terminal = stream.terminal().cloned().ok_or("query terminal missing")?;
        Ok::<_, JourneyError>((rows, terminal))
    });
    tokio::select! {
        () = probe.wait_selected() => {}
        result = &mut query => panic!("query ended before remote selection: {result:?}"),
    }
    let selected = probe.selected_worker().expect("remote selected worker");
    let old_peer = cluster
        .server_by_node(selected)
        .and_then(|server| server.state().oracle_peer.as_ref())
        .expect("selected peer")
        .clone();
    cluster
        .stop_node(selected)
        .await
        .expect("selected worker stops");
    cluster
        .restart_node_at_new_address(selected)
        .await
        .expect("selected worker replacement");
    cluster
        .refresh_oracle_snapshots()
        .await
        .expect("replacement snapshot");
    probe.resume();
    let (rows, terminal) = query
        .await
        .expect("query task joins")
        .expect("replacement query succeeds");
    assert_eq!(rows, 64);
    assert_eq!(terminal.outcome, QueryTerminalOutcome::Success);
    assert!(terminal.warnings.contains(&QueryWarning::StaleCutReplanned));
    assert_eq!(old_peer.worker().pending_reservations(), 0);
    cluster.shutdown().await.expect("topology replan shutdown");
}

/// J-typed proves Vala's typed route enters the same Oracle cut as SQL.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn typed_vala_route_uses_oracle_cut() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("start Oracle journey cluster");
    let server = cluster.server(0).expect("query server");
    let reader = client(server, "typed-route-reader")
        .await
        .expect("typed-route client");
    let connection = reader.connect_grpc().await.expect("gRPC endpoint connects");
    let token = connection
        .auth()
        .bearer()
        .await
        .expect("reader bearer")
        .expose()
        .to_owned();
    let channel = connection.channel();
    let trace = typed_route_trace();
    let mut otlp = TraceServiceClient::new(channel.clone());
    otlp.export(with_access_token(Request::new(trace.clone()), &token))
        .await
        .expect("sealed typed-route trace export");
    server.flush_bifrost().await.expect("flush sealed trace");
    otlp.export(with_access_token(Request::new(trace), &token))
        .await
        .expect("hot typed-route trace export");
    let mut typed = ValaQueryServiceClient::new(channel);
    let response = typed
        .query_traces(with_access_token(
            Request::new(QueryTracesRequest {
                window: Some(QueryWindow {
                    since: String::new(),
                    until: String::new(),
                    limit: 100,
                    page_token: String::new(),
                }),
                service: "typed-route".to_owned(),
                min_duration_ms: 0,
                status: String::new(),
                name: "typed-cut-span".to_owned(),
            }),
            &token,
        ))
        .await
        .expect("typed Vala route succeeds");
    let typed_row = response
        .into_inner()
        .rows
        .into_iter()
        .next()
        .expect("typed route returns one trace summary");
    assert_eq!(typed_row.trace_id, "11".repeat(16));
    assert_eq!(typed_row.root_name, "typed-cut-span");
    assert_eq!(typed_row.service, "typed-route");
    assert_eq!(typed_row.span_count, 1);
    assert!(!typed_row.error);
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("typed-route owner pool");
    let audit_baseline: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'bifrost.query.security_violation'",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_one(&owner)
    .await
    .expect("typed audit baseline");
    let sql_identity = query_trace_identity_value(&reader)
        .await
        .expect("SQL Oracle identity/value query succeeds");
    assert_eq!(
        sql_identity,
        vec![(
            "11".repeat(16),
            "22".repeat(8),
            "typed-cut-span".to_owned(),
            "typed-route".to_owned(),
        )]
    );
    let (rows, outcome, error) =
        query_statement(
            &reader,
            "SELECT * FROM vala.traces.spans WHERE service_name = 'typed-route' AND name = 'typed-cut-span'"
                .to_owned(),
        )
            .await
            .expect("SQL Oracle cut succeeds");
    assert_eq!(rows, 1);
    assert_eq!(outcome, QueryTerminalOutcome::Success);
    assert!(error.is_none());

    let foreign_tenant = cluster
        .add_tenant("typed-route-foreign")
        .await
        .expect("foreign tenant");
    seed_foreign_trace_row(&cluster, cluster.data_tenant_id(), foreign_tenant)
        .await
        .expect("foreign trace fixture");
    let typed_error = typed
        .query_traces(with_access_token(
            Request::new(QueryTracesRequest {
                window: Some(QueryWindow {
                    since: String::new(),
                    until: String::new(),
                    limit: 100,
                    page_token: String::new(),
                }),
                service: "typed-route".to_owned(),
                min_duration_ms: 0,
                status: String::new(),
                name: "typed-cut-span".to_owned(),
            }),
            &token,
        ))
        .await;
    let typed_error = typed_error.expect_err("foreign tenant must fail typed query closed");
    assert_eq!(typed_error.code(), wyrd_tonic::tonic::Code::Internal);
    assert_eq!(typed_error.message(), "query tenant invariant violated");
    let (sql_rows, sql_outcome, sql_error) = query_statement(
        &reader,
        "SELECT * FROM vala.traces.spans WHERE service_name = 'typed-route' AND name = 'typed-cut-span'"
            .to_owned(),
    )
    .await
    .expect("foreign SQL terminal");
    assert_eq!(sql_rows, 0);
    assert_eq!(sql_outcome, QueryTerminalOutcome::Failed);
    assert_eq!(
        sql_error,
        Some(QueryTerminalErrorCode::QueryTenantInvariant)
    );
    let audit_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let security_audits: i64 = loop {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'bifrost.query.security_violation'",
        )
        .bind(cluster.data_tenant_id().as_uuid())
        .fetch_one(&owner)
        .await
        .expect("typed security audit");
        if count >= audit_baseline + 2 || tokio::time::Instant::now() >= audit_deadline {
            break count;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    assert_eq!(
        security_audits - audit_baseline,
        2,
        "typed and SQL tripwires each append exactly one durable audit"
    );
    cluster.shutdown().await.expect("shutdown cluster");
}

/// Builds one deterministic OTLP span reused across sealed and hot writes.
fn typed_route_trace() -> ExportTraceServiceRequest {
    let span = OtlpSpan {
        trace_id: vec![0x11; 16],
        span_id: vec![0x22; 8],
        parent_span_id: Vec::new(),
        trace_state: String::new(),
        flags: 0,
        name: "typed-cut-span".to_owned(),
        kind: span::SpanKind::Server as i32,
        start_time_unix_nano: 1_700_000_000_000_000_000,
        end_time_unix_nano: 1_700_000_000_050_000_000,
        attributes: Vec::new(),
        dropped_attributes_count: 0,
        events: Vec::new(),
        dropped_events_count: 0,
        links: Vec::new(),
        dropped_links_count: 0,
        status: Some(OtlpStatus {
            message: String::new(),
            code: StatusCode::Ok as i32,
        }),
    };
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(OtlpResource {
                attributes: vec![KeyValue {
                    key: "service.name".to_owned(),
                    value: Some(AnyValue {
                        value: Some(any_value::Value::StringValue("typed-route".to_owned())),
                    }),
                }],
                dropped_attributes_count: 0,
                entity_refs: Vec::new(),
            }),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![span],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

/// Adds the OTLP access-token metadata expected by the Gate collector.
fn with_access_token<T>(mut request: Request<T>, token: &str) -> Request<T> {
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {token}").parse().expect("token metadata"),
    );
    request
}

/// Persists one foreign-tenant trace row beneath the built-in spans table.
async fn seed_foreign_trace_row(
    cluster: &WyrdTestCluster,
    owner: DataTenantId,
    foreign: DataTenantId,
) -> Result<(), JourneyError> {
    let binding =
        TenantTableBinding::resolve((owner, TableRef::new(BifrostNamespace::Traces, "spans")))?;
    let user_fields = vec![
        Field::new("trace_id", DataType::FixedSizeBinary(16), false),
        Field::new("span_id", DataType::FixedSizeBinary(8), false),
        Field::new("parent_span_id", DataType::FixedSizeBinary(8), true),
        Field::new("flags", DataType::Int64, false),
        Field::new("trace_state", DataType::Utf8, true),
        Field::new("name", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new(
            "start_time",
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new(
            "end_time",
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("duration_ms", DataType::Int64, false),
        Field::new("status", DataType::Utf8, false),
        Field::new("attributes", DataType::Utf8, true),
        Field::new("dropped_attributes_count", DataType::Int64, false),
        Field::new("dropped_events_count", DataType::Int64, false),
        Field::new("dropped_links_count", DataType::Int64, false),
        Field::new("scope_name", DataType::Utf8, true),
        Field::new("scope_version", DataType::Utf8, true),
        Field::new("service_name", DataType::Utf8, false),
    ];
    let schema = Arc::new(Schema::new(with_managed_columns(user_fields)));
    let mut trace_id = FixedSizeBinaryBuilder::with_capacity(1, 16);
    trace_id.append_value([0x31; 16])?;
    let mut span_id = FixedSizeBinaryBuilder::with_capacity(1, 8);
    span_id.append_value([0x41; 8])?;
    let mut parent_span_id = FixedSizeBinaryBuilder::with_capacity(1, 8);
    parent_span_id.append_null();
    let mut batch_id = FixedSizeBinaryBuilder::with_capacity(1, 16);
    batch_id.append_value(uuid::Uuid::now_v7().as_bytes())?;
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(trace_id.finish()),
            Arc::new(span_id.finish()),
            Arc::new(parent_span_id.finish()),
            Arc::new(Int64Array::from(vec![0])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec!["typed-cut-span"])),
            Arc::new(StringArray::from(vec!["SERVER"])),
            Arc::new(
                TimestampMicrosecondArray::from(vec![1_700_000_000_000_000_i64])
                    .with_timezone("UTC"),
            ),
            Arc::new(
                TimestampMicrosecondArray::from(vec![1_700_000_000_050_000_i64])
                    .with_timezone("UTC"),
            ),
            Arc::new(Int64Array::from(vec![50])),
            Arc::new(StringArray::from(vec!["OK"])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(Int64Array::from(vec![0])),
            Arc::new(Int64Array::from(vec![0])),
            Arc::new(Int64Array::from(vec![0])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec!["typed-route"])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![uuid::Uuid::now_v7().to_string()])),
            Arc::new(StringArray::from(vec![RequestId::now_v7().to_string()])),
            Arc::new(
                TimestampMicrosecondArray::from(vec![1_700_000_000_050_000_i64])
                    .with_timezone("UTC"),
            ),
            Arc::new(
                TimestampMicrosecondArray::from(vec![1_700_000_000_050_001_i64])
                    .with_timezone("UTC"),
            ),
            Arc::new(batch_id.finish()),
            Arc::new(Int32Array::from(vec![0])),
            Arc::new(StringArray::from(vec![foreign.to_string()])),
        ],
    )?;
    let mut parquet = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut parquet, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    let path = format!("{}/typed-route-foreign.parquet", binding.object_prefix);
    cluster
        .storage_operator()
        .write(&path, Buffer::from(parquet.clone()))
        .await?;
    let event = AuditEvent::new(
        RequestId::now_v7(),
        None,
        "oracle.journey.typed_foreign_row".to_owned(),
        "bifrost.oracle.journey".to_owned(),
        None,
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKindTag::User,
        AuthMethod::Internal,
        "bifrost_query:read".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "typed foreign tripwire fixture".to_owned(),
    );
    let mut conn = cluster.pg_fixture().tenant_conn_for(owner).await?;
    insert_and_audit(
        &mut conn,
        &FileListInsert {
            id: uuid::Uuid::now_v7(),
            data_tenant_id: owner,
            namespace: &binding.logical_namespace,
            table_name: &binding.table_name,
            file_path: &path,
            file_size: i64::try_from(parquet.len())?,
            row_count: 1,
            min_event_time: Utc::now(),
            max_event_time: Utc::now(),
            partition_day: NaiveDate::from_ymd_opt(2023, 11, 14).ok_or("invalid fixture day")?,
            node_id: uuid::Uuid::now_v7(),
            writer_epoch: 1,
            wal_lsn_min: 9_101,
            wal_lsn_max: 9_101,
        },
        &[event],
    )
    .await?;
    conn.commit().await?;
    Ok(())
}

/// Assert production recorder labels and production-pipeline spans for a real query.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn oracle_production_telemetry_contract() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("telemetry cluster");
    let server = cluster.server(0).expect("telemetry server");
    let query_server = cluster.server(0).expect("telemetry query server");
    let table = unique_table("oracle_telemetry");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("telemetry table");
    let writer = client(server, "oracle-telemetry-writer")
        .await
        .expect("telemetry writer");
    let reader = client(query_server, "oracle-telemetry-reader")
        .await
        .expect("telemetry reader");
    let published_rows = (0_i64..10_000).collect::<Vec<_>>();
    ingest(&writer, &format!("vala.bifrost.{table}"), &published_rows)
        .await
        .expect("telemetry ingest");
    server.flush_bifrost().await.expect("telemetry flush");
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .expect("telemetry checkpoint");
    assert_eq!(
        query_rows(&reader, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("telemetry query"),
        10_000
    );
    ingest(&writer, &format!("vala.bifrost.{table}"), &[10_000])
        .await
        .expect("telemetry live ingest");
    cluster
        .observe_live_tail(
            &format!("vala.bifrost.{table}"),
            EventDay::new(chrono::Utc::now().format("%Y-%m-%d").to_string()).expect("event day"),
        )
        .await
        .expect("telemetry tail observation");
    assert_eq!(
        query_rows(&reader, &table, VisibilityMode::Fused)
            .await
            .expect("telemetry fused query"),
        10_001
    );
    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .expect("telemetry delta");
    let mut observed_families = delta
        .metrics
        .iter()
        .map(|sample| sample.family.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let absolute_families = cluster
        .telemetry()
        .snapshot()
        .expect("absolute telemetry snapshot")
        .into_iter()
        .map(|sample| sample.family)
        .collect::<std::collections::BTreeSet<_>>();
    observed_families.extend(absolute_families.iter().cloned());
    for (required, _) in ORACLE_METRIC_LABELS {
        assert!(
            observed_families.contains(*required),
            "missing production metric {required}: {observed_families:?}"
        );
    }
    for family in [
        "oracle_query_logical_bytes_selected_total",
        "oracle_query_bytes_scanned_total",
    ] {
        assert!(
            delta
                .metrics
                .iter()
                .any(|sample| sample.family == family && sample.value > 0.0),
            "canonical Parquet query did not emit positive {family}"
        );
    }
    let prohibited = [
        "tenant",
        "tenant_id",
        "principal",
        "principal_id",
        "request_id",
        "query_id",
        "fragment_id",
        "path",
        "sql",
        "error",
        "error_text",
    ];
    for sample in delta.metrics.iter().filter(|sample| {
        sample.family.starts_with("oracle_") || sample.family == "bifrost_role_ready"
    }) {
        if let Some((_, expected)) = ORACLE_METRIC_LABELS
            .iter()
            .find(|(family, _)| *family == sample.family)
        {
            let actual = sample
                .labels
                .keys()
                .filter(|label| !matches!(label.as_str(), "quantile" | "le"))
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            let expected = expected
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                actual, expected,
                "metric {} changed its closed label contract",
                sample.family
            );
            for (label, value) in &sample.labels {
                if matches!(label.as_str(), "quantile" | "le") {
                    continue;
                }
                assert!(
                    metric_label_value_is_closed(label, value),
                    "metric {} emitted open label {label}={value}",
                    sample.family
                );
            }
        }
        assert!(
            sample
                .labels
                .keys()
                .all(|key| !prohibited.contains(&key.as_str())),
            "high-cardinality label leaked on {}: {:?}",
            sample.family,
            sample.labels
        );
    }
    let spans = delta
        .spans
        .iter()
        .map(|span| span.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for required in ORACLE_SPANS {
        assert!(
            spans.contains(required),
            "missing production span {required}: {spans:?}"
        );
    }
    for span in delta
        .spans
        .iter()
        .filter(|span| ORACLE_SPANS.contains(&span.name.as_str()))
    {
        assert!(
            span.attributes
                .keys()
                .all(|key| !prohibited.contains(&key.as_str())),
            "high-cardinality span field leaked on {}: {:?}",
            span.name,
            span.attributes
        );
    }
    let query_spans = delta
        .spans
        .iter()
        .filter(|span| span.name == "bifrost.oracle.query")
        .collect::<Vec<_>>();
    assert!(
        !query_spans.is_empty(),
        "query span attributes were not captured"
    );
    for span in query_spans {
        for field in ["request_node_id", "leader_node_id"] {
            let value = span
                .attributes
                .get(field)
                .expect("required scrubbed node field");
            uuid::Uuid::parse_str(value).expect("node field must be a scrubbed UUID");
        }
        assert_eq!(
            span.attributes.get("search_role").map(String::as_str),
            Some("oracle")
        );
    }
    assert!(absolute_families.contains("oracle_fragments_active"));
    let residual = cluster
        .oracle_inspection()
        .await
        .expect("telemetry residual inspection");
    assert_eq!(residual.active_queries, 0);
    assert_eq!(residual.queued_queries, 0);
    assert_eq!(residual.reserved_memory_bytes, 0);
    assert_eq!(residual.reserved_spill_bytes, 0);
    assert_eq!(residual.peer_pending, 0);
    assert_eq!(residual.peer_running, 0);
    cluster.shutdown().await.expect("telemetry shutdown");

    let role_cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::role_separated())
        .await
        .expect("role telemetry cluster");
    for server in role_cluster.servers() {
        assert_eq!(
            server.forge_process_role(),
            wyrd_server::config::ForgeProcessRole::Server,
            "role-separated roster must use full Server processes"
        );
        assert!(server.bifrost_scribe().is_some());
        assert!(server.state().bifrost_query().is_some());
    }
    let role_samples = role_cluster
        .telemetry()
        .snapshot()
        .expect("role gauge snapshot");
    observed_families.extend(role_samples.iter().map(|sample| sample.family.clone()));
    for role in ["scribe", "oracle"] {
        assert!(role_samples.iter().any(|sample| {
            sample.family == "bifrost_role_ready"
                && sample.labels.get("role").map(String::as_str) == Some(role)
        }));
    }
    for (family, _) in ORACLE_METRIC_LABELS {
        assert!(
            observed_families.contains(*family),
            "required production metric family {family} was absent across captured scenarios: {observed_families:?}"
        );
    }
    role_cluster
        .shutdown()
        .await
        .expect("role telemetry shutdown");
}

/// Validate every normative label value against its closed vocabulary.
fn metric_label_value_is_closed(label: &str, value: &str) -> bool {
    match label {
        "visibility" => matches!(value, "published_only" | "fused"),
        "query_class" | "class" => matches!(value, "interactive" | "analytical"),
        "role" | "required_role" => matches!(value, "leader" | "worker" | "oracle" | "scribe"),
        "locality" => matches!(value, "local" | "remote"),
        "source" | "losing_source" => matches!(value, "iceberg" | "hot_sealed" | "live_tail"),
        "freshness" => matches!(value, "complete" | "degraded"),
        "memory_kind" => matches!(value, "source" | "tail" | "reconciliation" | "attempt"),
        "operator" => matches!(value, "reconcile" | "attempt"),
        "audit_kind" => matches!(value, "read_decision" | "security_violation"),
        "event_class" => matches!(value, "tenant_row" | "ticket" | "claims" | "fragment"),
        "scope" => matches!(value, "cluster" | "class" | "tenant"),
        "reason" => matches!(
            value,
            "class_capacity"
                | "tenant_budget"
                | "queue_full"
                | "queue_deadline"
                | "memory"
                | "spill"
                | "audit_unavailable"
                | "shutdown"
                | "client_drop"
                | "deadline"
                | "peer_failure"
                | "postgres"
                | "timeout"
                | "serialization"
        ),
        "error_class" => matches!(
            value,
            "none" | "availability" | "security" | "exhausted" | "footer" | "attempt"
        ),
        "outcome" => matches!(
            value,
            "success"
                | "failed"
                | "cancelled"
                | "degraded"
                | "acquired"
                | "rejected"
                | "pending"
                | "running"
                | "spilled"
                | "retried"
                | "admitted"
                | "retried_transient"
                | "committed"
        ),
        _ => false,
    }
}

/// Run one public gRPC-ingest to HTTP-query roundtrip and validate the terminal.
async fn public_roundtrip(
    spec: BifrostClusterSpec,
    visibility: VisibilityMode,
    flush: bool,
    query_index: usize,
    expected_metric: Option<&str>,
    assert_distributed_physical: bool,
) -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(spec).await?;
    let ingest_server = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let table = unique_table("oracle_journey");
    register_table(ingest_server, cluster.data_tenant_id(), &table).await?;
    let writer = client(ingest_server, "oracle-journey-writer").await?;
    ingest(&writer, &format!("vala.bifrost.{table}"), &[1, 2]).await?;
    if flush {
        ingest_server.flush_bifrost().await?;
    } else {
        let day = EventDay::new(chrono::Utc::now().format("%Y-%m-%d").to_string())?;
        cluster
            .observe_live_tail(&format!("vala.bifrost.{table}"), day)
            .await?;
    }
    let query_server = cluster.server(query_index).ok_or("missing query node")?;
    let reader = client(query_server, "oracle-journey-reader").await?;
    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .expect("helper telemetry checkpoint");
    assert_eq!(query_rows(&reader, &table, visibility).await?, 2);
    if let Some(family) = expected_metric {
        let delta = cluster
            .telemetry()
            .delta_since(&checkpoint)
            .map_err(|error| error.to_string())?;
        assert!(
            delta
                .metrics
                .iter()
                .any(|sample| sample.family == family && sample.value > 0.0),
            "journey did not emit required production metric {family}: {:?}",
            delta.metrics
        );
        if assert_distributed_physical {
            for family in [
                "oracle_query_bytes_scanned_total",
                "oracle_query_files_scanned_total",
                "oracle_query_partitions_scanned_total",
            ] {
                let physical = delta
                    .metrics
                    .iter()
                    .filter(|sample| sample.family == family && sample.value > 0.0)
                    .count();
                assert_eq!(physical, 1, "distributed worker must emit {family} once");
            }
        }
    }
    let inspection = cluster.oracle_inspection().await?;
    assert_eq!(inspection.active_queries, 0);
    assert_eq!(inspection.queued_queries, 0);
    assert_eq!(inspection.reserved_memory_bytes, 0);
    assert_eq!(inspection.reserved_spill_bytes, 0);
    assert_eq!(inspection.peer_pending, 0);
    assert_eq!(inspection.peer_running, 0);
    cluster.shutdown().await?;
    Ok(())
}

/// Register one tenant-owned Redux table through the server-owned catalog.
async fn register_table(
    server: &wyrd_testing::WyrdTestServer,
    tenant: DataTenantId,
    table: &str,
) -> Result<(), JourneyError> {
    server
        .state()
        .bifrost_redux
        .as_ref()
        .ok_or("missing Redux catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, table),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            audit: None,
        })
        .await?;
    Ok(())
}

/// Build one authenticated public client for the fixture tenant.
async fn client(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
) -> Result<WyrdClient, JourneyError> {
    client_for_tenant(server, server.data_tenant_id(), name).await
}

/// Build one authenticated public client for an explicit tenant.
async fn client_for_tenant(
    server: &wyrd_testing::WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
) -> Result<WyrdClient, JourneyError> {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, name, &["admin"])
        .await?;
    client_from_bootstrap(server, bootstrap).await
}

/// Build a client while retaining the bootstrap principal for exact audit correlation.
async fn client_from_bootstrap(
    server: &wyrd_testing::WyrdTestServer,
    bootstrap: Bootstrap,
) -> Result<WyrdClient, JourneyError> {
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("machine bootstrap returned user".into()),
    };
    Ok(WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing gRPC URL")?,
            connect_retries: 0,
            max_message_bytes: 32 * 1024 * 1024,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().ok_or("missing HTTP URL")?.to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key),
        ..ClientConfig::default()
    })?)
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

/// Send one Arrow IPC batch through the public authenticated Gate transport.
async fn ingest(client: &WyrdClient, table: &str, ids: &[i64]) -> Result<(), JourneyError> {
    BifrostGrpcTransport::connect(client)
        .await?
        .insert_batch(table, uuid::Uuid::now_v7().into_bytes(), ipc(ids))
        .await?;
    Ok(())
}

/// Execute arbitrary read-only SQL and retain the exact terminal after draining frames.
///
/// # Errors
///
/// Returns a client error before a terminal or when a successful stream is malformed.
async fn query_statement(
    client: &WyrdClient,
    sql: String,
) -> Result<(u64, QueryTerminalOutcome, Option<QueryTerminalErrorCode>), JourneyError> {
    let mut stream = QueryClient::new(client)
        .query(&BifrostQueryRequest {
            sql,
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await?;
    let mut rows = 0_u64;
    loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => {
                rows = rows.saturating_add(u64::try_from(batch.num_rows())?);
            }
            Ok(None) => break,
            Err(error) if stream.terminal().is_some() => {
                let _ = error;
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    Ok((
        rows,
        terminal.outcome,
        terminal.error.as_ref().map(|error| error.code),
    ))
}

/// Read the returned trace identity and user-visible values through SQL.
///
/// # Errors
///
/// Returns client, protocol, Arrow, or terminal errors when the query cannot
/// be drained or its typed columns do not match the trace contract.
async fn query_trace_identity_value(
    client: &WyrdClient,
) -> Result<Vec<(String, String, String, String)>, JourneyError> {
    let mut stream = QueryClient::new(client)
        .query(&BifrostQueryRequest {
            sql: "SELECT trace_id, span_id, name, service_name FROM vala.traces.spans WHERE service_name = 'typed-route' AND name = 'typed-cut-span'".to_owned(),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await?;
    let mut rows = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        let trace_ids = batch
            .column_by_name("trace_id")
            .and_then(|column| column.as_any().downcast_ref::<FixedSizeBinaryArray>())
            .ok_or("trace_id column has unexpected Arrow type")?;
        let span_ids = batch
            .column_by_name("span_id")
            .and_then(|column| column.as_any().downcast_ref::<FixedSizeBinaryArray>())
            .ok_or("span_id column has unexpected Arrow type")?;
        let names = batch
            .column_by_name("name")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .ok_or("name column has unexpected Arrow type")?;
        let services = batch
            .column_by_name("service_name")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .ok_or("service_name column has unexpected Arrow type")?;
        for row in 0..batch.num_rows() {
            rows.push((
                hex::encode(trace_ids.value(row)),
                hex::encode(span_ids.value(row)),
                names.value(row).to_owned(),
                services.value(row).to_owned(),
            ));
        }
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    if terminal.outcome != QueryTerminalOutcome::Success {
        return Err(format!("identity query failed: {:?}", terminal.error).into());
    }
    Ok(rows)
}

/// Persist one foreign-tenant physical row beneath the production provider union.
///
/// # Errors
///
/// Returns an Arrow, Parquet, storage, tenant-SQL, or manifest persistence error.
async fn seed_foreign_hot_row(
    cluster: &WyrdTestCluster,
    owner: DataTenantId,
    table: &str,
    foreign: DataTenantId,
    path_tag: &str,
) -> Result<(), JourneyError> {
    let table_ref = TableRef::new(BifrostNamespace::Bifrost, table);
    let binding = TenantTableBinding::resolve((owner, table_ref))?;
    let schema = Arc::new(Schema::new(with_managed_columns(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ])));
    let mut batch_ids = FixedSizeBinaryBuilder::with_capacity(1, 16);
    batch_ids.append_value(uuid::Uuid::now_v7().as_bytes())?;
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(vec![999_i64])) as ArrayRef,
            Arc::new(StringArray::from(vec!["foreign"])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![uuid::Uuid::now_v7().to_string()])),
            Arc::new(StringArray::from(vec![RequestId::now_v7().to_string()])),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_000_i64]).with_timezone("UTC")),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_001_i64]).with_timezone("UTC")),
            Arc::new(batch_ids.finish()),
            Arc::new(Int32Array::from(vec![0])),
            Arc::new(StringArray::from(vec![foreign.to_string()])),
        ],
    )?;
    let mut parquet = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut parquet, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    let path = format!("{}/{path_tag}.parquet", binding.object_prefix);
    cluster
        .storage_operator()
        .write(&path, Buffer::from(parquet.clone()))
        .await?;
    let event = AuditEvent::new(
        RequestId::now_v7(),
        None,
        "oracle.journey.foreign_row".to_owned(),
        "bifrost.oracle.journey".to_owned(),
        None,
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKindTag::User,
        AuthMethod::Internal,
        "bifrost_query:read".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "foreign tripwire fixture".to_owned(),
    );
    let mut conn = cluster.pg_fixture().tenant_conn_for(owner).await?;
    insert_and_audit(
        &mut conn,
        &FileListInsert {
            id: uuid::Uuid::now_v7(),
            data_tenant_id: owner,
            namespace: &binding.logical_namespace,
            table_name: &binding.table_name,
            file_path: &path,
            file_size: i64::try_from(parquet.len())?,
            row_count: 1,
            min_event_time: Utc::now(),
            max_event_time: Utc::now(),
            partition_day: NaiveDate::from_ymd_opt(1970, 1, 1).ok_or("invalid fixture day")?,
            node_id: uuid::Uuid::now_v7(),
            writer_epoch: 1,
            wal_lsn_min: 9_001,
            wal_lsn_max: 9_001,
        },
        &[event],
    )
    .await?;
    conn.commit().await?;
    Ok(())
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
            partition_day: NaiveDate::from_ymd_opt(1970, 1, 1).ok_or("invalid fixture day")?,
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

/// Drain a public query stream and require its terminal row count to match frames.
async fn query_rows(
    client: &WyrdClient,
    table: &str,
    visibility: VisibilityMode,
) -> Result<u64, JourneyError> {
    let mut stream = QueryClient::new(client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id, value FROM vala.bifrost.{table} ORDER BY id"),
            visibility,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await?;
    let mut rows = 0_u64;
    while let Some(batch) = stream.next_batch().await? {
        rows = rows.saturating_add(u64::try_from(batch.num_rows())?);
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    if terminal.row_count != rows {
        return Err("terminal row count differs from Arrow frames".into());
    }
    Ok(rows)
}

/// Encode deterministic journey rows as one Arrow stream.
fn ipc(ids: &[i64]) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(ids.to_vec())),
            Arc::new(StringArray::from(vec!["oracle"; ids.len()])),
        ],
    )
    .expect("fixed journey arrays share a length");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("valid schema");
    writer.write(&batch).expect("in-memory IPC write");
    writer.finish().expect("in-memory IPC finish");
    bytes
}

/// Return a collision-free SQL identifier for one serialized journey.
fn unique_table(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}

/// Derive a deterministic row marker without exposing tenant identity in telemetry.
fn tenant_marker(tenant: DataTenantId) -> i64 {
    i64::from(tenant.as_uuid().as_bytes()[0])
}
