use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use wyrd_runtime::Principal;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::components::auth::AuthenticatedPrincipal;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Authenticated caller context used by tenant-scoped service code.
#[derive(Debug, Clone)]
pub struct Caller {
    /// Resolved tenant isolation key for data-plane access.
    pub data_tenant_id: DataTenantId,
    /// Authenticated principal.
    pub principal: Principal,
    /// Request correlation ID.
    pub request_id: RequestId,
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = WyrdErrorResponse;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let principal = AuthenticatedPrincipal::from_request_parts(parts, state)
            .await?
            .principal;
        let request_id = parts
            .extensions
            .get::<RequestId>()
            .cloned()
            .ok_or_else(missing_request_id)
            .map_err(WyrdErrorResponse::from)?;

        Ok(Self {
            data_tenant_id: principal.tenant_id,
            principal,
            request_id,
        })
    }
}

fn missing_request_id() -> WyrdError {
    WyrdError::Internal {
        message: "missing RequestId extension".to_owned(),
        details: serde_json::json!({ "extension": "RequestId" }),
    }
}

#[cfg(test)]
mod tests {
    use axum::extract::FromRequestParts;
    use axum::http::Request;
    use std::sync::Arc;
    use std::time::Duration as StdDuration;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, PrincipalKindWire, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings,
        public_key_from_pem,
    };
    use wyrd_runtime::PrincipalId;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::request_id::RequestId;

    use crate::auth::permission_resolver::SqlPermissionResolver;
    use crate::components::auth::Caller;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[tokio::test]
    async fn caller_data_tenant_id_sourced_from_principal() {
        let tenant = DataTenantId::new_v7();
        let state = test_state();
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
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None);
        let vala = vala_sql::ValaPostgres::from_pools(app_pool, None);
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(wyrd, vala));
        crate::state::AppState::new(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
        )
        .with_auth(crate::components::auth::ServerAuth {
            issuing_key: Some(issuing_key),
            token_verifier: Some(verifier),
            ..crate::components::auth::ServerAuth::default()
        })
    }

    fn mint_test_user_jwt(state: &crate::state::AppState, tenant: DataTenantId) -> String {
        let principal = TokenPrincipalRef {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKindWire::User,
            tenant_id: tenant,
            card_ref: None,
            card_ref_scope: Default::default(),
        };
        state
            .auth
            .issuing_key
            .as_ref()
            .expect("test state has issuing key")
            .issue_user_access_token(principal, vec![], chrono::Duration::minutes(5))
            .expect("test jwt mints")
    }
}
