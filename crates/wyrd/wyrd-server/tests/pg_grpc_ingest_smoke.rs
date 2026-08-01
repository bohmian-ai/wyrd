//! Process-level gRPC ingest smoke tests.
//!
//! Stands up `build_app_grpc` + `serve_grpc` and proves the C1 ingest auth
//! seam works correctly: an unauthenticated stream is rejected, and a valid
//! token passes auth (the empty stream then terminates without an Unauthenticated
//! error). Reuses the same key/verifier setup as `router_smoke.rs`.

pub mod support;

mod pg_tests {
    use super::*;
    use arrow::array::{Int64Array, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::record_batch::RecordBatch;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use chrono::{Duration as ChronoDuration, NaiveDate};
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use tokio_util::sync::CancellationToken;
    use vala_bifrost_redux::catalog::TableRef;
    use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint as ReduxSchemaFingerprint;
    use vala_bifrost_redux::scribe::{
        ScribeImpl,
        tail_rpc::{LocalTailReadTransport, TailReadTransport, TonicTailReadTransport},
        wal::{WalConfig, WalWriter},
    };
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_runtime::{PermissionSet, Principal, PrincipalId, PrincipalKind};
    use wyrd_semver::VersionBlock;
    use wyrd_server::AppState;
    use wyrd_server::components::auth::ServerAuth;
    use wyrd_server::grpc::{GrpcRouterConfig, build_app_grpc, serve_grpc};
    use wyrd_server::postgres::ServerPostgres;
    use wyrd_server::state::BifrostIngestRuntime;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        AcquireTailFenceRequest as DomainAcquireTailFenceRequest, EventDay,
        SchemaFingerprint as WireSchemaFingerprint, TailCursor,
        TailPageRequest as DomainTailPageRequest, TenantTableBinding,
    };
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};
    use wyrd_tonic::tonic::transport::Channel;
    use wyrd_tonic::tonic::{Code, Request};
    use wyrd_tonic::tonic_health::server::health_reporter;
    use wyrd_tonic::wyrd::v1::bifrost_ingest_service_client::BifrostIngestServiceClient;
    use wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient;
    use wyrd_tonic::wyrd::v1::{
        AcquireTailFenceRequest, InsertBatchRequest, ReleaseTailFenceRequest,
    };

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    /// Builds one process fixture and returns the temporary roots that keep its IO live.
    async fn test_state() -> (AppState, tempfile::TempDir, tempfile::TempDir) {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let postgres = Arc::new(ServerPostgres::from_parts(
            wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None),
            vala_sql::ValaPostgres::from_pool(app_pool.clone()),
        ));
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let issuing_key = Arc::new(
            IssuingKey::from_ed_pem(
                secrecy::SecretString::from(PRIVATE_KEY_PEM),
                Kid::new("k1").expect("kid is valid"),
                "wyrd",
            )
            .expect("test issuing key loads"),
        );
        let mut keys = HashMap::new();
        keys.insert(
            Kid::new("k1").expect("kid is valid"),
            Arc::new(public_key_from_pem(PUBLIC_KEY_PEM).expect("public key loads")),
        );
        let verifier = Arc::new(TokenVerifier::new(
            keys,
            "wyrd",
            Arc::new(
                wyrd_server::auth::permission_resolver::SqlPermissionResolver::new(Arc::new(
                    app_pool,
                )),
            ),
            WyrdAuthVerifySettings::default(),
        ));
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(signer)));
        let catalog = support::test_catalog().await;
        let redux_catalog = support::test_redux_catalog().await;
        let wal_root = tempfile::tempdir().expect("wal temp dir");
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *uuid::Uuid::now_v7().as_bytes(),
                1,
                WalConfig::default(),
            )
            .expect("wal initializes"),
        );
        let scribe = Arc::new(ScribeImpl::new_for_embedded_with_deps(
            Arc::new(storage.operator().clone()),
            wal,
            &uuid::Uuid::now_v7().to_string(),
            1,
        ));
        let ingest = Arc::new(BifrostIngestRuntime::new(
            scribe,
            Arc::clone(&redux_catalog),
            Arc::clone(&verifier),
            vala_bifrost_redux::gate::limits::IngestLimits::default(),
            None,
        ));
        let gate = ingest.gate();
        (
            AppState::new(postgres, storage, catalog)
                .with_bifrost_redux(redux_catalog)
                .with_bifrost_ingest(ingest)
                .with_bifrost_gate(gate)
                .with_auth(ServerAuth {
                    issuing_key: Some(issuing_key),
                    token_verifier: Some(verifier),
                    ..ServerAuth::default()
                }),
            root,
            wal_root,
        )
    }

    fn mint_user_jwt(state: &AppState, tenant: DataTenantId) -> String {
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
            .issue_user_access_token(principal, vec![], ChronoDuration::minutes(5))
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

    /// Builds two logical rows on the private-tail fixture's UTC event day.
    fn non_empty_tail_batch() -> RecordBatch {
        let timestamp = NaiveDate::from_ymd_opt(2026, 7, 14)
            .expect("static date is valid")
            .and_hms_opt(12, 0, 0)
            .expect("static time is valid")
            .and_utc()
            .timestamp_micros();
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                wyrd_spec::vala::WYRD_EVENT_TIME,
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("value", DataType::Int64, false),
        ]));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(TimestampMicrosecondArray::from(vec![timestamp, timestamp])),
                Arc::new(Int64Array::from(vec![11, 22])),
            ],
        )
        .expect("private-tail fixture batch is valid")
    }

    /// Creates a metadata-only fence request matching the seeded logical schema.
    fn non_empty_tail_request(tenant: DataTenantId) -> DomainAcquireTailFenceRequest {
        let expected = ReduxSchemaFingerprint::from_arrow_schema(&Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        DomainAcquireTailFenceRequest {
            binding: TenantTableBinding {
                tenant_id: tenant,
                namespace: "bifrost".to_owned(),
                table: "events".to_owned(),
            },
            event_day: EventDay::new("2026-07-14").expect("static day is valid"),
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
    async fn seed_tail_rows(state: &AppState, tenant: DataTenantId) {
        let rows = non_empty_tail_batch();
        state
            .bifrost_ingest
            .as_ref()
            .expect("embedded state retains Scribe")
            .scribe()
            .append_durable(ScribeAppend {
                principal: test_principal(tenant),
                table: TableRef::new(BifrostNamespace::Bifrost, "events"),
                schema_fingerprint: ReduxSchemaFingerprint::from_arrow_schema(
                    rows.schema().as_ref(),
                ),
                request_id: RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
                measured_wire_bytes: rows.get_array_memory_size(),
                rows,
            })
            .await
            .expect("embedded Scribe admits two logical rows");
    }

    fn empty_request() -> InsertBatchRequest {
        InsertBatchRequest {
            table: String::new(),
            arrow_ipc: Default::default(),
            wyrd_batch_id: uuid::Uuid::now_v7().as_bytes().to_vec().into(),
        }
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

    #[tokio::test]
    async fn ingest_unauthenticated_is_rejected() {
        let (state, _storage_root, _wal_root) = test_state().await;
        let (_, health_service) = health_reporter();
        let router = build_app_grpc(
            &state,
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
    }

    #[tokio::test]
    async fn ingest_valid_token_is_not_rejected_as_unauthenticated() {
        let (state, _storage_root, _wal_root) = test_state().await;
        let tenant = DataTenantId::new_v7();
        let jwt = mint_user_jwt(&state, tenant);

        let (_, health_service) = health_reporter();
        let router = build_app_grpc(
            &state,
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
    }

    /// Rejects an unauthenticated private tail request before malformed input reaches lookup.
    #[tokio::test]
    async fn scribe_tail_unauthenticated_is_rejected_before_lookup() {
        let (state, _storage_root, _wal_root) = test_state().await;
        let (_, health_service) = health_reporter();
        let router = build_app_grpc(
            &state,
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
    }

    /// Produces identical bounded non-empty pages through local and generated-tonic readers.
    #[tokio::test]
    async fn scribe_tail_local_and_tonic_pages_match() {
        let (state, _storage_root, _wal_root) = test_state().await;
        let tenant = DataTenantId::new_v7();
        seed_tail_rows(&state, tenant).await;
        let request = non_empty_tail_request(tenant);
        let local_reader = state
            .bifrost_ingest
            .as_ref()
            .expect("embedded state retains Scribe")
            .tail_reader();
        let local = LocalTailReadTransport::new(local_reader);
        let local_fence = local
            .acquire_fence(request.clone())
            .await
            .expect("local metadata fence acquires");

        let (_, health_service) = health_reporter();
        let router = build_app_grpc(
            &state,
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
            &mint_service_jwt(&state, tenant),
        )
        .expect("workload token configures remote transport");
        let remote_fence = remote
            .acquire_fence(request)
            .await
            .expect("remote metadata fence acquires");
        assert_eq!(local_fence.inclusive_live, remote_fence.inclusive_live);

        let local_first = local
            .read_page(DomainTailPageRequest {
                fence_id: local_fence.fence_id,
                after: None,
                max_rows: 1,
                max_encoded_bytes: 1024 * 1024,
            })
            .await
            .expect("local first page reads");
        let remote_first = remote
            .read_page(DomainTailPageRequest {
                fence_id: remote_fence.fence_id,
                after: None,
                max_rows: 1,
                max_encoded_bytes: 1024 * 1024,
            })
            .await
            .expect("remote first page reads");
        assert_eq!(local_first.batches, remote_first.batches);
        assert_eq!(local_first.next, remote_first.next);
        assert!(!local_first.complete);
        assert!(!remote_first.complete);

        let local_second = local
            .read_page(DomainTailPageRequest {
                fence_id: local_fence.fence_id,
                after: local_first.next,
                max_rows: 1,
                max_encoded_bytes: 1024 * 1024,
            })
            .await
            .expect("local continuation reads");
        let remote_second = remote
            .read_page(DomainTailPageRequest {
                fence_id: remote_fence.fence_id,
                after: remote_first.next,
                max_rows: 1,
                max_encoded_bytes: 1024 * 1024,
            })
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
            .release_fence(remote_fence.fence_id)
            .await
            .expect("remote fence release succeeds");
        shutdown.cancel();
    }

    /// Denies cross-tenant page and release RPCs without exposing or removing the fence.
    #[tokio::test]
    async fn scribe_tail_cross_tenant_read_and_release_are_denied() {
        let (state, _storage_root, _wal_root) = test_state().await;
        let owner_tenant = DataTenantId::new_v7();
        let other_tenant = DataTenantId::new_v7();
        seed_tail_rows(&state, owner_tenant).await;

        let (_, health_service) = health_reporter();
        let router = build_app_grpc(
            &state,
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
            &mint_service_jwt(&state, owner_tenant),
        )
        .expect("owner transport configures");
        let fence = owner
            .acquire_fence(non_empty_tail_request(owner_tenant))
            .await
            .expect("owner fence acquires");

        let other_jwt = mint_service_jwt(&state, other_tenant);
        let mut client = ScribeTailServiceClient::new(channel);
        let mut page = Request::new(
            DomainTailPageRequest {
                fence_id: fence.fence_id,
                after: None,
                max_rows: 1,
                max_encoded_bytes: 1024 * 1024,
            }
            .into(),
        );
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

        let mut release = Request::new(ReleaseTailFenceRequest {
            fence_id: fence.fence_id.as_uuid().as_bytes().to_vec(),
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

        let owner_page = owner
            .read_page(DomainTailPageRequest {
                fence_id: fence.fence_id,
                after: None,
                max_rows: 1,
                max_encoded_bytes: 1024 * 1024,
            })
            .await
            .expect("denied release leaves the owner fence readable");
        assert_eq!(owner_page.batches.len(), 1);
        owner
            .release_fence(fence.fence_id)
            .await
            .expect("owner can still release its fence");
        shutdown.cancel();
    }

    /// Keeps the legacy managed-column parity contract exact during Redux migration.
    ///
    /// # Panics
    ///
    /// Panics when any managed field differs in name, type, order, or nullability.
    #[test]
    fn legacy_and_redux_managed_columns_are_exactly_equal() {
        let user_fields = vec![Field::new("value", DataType::UInt64, false)];
        let legacy_dynamic = vala_bifrost::schema::with_managed_columns(user_fields.clone());
        let redux_dynamic = vala_bifrost_redux::schema::with_managed_columns(user_fields);
        assert_eq!(legacy_dynamic, redux_dynamic);
        assert_eq!(
            legacy_dynamic
                .iter()
                .map(|field| (
                    field.name().as_str(),
                    field.data_type(),
                    field.is_nullable()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("value", &DataType::UInt64, false),
                ("run_id", &DataType::Utf8, true),
                ("card_uid", &DataType::Utf8, true),
                ("principal_id", &DataType::Utf8, false),
                ("wyrd_request_id", &DataType::Utf8, false),
                (
                    "wyrd_event_time",
                    &DataType::Timestamp(
                        arrow::datatypes::TimeUnit::Microsecond,
                        Some("UTC".into())
                    ),
                    false,
                ),
                (
                    "wyrd_ingested_at",
                    &DataType::Timestamp(
                        arrow::datatypes::TimeUnit::Microsecond,
                        Some("UTC".into())
                    ),
                    false,
                ),
                ("wyrd_batch_id", &DataType::FixedSizeBinary(16), false),
                ("wyrd_row_ordinal", &DataType::Int32, false),
                ("data_tenant_id", &DataType::Utf8, false),
            ]
        );

        for (legacy, redux) in [
            (
                vala_bifrost::tables::managed_columns::ensure_managed_columns(
                    Vec::new(),
                    vala_bifrost::tables::CorrelationPolicy::Observation,
                ),
                vala_bifrost_redux::tables::managed_columns::ensure_managed_columns(
                    Vec::new(),
                    vala_bifrost_redux::tables::CorrelationPolicy::Observation,
                ),
            ),
            (
                vala_bifrost::tables::managed_columns::ensure_managed_columns(
                    Vec::new(),
                    vala_bifrost::tables::CorrelationPolicy::CodeAxis,
                ),
                vala_bifrost_redux::tables::managed_columns::ensure_managed_columns(
                    Vec::new(),
                    vala_bifrost_redux::tables::CorrelationPolicy::CodeAxis,
                ),
            ),
        ] {
            assert_eq!(legacy, redux);
            assert_eq!(
                legacy.last().map(|field| field.name().as_str()),
                Some("data_tenant_id")
            );
            assert_eq!(
                legacy
                    .iter()
                    .find(|field| field.name() == "wyrd_row_ordinal")
                    .map(|field| (field.data_type(), field.is_nullable())),
                Some((&DataType::Int32, false))
            );
        }

        for definition in vala_bifrost_redux::tables::builtin_tables() {
            let schema = (definition.schema)();
            let ordinal = schema
                .field_with_name("wyrd_row_ordinal")
                .expect("every initial built-in recipe carries row identity");
            assert_eq!(
                (ordinal.data_type(), ordinal.is_nullable()),
                (&DataType::Int32, false),
                "{}.{},",
                definition.namespace,
                definition.name,
            );
        }
    }
}
