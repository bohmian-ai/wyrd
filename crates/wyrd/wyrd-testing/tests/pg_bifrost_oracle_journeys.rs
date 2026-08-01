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
use vala_sdk::{BifrostGrpcTransport, IngestTransport, QueryClient, ValaSdkError};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_server::config::BifrostRuntimeRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest, EventDay,
    FreshnessPolicy, QueryTerminalErrorCode, QueryTerminalOutcome, VisibilityMode,
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
use wyrd_tonic::tonic::Code;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::wyrd::v1 as proto;
use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;
use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;
use wyrd_tonic::wyrd::v1::{QueryTracesRequest, QueryWindow};

type JourneyError = Box<dyn std::error::Error + Send + Sync>;

/// Complete normative production metric inventory and exact label-key sets.
const ORACLE_METRIC_LABELS: &[(&str, &[&str])] = &[
    (
        "bifrost_oracle_queries_total",
        &["outcome", "query_class", "visibility"],
    ),
    (
        "bifrost_oracle_query_duration_seconds",
        &["outcome", "query_class", "visibility"],
    ),
    (
        "bifrost_oracle_time_to_first_batch_seconds",
        &["query_class", "visibility"],
    ),
    ("bifrost_oracle_in_flight", &["query_class", "visibility"]),
    ("bifrost_role_ready", &["role"]),
    (
        "bifrost_gate_role_unavailable_total",
        &["reason", "required_role"],
    ),
    (
        "bifrost_oracle_admission_wait_seconds",
        &["outcome", "scope"],
    ),
    (
        "bifrost_oracle_admission_rejections_total",
        &["query_class", "scope"],
    ),
    (
        "bifrost_oracle_classification_total",
        &["query_class", "reason"],
    ),
    ("bifrost_oracle_predicted_scan_seconds", &["query_class"]),
    ("bifrost_oracle_slots_total", &["role"]),
    ("bifrost_oracle_slots_in_use", &["query_class", "role"]),
    (
        "bifrost_oracle_slot_reservations_total",
        &["outcome", "query_class", "role"],
    ),
    ("bifrost_oracle_admission_waiters", &["query_class"]),
    ("bifrost_oracle_memory_bytes", &["role"]),
    (
        "bifrost_oracle_class_memory_bytes",
        &["memory_kind", "query_class"],
    ),
    ("bifrost_oracle_spill_bytes_total", &["operator", "role"]),
    (
        "bifrost_oracle_spill_operations_total",
        &["operator", "outcome", "role"],
    ),
    ("bifrost_oracle_source_rows_total", &["source"]),
    ("bifrost_oracle_source_bytes_total", &["source"]),
    ("bifrost_oracle_files_pruned_total", &["reason", "source"]),
    ("bifrost_oracle_rows_deduplicated_total", &["losing_source"]),
    ("bifrost_oracle_tail_pages_total", &["locality", "outcome"]),
    ("bifrost_oracle_tail_page_seconds", &["locality", "outcome"]),
    ("bifrost_oracle_tail_fences_total", &["locality", "outcome"]),
    (
        "bifrost_oracle_tail_fence_hold_seconds",
        &["locality", "outcome"],
    ),
    ("bifrost_oracle_fragments_total", &["locality", "outcome"]),
    ("bifrost_oracle_fragments_in_flight", &["locality"]),
    ("bifrost_oracle_fragment_seconds", &["locality", "outcome"]),
    ("bifrost_oracle_fragment_bytes_total", &["locality"]),
    (
        "bifrost_oracle_peer_attempts_total",
        &["error_class", "outcome"],
    ),
    ("bifrost_oracle_stale_replans_total", &["outcome"]),
    ("bifrost_oracle_streams_total", &["freshness", "outcome"]),
    ("bifrost_oracle_audit_seconds", &["audit_kind", "outcome"]),
    ("bifrost_oracle_security_events_total", &["event_class"]),
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
    )
    .await
    .expect("J3 role-separated journey");
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
        Some("bifrost_oracle_peer_attempts_total"),
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

/// Proves a real Gate without Oracle returns typed gRPC UNAVAILABLE before planning.
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
    let cluster = WyrdTestCluster::start_spec(spec)
        .await
        .expect("no-Oracle cluster");
    let server = cluster.server(0).expect("no-Oracle server");
    let table = unique_table("oracle_unavailable");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("register unavailable table");
    let client = client(server, "grpc-unavailable").await.expect("client");
    let before = server
        .bifrost_read_decision_count()
        .await
        .expect("read-decision count");
    let code = grpc_query_status(
        &client,
        &BifrostQueryRequest {
            sql: format!("SELECT * FROM vala.bifrost.{table}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        },
    )
    .await
    .expect("missing Oracle status");
    assert_eq!(code, Code::Unavailable);
    let after = server
        .bifrost_read_decision_count()
        .await
        .expect("read-decision count after denial");
    assert_eq!(before, after);
    cluster.shutdown().await.expect("unavailable shutdown");
}

/// Proves dropping a public gRPC stream releases Oracle admission resources.
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
    let mut stream = grpc_query_stream(&client, &request)
        .await
        .expect("drop stream");
    assert!(stream.message().await.expect("schema result").is_some());
    assert!(stream.message().await.expect("batch result").is_some());
    drop(stream);
    wait_for_oracle_cleanup(&cluster)
        .await
        .expect("drop cleanup");
    let inspection = cluster.oracle_inspection().await.expect("drop inspection");
    assert_eq!(inspection.active_leases, 0);
    assert_eq!(inspection.slots_in_use, 0);
    cluster.shutdown().await.expect("drop shutdown");
}

/// J5 proves tenant tripwire, durable audit, admission cleanup, and audit refusal.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_multitenant_admission_journey() {
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
    seed_foreign_hot_row(&cluster, tenant_a, &table_name, tenant_b)
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

    sqlx::query("REVOKE INSERT ON vala.audit_outbox FROM wyrd_app")
        .execute(&owner)
        .await
        .expect("revoke audit insert");
    let refusal_client = client_for_tenant(server, tenant_b, "j5-audit-refusal")
        .await
        .expect("audit refusal client");
    let refused = QueryClient::new(&refusal_client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT count(*) FROM {table_fqn}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await;
    sqlx::query("GRANT INSERT ON vala.audit_outbox TO wyrd_app")
        .execute(&owner)
        .await
        .expect("restore audit insert");
    assert!(
        refused.is_err(),
        "audit refusal must fail before a stream exists"
    );

    wait_for_oracle_cleanup(&cluster)
        .await
        .expect("J5 admission cleanup");
    let inspection = cluster.oracle_inspection().await.expect("J5 inspection");
    assert_eq!(inspection.active_leases, 0);
    assert_eq!(inspection.slots_in_use, 0);
    assert!(inspection.audit_rows >= 4);
    owner.close().await;
    cluster.shutdown().await.expect("J5 shutdown");
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
    let checkpoint = cluster.telemetry().checkpoint();
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
    let stale_delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .expect("J7 stale telemetry");
    assert!(stale_delta.metrics.iter().any(|sample| {
        sample.family == "bifrost_oracle_stale_replans_total"
            && sample.labels.get("outcome").map(String::as_str) == Some("retried")
            && sample.value == 1.0
    }));
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("J7 owner pool");
    let retry_details: Vec<String> = sqlx::query_scalar(
        "SELECT detail::text FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 \
           AND operation = 'bifrost.query.read_decision' \
         ORDER BY seq",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_all(&owner)
    .await
    .expect("J7 retry audits");
    assert_eq!(retry_details.len(), 2);
    assert!(retry_details[0].contains("\"retry_ordinal\":0"));
    assert!(retry_details[1].contains("\"retry_ordinal\":1"));
    sqlx::query("DELETE FROM vala.file_list WHERE file_path = $1")
        .bind(&missing_path)
        .execute(&owner)
        .await
        .expect("remove missing manifest row");

    sqlx::query("REVOKE INSERT ON vala.audit_outbox FROM wyrd_app")
        .execute(&owner)
        .await
        .expect("revoke J7 audit insert");
    let audit_refused = QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT * FROM vala.bifrost.{table}"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await;
    sqlx::query("GRANT INSERT ON vala.audit_outbox TO wyrd_app")
        .execute(&owner)
        .await
        .expect("restore J7 audit insert");
    assert!(
        audit_refused.is_err(),
        "audit failure must precede a first byte"
    );

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
    wait_for_oracle_cleanup(&cluster)
        .await
        .expect("J7 admission cleanup");
    owner.close().await;
    cluster.shutdown().await.expect("J7 shutdown");
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
    let security_audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = 'bifrost.query.security_violation'",
    )
    .bind(cluster.data_tenant_id().as_uuid())
    .fetch_one(&owner)
    .await
    .expect("typed security audit");
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
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_mixed())
        .await
        .expect("telemetry cluster");
    let server = cluster.server(0).expect("telemetry server");
    let query_server = cluster.server(2).expect("telemetry query server");
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
    let checkpoint = cluster.telemetry().checkpoint();
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
        .register_live_tail(
            &format!("vala.bifrost.{table}"),
            EventDay::new(chrono::Utc::now().format("%Y-%m-%d").to_string()).expect("event day"),
        )
        .await
        .expect("telemetry tail registration");
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
    for required in [
        "bifrost_oracle_queries_total",
        "bifrost_oracle_query_duration_seconds",
        "bifrost_oracle_time_to_first_batch_seconds",
        "bifrost_oracle_source_rows_total",
        "bifrost_oracle_streams_total",
        "bifrost_oracle_audit_seconds",
        "bifrost_oracle_tail_pages_total",
        "bifrost_oracle_tail_page_seconds",
        "bifrost_oracle_tail_fences_total",
        "bifrost_oracle_tail_fence_hold_seconds",
        "bifrost_oracle_fragments_total",
        "bifrost_oracle_fragment_seconds",
        "bifrost_oracle_fragment_bytes_total",
        "bifrost_oracle_peer_attempts_total",
    ] {
        assert!(
            observed_families.contains(required),
            "missing production metric {required}: {observed_families:?}"
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
    for sample in delta
        .metrics
        .iter()
        .filter(|sample| sample.family.starts_with("bifrost_"))
    {
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
    let absolute_families = cluster
        .telemetry()
        .snapshot()
        .expect("absolute telemetry snapshot")
        .into_iter()
        .map(|sample| sample.family)
        .collect::<std::collections::BTreeSet<_>>();
    observed_families.extend(absolute_families.iter().cloned());
    assert!(
        absolute_families.contains("bifrost_oracle_fragments_in_flight"),
        "fragment in-flight gauge was never registered by production execution"
    );
    cluster.shutdown().await.expect("telemetry shutdown");

    let unavailable_cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::role_separated())
        .await
        .expect("role telemetry cluster");
    let ingest_only = unavailable_cluster.server(0).expect("ingest-only server");
    let unavailable_client = client(ingest_only, "oracle-role-unavailable")
        .await
        .expect("unavailable client");
    let checkpoint = unavailable_cluster.telemetry().checkpoint();
    let unavailable = QueryClient::new(&unavailable_client)
        .query(&BifrostQueryRequest {
            sql: "SELECT * FROM vala.bifrost.unavailable".to_owned(),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await;
    assert!(
        unavailable.is_err(),
        "ingest-only Gate must reject query routing"
    );
    let unavailable_delta = unavailable_cluster
        .telemetry()
        .delta_since(&checkpoint)
        .expect("unavailable telemetry delta");
    observed_families.extend(
        unavailable_delta
            .metrics
            .iter()
            .map(|sample| sample.family.clone()),
    );
    assert!(unavailable_delta.metrics.iter().any(|sample| {
        sample.family == "bifrost_gate_role_unavailable_total"
            && sample.labels.get("required_role").map(String::as_str) == Some("oracle")
            && sample.labels.get("reason").map(String::as_str) == Some("not_configured")
            && sample.value == 1.0
    }));
    let role_samples = unavailable_cluster
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
    unavailable_cluster
        .shutdown()
        .await
        .expect("role telemetry shutdown");
}

/// Validate every normative label value against its closed vocabulary.
fn metric_label_value_is_closed(label: &str, value: &str) -> bool {
    match label {
        "visibility" => matches!(value, "published_only" | "fused"),
        "query_class" => matches!(value, "interactive" | "analytical"),
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
            "estimated_scan"
                | "predicted_scan"
                | "global_operator"
                | "typed_plan"
                | "snapshot_overlap"
                | "not_configured"
                | "not_ready"
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
            .register_live_tail(&format!("vala.bifrost.{table}"), day)
            .await?;
    }
    let query_server = cluster.server(query_index).ok_or("missing query node")?;
    let reader = client(query_server, "oracle-journey-reader").await?;
    let checkpoint = cluster.telemetry().checkpoint();
    assert_eq!(query_rows(&reader, &table, visibility).await?, 2);
    if let Some(family) = expected_metric {
        let delta = cluster.telemetry().delta_since(&checkpoint)?;
        assert!(
            delta
                .metrics
                .iter()
                .any(|sample| sample.family == family && sample.value > 0.0),
            "journey did not emit required production metric {family}: {:?}",
            delta.metrics
        );
    }
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

/// Return the typed status from a query that should fail before its first frame.
///
/// # Errors
///
/// Returns setup errors or an error when the server unexpectedly accepts the
/// query and opens a stream.
async fn grpc_query_status(
    client: &WyrdClient,
    request: &BifrostQueryRequest,
) -> Result<Code, JourneyError> {
    let connection = client.connect_grpc().await?;
    let bearer = connection.auth().bearer().await?;
    let mut rpc = BifrostQueryServiceClient::new(connection.channel());
    let mut rpc_request =
        wyrd_tonic::tonic::Request::new(proto::BifrostQueryRequest::from(request.clone()));
    rpc_request.metadata_mut().insert(
        "x-wyrd-access-token",
        MetadataValue::try_from(format!("Bearer {}", bearer.expose()))?,
    );
    match rpc.query(rpc_request).await {
        Ok(_) => Err("query unexpectedly returned a stream".into()),
        Err(status) => Ok(status.code()),
    }
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
    let path = format!("{}/j5-foreign.parquet", binding.object_prefix);
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

/// Wait until durable and local Oracle admission capacity returns to zero.
///
/// # Errors
///
/// Returns an inspection error or a timeout when cleanup fails to restore all
/// capacity after terminal validation.
async fn wait_for_oracle_cleanup(cluster: &WyrdTestCluster) -> Result<(), JourneyError> {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let inspection = cluster.oracle_inspection().await?;
            if inspection.active_leases == 0 && inspection.slots_in_use == 0 {
                return Ok::<_, JourneyError>(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| -> JourneyError { "Oracle cleanup did not restore capacity".into() })?
}
