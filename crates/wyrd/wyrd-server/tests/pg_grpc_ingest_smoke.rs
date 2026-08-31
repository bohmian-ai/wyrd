//! Process-level gRPC ingest smoke tests.
//!
//! Stands up `build_app_grpc` + `serve_grpc` and proves the C1 ingest auth
//! seam works correctly: an unauthenticated stream is rejected, and a valid
//! token passes auth (the empty stream then terminates without an Unauthenticated
//! error). Reuses the same key/verifier setup as `router_smoke.rs`.

use arrow::array::{Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{IngressPayload, Scribe, ScribeIngressFrame};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint as ReduxSchemaFingerprint;
use vala_bifrost_redux::scribe::tail_rpc::{
    TailTicketAudience, TailTicketClaims, TailTicketMinter,
};
use vala_bifrost_redux::scribe::{
    replay::replay_wal_directory,
    tail_rpc::{LocalTailReadTransport, TailReadTransport, TonicTailReadTransport},
};
use vala_sql::TenantConn;
use wyrd_auth_verify::TokenPrincipalRef;
use wyrd_runtime::{PermissionSet, Principal, PrincipalId, PrincipalKind, RoleRef};
use wyrd_semver::VersionBlock;
use wyrd_server::AppState;
use wyrd_server::auth::seed::seed_builtin_roles_for_tenant;
use wyrd_server::grpc::{GrpcRouterConfig, build_app_grpc, serve_grpc};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PLATFORM_AUDIT_PRINCIPAL, PrincipalKindTag};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AcquireTailFenceRequest as DomainAcquireTailFenceRequest,
    SchemaFingerprint as WireSchemaFingerprint, TailCursor,
    TailPageRequest as DomainTailPageRequest, TenantTableBinding,
};
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
use wyrd_testing::WyrdTestServer;
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
use wyrd_tonic::otlp::logs::v1::ResourceLogs;
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::logs_service::logs_service_client::LogsServiceClient;
use wyrd_tonic::otlp::metrics::v1::ResourceMetrics;
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::metrics_service::metrics_service_client::MetricsServiceClient;
use wyrd_tonic::otlp::resource::v1::Resource;
use wyrd_tonic::otlp::trace::v1::ResourceSpans;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::tonic::{Code, Request};
use wyrd_tonic::tonic_health::server::health_reporter;
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_client::BifrostIngestServiceClient;
use wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient;
use wyrd_tonic::wyrd::v1::{AcquireTailFenceRequest, InsertBatchRequest, ReleaseTailFenceRequest};

/// Seed one additional data tenant through the composed fixture's operator.
///
/// # Panics
/// Panics when the fixture cannot seed the requested tenant.
async fn seed_tenant(server: &WyrdTestServer, tenant: DataTenantId, slug: &str) {
    server
        .pg_fixture()
        .seed_additional_tenant_with_uuid(tenant, slug)
        .await
        .expect("fixture seeds the requested tenant");
}

/// Mints one user token with the requested SQL-resolved built-in roles.
///
/// # Panics
///
/// Panics when a static role is invalid or the fixture key cannot sign the token.
fn mint_user_jwt(state: &AppState, tenant: DataTenantId, roles: &[&str]) -> String {
    let principal = TokenPrincipalRef {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKindTag::User,
        tenant_id: tenant,
        card_ref: None,
        card_ref_scope: Default::default(),
    };
    state
        .auth
        .issuing_key
        .as_ref()
        .expect("test state has issuing key")
        .issue_user_access_token(
            principal,
            roles
                .iter()
                .map(|role| RoleRef::new(role).expect("static role is valid"))
                .collect(),
            ChronoDuration::minutes(5),
        )
        .expect("test jwt mints")
}

/// Mints the workload identity required by the private Scribe tail service.
fn mint_service_jwt(state: &AppState, tenant: DataTenantId) -> String {
    let card_ref = CardRef {
        kind: CardKind::Service,
        name: CardName::new("tail-reader").expect("static service name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: SpaceName::new("prod").expect("static space is valid"),
        uid: None,
    };
    state
        .auth
        .issuing_key
        .as_ref()
        .expect("test state has issuing key")
        .issue_service_access_token(
            PrincipalId::new(uuid::Uuid::now_v7()),
            tenant,
            card_ref.clone(),
            CardRefScope::own(&card_ref),
            Vec::new(),
            ChronoDuration::minutes(5),
        )
        .expect("service jwt mints")
}

/// The instant the private-tail fixture writes its rows at.
///
/// Production Scribe admission bounds event time to a window around now, so a
/// frozen literal would age out of the accepted range and start failing. The
/// batch timestamp and the fence request's time partition both derive from
/// this one value so they always name the same UTC partition.
fn fixture_event_time() -> DateTime<Utc> {
    Utc::now()
}

/// Builds two logical rows on the private-tail fixture's UTC event day.
fn non_empty_tail_batch() -> RecordBatch {
    let timestamp = fixture_event_time().timestamp_micros();
    let schema = Arc::new(Schema::new(vec![
        Field::new(
            wyrd_spec::vala::WYRD_EVENT_TIME,
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("value", DataType::Int64, false),
    ]));
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(
                TimestampMicrosecondArray::from(vec![timestamp, timestamp]).with_timezone("UTC"),
            ),
            Arc::new(Int64Array::from(vec![11, 22])),
        ],
    )
    .expect("private-tail fixture batch is valid")
}

/// The caller-owned dataset the private-tail fixture writes and reads.
///
/// The tail path needs a table that actually exists for the tenant, and the
/// only registration a caller may perform is in `vala.datasets`: `vala.bifrost`
/// is Bifrost-internal and every `vala.<domain>` built-in is provisioned from
/// its own canonical definition, neither of which this fixture's single
/// `value` column matches. Registering one dataset keeps the seeded frame, the
/// signed ticket's canonical table, and the fence binding on one identity.
const TAIL_TABLE_NAME: &str = "scribe_tail_smoke";

/// The logical FQN of [`TAIL_TABLE_NAME`], as the signed ticket spells it.
const TAIL_TABLE_FQN: &str = "vala.datasets.scribe_tail_smoke";

/// Creates a metadata-only fence request matching the seeded logical schema.
fn non_empty_tail_request(tenant: DataTenantId) -> DomainAcquireTailFenceRequest {
    let expected = ReduxSchemaFingerprint::from_arrow_schema(&Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    DomainAcquireTailFenceRequest {
        query_id: uuid::Uuid::now_v7(),
        binding: TenantTableBinding {
            tenant_id: tenant,
            namespace: "datasets".to_owned(),
            table: TAIL_TABLE_NAME.to_owned(),
        },
        time_partition: vala_bifrost_redux::catalog::TimeGranularity::Hour
            .bucket(fixture_event_time())
            .expect("fixture instant buckets to an exact hour")
            .to_wire(),
        exclusive_sealed: TailCursor {
            writer_epoch: 1,
            wal_lsn: 0,
            batch_id: uuid::Uuid::nil(),
            row_ordinal: 0,
        },
        deadline: chrono::Utc::now() + ChronoDuration::seconds(5),
        schema_fingerprint: WireSchemaFingerprint::new(hex::encode(expected.0))
            .expect("fixture fingerprint is valid"),
        tail_protocol_version: 1,
    }
}

/// Mints the private acquire ticket bound to the fixture's exact stream tuple.
fn mint_tail_ticket(state: &AppState, request: &DomainAcquireTailFenceRequest) -> Vec<u8> {
    let ingest = state
        .bifrost_ingest()
        .expect("fixture retains ingest runtime");
    let authority = ingest
        .tail_authority()
        .expect("fixture retains tail authority");
    let stream = ingest.tail_reader().stream_identity();
    authority
        .mint_tail_ticket(&TailTicketClaims {
            query_id: request.query_id,
            tenant_id: request.binding.tenant_id,
            canonical_table: TAIL_TABLE_FQN.to_owned(),
            node_id: stream.node_id.as_uuid(),
            writer_epoch: u64::try_from(stream.writer_epoch.as_i64())
                .expect("fixture epoch is non-negative"),
            deadline: request.deadline,
            audience: TailTicketAudience::Acquire,
            nonce: vec![7; 16],
        })
        .expect("fixture tail ticket signs")
}

/// Creates the server-verified identity used to seed the embedded Scribe.
fn test_principal(tenant: DataTenantId) -> Principal {
    Principal::new(
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::new(),
    )
}

/// Seeds two logical rows through the production embedded Scribe boundary.
///
/// The frame is the one a transport request would carry: an authenticated
/// tenant, the Gate-owned allow/success audit event, the projected `value`-only
/// source fingerprint (`wyrd_event_time` is server-managed and never part of
/// schema identity), and the measured wire size.
async fn seed_tail_rows(state: &AppState, tenant: DataTenantId) {
    let rows = non_empty_tail_batch();
    let principal = test_principal(tenant);
    let table = TableRef::new(BifrostNamespace::Datasets, TAIL_TABLE_NAME);
    state
        .bifrost_catalog()
        .expect("composed server exposes its Bifrost catalog")
        .register_dataset(
            tenant,
            table.clone(),
            vec![Field::new("value", DataType::Int64, false)],
            None,
            None,
        )
        .await
        .expect("private-tail dataset registers for the seeding tenant");
    let request_id = RequestId::now_v7();
    let measured_wire_bytes = rows.get_array_memory_size();
    let audit_event = AuditEvent {
        request_id: request_id.clone(),
        trace_id: None,
        operation: "bifrost.append".to_owned(),
        resource: table.fqn(),
        card_ref: principal.card_ref().cloned(),
        principal_id: principal.id,
        principal_kind: principal.kind.tag(),
        auth_method: AuthMethod::Jwt,
        permission: "bifrost:append".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: format!("{} rows", rows.num_rows()),
        detail: None,
    };
    let admission = Scribe::ingest_frame(
        state
            .bifrost_ingest()
            .expect("embedded state retains Scribe")
            .scribe()
            .as_ref(),
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            principal,
            table,
            expected_schema_fingerprint: Some(ReduxSchemaFingerprint::from_arrow_schema(
                &Schema::new(vec![Field::new("value", DataType::Int64, false)]),
            )),
            request_id,
            batch_id: uuid::Uuid::now_v7(),
            audit_event,
            measured_wire_bytes,
            payload: IngressPayload::ProjectedArrow(vec![rows]),
        },
    )
    .await
    .expect("embedded Scribe admits two logical rows");
    assert_eq!(admission.rows_accepted, 2);
}

fn empty_request() -> InsertBatchRequest {
    InsertBatchRequest {
        table: String::new(),
        arrow_ipc: Default::default(),
        wyrd_batch_id: uuid::Uuid::now_v7().as_bytes().to_vec().into(),
    }
}

/// Encodes one non-empty logical batch for the public native ingress regression.
///
/// # Panics
///
/// Panics when the fixed Arrow batch or IPC stream cannot be constructed.
fn valid_arrow_ipc() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(Int64Array::from(vec![11_i64, 22_i64]))],
    )
    .expect("valid ingress batch");
    let mut bytes = Vec::new();
    let mut writer =
        StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("IPC writer initializes");
    writer.write(&batch).expect("IPC batch writes");
    writer.finish().expect("IPC stream finishes");
    bytes
}

async fn bind_free_loopback() -> SocketAddr {
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind probe listener");
    let addr = probe.local_addr().expect("probe local addr");
    drop(probe);
    addr
}

async fn connect_channel(addr: SocketAddr) -> Channel {
    let url = format!("http://{addr}");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match Channel::from_shared(url.clone())
            .expect("endpoint parses")
            .connect()
            .await
        {
            Ok(channel) => return channel,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "gRPC channel never connected within 5s: {error}"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn connect_grpc(addr: SocketAddr) -> BifrostIngestServiceClient<Channel> {
    BifrostIngestServiceClient::new(connect_channel(addr).await)
}

/// Builds a valid resource whose encoded body is bulky but transport-admissible.
///
/// The size must clear two independent ceilings for this fixture to prove what
/// it claims. Byte-weighted transport admission runs at the server edge, ahead
/// of authentication, and refuses an encoded body larger than the composed
/// aggregate reserve with `ResourceExhausted` — a refusal that would mask the
/// authentication decision under test. Sizing the payload from the live
/// admission bounds keeps the body large enough that decoding it would be
/// visible work, while guaranteeing the request reaches the authenticator.
fn near_cap_resource(payload_bytes: usize) -> Resource {
    Resource {
        attributes: vec![KeyValue {
            key: "payload".to_owned(),
            value: Some(AnyValue {
                value: Some(any_value::Value::StringValue("x".repeat(payload_bytes))),
            }),
        }],
        dropped_attributes_count: 0,
        entity_refs: Vec::new(),
    }
}

/// Proves outer authentication rejects every signal before codec or Scribe work.
#[tokio::test]
async fn otlp_unauthenticated_precedes_decode() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server.state();
    let (_, health_service) = health_reporter();
    let router = build_app_grpc(
        state,
        health_service,
        GrpcRouterConfig {
            reflection_enabled: false,
            tls_identity: None,
        },
    )
    .expect("gRPC router builds");
    let bind = bind_free_loopback().await;
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    tokio::spawn(async move { serve_grpc(router, bind, token).await });
    let channel = connect_channel(bind).await;
    wyrd_server::grpc::reset_otlp_codec_activity();
    let admission = state.bifrost.transport_admission();
    let payload_bytes = admission.limit_bytes().min(admission.message_limit_bytes()) / 2;
    assert!(
        payload_bytes > 0,
        "the composed transport reserve must admit a non-empty encoded body"
    );

    let trace = ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(near_cap_resource(payload_bytes)),
            ..ResourceSpans::default()
        }],
    };
    let trace_status = TraceServiceClient::new(channel.clone())
        .export(Request::new(trace))
        .await
        .expect_err("missing trace auth must win");
    assert_eq!(trace_status.code(), Code::Unauthenticated);

    let metrics = ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(near_cap_resource(payload_bytes)),
            ..ResourceMetrics::default()
        }],
    };
    let mut metrics_request = Request::new(metrics);
    metrics_request.metadata_mut().insert(
        "authorization",
        "Bearer invalid"
            .parse()
            .expect("static invalid token metadata"),
    );
    let metrics_status = MetricsServiceClient::new(channel.clone())
        .export(metrics_request)
        .await
        .expect_err("invalid metrics auth must win");
    assert_eq!(metrics_status.code(), Code::Unauthenticated);

    let logs = ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(near_cap_resource(payload_bytes)),
            ..ResourceLogs::default()
        }],
    };
    let logs_status = LogsServiceClient::new(channel)
        .export(Request::new(logs))
        .await
        .expect_err("missing logs auth must win");
    assert_eq!(logs_status.code(), Code::Unauthenticated);

    shutdown.cancel();
    let activity = wyrd_server::grpc::snapshot_otlp_codec_activity();
    assert_eq!(activity.preflight, 0);
    assert_eq!(activity.decode, 0);
    assert_eq!(activity.scribe_reservation, 0);

    server.shutdown().await.expect("server shuts down");
}

#[tokio::test]
async fn ingest_unauthenticated_is_rejected() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server.state();
    let (_, health_service) = health_reporter();
    let router = build_app_grpc(
        state,
        health_service,
        GrpcRouterConfig {
            reflection_enabled: false,
            tls_identity: None,
        },
    )
    .expect("gRPC router builds with token verifier");

    let bind = bind_free_loopback().await;
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    tokio::spawn(async move { serve_grpc(router, bind, token).await });

    let mut client = connect_grpc(bind).await;
    let status = client
        .insert_batch(Request::new(empty_request()))
        .await
        .expect_err("unauthenticated ingest must be rejected");

    shutdown.cancel();

    assert_eq!(
        status.code(),
        Code::Unauthenticated,
        "missing token must yield Unauthenticated; got: {status:?}"
    );

    server.shutdown().await.expect("server shuts down");
}

#[tokio::test]
async fn ingest_valid_token_is_not_rejected_as_unauthenticated() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server.state();
    let tenant = DataTenantId::new_v7();
    let jwt = mint_user_jwt(state, tenant, &[]);

    let (_, health_service) = health_reporter();
    let router = build_app_grpc(
        state,
        health_service,
        GrpcRouterConfig {
            reflection_enabled: false,
            tls_identity: None,
        },
    )
    .expect("gRPC router builds with token verifier");

    let bind = bind_free_loopback().await;
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    tokio::spawn(async move { serve_grpc(router, bind, token).await });

    let mut client = connect_grpc(bind).await;
    let mut request = Request::new(empty_request());
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {jwt}").parse().expect("metadata value"),
    );

    let result = client.insert_batch(request).await;
    shutdown.cancel();

    match result {
        Ok(_) => {}
        Err(status) => {
            assert_ne!(
                status.code(),
                Code::Unauthenticated,
                "valid token must not yield Unauthenticated; got: {status:?}"
            );
        }
    }

    server.shutdown().await.expect("server shuts down");
}

/// Routes valid Arrow IPC through Gate, Scribe catalog resolution, and durable WAL ACK.
///
/// # Panics
///
/// Panics when fixture construction, authenticated ingress, shutdown, WAL
/// replay, or the durability assertions fail.
#[tokio::test]
async fn embedded_ingest_resolves_catalog_and_durably_acknowledges_arrow() {
    const TABLE_NAME: &str = "embedded_ingress_catalog";
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server.state();
    let tenant = DataTenantId::new_v7();
    seed_tenant(&server, tenant, "embedded-ingress-catalog").await;
    let mut tenant_conn = TenantConn::acquire(state.postgres.app_pool(), tenant)
        .await
        .expect("tenant role seed connection");
    seed_builtin_roles_for_tenant(&mut tenant_conn, tenant)
        .await
        .expect("built-in roles seed for ingress principal");
    tenant_conn.commit().await.expect("role seed commits");
    state
        .bifrost_catalog()
        .expect("composed server exposes its Bifrost catalog")
        .register_dataset(
            tenant,
            TableRef::new(BifrostNamespace::Datasets, TABLE_NAME),
            vec![Field::new("value", DataType::Int64, false)],
            None,
            None,
        )
        .await
        .expect("logical dataset registers for the authenticated tenant");
    let jwt = mint_user_jwt(state, tenant, &["admin"]);
    let batch_id = uuid::Uuid::now_v7();

    let (_, health_service) = health_reporter();
    let router = build_app_grpc(
        state,
        health_service,
        GrpcRouterConfig {
            reflection_enabled: false,
            tls_identity: None,
        },
    )
    .expect("gRPC router builds with token verifier");
    let bind = bind_free_loopback().await;
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    tokio::spawn(async move { serve_grpc(router, bind, token).await });

    let mut request = Request::new(InsertBatchRequest {
        table: format!("vala.datasets.{TABLE_NAME}"),
        arrow_ipc: valid_arrow_ipc().into(),
        wyrd_batch_id: batch_id.as_bytes().to_vec().into(),
    });
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {jwt}").parse().expect("metadata value"),
    );
    let response = connect_grpc(bind)
        .await
        .insert_batch(request)
        .await
        .expect("valid logical ingress reaches Scribe and is durably acknowledged")
        .into_inner();
    assert_eq!(response.wyrd_batch_id.as_ref(), batch_id.as_bytes());

    // The WAL is read before shutdown on purpose. The acknowledgement above is
    // the durability claim under test: it is returned only once the record is
    // fsynced into the WAL, so the segment must be replayable at this instant.
    // Shutdown drains and persists the stream, which retires the very segments
    // that carry the evidence, so a replay after it legitimately finds nothing.
    let replayed = replay_wal_directory(
        server
            .scribe_wal_root_for_test()
            .expect("Scribe-composed server owns a WAL root"),
    )
    .expect("acknowledged WAL replays");
    let accepted = replayed
        .values()
        .find(|stream| stream.seal_key.table.name == TABLE_NAME)
        .unwrap_or_else(|| {
            let replayed_tables = replayed
                .values()
                .map(|stream| stream.seal_key.table.fqn())
                .collect::<Vec<_>>();
            panic!(
                "catalog-resolved logical table has durable WAL state; replayed: {replayed_tables:?}"
            )
        });
    assert_eq!(accepted.data_records.len(), 1);

    state
        .bifrost_ingest()
        .expect("fixture retains Scribe")
        .scribe()
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
    shutdown.cancel();
    server.shutdown().await.expect("server shuts down");
}

/// Rejects an unauthenticated private tail request before malformed input reaches lookup.
#[tokio::test]
async fn scribe_tail_unauthenticated_is_rejected_before_lookup() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server.state();
    let (_, health_service) = health_reporter();
    let router = build_app_grpc(
        state,
        health_service,
        GrpcRouterConfig {
            reflection_enabled: false,
            tls_identity: None,
        },
    )
    .expect("gRPC router builds with token verifier");
    let bind = bind_free_loopback().await;
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    tokio::spawn(async move { serve_grpc(router, bind, token).await });

    let mut client = ScribeTailServiceClient::new(connect_channel(bind).await);
    let status = client
        .acquire_fence(Request::new(AcquireTailFenceRequest::default()))
        .await
        .expect_err("missing workload token must fail before request conversion");
    shutdown.cancel();
    assert_eq!(status.code(), Code::Unauthenticated);

    server.shutdown().await.expect("server shuts down");
}

/// Produces identical bounded non-empty pages through local and generated-tonic readers.
#[tokio::test]
async fn scribe_tail_local_and_tonic_pages_match() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server.state();
    let tenant = server.data_tenant_id();
    seed_tail_rows(state, tenant).await;
    let request = non_empty_tail_request(tenant);
    let local_reader = state
        .bifrost_ingest()
        .expect("embedded state retains Scribe")
        .tail_reader();
    let local = LocalTailReadTransport::new(local_reader);
    let local_fence = local
        .acquire_fence(request.clone())
        .await
        .expect("local metadata fence acquires");

    let (_, health_service) = health_reporter();
    let router = build_app_grpc(
        state,
        health_service,
        GrpcRouterConfig {
            reflection_enabled: false,
            tls_identity: None,
        },
    )
    .expect("gRPC router builds with token verifier");
    let bind = bind_free_loopback().await;
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    tokio::spawn(async move { serve_grpc(router, bind, token).await });
    let remote = TonicTailReadTransport::new(
        ScribeTailServiceClient::new(connect_channel(bind).await),
        &mint_service_jwt(state, tenant),
    )
    .expect("workload token configures remote transport");
    let query_id = request.query_id;
    let remote_lease = remote
        .acquire_fence_with_capability(request.clone(), mint_tail_ticket(state, &request))
        .await
        .expect("remote metadata fence and capability acquire");
    let remote_fence = remote_lease.fence.clone();
    assert_eq!(local_fence.inclusive_live, remote_fence.inclusive_live);

    let local_first = local
        .read_page(DomainTailPageRequest {
            query_id,
            fence_id: local_fence.fence_id,
            after: None,
            max_rows: 1,
            max_encoded_bytes: 1024 * 1024,
        })
        .await
        .expect("local first page reads");
    let remote_first = remote
        .read_page_with_capability(
            DomainTailPageRequest {
                query_id,
                fence_id: remote_fence.fence_id,
                after: None,
                max_rows: 1,
                max_encoded_bytes: 1024 * 1024,
            },
            remote_lease.capability.clone(),
        )
        .await
        .expect("remote first page reads");
    assert_eq!(local_first.batches, remote_first.batches);
    assert_eq!(local_first.next, remote_first.next);
    assert!(!local_first.complete);
    assert!(!remote_first.complete);

    let local_second = local
        .read_page(DomainTailPageRequest {
            query_id,
            fence_id: local_fence.fence_id,
            after: local_first.next,
            max_rows: 1,
            max_encoded_bytes: 1024 * 1024,
        })
        .await
        .expect("local continuation reads");
    let remote_second = remote
        .read_page_with_capability(
            DomainTailPageRequest {
                query_id,
                fence_id: remote_fence.fence_id,
                after: remote_first.next,
                max_rows: 1,
                max_encoded_bytes: 1024 * 1024,
            },
            remote_lease.capability.clone(),
        )
        .await
        .expect("remote continuation reads");
    assert_eq!(local_second.batches, remote_second.batches);
    assert_eq!(local_second.next, remote_second.next);
    assert!(local_second.complete);
    assert!(remote_second.complete);

    assert!(
        local
            .release_fence(local_fence.fence_id)
            .expect("local fence release succeeds")
            .released
    );
    remote
        .release_fence_with_capability(query_id, remote_fence.fence_id, remote_lease.capability)
        .await
        .expect("remote fence release succeeds");
    shutdown.cancel();

    server.shutdown().await.expect("server shuts down");
}

/// Denies cross-tenant page and release RPCs without exposing or removing the fence.
#[tokio::test]
async fn scribe_tail_cross_tenant_read_and_release_are_denied() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server.state();
    let owner_tenant = DataTenantId::new_v7();
    let other_tenant = DataTenantId::new_v7();
    seed_tenant(&server, owner_tenant, "tail-owner").await;
    seed_tenant(&server, other_tenant, "tail-other").await;
    seed_tail_rows(state, owner_tenant).await;

    let (_, health_service) = health_reporter();
    let router = build_app_grpc(
        state,
        health_service,
        GrpcRouterConfig {
            reflection_enabled: false,
            tls_identity: None,
        },
    )
    .expect("gRPC router builds with token verifier");
    let bind = bind_free_loopback().await;
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    tokio::spawn(async move { serve_grpc(router, bind, token).await });
    let channel = connect_channel(bind).await;
    let owner = TonicTailReadTransport::new(
        ScribeTailServiceClient::new(channel.clone()),
        &mint_service_jwt(state, owner_tenant),
    )
    .expect("owner transport configures");
    let owner_request = non_empty_tail_request(owner_tenant);
    let owner_query_id = owner_request.query_id;
    let owner_lease = owner
        .acquire_fence_with_capability(
            owner_request.clone(),
            mint_tail_ticket(state, &owner_request),
        )
        .await
        .expect("owner fence acquires");
    let fence = owner_lease.fence.clone();
    let assertion_pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("fixture exposes a migrator assertion pool");

    let (owner_seq_before, owner_count_before): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(MAX(seq), 0), COUNT(*) FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND operation = 'bifrost.scribe.tail_security'",
    )
    .bind(owner_tenant.as_uuid())
    .fetch_one(&assertion_pool)
    .await
    .expect("owner tail-security audit baseline");
    let system_seq_before: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM vala.audit_outbox \
         WHERE data_tenant_id = $1",
    )
    .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
    .fetch_one(&assertion_pool)
    .await
    .expect("system audit-chain baseline");

    let other_jwt = mint_service_jwt(state, other_tenant);
    let mut client = ScribeTailServiceClient::new(channel);
    let mut page: Request<wyrd_tonic::wyrd::v1::TailPageRequest> = Request::new(
        DomainTailPageRequest {
            query_id: owner_query_id,
            fence_id: fence.fence_id,
            after: None,
            max_rows: 1,
            max_encoded_bytes: 1024 * 1024,
        }
        .into(),
    );
    page.get_mut().tail_capability = owner_lease.capability.clone();
    page.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {other_jwt}")
            .parse()
            .expect("metadata value"),
    );
    let page_status = client
        .read_fence_page(page)
        .await
        .expect_err("foreign tenant cannot read retained rows");
    assert_eq!(page_status.code(), Code::PermissionDenied);

    let owner_jwt = mint_service_jwt(state, owner_tenant);
    let mut wrong_query_page: Request<wyrd_tonic::wyrd::v1::TailPageRequest> = Request::new(
        DomainTailPageRequest {
            query_id: uuid::Uuid::now_v7(),
            fence_id: fence.fence_id,
            after: None,
            max_rows: 1,
            max_encoded_bytes: 1024 * 1024,
        }
        .into(),
    );
    wrong_query_page.get_mut().tail_capability = owner_lease.capability.clone();
    wrong_query_page.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {owner_jwt}")
            .parse()
            .expect("metadata value"),
    );
    let wrong_query_status = client
        .read_fence_page(wrong_query_page)
        .await
        .expect_err("query mismatch cannot read retained rows");
    assert_eq!(wrong_query_status.code(), Code::PermissionDenied);

    let mut release = Request::new(ReleaseTailFenceRequest {
        query_id: owner_query_id.as_bytes().to_vec(),
        fence_id: fence.fence_id.as_uuid().as_bytes().to_vec(),
        tail_capability: owner_lease.capability.clone(),
    });
    release.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {other_jwt}")
            .parse()
            .expect("metadata value"),
    );
    let release_status = client
        .release_fence(release)
        .await
        .expect_err("foreign tenant cannot remove retained rows");
    assert_eq!(release_status.code(), Code::PermissionDenied);

    let mut wrong_query_release = Request::new(ReleaseTailFenceRequest {
        query_id: uuid::Uuid::now_v7().as_bytes().to_vec(),
        fence_id: fence.fence_id.as_uuid().as_bytes().to_vec(),
        tail_capability: owner_lease.capability.clone(),
    });
    wrong_query_release.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {owner_jwt}")
            .parse()
            .expect("metadata value"),
    );
    let wrong_query_release_status = client
        .release_fence(wrong_query_release)
        .await
        .expect_err("query mismatch cannot release retained rows");
    assert_eq!(wrong_query_release_status.code(), Code::PermissionDenied);

    /// One `bifrost.scribe.tail_security` audit row projected from
    /// `vala.audit_outbox`: `(data_tenant_id, principal_id, principal_kind,
    /// auth_method, permission, decision, result, detail)`.
    type SecurityAuditRow = (
        uuid::Uuid,
        uuid::Uuid,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
    );
    let security_rows: Vec<SecurityAuditRow> = sqlx::query_as(
        "SELECT data_tenant_id, principal_id, principal_kind, auth_method, permission, \
                decision, result, detail::text \
         FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND operation = 'bifrost.scribe.tail_security' AND seq > $2 \
         ORDER BY seq",
    )
    .bind(owner_tenant.as_uuid())
    .bind(owner_seq_before)
    .fetch_all(&assertion_pool)
    .await
    .expect("durable tail-security audit rows");
    assert_eq!(
        security_rows.len() as i64,
        4,
        "one committed TailBinding row per denied page/release request"
    );
    let owner_count_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 AND operation = 'bifrost.scribe.tail_security'",
    )
    .bind(owner_tenant.as_uuid())
    .fetch_one(&assertion_pool)
    .await
    .expect("owner tail-security audit observation");
    assert_eq!(owner_count_after - owner_count_before, 4);
    for (
        data_tenant_id,
        principal_id,
        principal_kind,
        auth_method,
        permission,
        decision,
        result,
        detail,
    ) in security_rows
    {
        assert_eq!(data_tenant_id, owner_tenant.as_uuid());
        assert_eq!(principal_id, PLATFORM_AUDIT_PRINCIPAL.as_uuid());
        assert_eq!(principal_kind, "service");
        assert_eq!(auth_method, "internal");
        assert_eq!(permission, "bifrost:query:tail");
        assert_eq!(decision, "deny");
        assert_eq!(result, "failure");
        let detail: serde_json::Value =
            serde_json::from_str(&detail.expect("security detail is present"))
                .expect("security detail is valid JSON");
        assert_eq!(detail["kind"], "bifrost_security_violation");
        assert_eq!(detail["violation"], "tail_binding");
        assert_eq!(detail["phase"], "peer");
        assert!(detail["query_digest"].is_null());
    }
    let system_seq_after: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM vala.audit_outbox \
         WHERE data_tenant_id = $1",
    )
    .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
    .fetch_one(&assertion_pool)
    .await
    .expect("system audit-chain observation");
    assert_eq!(system_seq_after, system_seq_before);

    let owner_page = owner
        .read_page_with_capability(
            DomainTailPageRequest {
                query_id: owner_query_id,
                fence_id: fence.fence_id,
                after: None,
                max_rows: 1,
                max_encoded_bytes: 1024 * 1024,
            },
            owner_lease.capability.clone(),
        )
        .await
        .expect("denied release leaves the owner fence readable");
    assert_eq!(owner_page.batches.len(), 1);
    owner
        .release_fence_with_capability(owner_query_id, fence.fence_id, owner_lease.capability)
        .await
        .expect("owner can still release its fence");
    shutdown.cancel();

    server.shutdown().await.expect("server shuts down");
}
