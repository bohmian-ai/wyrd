use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use wyrd_runtime::{DelegationStep, Principal};
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;

use crate::components::auth::{AuthenticatedPrincipal, token_extract};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Authenticated caller context used by tenant-scoped service code.
#[derive(Debug, Clone)]
pub struct Caller {
    /// Resolved tenant isolation key for data-plane access.
    pub data_tenant_id: DataTenantId,
    /// Authenticated principal.
    ///
    /// This is the *effective* principal — the identity every authorization
    /// decision is made against. Delegation never displaces it.
    pub principal: Principal,
    /// Request correlation ID.
    pub request_id: RequestId,
    /// Verified initiator-first delegation chain, empty for a nondelegated
    /// caller.
    ///
    /// Attribution only: the chain says who was acting for whom and is copied
    /// into the audit record, while authorization stays bound to
    /// [`Self::principal`]. It is only ever populated from a verifier result,
    /// never from tool arguments, request bodies, or arbitrary headers.
    pub delegation_chain: Vec<DelegationStep>,
}

impl Caller {
    /// Derives the tenant-scoped caller from one already verified principal.
    ///
    /// Tenant, effective principal, and delegation chain are taken together
    /// from the same [`AuthenticatedPrincipal`] so they cannot disagree: the
    /// tenant is the principal's own, and the chain is the verifier's, in the
    /// order it produced. Every authenticated Wyrd boundary — the HTTP
    /// extractor below and the MCP handler — goes through here rather than
    /// building the struct field by field, which is what previously let the
    /// chain be silently dropped at one of them.
    #[must_use]
    pub fn from_authenticated(principal: &AuthenticatedPrincipal, request_id: RequestId) -> Self {
        let delegation_chain = principal.delegation_chain().to_vec();
        let principal = principal.principal().clone();
        Self {
            data_tenant_id: principal.tenant_id,
            principal,
            request_id,
            delegation_chain,
        }
    }
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = WyrdErrorResponse;

    /// Resolve the authenticated principal and the request id a served
    /// handler needs, so every handler takes one already-verified `Caller`.
    ///
    /// # Errors
    /// Returns [`WyrdErrorResponse`] when authentication fails — no usable
    /// token, an invalid or expired one, or no configured verifier — or when
    /// the request carries no request id.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let principal = AuthenticatedPrincipal::from_request_parts(parts, state).await?;
        let request_id = token_extract::request_id(parts)?;

        Ok(Self::from_authenticated(&principal, request_id))
    }
}

#[cfg(test)]
mod pg_tests {
    use crate::components::auth::Caller;
    use axum::extract::FromRequestParts;
    use axum::http::Request;
    use std::sync::Arc;
    use std::time::Duration as StdDuration;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_runtime::PrincipalId;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::request_id::RequestId;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[tokio::test]
    async fn caller_data_tenant_id_sourced_from_principal() {
        let tenant = DataTenantId::new_v7();
        let state = test_state().await;
        let token = mint_test_user_jwt(&state, tenant);
        let mut parts = Request::builder()
            .uri("/v1/cards/upload/init")
            .header("x-wyrd-access-token", format!("Bearer {token}"))
            .body(())
            .expect("request builds")
            .into_parts()
            .0;
        parts.extensions.insert(
            RequestId::parse(&uuid::Uuid::now_v7().to_string()).expect("request id parses"),
        );

        let caller = Caller::from_request_parts(&mut parts, &state)
            .await
            .expect("caller extracts");

        assert_eq!(caller.data_tenant_id, tenant);
        assert_eq!(caller.principal.tenant_id, tenant);
    }

    async fn test_state() -> crate::state::AppState {
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
            WyrdAuthVerifySettings {
                allowed_clock_skew: StdDuration::ZERO,
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

    /// Mint a five-minute `wyrd`-audience access token for a fresh User
    /// principal in `tenant`, signed by the test state's issuing key and
    /// carrying no permissions.
    ///
    /// # Panics
    /// Panics when the test state has no issuing key or signing fails.
    fn mint_test_user_jwt(state: &crate::state::AppState, tenant: DataTenantId) -> String {
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
            .issue_access_token(
                wyrd_auth_issue::AccessGrant {
                    principal,
                    roles: vec![],
                    permissions: wyrd_runtime::PermissionSet::new(),
                    credential_id: None,
                    act: None,
                    audience: wyrd_spec::auth::TokenAudience::Wyrd,
                },
                chrono::Duration::minutes(5),
            )
            .expect("test jwt mints")
    }
}
