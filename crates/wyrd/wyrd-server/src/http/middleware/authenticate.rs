//! Require-authenticated middleware for Wyrd protected surfaces.

use axum::body::Body;
use axum::extract::State;
use axum::http::uri::Authority;
use axum::http::{Method, Request};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use std::sync::Arc;

use secrecy::SecretString;
use wyrd_auth_verify::TokenAudience;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::auth::RequestOrigin;
use crate::auth::browser::{browser_sessions, require_same_origin, session_cookies};
use crate::auth::exchange_api_key::api_key_invalid;
use crate::components::auth::AuthenticatedPrincipal;
use crate::components::auth::routes::TokenGrants;
use crate::components::auth::token_extract::{
    WYRD_ACCESS_TOKEN_HEADER, WYRD_API_KEY_HEADER, verify_authenticated_principal,
    workload_uid_header,
};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Reject unauthenticated requests before they reach protected handlers.
///
/// A `from_fn_with_state` layer modeled on `attach_request_id`: it verifies the
/// `wyrd`-audience access token carried in `X-Wyrd-Access-Token`, and on success inserts the
/// verified [`AuthenticatedPrincipal`](crate::components::auth::principal_extractor::AuthenticatedPrincipal) into request extensions before calling
/// `next`. On any failure it returns the mapped Wyrd error response immediately
/// and does not call `next`.
///
/// It reuses the same token-extract and error-mapping helpers as
/// [`AuthenticatedPrincipal`](crate::components::auth::principal_extractor::AuthenticatedPrincipal)'s extractor, so its `400`/`401`/`503` rejections
/// are identical by construction.
pub async fn require_authenticated(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    authenticate_on(&state, request, next, TokenAudience::Wyrd).await
}

/// Reject unauthenticated requests to the protected tenant `/v1` HTTP
/// surface, which alone also accepts a browser session.
///
/// Identical to [`require_authenticated`] for every request carrying
/// `X-Wyrd-Access-Token`, and for every request carrying no browser-session
/// cookie; otherwise see [`authenticate_browser_session`]. MCP, OTLP, platform
/// and gateway ingress keep their bearer-only layers.
pub async fn require_tenant_authenticated(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    authenticate_tenant_http(&state, request, next, TokenAudience::Wyrd).await
}

/// Reject unauthenticated requests to the Bifrost HTTP surface.
///
/// Identical to [`require_tenant_authenticated`] except that it also accepts
/// a `bifrost`-audience delegated token. It layers only the Bifrost and query
/// routers, which is what confines a Bifrost-bound token to Bifrost.
pub async fn require_bifrost_authenticated(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    authenticate_tenant_http(&state, request, next, TokenAudience::Bifrost).await
}

/// Authenticate a tenant `/v1` request by its bearer token or, without one,
/// by its browser session.
///
/// An explicit `X-Wyrd-Access-Token` always takes the bearer path, and a
/// refused token never falls back to a cookie. A request with no session
/// cookie takes the bearer path too, so its refusal is unchanged. More than
/// one session cookie is refused rather than choosing one.
async fn authenticate_tenant_http(
    state: &AppState,
    request: Request<Body>,
    next: Next,
    surface: TokenAudience,
) -> Response {
    if request.headers().contains_key(WYRD_ACCESS_TOKEN_HEADER) {
        return authenticate_on(state, request, next, surface).await;
    }
    let mut cookies = session_cookies(request.headers());
    let Some((_, cookie)) = cookies.pop() else {
        return authenticate_on(state, request, next, surface).await;
    };
    if !cookies.is_empty() {
        return WyrdErrorResponse(WyrdError::Validation {
            message: "send exactly one browser-session cookie with a tenant API request".to_owned(),
            details: serde_json::json!({ "reason": "browser_session_ambiguous" }),
        })
        .into_response();
    }
    authenticate_browser_session(state, request, next, &cookie).await
}

/// Authenticate a tenant `/v1` request by the one browser session it presents.
///
/// A request that may mutate (any method but `GET`, `HEAD` and `OPTIONS`)
/// must carry the exact `Origin` it arrived at. The session is then resolved
/// with the principal's current authority, never extending it, and the
/// principal is inserted exactly as a verified bearer's would be, so every
/// handler's permission checks and audit run unchanged.
async fn authenticate_browser_session(
    state: &AppState,
    mut request: Request<Body>,
    next: Next,
    cookie: &str,
) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if !safe {
        let authority = request.uri().authority().map(Authority::as_str);
        let checked = RequestOrigin::from_parts(request.headers(), authority)
            .ok_or_else(|| WyrdError::OriginRefused {
                message: "the request names no usable Host".to_owned(),
                details: serde_json::json!({}),
            })
            .and_then(|origin| require_same_origin(request.headers(), &origin));
        if let Err(error) = checked {
            return WyrdErrorResponse(error).into_response();
        }
    }
    let sessions = match browser_sessions(state) {
        Ok(sessions) => sessions,
        Err(rejection) => return rejection.into_response(),
    };
    match sessions.resolve(cookie).await {
        Ok(session) => {
            request
                .extensions_mut()
                .insert(AuthenticatedPrincipal::from_verified(Arc::new(
                    session.verified,
                )));
            next.run(request).await
        }
        Err(error) => WyrdErrorResponse(error).into_response(),
    }
}

/// Reject unauthenticated requests to the OTLP HTTP routes.
///
/// Identical to [`require_authenticated`], except that a request with no
/// access token may carry its exporter's Wyrd API key in `x-wyrd-api-key`,
/// since a stock OpenTelemetry exporter sends only static headers, with an
/// optional `x-wyrd-workload-uid` root. The key is verified on every request
/// through [`TokenGrants::api_key_principal`] and yields the principal its
/// exchanged token would carry under that root. An access token, when
/// present, always wins.
pub async fn require_otlp_authenticated(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    if request.headers().contains_key(WYRD_ACCESS_TOKEN_HEADER) {
        return authenticate_on(&state, request, next, TokenAudience::Wyrd).await;
    }
    let Some(presented) = request.headers().get(WYRD_API_KEY_HEADER) else {
        return authenticate_on(&state, request, next, TokenAudience::Wyrd).await;
    };
    let Ok(presented) = presented.to_str() else {
        return WyrdErrorResponse(api_key_invalid()).into_response();
    };
    let api_key = SecretString::from(presented.to_owned());
    let workload_uid = match workload_uid_header(request.headers()) {
        Ok(workload_uid) => workload_uid,
        Err(error) => return WyrdErrorResponse(error).into_response(),
    };
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .cloned()
        .unwrap_or_else(RequestId::now_v7);
    let grants = TokenGrants {
        state: &state,
        request_id: request_id.as_str(),
    };
    match grants.api_key_principal(&api_key, workload_uid).await {
        Ok(verified) => {
            request
                .extensions_mut()
                .insert(AuthenticatedPrincipal::from_verified(Arc::new(verified)));
            next.run(request).await
        }
        Err(error) => WyrdErrorResponse(error).into_response(),
    }
}

/// Verify the request's token for `surface`, apply its optional
/// `x-wyrd-workload-uid` selector, insert the principal, and run `next`; any
/// verification or selection failure is returned without calling `next`.
async fn authenticate_on(
    state: &AppState,
    mut request: Request<Body>,
    next: Next,
    surface: TokenAudience,
) -> Response {
    let verified = match verify_authenticated_principal(
        state.auth.token_verifier.as_deref(),
        request.headers(),
        surface,
    ) {
        Ok(principal) => principal,
        Err(rejection) => return rejection.into_response(),
    };
    let selected = match workload_uid_header(request.headers()) {
        Ok(root) => verified.select_root(state, root.as_ref()).await,
        Err(error) => Err(error),
    };
    match selected {
        Ok(principal) => {
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Err(error) => WyrdErrorResponse(error).into_response(),
    }
}

#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;

    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    use axum::Router;
    use axum::body::Body;
    use axum::extract::Extension;
    use axum::http::{Request, StatusCode};
    use axum::middleware::from_fn_with_state;
    use axum::routing::get;
    use chrono::Duration;
    use tower::ServiceExt;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_runtime::PrincipalId;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;

    use super::require_authenticated;
    use crate::components::auth::AuthenticatedPrincipal;
    use crate::state::AppState;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[tokio::test]
    async fn valid_token_runs_next_and_inserts_principal() {
        let tenant = DataTenantId::new_v7();
        let state = test_state().await;
        let jwt = mint_test_user_jwt(&state, tenant, chrono::Duration::minutes(5));

        let response = test_router(state)
            .oneshot(
                Request::builder()
                    .uri("/v1/protected")
                    .header("x-wyrd-access-token", format!("Bearer {jwt}"))
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_header_returns_unauthenticated() {
        let state = test_state().await;

        let response = test_router(state)
            .oneshot(
                Request::builder()
                    .uri("/v1/protected")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_error_code(response, "WYRD_AUTH_401_UNAUTHENTICATED").await;
    }

    #[tokio::test]
    async fn malformed_header_returns_bad_token_format() {
        let state = test_state().await;

        let response = test_router(state)
            .oneshot(
                Request::builder()
                    .uri("/v1/protected")
                    .header("x-wyrd-access-token", "Bearer not-a-jwt")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_error_code(response, "WYRD_AUTH_400_BAD_TOKEN_FORMAT").await;
    }

    /// A protected route must refuse when no token verifier is configured.
    ///
    /// The token itself is good, so anything other than the retryable `503`
    /// would be the served surface admitting a principal it never verified.
    ///
    /// # Panics
    /// Panics when the router fails to respond, or when the response is not the
    /// stable retryable verify-unavailable problem.
    #[tokio::test]
    async fn missing_verifier_refuses_a_protected_route() {
        let tenant = DataTenantId::new_v7();
        let state = test_state().await;
        let jwt = mint_test_user_jwt(&state, tenant, chrono::Duration::minutes(5));
        let issuing_key = state.auth.issuing_key.clone();
        let state = state.with_owners_for_test(|owners| {
            owners.auth = crate::components::auth::ServerAuth {
                issuing_key,
                token_verifier: None,
                ..crate::components::auth::ServerAuth::default()
            }
        });

        let response = test_router(state)
            .oneshot(
                Request::builder()
                    .uri("/v1/protected")
                    .header("x-wyrd-access-token", format!("Bearer {jwt}"))
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_error_code(response, "WYRD_AUTH_503_VERIFY_UNAVAILABLE").await;
    }

    async fn protected(Extension(principal): Extension<AuthenticatedPrincipal>) -> StatusCode {
        let _ = principal.principal().tenant_id;
        StatusCode::OK
    }

    fn test_router(state: AppState) -> Router {
        Router::new()
            .route("/v1/protected", get(protected))
            .layer(from_fn_with_state(state.clone(), require_authenticated))
            .with_state(state)
    }

    async fn test_state() -> AppState {
        use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
        use wyrd_storage::{LocalSigner, StorageHandle};

        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
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
        let verifier = Arc::new(build_verifier());
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None);
        let vala = vala_sql::ValaPostgres::from_pool(app_pool);
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(wyrd, vala));
        crate::state::AppState::from_owners_for_test(crate::state::AppStateInner {
            auth: crate::components::auth::ServerAuth {
                issuing_key: Some(issuing_key),
                token_verifier: Some(verifier),
                ..crate::components::auth::ServerAuth::default()
            },
            ..crate::test_support::test_owners(
                postgres,
                Arc::new(StorageHandle::new(signer)),
                crate::test_support::test_catalog().await,
            )
        })
    }

    /// Mint a `wyrd`-audience access token lasting `ttl` for a fresh User
    /// principal in `tenant`, signed by the test state's issuing key and
    /// carrying no permissions; a negative `ttl` yields an expired token.
    ///
    /// # Panics
    /// Panics when the test state has no issuing key or signing fails.
    fn mint_test_user_jwt(state: &AppState, tenant: DataTenantId, ttl: Duration) -> String {
        let principal = TokenPrincipalRef {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKindTag::User,
            tenant_id: tenant,
        };
        state
            .auth
            .issuing_key
            .as_ref()
            .expect("test state has issuing key")
            .issue_access_token(
                wyrd_auth_issue::AccessGrant {
                    principal,
                    roles: vec![],
                    permissions: wyrd_runtime::PermissionSet::new(),
                    card_access: wyrd_spec::auth::CardAccess::AnyRegistered,
                    workload_uid: None,
                    credential_id: None,
                    act: None,
                    audience: wyrd_spec::auth::TokenAudience::Wyrd,
                },
                ttl,
            )
            .expect("test jwt mints")
    }

    /// Build the local token verifier the served tests use, with zero clock
    /// skew so expiry cases observe `exp` exactly.
    fn build_verifier() -> TokenVerifier {
        let mut keys = HashMap::new();
        keys.insert(
            Kid::new("k1").expect("kid is valid"),
            Arc::new(public_key_from_pem(PUBLIC_KEY_PEM).expect("public key loads")),
        );
        TokenVerifier::new(
            keys,
            "wyrd",
            WyrdAuthVerifySettings {
                allowed_clock_skew: StdDuration::ZERO,
            },
        )
    }

    async fn assert_error_code(response: axum::response::Response, code: &str) {
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body reads");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("problem+json body");
        assert_eq!(
            json.get("code").and_then(serde_json::Value::as_str),
            Some(code)
        );
    }
}
