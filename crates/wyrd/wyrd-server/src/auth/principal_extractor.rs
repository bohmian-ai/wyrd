use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use secrecy::{ExposeSecret, SecretString};
use wyrd_runtime::Principal;
use wyrd_spec::error::WyrdError;

use crate::auth::token_extract::{
    WYRD_ACCESS_TOKEN_HEADER, auth_not_configured, tenant_from_unverified_access_token,
};
use crate::error::WyrdErrorResponse;
use crate::state::AppState;

/// Server-owned extractor wrapper for an authenticated Wyrd principal.
///
/// The bearer credential is the Wyrd access token carried in
/// `X-Wyrd-Access-Token`; application-owned `Authorization` headers are left
/// untouched.
#[derive(Debug, Clone)]
pub struct AuthenticatedPrincipal {
    /// The authenticated Wyrd principal.
    pub principal: Principal,
}

impl From<AuthenticatedPrincipal> for Principal {
    fn from(value: AuthenticatedPrincipal) -> Self {
        value.principal
    }
}

impl FromRequestParts<AppState> for AuthenticatedPrincipal {
    type Rejection = WyrdErrorResponse;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_wyrd_access_token(parts)?;
        let expected_tenant = tenant_from_unverified_access_token(token.expose_secret())?;
        let verifier = state
            .token_verifier
            .clone()
            .ok_or_else(auth_not_configured)?;
        let verified = verifier
            .verify(&token, &expected_tenant)
            .await
            .map_err(WyrdErrorResponse::from)?;
        Ok(Self {
            principal: verified.principal.clone(),
        })
    }
}

fn extract_wyrd_access_token(parts: &Parts) -> Result<SecretString, WyrdErrorResponse> {
    let raw = parts
        .headers
        .get(WYRD_ACCESS_TOKEN_HEADER)
        .ok_or_else(|| {
            WyrdErrorResponse::from(WyrdError::Unauthenticated {
                message: "missing X-Wyrd-Access-Token header".to_owned(),
                details: serde_json::json!({ "header": "x-wyrd-access-token" }),
            })
        })?
        .to_str()
        .map_err(|_| {
            WyrdErrorResponse::from(WyrdError::BadTokenFormat {
                message: "X-Wyrd-Access-Token header is not valid UTF-8".to_owned(),
                details: serde_json::json!({ "header": "x-wyrd-access-token" }),
            })
        })?;
    let Some(token) = raw.strip_prefix("Bearer ") else {
        return Err(WyrdErrorResponse::from(WyrdError::BadTokenFormat {
            message: "X-Wyrd-Access-Token header must be a Bearer credential".to_owned(),
            details: serde_json::json!({ "header": "x-wyrd-access-token" }),
        }));
    };
    if token.is_empty() {
        return Err(WyrdErrorResponse::from(WyrdError::BadTokenFormat {
            message: "X-Wyrd-Access-Token bearer token is empty".to_owned(),
            details: serde_json::json!({ "header": "x-wyrd-access-token" }),
        }));
    }
    Ok(SecretString::from(token.to_owned()))
}

#[cfg(test)]
mod tests {
    use axum::extract::FromRequestParts;
    use axum::http::Request;
    use axum::http::request::Parts;
    use std::sync::Arc;
    use std::time::Duration as StdDuration;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_runtime::PrincipalId;
    use wyrd_spec::DataTenantId;

    use crate::auth::permission_resolver::SqlPermissionResolver;
    use crate::error::WyrdErrorResponse;

    use super::AuthenticatedPrincipal;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[tokio::test]
    async fn extractor_passes_real_jwt() {
        let tenant = DataTenantId::new_v7();
        let state = test_state(tenant);
        let token = mint_test_user_jwt(&state, tenant, chrono::Duration::minutes(5));
        let mut parts = request_parts(Some(&token));

        let extracted = AuthenticatedPrincipal::from_request_parts(&mut parts, &state)
            .await
            .expect("principal extracts");

        assert_eq!(extracted.principal.tenant_id, tenant);
    }

    #[tokio::test]
    async fn missing_header_returns_unauthenticated() {
        let tenant = DataTenantId::new_v7();
        let state = test_state(tenant);
        let mut parts = request_parts(None);

        let error = AuthenticatedPrincipal::from_request_parts(&mut parts, &state)
            .await
            .expect_err("missing token fails");

        assert_error_code(error, "WYRD_AUTH_401_UNAUTHENTICATED");
    }

    #[tokio::test]
    async fn malformed_header_returns_bad_token_format() {
        let tenant = DataTenantId::new_v7();
        let state = test_state(tenant);
        let mut parts = request_parts(Some("not-a-jwt"));

        let error = AuthenticatedPrincipal::from_request_parts(&mut parts, &state)
            .await
            .expect_err("malformed token fails");

        assert_error_code(error, "WYRD_AUTH_400_BAD_TOKEN_FORMAT");
    }

    fn test_state(_tenant: DataTenantId) -> crate::state::AppState {
        use object_store::local::LocalFileSystem;
        use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
        use std::collections::HashMap;
        use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let object_store =
            Arc::new(LocalFileSystem::new_with_prefix(root.path()).expect("local object store"));
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
            Arc::new(StorageHandle::new(
                BackendSigner::Local(signer),
                object_store,
            )),
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

    fn request_parts(token: Option<&str>) -> Parts {
        let mut builder = Request::builder().uri("/v1/cards/upload/init");
        if let Some(token) = token {
            builder = builder.header("x-wyrd-access-token", format!("Bearer {token}"));
        }
        builder.body(()).expect("request builds").into_parts().0
    }

    fn assert_error_code(error: WyrdErrorResponse, code: &str) {
        assert_eq!(error.0.code(), code);
    }
}
