//! Tenant human OIDC login initiation HTTP adapter.

use axum::Json;
use axum::extract::{Extension, State};
use wyrd_spec::auth::{BeginLogin, BeginLoginResponse};
use wyrd_spec::error::WyrdProblem;
use wyrd_spec::request_id::RequestId;

use crate::auth::auth_not_configured;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Handler for `POST /auth/login`.
///
/// Begins a tenant human SSO login for the BFF (browser flow binding) or the
/// CLI (handoff binding) and returns only the provider authorization URL. The
/// tenant comes from the body's route key; no header — `Host`, forwarded
/// headers, or anything else — selects the tenant, connection, or redirect.
///
/// Login initiation evaluates no principal permission — it answers "who is
/// calling?", not "may they do this?" — so it appends no canonical audit event.
/// A refused attempt is recorded as structured diagnostics instead.
///
/// # Errors
/// Returns the refusals of [`wyrd_auth::connections::HumanConnections::begin_login`]
/// and a `500` when auth is not configured.
#[utoipa::path(
    post,
    path = "/auth/login",
    request_body = BeginLogin,
    responses(
        (status = 200, description = "Login begun; send the browser to the returned provider \
          authorization URL", body = BeginLoginResponse),
        (status = 400, description = "Both or neither initiation binding was supplied, or the \
          deployment has no public origin or sealing key (WYRD_SPEC_400_VALIDATION); or the CLI \
          handoff is unknown or the flow binding was already used \
          (WYRD_AUTH_400_INVALID_STATE)", body = WyrdProblem),
        (status = 401, description = "SSO login is not available for this tenant route key \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 503, description = "The identity provider or auth backend is unavailable \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)",
          body = WyrdProblem)
    ),
    // No session exists yet at this operation, so it clears the document-wide
    // requirement instead of inheriting it.
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(
    level = "debug",
    skip(state, maybe_request_id, request),
    fields(tenant_route_key = %request.tenant_route_key)
)]
pub async fn login(
    State(state): State<AppState>,
    maybe_request_id: Option<Extension<RequestId>>,
    Json(request): Json<BeginLogin>,
) -> Result<Json<BeginLoginResponse>, WyrdErrorResponse> {
    let request_id = maybe_request_id
        .map(|Extension(id)| id)
        .unwrap_or_else(RequestId::now_v7);
    let connections = state
        .auth
        .human_connections
        .as_ref()
        .ok_or_else(auth_not_configured)?;
    connections
        .begin_login(&request)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::info!(
                request_id = %request_id,
                tenant_route_key = %request.tenant_route_key,
                code = error.code(),
                "login initiation refused"
            );
            WyrdErrorResponse::from(error)
        })
}

/// Login initiation against a mock provider, proving that request headers take
/// no part in tenant, connection, or redirect selection.
#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth::connections::HumanConnections;
    use wyrd_auth_oidc::ScreenedHttp;
    use wyrd_crypt::{SealingKeyring, SecretKey};
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::components::auth::ServerAuth;
    use crate::state::AppState;

    /// App state over `fixture` whose tenant's Active connection points at a
    /// mock provider, with a sealing keyring and a fixed public origin.
    ///
    /// # Panics
    /// Panics when the fixture cannot be seeded or the state cannot be built.
    async fn login_state(fixture: &PgFixture, provider: &MockServer) -> AppState {
        let issuer = provider.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "id_token_signing_alg_values_supported": ["EdDSA"],
            })))
            .mount(provider)
            .await;
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");
        conn.commit().await.expect("seed commits");
        sqlx::query(
            "UPDATE wyrd.auth_human_connections SET issuer_url = $1 WHERE connection_id = $2",
        )
        .bind(&issuer)
        .bind(binding.connection_id)
        .execute(
            &fixture
                .superuser_pool()
                .await
                .expect("superuser pool opens"),
        )
        .await
        .expect("connection points at the mock provider");
        let origin = Url::parse("https://wyrd.example.com").expect("origin parses");
        let tempdir = tempfile::tempdir().expect("login storage tempdir");
        let signer = LocalSigner::new(tempdir.keep()).expect("local signer creates");
        crate::test_support::test_app_state(
            Arc::new(crate::postgres::ServerPostgres::from_parts(
                fixture.wyrd_postgres().clone(),
                fixture.vala_postgres().clone(),
            )),
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
        .with_auth(ServerAuth {
            human_connections: Some(HumanConnections::new(
                fixture.wyrd_postgres().clone(),
                Some(Arc::new(SealingKeyring::new(SecretKey::from_bytes(
                    [3_u8; 32],
                )))),
                ScreenedHttp::allowing_internal(),
                Some(&origin),
            )),
            ..ServerAuth::default()
        })
    }

    /// The login handler alone, without the auth router's per-peer rate
    /// limiter, which needs a real socket's peer address.
    fn router(state: AppState) -> axum::Router {
        axum::Router::new()
            .route("/auth/login", axum::routing::post(super::login))
            .with_state(state)
    }

    /// POST `body` to `/auth/login` with hostile `Host` and forwarded headers.
    ///
    /// # Panics
    /// Panics when the request cannot be built or its body read.
    async fn post_login(
        state: AppState,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let router = router(state);
        let response = router
            .oneshot(
                Request::post("/auth/login")
                    .header(header::HOST, "attacker.example.net")
                    .header("x-forwarded-host", "evil.example.org")
                    .header("x-forwarded-proto", "http")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("request builds"),
            )
            .await
            .expect("router answers");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body reads");
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    /// Login resolves the tenant from the body route key and sends the
    /// provider back to the deployment's configured callback, whatever `Host`
    /// or forwarded headers the request carried. The response carries only the
    /// authorization URL.
    ///
    /// # Panics
    /// Panics when login fails or the URL carries another `redirect_uri`.
    #[tokio::test]
    async fn login_ignores_request_headers() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let state = login_state(&fixture, &provider).await;

        let (status, body) = post_login(
            state,
            serde_json::json!({
                "tenant_route_key": fixture.tenant_slug(),
                "browser_flow_hash": "a".repeat(64),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{body}");
        let fields: Vec<&String> = body.as_object().expect("object").keys().collect();
        assert_eq!(fields, vec!["authorization_url"]);
        let authorization_url = Url::parse(
            body["authorization_url"]
                .as_str()
                .expect("authorization URL present"),
        )
        .expect("authorization URL parses");
        assert!(authorization_url.as_str().starts_with(&provider.uri()));
        let redirect_uris: Vec<String> = authorization_url
            .query_pairs()
            .filter(|(key, _)| key == "redirect_uri")
            .map(|(_, value)| value.into_owned())
            .collect();
        assert_eq!(
            redirect_uris,
            vec!["https://wyrd.example.com/auth/callback"]
        );
    }

    /// A `Host` naming the tenant does not stand in for the body route key:
    /// an unknown route key is refused even when the host names a real tenant.
    ///
    /// # Panics
    /// Panics when login is accepted.
    #[tokio::test]
    async fn the_host_header_cannot_select_a_tenant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let state = login_state(&fixture, &provider).await;
        let router = router(state);

        let response = router
            .oneshot(
                Request::post("/auth/login")
                    .header(
                        header::HOST,
                        format!("{}.wyrd.example.com", fixture.tenant_slug()),
                    )
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "tenant_route_key": "no-such-tenant",
                            "browser_flow_hash": "b".repeat(64),
                        })
                        .to_string(),
                    ))
                    .expect("request builds"),
            )
            .await
            .expect("router answers");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
