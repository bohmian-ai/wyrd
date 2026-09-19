use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use wyrd_auth_verify::VerifiedToken;
use wyrd_runtime::{DelegationStep, Principal};

use crate::components::auth::token_extract::verify_authenticated_principal;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Server-owned extractor wrapper for an authenticated Wyrd principal.
///
/// The bearer credential is the Wyrd access token carried in
/// `X-Wyrd-Access-Token`; application-owned `Authorization` headers are left
/// untouched.
///
/// The complete verifier result is retained rather than narrowed to the
/// effective principal: delegation-aware surfaces — MCP attribution and audit
/// in particular — need the same token's initiator-first `act` chain, and
/// reconstructing it downstream would mean trusting something other than the
/// verifier. Consumers that require an owned [`Principal`] use the
/// `From<AuthenticatedPrincipal>` conversion; everything else borrows.
#[derive(Debug, Clone)]
pub struct AuthenticatedPrincipal {
    /// Verified token backing both the effective principal and its `act` chain.
    verified: Arc<VerifiedToken>,
}

impl AuthenticatedPrincipal {
    /// Wrap a verifier result. Crate-private so the verify pipeline in
    /// [`verify_authenticated_principal`] stays the only producer: an
    /// unverified construction here would be a silent authentication bypass.
    pub(crate) fn from_verified(verified: Arc<VerifiedToken>) -> Self {
        Self { verified }
    }

    /// The verified effective principal, with its resolved permissions.
    #[must_use]
    pub fn principal(&self) -> &Principal {
        &self.verified.principal
    }

    /// The verified delegation chain in initiator-first order, empty when the
    /// token carries no `act` claim.
    #[must_use]
    pub fn delegation_chain(&self) -> &[DelegationStep] {
        &self.verified.delegation_chain
    }
}

impl From<AuthenticatedPrincipal> for Principal {
    fn from(value: AuthenticatedPrincipal) -> Self {
        value.verified.principal.clone()
    }
}

impl FromRequestParts<AppState> for AuthenticatedPrincipal {
    type Rejection = WyrdErrorResponse;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Fast path: require_authenticated (post-verify) is the sole trusted producer
        // of this extension. Any future insertion site must verify before inserting;
        // reading an unverified principal here would be a silent auth bypass.
        if let Some(principal) = parts.extensions.get::<AuthenticatedPrincipal>() {
            return Ok(principal.clone());
        }
        verify_authenticated_principal(state.auth.token_verifier.clone(), &parts.headers).await
    }
}

#[cfg(test)]
mod pg_tests {
    use axum::extract::FromRequestParts;
    use axum::http::Request;
    use axum::http::request::Parts;
    use std::sync::Arc;
    use std::time::Duration as StdDuration;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, VerifiedToken, WyrdAuthVerifySettings,
        public_key_from_pem,
    };
    use wyrd_runtime::{PrincipalId, PrincipalKind};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;

    use crate::auth::permission_resolver::SqlPermissionResolver;
    use crate::http::error::WyrdErrorResponse;

    use super::AuthenticatedPrincipal;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[tokio::test]
    async fn extractor_passes_real_jwt() {
        let tenant = DataTenantId::new_v7();
        let state = test_state(tenant).await;
        let token = mint_test_user_jwt(&state, tenant, chrono::Duration::minutes(5));
        let mut parts = request_parts(Some(&token));

        let extracted = AuthenticatedPrincipal::from_request_parts(&mut parts, &state)
            .await
            .expect("principal extracts");

        assert_eq!(extracted.principal().tenant_id, tenant);
    }

    #[tokio::test]
    async fn missing_header_returns_unauthenticated() {
        let tenant = DataTenantId::new_v7();
        let state = test_state(tenant).await;
        let mut parts = request_parts(None);

        let error = AuthenticatedPrincipal::from_request_parts(&mut parts, &state)
            .await
            .expect_err("missing token fails");

        assert_error_code(error, "WYRD_AUTH_401_UNAUTHENTICATED");
    }

    #[tokio::test]
    async fn extension_hit_returns_stored_principal_without_invoking_verifier() {
        let tenant = DataTenantId::new_v7();
        let state = test_state(tenant).await;
        let principal_id = PrincipalId::new(uuid::Uuid::now_v7());
        let stored = AuthenticatedPrincipal::from_verified(Arc::new(VerifiedToken {
            principal: wyrd_runtime::Principal {
                id: principal_id,
                kind: PrincipalKind::User,
                tenant_id: tenant,
                roles: Vec::new(),
                effective_permissions: Default::default(),
                credential_id: None,
            },
            delegation_chain: Vec::new(),
            exp: chrono::Utc::now() + chrono::Duration::minutes(5),
            iat: chrono::Utc::now(),
        }));
        // No token header — fallback path would reject with Unauthenticated;
        // extension hit must short-circuit and return the stored principal.
        let mut parts = request_parts(None);
        parts.extensions.insert(stored.clone());

        let extracted = AuthenticatedPrincipal::from_request_parts(&mut parts, &state)
            .await
            .expect("extension hit bypasses verifier");

        assert_eq!(extracted.principal().id, stored.principal().id);
        assert_eq!(extracted.principal().tenant_id, tenant);
    }

    /// A delegated token's verified effective principal and complete
    /// initiator-first `act` chain both survive on the retained
    /// [`wyrd_auth_verify::VerifiedToken`].
    ///
    /// The MCP adapter attributes delegated work from `delegation_chain()`, so
    /// the extractor may not narrow the verifier result to its effective
    /// principal on the way through.
    #[tokio::test]
    async fn authenticated_principal_preserves_verified_delegation_chain() {
        let tenant = DataTenantId::new_v7();
        let state = test_state(tenant).await;
        let initiator = PrincipalId::new(uuid::Uuid::now_v7());
        let subject = PrincipalId::new(uuid::Uuid::now_v7());
        let token = mint_delegated_user_jwt(&state, tenant, initiator, subject);
        let mut parts = request_parts(Some(&token));

        let extracted = AuthenticatedPrincipal::from_request_parts(&mut parts, &state)
            .await
            .expect("delegated principal extracts");

        assert_eq!(extracted.principal().id, subject);
        assert_eq!(extracted.principal().tenant_id, tenant);
        let chain = extracted.delegation_chain();
        assert_eq!(chain.len(), 1, "one delegating actor is present");
        assert_eq!(chain[0].principal.id, initiator);
    }

    #[tokio::test]
    async fn malformed_header_returns_bad_token_format() {
        let tenant = DataTenantId::new_v7();
        let state = test_state(tenant).await;
        let mut parts = request_parts(Some("not-a-jwt"));

        let error = AuthenticatedPrincipal::from_request_parts(&mut parts, &state)
            .await
            .expect_err("malformed token fails");

        assert_error_code(error, "WYRD_AUTH_400_BAD_TOKEN_FORMAT");
    }

    async fn test_state(_tenant: DataTenantId) -> crate::state::AppState {
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
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None);
        let vala = vala_sql::ValaPostgres::from_pool(app_pool);
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(wyrd, vala));
        crate::test_support::test_app_state(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
        .with_auth(crate::components::auth::ServerAuth {
            issuing_key: Some(issuing_key),
            token_verifier: Some(verifier),
            ..crate::components::auth::ServerAuth::default()
        })
    }

    fn mint_test_user_jwt(
        state: &crate::state::AppState,
        tenant: DataTenantId,
        ttl: chrono::Duration,
    ) -> String {
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
            .issue_user_access_token(principal, vec![], ttl)
            .expect("test jwt mints")
    }

    /// Mint an access token whose `act` chain names `initiator` as the sole
    /// delegating actor and `subject` as the effective principal.
    fn mint_delegated_user_jwt(
        state: &crate::state::AppState,
        tenant: DataTenantId,
        initiator: PrincipalId,
        subject: PrincipalId,
    ) -> String {
        let caller = wyrd_auth_issue::DelegationCaller {
            sub: initiator.to_string(),
            principal: TokenPrincipalRef {
                id: initiator,
                kind: PrincipalKindTag::User,
                tenant_id: tenant,
                card_ref: None,
                card_ref_scope: Default::default(),
            },
            act: None,
        };
        state
            .auth
            .issuing_key
            .as_ref()
            .expect("test state has issuing key")
            .issue_delegated_access_token(
                &caller,
                TokenPrincipalRef {
                    id: subject,
                    kind: PrincipalKindTag::User,
                    tenant_id: tenant,
                    card_ref: None,
                    card_ref_scope: Default::default(),
                },
                vec![],
                chrono::Duration::minutes(5),
            )
            .expect("delegated jwt mints")
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
