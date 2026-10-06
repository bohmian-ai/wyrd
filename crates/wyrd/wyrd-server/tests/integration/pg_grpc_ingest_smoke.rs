//! Process-level gRPC ingest smoke tests.
//!
//! Stands up `build_app_grpc` + `serve_grpc` and proves the C1 ingest auth
//! seam works correctly: an unauthenticated stream is rejected, and a valid
//! token passes auth (the empty stream then terminates without an Unauthenticated
//! error). Reuses the same key/verifier setup as `router_smoke.rs`.

use arrow::array::{ArrayRef, Int64Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sqlx::PgPool;
use uuid::Uuid;

use bytes::Bytes;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{CompactionRegistration, TableRef};
use vala_bifrost_redux::contracts::{IngressPayload, Scribe, ScribeIngressFrame};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint as ReduxSchemaFingerprint;
use vala_bifrost_redux::scribe::{replay::replay_wal_directory, tail_rpc::TonicTailReadTransport};
use vala_sql::TenantConn;
use wyrd_auth_verify::TokenPrincipalRef;
use wyrd_runtime::{
    BifrostPermissionScope, BifrostTableScope, PermissionScope, PermissionSet, Principal,
    PrincipalId, PrincipalKind, RoleRef,
};
use wyrd_server::AppState;
use wyrd_server::auth::seed::seed_builtin_roles_for_tenant;
use wyrd_server::grpc::{GrpcRouterConfig, build_app_grpc, build_peer_grpc, serve_grpc};
use wyrd_server::verification::authority::{SystemReadAuthority, SystemReadAuthorityError};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::reference::CardRefScope;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::TenantTableBinding;
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
use wyrd_tonic::tonic::{Code, Request, Status};
use wyrd_tonic::tonic_health::server::health_reporter;
use wyrd_tonic::wyrd::v1::bifrost_ingest_service_client::BifrostIngestServiceClient;
use wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient;
use wyrd_tonic::wyrd::v1::{InsertBatchRequest, ListActiveStreamsRequest};

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
    server
        .ensure_builtin_tables_for_test(tenant)
        .await
        .expect("the seeded tenant receives every built-in table");
}

/// Mints one user token carrying the requested built-in roles' permissions.
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
        card_ref_scope: CardRefScope::default(),
    };
    state
        .auth
        .issuing_key
        .as_ref()
        .expect("test state has issuing key")
        .issue_access_token(
            wyrd_auth_issue::AccessGrant {
                principal,
                roles: roles
                    .iter()
                    .map(|role| RoleRef::new(role).expect("static role is valid"))
                    .collect(),
                permissions: wyrd_runtime::builtin_roles::BUILTIN_ROLES
                    .iter()
                    .filter(|builtin| roles.contains(&builtin.name))
                    .flat_map(|builtin| builtin.permissions.iter().cloned())
                    .collect(),
                credential_id: None,
                act: None,
                audience: wyrd_spec::auth::TokenAudience::Wyrd,
            },
            ChronoDuration::minutes(5),
        )
        .expect("test jwt mints")
}

/// The instant the private-tail fixture writes its rows at.
///
/// Production Scribe admission bounds event time to a window around now, so a
/// frozen literal would age out of the accepted range and start failing.
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

/// The caller-owned logical table the private-tail fixtures read.
///
/// `vala.bifrost` is a control namespace with no registrable table, and the
/// Scribe catalog owner refuses an unregistered logical frame. A caller-owned
/// `vala.datasets` table is the only registrable shape for a fixture schema,
/// so the tail fixtures seed and read this one.
const TAIL_TABLE: &str = "tail_events";

/// Names the private-tail fixture table for `tenant` on the discovery wire.
fn tail_binding(tenant: DataTenantId) -> TenantTableBinding {
    TenantTableBinding {
        tenant_id: tenant,
        namespace: "datasets".to_owned(),
        table: TAIL_TABLE.to_owned(),
    }
}

/// Builds one raw `ListActiveStreams` request; the mTLS peer certificate is
/// the only credential.
fn list_request(binding: TenantTableBinding) -> Request<ListActiveStreamsRequest> {
    Request::new(ListActiveStreamsRequest {
        binding: Some(binding.into()),
    })
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
///
/// # Panics
///
/// Panics if the composed server exposes no Bifrost catalog or Scribe, if
/// dataset registration fails, or if Scribe does not admit both rows.
async fn seed_tail_rows(state: &AppState, tenant: DataTenantId) {
    let rows = non_empty_tail_batch();
    let principal = test_principal(tenant);
    let table = TableRef::new(BifrostNamespace::Datasets, TAIL_TABLE);
    state
        .bifrost_catalog()
        .expect("composed server exposes its Bifrost catalog")
        .register_dataset(
            tenant,
            table.clone(),
            vec![Field::new("value", DataType::Int64, false)],
            None,
            CompactionRegistration::default(),
        )
        .await
        .expect("tail fixture dataset registers for the authenticated tenant");
    let request_id = RequestId::now_v7();
    let measured_wire_bytes = rows.get_array_memory_size();
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
            measured_wire_bytes,
            payload: IngressPayload::Canonical(
                vala_bifrost_redux::contracts::CanonicalIngress::unreserved(vec![rows]),
            ),
        },
    )
    .await
    .expect("embedded Scribe admits two logical rows");
    assert_eq!(admission.rows_accepted, 2);
}

fn empty_request() -> InsertBatchRequest {
    InsertBatchRequest {
        table: String::new(),
        arrow_ipc: Bytes::default(),
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

/// Serves this state's private peer router and returns its address with the
/// authority every client dial must chain to.
///
/// `ScribeTailService` is mounted only on the mutually authenticated peer
/// plane, so a tail contract can only be exercised through a real peer
/// listener. The certificate authority is minted per call: the router is built
/// here rather than taken from a bound server, so nothing else trusts it.
async fn serve_peer_grpc(
    state: &AppState,
) -> (
    SocketAddr,
    CancellationToken,
    wyrd_testing::bifrost::peer_ca::BifrostPeerCa,
) {
    let authority = wyrd_testing::bifrost::peer_ca::BifrostPeerCa::generate(
        wyrd_server::config::PEER_SERVER_NAME,
    )
    .expect("peer certificate authority mints");
    let leaf = authority
        .issue_leaf("peer-listener")
        .expect("peer leaf issues");
    let tls = wyrd_tonic::server::MutualTlsServerConfig::from_pem(
        leaf.certificate_pem().as_bytes(),
        leaf.private_key_pem().as_bytes(),
        authority.ca_certificate_pem().as_bytes(),
    );
    let router = build_peer_grpc(state, tls)
        .expect("peer router builds")
        .expect("an API-serving target mounts the peer plane");
    let bind = bind_free_loopback().await;
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    tokio::spawn(async move { serve_grpc(router, bind, token).await });
    (bind, shutdown, authority)
}

/// Dials a served peer listener as an authorized member of its trust domain.
async fn connect_peer_channel(
    addr: SocketAddr,
    authority: &wyrd_testing::bifrost::peer_ca::BifrostPeerCa,
) -> Channel {
    let leaf = authority
        .issue_leaf("peer-client")
        .expect("client leaf issues");
    let endpoint = wyrd_tonic::transport::mutually_authenticated_tls_endpoint(
        format!("https://{addr}"),
        authority.ca_certificate_pem().as_bytes(),
        authority.server_name().to_owned(),
        leaf.certificate_pem().as_bytes(),
        leaf.private_key_pem().as_bytes(),
    )
    .expect("mutually authenticated endpoint builds");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match endpoint.clone().connect().await {
            Ok(channel) => return channel,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "peer listener never accepted a dial: {error}"
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
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

/// A well-formed token for the fixture tenant reaches the ingest service.
///
/// The negative sibling proves a missing token is refused; this one pins the
/// other side, so a revocation or tenant-admission change cannot start
/// rejecting every valid caller while the refusal test still passes.
///
/// # Panics
/// Panics when the test server fails to start, seed its tenant, build its
/// gRPC router, bind its listener, or shut down, and when a valid token is
/// answered with `Unauthenticated`.
#[tokio::test]
async fn ingest_valid_token_is_not_rejected_as_unauthenticated() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server.state();
    // The fixture's own tenant, so any refusal past verification is about the
    // request rather than a tenant the fixture never provisioned.
    let tenant = server.data_tenant_id();
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
            CompactionRegistration::default(),
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

    // The ACK is returned only after the WAL fsync, so the accepted record is
    // already on disk. Replay before draining Scribe: a graceful shutdown
    // flushes and publishes every accepted generation and seals its LSNs, and
    // replay suppresses sealed records, so a post-shutdown replay is correctly
    // empty and proves nothing about the acknowledged append.
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

/// Lists the seeded live partition through the mTLS generated-tonic client.
///
/// # Panics
/// Panics if discovery fails or names another stream than the local Scribe.
#[tokio::test]
async fn scribe_tail_tonic_lists_seeded_partition() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server.state();
    let tenant = server.data_tenant_id();
    seed_tail_rows(state, tenant).await;
    let stream = state
        .bifrost_ingest()
        .expect("embedded state retains Scribe")
        .tail_service()
        .stream();

    let (bind, shutdown, authority) = serve_peer_grpc(state).await;
    let remote = TonicTailReadTransport::new(ScribeTailServiceClient::new(
        connect_peer_channel(bind, &authority).await,
    ));
    let streams = remote
        .list_active_streams(tail_binding(tenant))
        .await
        .expect("authorized discovery lists the seeded partition");
    assert_eq!(streams.len(), 1, "one live partition holds the seeded rows");
    assert_eq!(
        streams[0].stream.node_id.as_uuid(),
        stream.node_id.as_uuid()
    );
    assert_eq!(
        streams[0].stream.writer_epoch,
        u64::try_from(stream.writer_epoch.as_i64()).expect("fixture epoch is non-negative")
    );
    shutdown.cancel();

    server.shutdown().await.expect("server shuts down");
}

/// Listing trusts the mTLS-authenticated internal peer and is scoped to one table.
///
/// The peer certificate is the only credential, and any cluster peer may list
/// any tenant's table: that is the accepted listing trust model, and no ticket
/// is presented. What it lists is still exactly the named tenant table, so
/// another tenant's live rows and another table of the same tenant never
/// appear.
///
/// # Panics
/// Panics when a listing fails or names another tenant's or table's partitions.
#[tokio::test]
async fn scribe_tail_listing_trusts_the_peer_and_scopes_to_the_table() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server.state();
    let owner_tenant = DataTenantId::new_v7();
    let other_tenant = DataTenantId::new_v7();
    seed_tenant(&server, owner_tenant, "tail-owner").await;
    seed_tenant(&server, other_tenant, "tail-other").await;
    seed_tail_rows(state, owner_tenant).await;

    let (bind, shutdown, authority) = serve_peer_grpc(state).await;
    let mut client = ScribeTailServiceClient::new(connect_peer_channel(bind, &authority).await);

    let owner = client
        .list_active_streams(list_request(tail_binding(owner_tenant)))
        .await
        .expect("the internal peer lists the owner's table")
        .into_inner();
    assert_eq!(
        owner.streams.len(),
        1,
        "the owner's seeded partition is listed"
    );

    let other = client
        .list_active_streams(list_request(tail_binding(other_tenant)))
        .await
        .expect("the internal peer lists another tenant's table")
        .into_inner();
    assert!(
        other.streams.is_empty(),
        "another tenant never sees the owner's rows"
    );

    let mut other_table = tail_binding(owner_tenant);
    other_table.table = "tail_events_absent".to_owned();
    let absent = client
        .list_active_streams(list_request(other_table))
        .await
        .expect("the internal peer lists an idle table")
        .into_inner();
    assert!(
        absent.streams.is_empty(),
        "another table never sees these rows"
    );
    shutdown.cancel();

    server.shutdown().await.expect("server shuts down");
}

/// Encodes one complete `vala.verification.results` row with `card_ref`.
///
/// Carries every authored column of the built-in result table, with
/// `card_ref` in the correlation column.
///
/// # Panics
///
/// Panics when the fixed Arrow batch or IPC stream cannot be constructed.
fn verification_result_ipc(card_ref: &str) -> Vec<u8> {
    let utc = || DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()));
    let text = |name: &str, nullable: bool| Field::new(name, DataType::Utf8, nullable);
    let fields = vec![
        text("result_id", false),
        text("implementation", false),
        text("execution_status", false),
        text("verdict", false),
        text("verifier_version", false),
        text("owner_card_uid", true),
        text("subject_card_uid", false),
        text("binding_id", true),
        text("trigger_identity", true),
        text("source_record_id", true),
        Field::new("window_start", utc(), true),
        Field::new("window_end", utc(), true),
        Field::new("started_at", utc(), false),
        Field::new("ended_at", utc(), false),
        text("details", true),
        text("card_ref", true),
    ];
    let schema = Arc::new(Schema::new(fields));
    let now = Utc::now().timestamp_micros();
    let value = |value: &str| -> ArrayRef { Arc::new(StringArray::from(vec![Some(value)])) };
    let null = || -> ArrayRef { Arc::new(StringArray::from(vec![None::<&str>])) };
    let at = |micros: Option<i64>| -> ArrayRef {
        Arc::new(TimestampMicrosecondArray::from(vec![micros]).with_timezone("UTC"))
    };
    let columns = vec![
        value(&uuid::Uuid::now_v7().to_string()),
        value("drift"),
        value("completed"),
        value("pass"),
        value("1.0.0"),
        null(),
        value(&uuid::Uuid::now_v7().to_string()),
        null(),
        null(),
        null(),
        at(None),
        at(None),
        at(Some(now)),
        at(Some(now)),
        null(),
        value(card_ref),
    ];
    let batch = RecordBatch::try_new(Arc::clone(&schema), columns)
        .expect("valid verification result batch");
    let mut bytes = Vec::new();
    let mut writer =
        StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("IPC writer initializes");
    writer.write(&batch).expect("IPC batch writes");
    writer.finish().expect("IPC stream finishes");
    bytes
}

/// Sends one native Arrow insert to `table` under `jwt`.
///
/// # Errors
///
/// Returns the gRPC status the server answers with when it refuses the write.
///
/// # Panics
///
/// Panics when the bearer metadata cannot be encoded.
async fn insert_as(
    addr: SocketAddr,
    jwt: &str,
    table: &str,
    arrow_ipc: Vec<u8>,
) -> Result<(), Status> {
    let mut request = Request::new(InsertBatchRequest {
        table: table.to_owned(),
        arrow_ipc: arrow_ipc.into(),
        wyrd_batch_id: uuid::Uuid::now_v7().as_bytes().to_vec().into(),
    });
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {jwt}").parse().expect("metadata value"),
    );
    connect_grpc(addr)
        .await
        .insert_batch(request)
        .await
        .map(|_| ())
}

/// One served tenant whose reserved result tables are provisioned.
///
/// Result-table tests share this real path: an in-process server with a
/// seeded tenant, its provisioned SYSTEM principal, the reserved result table
/// provisioned, and a public gRPC listener. Its fields let each test write
/// over gRPC and read the staged audit decisions and replayed WAL.
struct ResultTableHarness {
    /// Composed server whose Scribe, WAL, and audit staging are under test.
    server: WyrdTestServer,
    /// Tenant every write belongs to.
    tenant: DataTenantId,
    /// Persisted SYSTEM principal identity, an attribution identity only.
    system: Uuid,
    /// Loopback address of the public gRPC listener.
    bind: SocketAddr,
    /// Cancels the gRPC listener on shutdown.
    shutdown: CancellationToken,
    /// Migrator pool used only for assertions over audit staging.
    assertion_pool: PgPool,
    /// Highest staged audit sequence for the tenant before any write.
    seq_before: i64,
}

impl ResultTableHarness {
    /// Starts the server, seeds a new tenant named `slug` with its built-in
    /// roles and SYSTEM principal, provisions the result table, and serves
    /// gRPC.
    ///
    /// # Panics
    ///
    /// Panics when any fixture, provisioning, or listener step fails.
    async fn start(slug: &str) -> Self {
        let server = WyrdTestServer::start_in_process()
            .await
            .expect("test server starts");
        let state = server.state();
        let tenant = DataTenantId::new_v7();
        seed_tenant(&server, tenant, slug).await;
        let mut conn = wyrd_sql::TenantConn::acquire(state.postgres.app_pool(), tenant)
            .await
            .expect("tenant connection opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("built-in roles seed");
        let system = wyrd_sql::queries::auth::provision_system_principal(&mut conn)
            .await
            .expect("system principal provisions");
        conn.commit().await.expect("provisioning commits");
        server
            .ensure_builtin_table_for_test(tenant, "verification", "results")
            .await
            .expect("result table provisions");
        let assertion_pool = server
            .pg_fixture()
            .superuser_pool()
            .expect("fixture exposes a migrator assertion pool");
        server
            .wait_oracle_audit_staged(Duration::from_secs(30))
            .await
            .expect("audit outbox settles");
        let seq_before: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(seq), 0) FROM vala.audit_staging WHERE data_tenant_id = $1",
        )
        .bind(tenant.as_uuid())
        .fetch_one(&assertion_pool)
        .await
        .expect("audit chain observation");

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
        Self {
            server,
            tenant,
            system,
            bind,
            shutdown,
            assertion_pool,
            seq_before,
        }
    }

    /// Staged `bifrost.record.write` decisions since start, in order.
    ///
    /// Each row is `(principal kind, resource, outcome)`.
    ///
    /// # Panics
    ///
    /// Panics when audit staging cannot be read or a decision names a
    /// permission other than `bifrost:record:write`.
    async fn write_decisions(&self) -> Vec<(String, String, String)> {
        self.server
            .wait_oracle_audit_staged(Duration::from_secs(30))
            .await
            .expect("audit outbox settles");
        let decisions: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT principal_kind, resource, permission, outcome \
             FROM vala.audit_staging \
             WHERE data_tenant_id = $1 AND operation = 'bifrost.record.write' AND seq > $2 \
             ORDER BY seq",
        )
        .bind(self.tenant.as_uuid())
        .bind(self.seq_before)
        .fetch_all(&self.assertion_pool)
        .await
        .expect("staged write decisions");
        decisions
            .into_iter()
            .map(|(kind, resource, permission, outcome)| {
                assert_eq!(permission, "bifrost:record:write");
                (kind, resource, outcome)
            })
            .collect()
    }

    /// Asserts the staged write decisions equal `expected` and the
    /// acknowledged WAL holds no verification result record.
    ///
    /// # Panics
    ///
    /// Panics when the staged decisions differ from `expected` or the
    /// acknowledged WAL replays any `results`, `result_features`, or
    /// `result_items` record.
    async fn assert_only_denials_and_no_durable_result(
        &self,
        expected: Vec<(String, String, String)>,
    ) {
        assert_eq!(self.write_decisions().await, expected);
        let replayed = replay_wal_directory(
            self.server
                .scribe_wal_root_for_test()
                .expect("Scribe-composed server owns a WAL root"),
        )
        .expect("acknowledged WAL replays");
        let durable_results = replayed
            .values()
            .filter(|stream| {
                ["results", "result_features", "result_items"]
                    .contains(&stream.seal_key.table.name.as_str())
            })
            .map(|stream| stream.data_records.len())
            .sum::<usize>();
        assert_eq!(durable_results, 0, "no refused result write became durable");
    }

    /// Drains Scribe, stops the listener, and shuts the server down.
    ///
    /// # Panics
    ///
    /// Panics when the fixture no longer retains Scribe or shutdown fails.
    async fn shutdown(self) {
        self.server
            .state()
            .bifrost_ingest()
            .expect("fixture retains Scribe")
            .scribe()
            .shutdown(Instant::now() + Duration::from_secs(1))
            .await;
        self.shutdown.cancel();
        self.server.shutdown().await.expect("server shuts down");
    }
}

/// Mints a five-minute token for `principal` carrying only
/// `bifrost:record:write`.
///
/// # Panics
///
/// Panics when the fixture key cannot sign the token.
fn mint_record_writer_jwt(state: &AppState, principal: TokenPrincipalRef) -> String {
    state
        .auth
        .issuing_key
        .as_ref()
        .expect("test state has issuing key")
        .issue_access_token(
            wyrd_auth_issue::AccessGrant {
                principal,
                roles: vec![],
                permissions: [wyrd_runtime::Permission::bifrost_record_write()]
                    .into_iter()
                    .collect(),
                credential_id: None,
                act: None,
                audience: wyrd_spec::auth::TokenAudience::Wyrd,
            },
            ChronoDuration::minutes(5),
        )
        .expect("record-writer jwt mints")
}

/// No public writer reaches a verification result table over gRPC.
///
/// Results are written only by the server's internal capture writer, which
/// never passes Gate. A wildcard administrator and a record-writing
/// card-free service are each refused every one of the three result tables
/// with Gate's reserved-table refusal, each refusal stages exactly one denied
/// decision carrying the true principal kind, and nothing reaches the WAL.
///
/// # Panics
///
/// Panics when any write is admitted or answered with another code or
/// message, when the staged decisions differ from one denial per write in
/// order, or when a result row becomes durable.
#[tokio::test]
async fn public_result_writes_are_refused_over_grpc() {
    let harness = ResultTableHarness::start("public-result-writes").await;
    let state = harness.server.state();
    let service_jwt = mint_record_writer_jwt(
        state,
        TokenPrincipalRef {
            id: PrincipalId::new(Uuid::now_v7()),
            kind: PrincipalKindTag::Service,
            tenant_id: harness.tenant,
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
        },
    );
    let writers = [
        ("user", mint_user_jwt(state, harness.tenant, &["admin"])),
        ("service", service_jwt),
    ];
    let mut expected = Vec::new();
    for table in [
        "vala.verification.results",
        "vala.drift.result_features",
        "vala.eval.result_items",
    ] {
        for (kind, jwt) in &writers {
            let refused = insert_as(
                harness.bind,
                jwt,
                table,
                verification_result_ipc("prod/Verifier/drift@1.0.0"),
            )
            .await
            .expect_err("a public result write is refused");
            assert_eq!(
                refused.code(),
                Code::PermissionDenied,
                "{kind} on {table}: {refused:?}"
            );
            assert!(
                refused.message().contains("reserved built-in table"),
                "{kind} on {table} answers the reserved-table refusal: {refused:?}"
            );
            expected.push(((*kind).to_owned(), table.to_owned(), "denied".to_owned()));
        }
    }
    harness
        .assert_only_denials_and_no_durable_result(expected)
        .await;

    harness.shutdown().await;
}

/// The tokenless SYSTEM read authority reads only the tables it names.
///
/// Resolving needs the tenant's SYSTEM principal; without one it is refused.
/// Naming a table the tenant never registered yields no grant. The
/// observation table is a built-in the tenant was provisioned with, so before
/// any observation the authority already holds exactly
/// `bifrost_query:read` on that table's UID under the SYSTEM principal kind
/// with an empty Card scope; Oracle admits its read of the observation table
/// with an audited read decision and refuses, with an audited denial, a read
/// of another table.
///
/// # Panics
///
/// Panics when resolution succeeds without a principal, the authority carries
/// other authority, or either query or its staged audit decision differs from
/// the expected one.
#[tokio::test]
async fn system_read_authority_reads_only_the_named_table() {
    let harness = ResultTableHarness::start("system-read-authority").await;
    let state = harness.server.state();
    let observations = [TableRef::new(BifrostNamespace::Drift, "observations")];
    let unregistered_table = [TableRef::new(BifrostNamespace::Datasets, "unregistered")];
    let system = Some(PrincipalId::new(harness.system));
    let resolve = |principal: Option<PrincipalId>| {
        SystemReadAuthority::resolve(state, harness.tenant, principal, &observations)
    };
    assert!(
        matches!(
            resolve(None).await,
            Err(SystemReadAuthorityError::SystemPrincipalMissing)
        ),
        "no SYSTEM principal means no read authority"
    );
    let unregistered =
        SystemReadAuthority::resolve(state, harness.tenant, system, &unregistered_table)
            .await
            .expect("authority resolves");
    assert!(
        unregistered
            .context()
            .principal
            .effective_permissions
            .is_empty(),
        "an unregistered table means nothing to read"
    );

    // The tenant was provisioned with every built-in, so the observation table
    // already exists without any write.
    let authority = resolve(system).await.expect("authority resolves");
    let table_uid: Vec<u8> = sqlx::query_scalar(
        "SELECT table_uid FROM vala.bifrost_tables \
         WHERE data_tenant_id = $1 AND fqn = 'vala.drift.observations'",
    )
    .bind(harness.tenant.as_uuid())
    .fetch_one(&harness.assertion_pool)
    .await
    .expect("observation table UID reads");
    let principal = &authority.context().principal;
    assert_eq!(principal.id.as_uuid(), harness.system);
    assert!(matches!(
        &principal.kind,
        PrincipalKind::System { card_ref_scope } if card_ref_scope.is_empty()
    ));
    assert_eq!(
        principal.effective_permissions,
        PermissionSet::from_iter([wyrd_runtime::Permission {
            scope: PermissionScope::Bifrost(BifrostPermissionScope::Table(BifrostTableScope {
                catalog: "vala".to_owned(),
                schema: "drift".to_owned(),
                table_uid: Uuid::from_slice(&table_uid).expect("16-byte table UID"),
            })),
            ..wyrd_runtime::Permission::bifrost_query_read()
        }]),
    );

    let query = |sql: &str| {
        let caller = wyrd_server::query::scheduled::ScheduledQueryCaller::new(
            state.clone(),
            authority.context().clone(),
            CancellationToken::new(),
        );
        let request = wyrd_spec::vala::api::BifrostQueryRequest {
            params: Vec::new(),
            sql: sql.to_owned(),
            deadline_ms: Some(30_000),
        };
        async move { caller.run(request).await }
    };
    query("SELECT COUNT(*) AS n FROM vala.drift.observations")
        .await
        .expect("the authority reads its observation table");
    let denied = query("SELECT COUNT(*) AS n FROM vala.verification.results")
        .await
        .expect_err("the authority is refused another table");
    assert_eq!(
        denied.code(),
        wyrd_spec::error::WyrdError::from(wyrd_spec::vala::error::BifrostError::QueryForbidden)
            .code(),
        "{denied:?}"
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    let decisions = loop {
        let decisions: Vec<(String, String)> = sqlx::query_as(
            "SELECT operation, outcome FROM vala.audit_staging \
             WHERE data_tenant_id = $1 AND principal_id = $2 AND seq > $3 \
               AND operation IN ('bifrost.query.read_decision', 'vala.query.sync') \
             ORDER BY operation",
        )
        .bind(harness.tenant.as_uuid())
        .bind(harness.system)
        .bind(harness.seq_before)
        .fetch_all(&harness.assertion_pool)
        .await
        .expect("staged read decisions");
        if decisions.len() >= 2 || Instant::now() > deadline {
            break decisions;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(
        decisions,
        vec![
            (
                "bifrost.query.read_decision".to_owned(),
                "allowed".to_owned()
            ),
            ("vala.query.sync".to_owned(), "denied".to_owned()),
        ]
    );

    harness.shutdown().await;
}

/// A result write whose token names a forged tenant is refused before any
/// decision or durable write.
///
/// Gate derives the tenant only from the signed token claim; there is no wire
/// tenant to forge. Rewriting an administrator token's `principal.tenant_id`
/// to another tenant or to the system owner, or stripping its signature,
/// breaks verification, so each attempt answers `Unauthenticated`, stages no
/// write decision in the real tenant, and nothing reaches the WAL.
///
/// # Panics
///
/// Panics when the token cannot be decoded or re-encoded, when a forged write
/// is admitted or answered with another code, or when any decision or result
/// row is recorded.
#[tokio::test]
async fn forged_tenant_result_writes_are_refused() {
    use base64::Engine as _;
    let harness = ResultTableHarness::start("result-forged-tenant").await;
    let foreign = DataTenantId::new_v7();
    seed_tenant(&harness.server, foreign, "result-forged-foreign").await;
    let admin_jwt = mint_user_jwt(harness.server.state(), harness.tenant, &["admin"]);
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let parts: Vec<&str> = admin_jwt.split('.').collect();
    let [header, payload, signature] = parts.as_slice() else {
        panic!("the administrator token is a compact JWS");
    };
    let forge = |tenant: DataTenantId| {
        let mut claims: serde_json::Value = serde_json::from_slice(
            &engine
                .decode(payload)
                .expect("the token payload is base64url"),
        )
        .expect("the token payload is JSON");
        claims["principal"]["tenant_id"] = serde_json::json!(tenant.to_string());
        let forged = engine.encode(serde_json::to_vec(&claims).expect("claims encode"));
        format!("{header}.{forged}.{signature}")
    };
    let attempts = [
        ("foreign tenant", forge(foreign)),
        ("system owner", forge(DataTenantId::SYSTEM_OWNER)),
        ("unsigned", format!("{header}.{payload}.")),
    ];
    for (label, jwt) in attempts {
        let refused = insert_as(
            harness.bind,
            &jwt,
            "vala.verification.results",
            verification_result_ipc("prod/Verifier/drift@1.0.0"),
        )
        .await
        .expect_err("a forged tenant is refused");
        assert_eq!(
            refused.code(),
            Code::Unauthenticated,
            "{label}: {refused:?}"
        );
    }
    harness
        .assert_only_denials_and_no_durable_result(vec![])
        .await;

    harness.shutdown().await;
}
