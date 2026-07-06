//! Workload OIDC `jwt-bearer` exchange.

use axum::http::HeaderMap;
use secrecy::SecretString;
use serde_json::json;
use wyrd_auth::exchange_api_key::{ExchangedToken, TokenExchangeSettings};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;

use crate::auth::{auth_not_configured, invalid_token, tenant_slug_from_host};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Workload OIDC `jwt-bearer` exchange service.
#[derive(Debug, Clone, Default)]
pub struct JwtBearer {
    /// Access/refresh token settings.
    pub settings: TokenExchangeSettings,
}

impl JwtBearer {
    /// Exchange a platform-attested workload assertion for a Wyrd token pair.
    #[tracing::instrument(level = "debug", skip(self, state, headers, assertion))]
    pub async fn execute(
        &self,
        state: &AppState,
        headers: &HeaderMap,
        assertion: SecretString,
        tenant: Option<TenantSlug>,
        request_id: &str,
    ) -> Result<ExchangedToken, WyrdErrorResponse> {
        let tenant_id = resolve_workload_tenant(state, headers, tenant).await?;
        let service = wyrd_auth::jwt_bearer::JwtBearer {
            settings: self.settings.clone(),
            issuing_key: state
                .auth
                .issuing_key
                .clone()
                .ok_or_else(auth_not_configured)?,
            verifier: state
                .auth
                .token_verifier
                .clone()
                .ok_or_else(auth_not_configured)?,
            workload_binding_resolver: state
                .auth
                .workload_binding_resolver
                .clone()
                .ok_or_else(auth_not_configured)?,
        };
        service
            .execute(state.postgres.wyrd(), tenant_id, assertion, request_id)
            .await
            .map_err(WyrdErrorResponse::from)
    }
}

async fn resolve_workload_tenant(
    state: &AppState,
    headers: &HeaderMap,
    tenant: Option<TenantSlug>,
) -> Result<DataTenantId, WyrdErrorResponse> {
    if let Some(host_tenant) = tenant_slug_from_host(headers) {
        return resolve_tenant_slug(state.postgres.app_pool(), &host_tenant)
            .await?
            .ok_or_else(|| invalid_token("request host tenant could not be resolved"));
    }

    let Some(fallback) = tenant else {
        return Err(invalid_token(
            "tenant could not be resolved for workload token",
        ));
    };
    resolve_tenant_slug(state.postgres.app_pool(), &fallback)
        .await?
        .ok_or_else(|| invalid_token("requested tenant could not be resolved"))
}

async fn resolve_tenant_slug(
    pool: &sqlx::PgPool,
    slug: &TenantSlug,
) -> Result<Option<DataTenantId>, WyrdErrorResponse> {
    wyrd_sql::queries::platform::tenant_resolver::resolve_by_slug_for_app(pool, slug)
        .await
        .map_err(sql_error)
}

fn sql_error(error: impl Into<wyrd_sql::SqlError>) -> WyrdErrorResponse {
    let error = error.into();
    tracing::warn!(error = %error, "auth db unavailable");
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: json!({ "retry_after_seconds": 1 }),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    use axum::http::{HeaderMap, header};
    use chrono::{Duration as ChronoDuration, Utc};
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use reqwest::Client;
    use secrecy::{ExposeSecret, SecretString};
    use uuid::Uuid;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, JwksCache, IssuerTokenPolicy, TrustedIssuer,
        WorkloadBinding,
    };
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_dev_fixtures::cards::seed_card_with_spec;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{PrincipalKind, RoleRef};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{IssuerUrl, TokenType};
    use wyrd_spec::envelope::{CardKind, Spec};
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::{CardName, SpaceName, TenantSlug};
    use wyrd_spec::reference::CardRef;
    use wyrd_sql::TenantConn;
    use wyrd_sql::queries::auth::{
        grant_role_to_service_account, insert_service_account, role_by_name, upsert_trusted_issuer,
        upsert_workload_binding,
    };
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::AppState;
    use crate::auth::issue_api_key::WyrdApiKey;
    use crate::auth::permission_resolver::SqlPermissionResolver;
    use crate::auth::pg_resolvers::{
        PgIssuerResolver, PgWorkloadBindingResolver, binding_write_from_binding,
        issuer_write_from_trusted,
    };
    use crate::auth::seed::seed_builtin_roles_for_tenant;

    use super::JwtBearer;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";
    const EXTERNAL_ISSUER: &str = "https://idp.example.com/realms/workload";
    const EXTERNAL_AUDIENCE: &str = "wyrd-workload";
    const EXTERNAL_KID: &str = "ext-workload-key";
    const ED_X: &str = "WhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ-DZ8Vw";

    #[tokio::test]
    async fn workload_jwt_bearer_mints_service_token() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let subject = "system:serviceaccount:default:svc";
        let role_name = builtin_role_name();
        let binding = binding(
            tenant,
            subject,
            CardKind::Service,
            "svc",
            Some(EXTERNAL_AUDIENCE),
        );
        let state = test_state_with_external(
            &fixture,
            vec![trusted_issuer(
                tenant,
                jwks_uri(&fixture).await,
                EXTERNAL_AUDIENCE,
            )],
            vec![binding.clone()],
        )
        .await;
        bootstrap_principal(&fixture, tenant, CardKind::Service, "svc", &[role_name])
            .await
            .expect("service principal seeds");

        let assertion = encode_external_token(&external_claims(
            subject,
            EXTERNAL_AUDIENCE,
            ChronoDuration::hours(1),
        ));
        let exchanged = JwtBearer::default()
            .execute(
                &state,
                &tenant_headers(tenant_slug(&fixture)),
                SecretString::from(assertion),
                None,
                "req-1",
            )
            .await
            .expect("jwt-bearer exchange succeeds");

        let verified = verify_issued_token(&state, tenant, exchanged.access_token.expose_secret())
            .await
            .expect("issued token verifies");
        assert!(matches!(
            &verified.principal.kind,
            PrincipalKind::Service { card_ref, .. } if *card_ref == binding.card_ref
        ));
        assert_eq!(role_names(&verified.principal.roles), set_of(&[role_name]));
        assert_eq!(exchanged.token_type, TokenType::Bearer);
        assert!(
            exchanged.refresh_token.is_none(),
            "workload jwt-bearer grant must not issue a refresh token"
        );

        let rows = audit_rows(&fixture, tenant).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].subject_principal_id, rows[0].actor_principal_id);
        assert_eq!(rows[0].error_tag, "");
    }

    #[tokio::test]
    async fn workload_jwt_bearer_falls_back_to_tenant_slug_for_agent() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let slug = tenant_slug(&fixture);
        let subject = "spiffe://cluster.local/ns/default/sa/agent";
        let role_name = builtin_role_name();
        let state = test_state_with_external(
            &fixture,
            vec![trusted_issuer(
                tenant,
                jwks_uri(&fixture).await,
                EXTERNAL_AUDIENCE,
            )],
            vec![binding(
                tenant,
                subject,
                CardKind::Agent,
                "agent",
                Some(EXTERNAL_AUDIENCE),
            )],
        )
        .await;
        bootstrap_principal(&fixture, tenant, CardKind::Agent, "agent", &[role_name])
            .await
            .expect("agent principal seeds");

        let assertion = encode_external_token(&external_claims(
            subject,
            EXTERNAL_AUDIENCE,
            ChronoDuration::hours(1),
        ));
        let exchanged = JwtBearer::default()
            .execute(
                &state,
                &localhost_headers(),
                SecretString::from(assertion),
                Some(slug),
                "req-2",
            )
            .await
            .expect("jwt-bearer exchange succeeds");

        let verified = verify_issued_token(&state, tenant, exchanged.access_token.expose_secret())
            .await
            .expect("issued token verifies");
        assert!(matches!(
            &verified.principal.kind,
            PrincipalKind::Agent { card_ref, .. } if card_ref.name.as_str() == "agent"
        ));
    }

    #[tokio::test]
    async fn wrong_audience_is_rejected_and_audited() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let role_name = builtin_role_name();
        let state = test_state_with_external(
            &fixture,
            vec![trusted_issuer(
                tenant,
                jwks_uri(&fixture).await,
                EXTERNAL_AUDIENCE,
            )],
            vec![binding(
                tenant,
                "system:serviceaccount:default:svc",
                CardKind::Service,
                "svc",
                Some(EXTERNAL_AUDIENCE),
            )],
        )
        .await;
        bootstrap_principal(&fixture, tenant, CardKind::Service, "svc", &[role_name])
            .await
            .expect("service principal seeds");

        let assertion = encode_external_token(&external_claims(
            "system:serviceaccount:default:svc",
            "wrong-audience",
            ChronoDuration::hours(1),
        ));
        let error = JwtBearer::default()
            .execute(
                &state,
                &tenant_headers(tenant_slug(&fixture)),
                SecretString::from(assertion),
                None,
                "req-3",
            )
            .await
            .expect_err("wrong audience rejected");
        assert!(matches!(error.0, WyrdError::InvalidToken { .. }));

        let rows = audit_rows(&fixture, tenant).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].subject_principal_id, Uuid::nil());
        assert_eq!(rows[0].actor_principal_id, Uuid::nil());
        assert_eq!(rows[0].error_tag, "InvalidToken");
    }

    #[tokio::test]
    async fn expired_assertion_is_rejected() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state_with_external(
            &fixture,
            vec![trusted_issuer(
                tenant,
                jwks_uri(&fixture).await,
                EXTERNAL_AUDIENCE,
            )],
            vec![binding(
                tenant,
                "system:serviceaccount:default:svc",
                CardKind::Service,
                "svc",
                Some(EXTERNAL_AUDIENCE),
            )],
        )
        .await;

        let assertion = encode_external_token(&external_claims(
            "system:serviceaccount:default:svc",
            EXTERNAL_AUDIENCE,
            ChronoDuration::seconds(-120),
        ));
        let error = JwtBearer::default()
            .execute(
                &state,
                &tenant_headers(tenant_slug(&fixture)),
                SecretString::from(assertion),
                None,
                "req-4",
            )
            .await
            .expect_err("expired assertion rejected");
        assert!(matches!(error.0, WyrdError::TokenExpired { .. }));
    }

    #[tokio::test]
    async fn untrusted_issuer_is_rejected() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state_with_external(
            &fixture,
            vec![trusted_issuer(
                tenant,
                jwks_uri(&fixture).await,
                EXTERNAL_AUDIENCE,
            )],
            vec![],
        )
        .await;

        let assertion = encode_external_token(&serde_json::json!({
            "sub": "system:serviceaccount:default:svc",
            "iss": "https://idp.example.com/realms/untrusted",
            "aud": EXTERNAL_AUDIENCE,
            "exp": (Utc::now() + ChronoDuration::hours(1)).timestamp(),
            "iat": Utc::now().timestamp(),
        }));
        let error = JwtBearer::default()
            .execute(
                &state,
                &tenant_headers(tenant_slug(&fixture)),
                SecretString::from(assertion),
                None,
                "req-5",
            )
            .await
            .expect_err("untrusted issuer rejected");
        assert!(matches!(error.0, WyrdError::InvalidToken { .. }));
    }

    #[tokio::test]
    async fn server_owned_binding_denial_returns_principal_not_found() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let role_name = builtin_role_name();
        let state = test_state_with_external(
            &fixture,
            vec![trusted_issuer(
                tenant,
                jwks_uri(&fixture).await,
                EXTERNAL_AUDIENCE,
            )],
            vec![],
        )
        .await;
        bootstrap_principal(&fixture, tenant, CardKind::Service, "svc", &[role_name])
            .await
            .expect("service principal seeds");

        let assertion = encode_external_token(&external_claims(
            "system:serviceaccount:default:svc",
            EXTERNAL_AUDIENCE,
            ChronoDuration::hours(1),
        ));
        let error = JwtBearer::default()
            .execute(
                &state,
                &tenant_headers(tenant_slug(&fixture)),
                SecretString::from(assertion),
                None,
                "req-6",
            )
            .await
            .expect_err("unbound workload rejected");
        assert!(matches!(error.0, WyrdError::PrincipalNotFound { .. }));
    }

    #[tokio::test]
    async fn cloud_workload_subject_shape_uses_server_owned_binding() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let subject = "principal://iam.googleapis.com/projects/123/locations/global/workloadIdentityPools/pool/subject/deploy";
        let role_name = builtin_role_name();
        let binding = binding(
            tenant,
            subject,
            CardKind::Service,
            "cloud-service",
            Some(EXTERNAL_AUDIENCE),
        );
        let state = test_state_with_external(
            &fixture,
            vec![trusted_issuer(
                tenant,
                jwks_uri(&fixture).await,
                EXTERNAL_AUDIENCE,
            )],
            vec![binding.clone()],
        )
        .await;
        bootstrap_principal(
            &fixture,
            tenant,
            CardKind::Service,
            "cloud-service",
            &[role_name],
        )
        .await
        .expect("cloud service principal seeds");

        let assertion = encode_external_token(&external_claims(
            subject,
            EXTERNAL_AUDIENCE,
            ChronoDuration::hours(1),
        ));
        let exchanged = JwtBearer::default()
            .execute(
                &state,
                &tenant_headers(tenant_slug(&fixture)),
                SecretString::from(assertion),
                None,
                "req-cloud",
            )
            .await
            .expect("cloud workload exchange succeeds");

        let verified = verify_issued_token(&state, tenant, exchanged.access_token.expose_secret())
            .await
            .expect("issued token verifies");
        assert!(matches!(
            &verified.principal.kind,
            PrincipalKind::Service { card_ref, .. } if *card_ref == binding.card_ref
        ));
    }

    #[tokio::test]
    async fn same_issuer_subject_is_tenant_scoped() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = tenant_b(&fixture).await;
        let role_name = builtin_role_name();
        let subject = "system:serviceaccount:default:svc";
        let jwks = jwks_uri(&fixture).await;
        let state = test_state_with_external(
            &fixture,
            vec![
                trusted_issuer(tenant_a, jwks.clone(), EXTERNAL_AUDIENCE),
                trusted_issuer(tenant_b, jwks, EXTERNAL_AUDIENCE),
            ],
            vec![binding(
                tenant_a,
                subject,
                CardKind::Service,
                "svc",
                Some(EXTERNAL_AUDIENCE),
            )],
        )
        .await;
        bootstrap_principal(&fixture, tenant_a, CardKind::Service, "svc", &[role_name])
            .await
            .expect("service principal seeds");

        let assertion = encode_external_token(&external_claims(
            subject,
            EXTERNAL_AUDIENCE,
            ChronoDuration::hours(1),
        ));
        let error = JwtBearer::default()
            .execute(
                &state,
                &localhost_headers(),
                SecretString::from(assertion),
                Some(TenantSlug::new("workload-tenant-b").expect("slug is valid")),
                "req-7",
            )
            .await
            .expect_err("tenant B binding missing");
        assert!(matches!(error.0, WyrdError::PrincipalNotFound { .. }));
    }

    #[tokio::test]
    async fn api_key_grant_still_mints() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let role_name = builtin_role_name();
        let state = test_state_with_external(&fixture, Vec::new(), Vec::new()).await;
        bootstrap_principal(&fixture, tenant, CardKind::Service, "api-key", &[role_name])
            .await
            .expect("service principal seeds");
        let token = issue_api_key(&fixture, tenant, "api-key").await;

        let mut conn = fixture
            .tenant_conn_for(tenant)
            .await
            .expect("tenant conn opens");
        let exchanged = crate::auth::exchange_api_key::ExchangeApiKey {
            issuing_key: state
                .auth
                .issuing_key
                .clone()
                .expect("issuing key configured"),
            settings: Default::default(),
        }
        .execute(&mut conn, SecretString::from(token), "req-api-key")
        .await
        .expect("api-key exchange still succeeds");
        conn.commit().await.expect("api-key exchange commits");

        assert_eq!(exchanged.token_type, TokenType::Bearer);
        assert!(
            exchanged.refresh_token.is_some(),
            "api-key exchange must still issue a refresh token"
        );
    }

    async fn test_state(fixture: &PgFixture) -> AppState {
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let dir = tempfile::tempdir().expect("jwt-bearer storage tempdir");
        let storage_root = dir.keep().join("jwt-bearer-storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        AppState::new(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
        )
    }

    async fn test_state_with_external(
        fixture: &PgFixture,
        trusted: Vec<TrustedIssuer>,
        bindings: Vec<WorkloadBinding>,
    ) -> AppState {
        // Seed issuers (and then bindings, which FK-reference them) directly into
        // Postgres so the production Pg resolvers serve them per-request. These
        // test issuers use PrivateKeyJwt and carry no secret, so no sealing key is
        // needed. Each issuer/binding carries its own tenant for tenant-scoping.
        for issuer in &trusted {
            let write = issuer_write_from_trusted(issuer, None).expect("issuer encodes");
            let mut conn = fixture
                .tenant_conn_for(issuer.tenant_id)
                .await
                .expect("tenant conn opens");
            upsert_trusted_issuer(&mut conn, &write)
                .await
                .expect("issuer upsert");
            conn.commit().await.expect("issuer seed commits");
        }
        for binding in &bindings {
            let write = binding_write_from_binding(binding).expect("binding encodes");
            let mut conn = fixture
                .tenant_conn_for(binding.tenant_id)
                .await
                .expect("tenant conn opens");
            upsert_workload_binding(&mut conn, &write)
                .await
                .expect("binding upsert");
            conn.commit().await.expect("binding seed commits");
        }

        let issuer_resolver = Arc::new(PgIssuerResolver::new(
            Arc::new(fixture.app_pool().clone()),
            None,
        ));
        let binding_resolver = Arc::new(PgWorkloadBindingResolver::new(Arc::new(
            fixture.app_pool().clone(),
        )));

        let issuing_key = Arc::new(
            IssuingKey::from_ed_pem(
                SecretString::from(PRIVATE_KEY_PEM.to_owned()),
                Kid::new("k1").expect("kid is valid"),
                "wyrd",
            )
            .expect("issuing key parses"),
        );
        let mut local_keys = HashMap::new();
        local_keys.insert(
            Kid::new("k1").expect("kid is valid"),
            Arc::new(public_key_from_pem(PUBLIC_KEY_PEM).expect("public key parses")),
        );
        let verifier = TokenVerifier::new(
            local_keys,
            "wyrd",
            Arc::new(SqlPermissionResolver::new(Arc::new(
                fixture.app_pool().clone(),
            ))),
            WyrdAuthVerifySettings::default(),
        )
        .with_external(
            Arc::new(JwksCache::new(
                Client::new(),
                StdDuration::from_secs(300),
                StdDuration::from_secs(5),
            )),
            Arc::clone(&issuer_resolver),
        );
        test_state(fixture)
            .await
            .with_auth(crate::components::auth::ServerAuth {
                issuing_key: Some(issuing_key),
                token_verifier: Some(Arc::new(verifier)),
                trusted_issuer_resolver: Some(issuer_resolver),
                workload_binding_resolver: Some(binding_resolver),
                ..crate::components::auth::ServerAuth::default()
            })
    }

    async fn bootstrap_principal(
        fixture: &PgFixture,
        tenant: DataTenantId,
        card_kind: CardKind,
        name: &str,
        role_names: &[&str],
    ) -> Result<Uuid, sqlx::Error> {
        let mut conn = fixture
            .tenant_conn_for(tenant)
            .await
            .expect("tenant conn opens");
        seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .expect("builtin roles seed");
        let creator_id = insert_creator_user(fixture, tenant).await?;
        let id = Uuid::now_v7();
        let principal_kind = match &card_kind {
            CardKind::Service => "service",
            CardKind::Agent => "agent",
            other => panic!("unexpected card kind in test: {other:?}"),
        };
        let card_ref = card_ref(card_kind, name);
        insert_test_card(&mut conn, &card_ref, creator_id).await?;
        insert_service_account(
            &mut conn,
            id,
            principal_kind,
            &card_ref,
            name,
            None,
            creator_id,
        )
        .await?;
        for role_name in role_names {
            let role = role_by_name(&mut conn, role_name)
                .await?
                .expect("builtin role exists");
            grant_role_to_service_account(&mut conn, id, role.id).await?;
        }
        conn.commit().await.expect("principal seed commits");
        Ok(id)
    }

    async fn insert_test_card(
        conn: &mut TenantConn<'_>,
        target_ref: &CardRef,
        created_by: Uuid,
    ) -> Result<(), sqlx::Error> {
        let spec = match &target_ref.kind {
            CardKind::Service => {
                Spec::from_kind_and_value(&CardKind::Service, serde_json::json!({}))
                    .expect("service fixture spec decodes")
            }
            CardKind::Agent => {
                let prompt_ref = card_ref(CardKind::Prompt, &format!("{}-prompt", target_ref.name));
                let prompt_spec = Spec::from_kind_and_value(
                    &CardKind::Prompt,
                    serde_json::json!({
                        "provider": "openai",
                        "model": "gpt-4o-mini",
                        "messages": "Fixture agent."
                    }),
                )
                .expect("prompt fixture spec decodes");
                seed_card_with_spec(conn, &prompt_ref, &prompt_spec, created_by).await;
                Spec::from_kind_and_value(
                    &CardKind::Agent,
                    serde_json::json!({
                        "prompt": prompt_ref,
                    }),
                )
                .expect("agent fixture spec decodes")
            }
            CardKind::Prompt => Spec::from_kind_and_value(
                &CardKind::Prompt,
                serde_json::json!({
                    "provider": "openai",
                    "model": "gpt-4o-mini",
                    "messages": "Fixture agent."
                }),
            )
            .expect("prompt fixture spec decodes"),
            other => panic!("unexpected test card kind: {other:?}"),
        };
        seed_card_with_spec(conn, target_ref, &spec, created_by).await;
        Ok(())
    }

    async fn insert_creator_user(
        fixture: &PgFixture,
        tenant: DataTenantId,
    ) -> Result<Uuid, sqlx::Error> {
        let mut conn = fixture
            .tenant_conn_for(tenant)
            .await
            .expect("tenant conn opens");
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, password_hash, auth_type, status)
             VALUES ($1, $2, $3, NULL, 'password', 'active')",
        )
        .bind(id)
        .bind(tenant.as_uuid())
        .bind(format!("creator-{id}@test.wyrd"))
        .execute(&mut **conn.transaction())
        .await?;
        conn.commit().await.expect("creator seed commits");
        Ok(id)
    }

    async fn issue_api_key(fixture: &PgFixture, tenant: DataTenantId, name: &str) -> String {
        let mut conn = fixture
            .tenant_conn_for(tenant)
            .await
            .expect("tenant conn opens");
        let row = sqlx::query_as::<_, (Uuid,)>(
            "SELECT id FROM wyrd.auth_service_accounts WHERE data_tenant_id = $1 AND name = $2",
        )
        .bind(tenant.as_uuid())
        .bind(name)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("service account exists");
        let principal_id = row.0;
        conn.commit().await.expect("principal lookup commits");
        let key = WyrdApiKey::generate(tenant);
        let key_hash = wyrd_auth_issue::hash_api_key(&key.secret).expect("api key hashes");
        let created_by = insert_creator_user(fixture, tenant)
            .await
            .expect("creator user inserts");
        let mut conn = fixture
            .tenant_conn_for(tenant)
            .await
            .expect("tenant conn opens");
        let prefix = key.prefix.clone();
        wyrd_sql::queries::auth::insert_api_key(
            &mut conn,
            Uuid::now_v7(),
            principal_id,
            &prefix,
            &key_hash,
            created_by,
            Utc::now() + ChronoDuration::days(30),
        )
        .await
        .expect("api key inserts");
        conn.commit().await.expect("api key seed commits");
        key.secret.expose_secret().to_owned()
    }

    async fn audit_rows(fixture: &PgFixture, tenant: DataTenantId) -> Vec<AuditRow> {
        let mut conn = fixture
            .tenant_conn_for(tenant)
            .await
            .expect("tenant conn opens");
        let rows = sqlx::query_as::<_, (Uuid, Uuid, serde_json::Value)>(
            "SELECT subject_principal_id, actor_principal_id, act_chain
             FROM wyrd.audit_token_exchange
             WHERE data_tenant_id = $1
             ORDER BY issued_at DESC",
        )
        .bind(tenant.as_uuid())
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("audit rows fetch");
        conn.commit().await.expect("audit query commits");
        rows.into_iter()
            .map(
                |(subject_principal_id, actor_principal_id, act_chain)| AuditRow {
                    subject_principal_id,
                    actor_principal_id,
                    error_tag: act_chain
                        .as_array()
                        .and_then(|items| items.first())
                        .and_then(|value| value.get("error"))
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_owned(),
                },
            )
            .collect()
    }

    async fn verify_issued_token(
        state: &AppState,
        tenant: DataTenantId,
        token: &str,
    ) -> Result<wyrd_auth_verify::VerifiedToken, wyrd_auth_verify::AuthError> {
        state
            .auth
            .token_verifier
            .as_ref()
            .expect("token verifier configured")
            .verify(&SecretString::from(token.to_owned()), &tenant)
            .await
            .map(|token| (*token).clone())
    }

    fn builtin_role_name() -> &'static str {
        wyrd_runtime::builtin_roles::BUILTIN_ROLES
            .first()
            .expect("builtin roles available")
            .name
    }

    fn tenant_slug(fixture: &PgFixture) -> TenantSlug {
        TenantSlug::new(fixture.tenant_slug().to_owned()).expect("tenant slug is valid")
    }

    fn tenant_headers(slug: TenantSlug) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            format!("{}.example.com", slug.as_str())
                .parse()
                .expect("host header is valid"),
        );
        headers
    }

    fn localhost_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            "localhost:3000".parse().expect("host header is valid"),
        );
        headers
    }

    async fn jwks_uri(fixture: &PgFixture) -> url::Url {
        let _ = fixture;
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;
        let url = format!("{}/jwks", server.uri())
            .parse()
            .expect("wiremock URI is valid");
        Box::leak(Box::new(server));
        url
    }

    async fn tenant_b(fixture: &PgFixture) -> DataTenantId {
        let tenant_b = DataTenantId::new_v7();
        sqlx::query(
            "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
             VALUES ($1, 'workload-tenant-b', 'Workload Tenant B', 'active')",
        )
        .bind(tenant_b.as_uuid())
        .execute(fixture.platform_admin_pool())
        .await
        .expect("tenant B inserts");
        tenant_b
    }

    fn binding(
        tenant: DataTenantId,
        subject: &str,
        card_kind: CardKind,
        name: &str,
        audience: Option<&str>,
    ) -> WorkloadBinding {
        WorkloadBinding {
            tenant_id: tenant,
            issuer: IssuerUrl::new(EXTERNAL_ISSUER).expect("issuer URL is valid"),
            subject: subject.to_owned(),
            audience: audience.map(|audience| audience.to_owned()),
            card_ref: card_ref(card_kind, name),
        }
    }

    fn trusted_issuer(tenant: DataTenantId, jwks_uri: url::Url, audience: &str) -> TrustedIssuer {
        TrustedIssuer {
            tenant_id: tenant,
            issuer: IssuerUrl::new(EXTERNAL_ISSUER).expect("issuer URL is valid"),
            jwks_uri,
            expected_audience: audience.to_owned(),
            client_id: audience.to_owned(),
            client_auth: ClientAuth::PrivateKeyJwt,
            claim_mapping: ClaimMapping {
                subject: ClaimPath::new("sub"),
                email: None,
                groups: None,
            },
            group_role_map: HashMap::new(),
            default_roles: Vec::new(),
            principal_kind: IssuerTokenPolicy::Workload,
            jwks_ttl: StdDuration::from_secs(300),
        }
    }

    fn external_claims(
        subject: &str,
        audience: &str,
        expires_in: ChronoDuration,
    ) -> serde_json::Value {
        let now = Utc::now();
        serde_json::json!({
            "sub": subject,
            "iss": EXTERNAL_ISSUER,
            "aud": audience,
            "exp": (now + expires_in).timestamp(),
            "iat": now.timestamp(),
        })
    }

    fn encode_external_token(claims: &serde_json::Value) -> String {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(EXTERNAL_KID.to_owned());
        let key = EncodingKey::from_ed_pem(PRIVATE_KEY_PEM.as_bytes())
            .expect("external private key parses");
        encode(&header, claims, &key).expect("external token signs")
    }

    fn role_names(roles: &[RoleRef]) -> BTreeSet<String> {
        roles.iter().map(|role| role.as_str().to_owned()).collect()
    }

    fn set_of(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    fn ed_jwks_json(kid: &str) -> serde_json::Value {
        serde_json::json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "kid": kid,
                "x": ED_X,
            }]
        })
    }

    #[derive(Debug)]
    struct AuditRow {
        subject_principal_id: Uuid,
        actor_principal_id: Uuid,
        error_tag: String,
    }
}
