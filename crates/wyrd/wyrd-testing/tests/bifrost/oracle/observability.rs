//! Oracle journeys — What an operator observes: audit relay, the production telemetry
//! contract, and the typed Vala route's Oracle cut.
//!
//! Module of the `oracle` binary; see `oracle.rs` for the capability it
//! proves and `support.rs` for the fixtures it shares.

use arrow::array::{
    Array, FixedSizeBinaryArray, FixedSizeBinaryBuilder, Int32Array, Int64Array, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use chrono::{NaiveDate, Utc};
use opendal::Buffer;
use parquet::arrow::ArrowWriter;
use std::sync::Arc;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::with_managed_columns;
use vala_bifrost_redux::scribe::file_list_writer::{FileListInsert, insert_and_audit};
use vala_sdk::QueryClient;
use wyrd_client::WyrdClient;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest, FreshnessPolicy,
    QueryTerminalErrorCode, QueryTerminalOutcome, VisibilityMode,
};
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
use wyrd_tonic::otlp::trace::v1::{
    ResourceSpans, ScopeSpans, Span as OtlpSpan, Status as OtlpStatus, span, status::StatusCode,
};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;
use wyrd_tonic::wyrd::v1::{QueryTracesRequest, QueryWindow};

use crate::support::*;

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
    ("oracle_query_spill_files_total", &["class"]),
    ("oracle_query_spill_queries_total", &["class", "outcome"]),
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
            partition: vala_bifrost_redux::catalog::layout::TimePartition::new(
                vala_bifrost_redux::catalog::layout::TimeGranularity::Day,
                NaiveDate::from_ymd_opt(2023, 11, 14)
                    .ok_or("invalid fixture day")?
                    .and_hms_opt(0, 0, 0)
                    .ok_or("invalid fixture midnight")?
                    .and_utc(),
            )?,
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
        .observe_live_tail(&format!("vala.bifrost.{table}"), current_hour_partition())
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
            wyrd_server::config::BifrostTarget::Server,
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
