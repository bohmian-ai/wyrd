//! Require-authenticated middleware for Wyrd protected surfaces.

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::auth::token_extract::verify_authenticated_principal;
use crate::state::AppState;

/// Reject unauthenticated requests before they reach protected handlers.
///
/// A `from_fn_with_state` layer modeled on `attach_request_id`: it verifies the
/// Wyrd access token carried in `X-Wyrd-Access-Token`, and on success inserts the
/// verified [`AuthenticatedPrincipal`] into request extensions before calling
/// `next`. On any failure it returns the mapped Wyrd error response immediately
/// and does not call `next`.
///
/// It reuses the same token-extract and error-mapping helpers as
/// [`AuthenticatedPrincipal`]'s extractor, so its `400`/`401`/`503` rejections
/// are identical by construction.
pub async fn require_authenticated(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    match verify_authenticated_principal(state.token_verifier.clone(), request.headers()).await {
        Ok(principal) => {
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Err(rejection) => rejection.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    use axum::Router;
    use axum::body::Body;
    use axum::extract::Extension;
    use axum::http::{Request, StatusCode};
    use axum::middleware::from_fn_with_state;
    use axum::routing::get;
    use tower::ServiceExt;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_runtime::PrincipalId;
    use wyrd_spec::DataTenantId;

    use super::require_authenticated;
    use crate::auth::AuthenticatedPrincipal;
    use crate::auth::permission_resolver::SqlPermissionResolver;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[tokio::test]
    async fn valid_token_runs_next_and_inserts_principal() {
        let tenant = DataTenantId::new_v7();
        let state = test_state();
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
        let state = test_state();

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
        let state = test_state();

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

    async fn protected(Extension(principal): Extension<AuthenticatedPrincipal>) -> StatusCode {
        let _ = principal.principal.tenant_id;
        StatusCode::OK
    }

    fn test_router(state: crate::state::AppState) -> Router {
        Router::new()
            .route("/v1/protected", get(protected))
            .layer(from_fn_with_state(state.clone(), require_authenticated))
            .with_state(state)
    }

    fn test_state() -> crate::state::AppState {
        use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
        use std::collections::HashMap;
        use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

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
        let mut keys = HashMap::new();
        keys.insert(
            Kid::new("k1").expect("kid is valid"),
            Arc::new(public_key_from_pem(PUBLIC_KEY_PEM).expect("public key loads")),
        );
        let verifier = Arc::new(TokenVerifier::new(
            keys,
            "wyrd",
            Arc::new(SqlPermissionResolver::new(Arc::new(app_pool.clone()))),
            WyrdAuthVerifySettings {
                allowed_clock_skew: StdDuration::ZERO,
                ..WyrdAuthVerifySettings::default()
            },
        ));
        crate::state::AppState::new(
            app_pool,
            None,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
        )
        .with_auth_handles(issuing_key, verifier)
    }

    fn mint_test_user_jwt(
        state: &crate::state::AppState,
        tenant: DataTenantId,
        ttl: chrono::Duration,
    ) -> String {
        let principal = TokenPrincipalRef {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: wyrd_auth_verify::PrincipalKindWire::User,
            tenant_id: tenant,
            card_ref: None,
        };
        state
            .issuing_key
            .as_ref()
            .expect("test state has issuing key")
            .issue_user_access_token(principal, vec![], ttl)
            .expect("test jwt mints")
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
