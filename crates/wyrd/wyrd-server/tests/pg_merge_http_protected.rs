//! Extension-route conformance tests for `WyrdServer::merge_http_protected`.
//!
//! Proves that routes merged via `merge_http_protected` receive the same
//! protective edge stack as core `/v1` routes: request-id propagation,
//! panic→500 mapping, body-limit enforcement, and authenticated `Principal`.
//! Also contrasts `merge_http` (unprotected) as a negative baseline.

pub mod support;

mod pg_tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use chrono::Duration;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use tower::ServiceExt;
    use vala_bifrost_redux::resources::{
        BifrostResourcePolicy, BifrostRole, BifrostRuntimeResources, ResourceSource,
        SystemResourceSnapshot,
    };
    use vala_bifrost_redux::scribe::{
        ScribeImpl,
        wal::{WalConfig, WalWriter},
    };
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_runtime::PrincipalId;
    use wyrd_server::components::auth::ServerAuth;
    use wyrd_server::config::{ScribeRuntimeConfig, WyrdServerConfig};
    use wyrd_server::postgres::ServerPostgres;
    use wyrd_server::state::{BifrostIngestRuntime, LimitsConfig};
    use wyrd_server::{AppState, WyrdServer};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    async fn test_state() -> AppState {
        test_state_with_ingest_limits(vala_bifrost_redux::gate::limits::IngestLimits::default())
            .await
    }

    /// Builds the route fixture with one shared Gate-and-Scribe limit snapshot.
    async fn test_state_with_ingest_limits(
        ingest_limits: vala_bifrost_redux::gate::limits::IngestLimits,
    ) -> AppState {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let postgres = Arc::new(ServerPostgres::from_parts(
            wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None),
            vala_sql::ValaPostgres::from_pool(app_pool.clone()),
        ));
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let kid = Kid::new("k1").expect("kid is valid");
        let pem = IssuingKey::generate_ephemeral_pem().expect("ephemeral key generates");
        let raw_issuing_key =
            IssuingKey::from_ed_pem(pem, kid.clone(), "wyrd").expect("ephemeral issuing key loads");
        let pub_pem = raw_issuing_key
            .verifying_key_pem()
            .expect("public key derives from ephemeral key");
        let issuing_key = Arc::new(raw_issuing_key);
        let mut keys = HashMap::new();
        keys.insert(
            kid,
            Arc::new(public_key_from_pem(pub_pem.as_bytes()).expect("public key loads")),
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
        let scribe = Arc::new(
            ScribeImpl::new_for_embedded_with_deps_and_catalog(
                Arc::new(storage.operator().clone()),
                wal,
                &uuid::Uuid::now_v7().to_string(),
                1,
                Arc::clone(&catalog),
            )
            .with_ingest_limits_for_test(ingest_limits),
        );
        let ingest = Arc::new(BifrostIngestRuntime::new(
            scribe,
            Arc::clone(&verifier),
            ingest_limits,
            None,
        ));
        let gate = ingest.gate();
        let resources = BifrostRuntimeResources::from_snapshot_with_transport_message_limit(
            SystemResourceSnapshot {
                memory_limit_bytes: 2 * 1024 * 1024 * 1024,
                effective_cpu: 2,
                scratch_capacity_bytes: 1024 * 1024 * 1024,
                scratch_available_bytes: 1024 * 1024 * 1024,
                memory_source: ResourceSource::Injected,
                cpu_source: ResourceSource::Injected,
            },
            BifrostResourcePolicy {
                roles: [BifrostRole::Scribe].into_iter().collect(),
                memory_limit_bytes: None,
                unmanaged_reserve_bytes: None,
                scratch_limit_bytes: Some(512 * 1024 * 1024),
                effective_cpu: None,
                scratch_root: std::env::temp_dir(),
                volume_roots: None,
            },
            ingest_limits.max_frame_bytes,
        )
        .expect("route-test resources")
        .compose_roles()
        .expect("route-test Scribe role");
        AppState::new(postgres, storage, catalog)
            .with_bifrost_resources(resources)
            .with_bifrost_ingest(ingest)
            .with_bifrost_gate(gate)
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
            .issue_user_access_token(principal, vec![], Duration::minutes(5))
            .expect("test jwt mints")
    }

    async fn panicking_handler() -> &'static str {
        panic!("test panic from extension")
    }

    fn probe_router() -> Router {
        Router::new()
            .route("/probe/ok", get(|| async { "ok" }))
            .route("/probe/panic", get(panicking_handler))
            .route("/probe/echo", post(|body: String| async move { body }))
    }

    async fn build_protected_server(state: AppState) -> WyrdServer {
        let config = WyrdServerConfig::default();
        WyrdServer::new(config, state).expect("WyrdServer builds with full auth state")
    }

    #[tokio::test]
    async fn extension_route_gets_request_id() {
        let state = test_state().await;
        let router = build_protected_server(state)
            .await
            .merge_http_protected(probe_router())
            .into_http_router();

        // Even an unauthenticated request gets the wyrd-request-id header because
        // attach_request_id runs before require_authenticated in the edge stack.
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/probe/ok")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert!(
            response.headers().contains_key("wyrd-request-id"),
            "protected extension route must carry wyrd-request-id; got headers: {:?}",
            response.headers()
        );
    }

    #[tokio::test]
    async fn extension_panic_maps_to_500() {
        let state = test_state().await;
        let tenant = DataTenantId::new_v7();
        let token = mint_user_jwt(&state, tenant);
        let router = build_protected_server(state)
            .await
            .merge_http_protected(probe_router())
            .into_http_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/probe/panic")
                    .header("x-wyrd-access-token", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds (CatchPanicLayer must catch the panic)");

        assert_eq!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "panicking handler must return 500 via CatchPanicLayer"
        );
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/problem+json"),
            "panic response must be application/problem+json WyrdError"
        );
    }

    /// Proves Scribe routes use their frozen ingest cap only while the role is active.
    #[tokio::test]
    async fn pg_scribe_body_limit_is_route_and_role_local() {
        let request_bytes = 201 * 1024 * 1024;
        let scribe_config = ScribeRuntimeConfig {
            ingest_request_bytes: request_bytes,
            ..ScribeRuntimeConfig::default()
        };
        scribe_config
            .validate()
            .expect("supported request above the default validates");
        let state = test_state_with_ingest_limits(scribe_config.ingest_limits())
            .await
            .with_limits(LimitsConfig {
                body_bytes: 1024,
                ..LimitsConfig::default()
            });
        let transport = state
            .bifrost_resources
            .as_ref()
            .expect("Scribe role resources")
            .transport_admission();
        assert_eq!(
            transport.message_limit_bytes(),
            request_bytes,
            "resource composition freezes the selected Scribe request maximum"
        );
        let tenant = DataTenantId::new_v7();
        let principal = TokenPrincipalRef {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKindTag::User,
            tenant_id: tenant,
            card_ref: None,
            card_ref_scope: Default::default(),
        };
        let token = state
            .auth
            .issuing_key
            .as_ref()
            .expect("route fixture issuing key")
            .issue_user_access_token(principal, vec![], Duration::minutes(5))
            .expect("route fixture token");
        let mut without_scribe = state.clone();
        without_scribe.bifrost_gate = None;
        without_scribe.bifrost_resources = None;
        let router = build_protected_server(state.clone())
            .await
            .merge_http_protected(probe_router())
            .into_http_router();

        // Use the Content-Length fast path: no need to stream 1025 bytes.
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/probe/echo")
                    .header("content-length", "2048")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(
            response.status(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "body exceeding the limit must return 413"
        );
        let above_default = 200 * 1024 * 1024 + 512 * 1024;
        let scribe_response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/traces")
                    .header("content-length", above_default.to_string())
                    .header("content-type", "application/json")
                    .header("x-wyrd-access-token", format!("Bearer {token}"))
                    .body(Body::from("{invalid-json"))
                    .expect("Scribe request builds"),
            )
            .await
            .expect("Scribe router responds");
        assert_eq!(
            scribe_response.status(),
            StatusCode::BAD_REQUEST,
            "enabled Scribe route must reach the downstream OTLP decoder"
        );
        let scribe_body = to_bytes(scribe_response.into_body(), usize::MAX)
            .await
            .expect("Scribe decoder response collects");
        let scribe_problem: serde_json::Value =
            serde_json::from_slice(&scribe_body).expect("Scribe decoder response is JSON");
        assert_eq!(
            scribe_problem["code"], "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
            "selected transport admission must yield the exact downstream decoder contract"
        );
        assert_eq!(
            transport.used_bytes(),
            0,
            "downstream decoder completion releases the selected transport charge"
        );
        let absent_response = build_protected_server(without_scribe)
            .await
            .into_http_router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/traces")
                    .header("content-length", above_default.to_string())
                    .body(Body::empty())
                    .expect("role-absent request builds"),
            )
            .await
            .expect("role-absent router responds");
        assert_eq!(absent_response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body collects");
        let problem: serde_json::Value =
            serde_json::from_slice(&body).expect("413 body must be JSON");
        assert_eq!(
            problem["code"], "WYRD_SPEC_413_PAYLOAD_TOO_LARGE",
            "error code must be WYRD_SPEC_413_PAYLOAD_TOO_LARGE; got {problem}"
        );
    }

    #[tokio::test]
    async fn extension_requires_auth() {
        let state = test_state().await;
        let router = build_protected_server(state)
            .await
            .merge_http_protected(probe_router())
            .into_http_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/probe/ok")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "extension route without auth must return 401"
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body collects");
        let problem: serde_json::Value =
            serde_json::from_slice(&body).expect("401 body must be JSON");
        assert_eq!(
            problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED",
            "error code must be WYRD_AUTH_401_UNAUTHENTICATED; got {problem}"
        );
    }

    #[tokio::test]
    async fn raw_merge_http_has_no_request_id() {
        let state = test_state().await;
        let router = build_protected_server(state)
            .await
            .merge_http(probe_router().with_state(()))
            .into_http_router();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/probe/ok")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert!(
            !response.headers().contains_key("wyrd-request-id"),
            "raw merge_http must NOT add wyrd-request-id (unprotected by design); got headers: {:?}",
            response.headers()
        );
    }
}
