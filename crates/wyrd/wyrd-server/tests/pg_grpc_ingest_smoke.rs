//! Process-level gRPC ingest smoke tests.
//!
//! Stands up `build_app_grpc` + `serve_grpc` and proves the C1 ingest auth
//! seam works correctly: an unauthenticated stream is rejected, and a valid
//! token passes auth (the empty stream then terminates without an Unauthenticated
//! error). Reuses the same key/verifier setup as `router_smoke.rs`.

mod support;

mod pg_tests {
    use super::*;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use chrono::Duration as ChronoDuration;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use tokio_util::sync::CancellationToken;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_runtime::PrincipalId;
    use wyrd_server::AppState;
    use wyrd_server::components::auth::ServerAuth;
    use wyrd_server::grpc::{GrpcRouterConfig, build_app_grpc, serve_grpc};
    use wyrd_server::postgres::ServerPostgres;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};
    use wyrd_tonic::tonic::transport::Channel;
    use wyrd_tonic::tonic::{Code, Request};
    use wyrd_tonic::tonic_health::server::health_reporter;
    use wyrd_tonic::wyrd::v1::InsertBatchRequest;
    use wyrd_tonic::wyrd::v1::bifrost_ingest_service_client::BifrostIngestServiceClient;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    async fn test_state() -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let postgres = Arc::new(ServerPostgres::from_parts(
            wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None),
            vala_sql::ValaPostgres::from_pools(app_pool.clone(), None),
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
        AppState::new(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            support::test_catalog().await,
        )
        .with_auth(ServerAuth {
            issuing_key: Some(issuing_key),
            token_verifier: Some(verifier),
            ..ServerAuth::default()
        })
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

    fn empty_stream() -> tokio_stream::Iter<std::vec::IntoIter<InsertBatchRequest>> {
        tokio_stream::iter(Vec::<InsertBatchRequest>::new())
    }

    async fn bind_free_loopback() -> SocketAddr {
        let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind probe listener");
        let addr = probe.local_addr().expect("probe local addr");
        drop(probe);
        addr
    }

    async fn connect_grpc(addr: SocketAddr) -> BifrostIngestServiceClient<Channel> {
        let url = format!("http://{addr}");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match Channel::from_shared(url.clone())
                .expect("endpoint parses")
                .connect()
                .await
            {
                Ok(channel) => return BifrostIngestServiceClient::new(channel),
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

    #[tokio::test]
    async fn ingest_unauthenticated_is_rejected() {
        let state = test_state().await;
        let (_, health_service) = health_reporter();
        let router = build_app_grpc(
            &state,
            health_service,
            GrpcRouterConfig {
                reflection_enabled: false,
            },
        )
        .expect("gRPC router builds with token verifier");

        let bind = bind_free_loopback().await;
        let shutdown = CancellationToken::new();
        let token = shutdown.clone();
        tokio::spawn(async move { serve_grpc(router, bind, token).await });

        let mut client = connect_grpc(bind).await;
        let status = client
            .insert_batch(Request::new(empty_stream()))
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
        let state = test_state().await;
        let tenant = DataTenantId::new_v7();
        let jwt = mint_user_jwt(&state, tenant);

        let (_, health_service) = health_reporter();
        let router = build_app_grpc(
            &state,
            health_service,
            GrpcRouterConfig {
                reflection_enabled: false,
            },
        )
        .expect("gRPC router builds with token verifier");

        let bind = bind_free_loopback().await;
        let shutdown = CancellationToken::new();
        let token = shutdown.clone();
        tokio::spawn(async move { serve_grpc(router, bind, token).await });

        let mut client = connect_grpc(bind).await;
        let mut request = Request::new(empty_stream());
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
}
