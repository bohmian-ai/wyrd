//! Human OIDC callback and authorization-code exchange.

use axum::Json;
use axum::extract::{Extension, Query, State};
use axum::http::HeaderMap;
use secrecy::SecretString;
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{CallbackQuery, TokenResponse};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_sql::SqlError;

use crate::auth::{auth_not_configured, invalid_token, tenant_slug_from_host};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Handler for `GET /auth/callback`.
#[tracing::instrument(level = "debug", skip(state, headers), fields(state = %query.state))]
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    request_id: Option<Extension<RequestId>>,
    Query(query): Query<CallbackQuery>,
) -> Result<Json<TokenResponse>, WyrdErrorResponse> {
    let request_id = request_id
        .as_ref()
        .map(|Extension(id)| id.as_str().to_owned())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let token = exchange_authorization_code(
        &state,
        &headers,
        query.code.into_secret_string(),
        &query.state,
        &request_id,
    )
    .await?;
    Ok(Json(token))
}

/// Execute the authorization-code grant for `POST /auth/token`.
pub async fn exchange_authorization_code(
    state: &AppState,
    headers: &HeaderMap,
    code: SecretString,
    state_key: &str,
    request_id: &str,
) -> Result<TokenResponse, WyrdErrorResponse> {
    let tenant_id = resolve_callback_tenant(state, headers).await?;
    let service = wyrd_auth::callback::AuthorizationCodeExchange {
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
        trusted_issuer_resolver: state
            .auth
            .trusted_issuer_resolver
            .clone()
            .ok_or_else(auth_not_configured)?,
        http: state.deployment_profile.screened_http(),
    };
    service
        .execute(
            state.postgres.wyrd(),
            tenant_id,
            code,
            state_key,
            request_id,
        )
        .await
        .map_err(WyrdErrorResponse::from)
}

async fn resolve_callback_tenant(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<DataTenantId, WyrdErrorResponse> {
    let Some(slug) = tenant_slug_from_host(headers) else {
        return Err(invalid_token("request host does not encode a tenant"));
    };
    match wyrd_sql::queries::platform::tenant_resolver::resolve_by_slug_for_app(
        state.postgres.app_pool(),
        &slug,
    )
    .await
    .map_err(sql_error)?
    {
        Some(tenant) => Ok(tenant),
        None => Err(invalid_token("request tenant could not be resolved")),
    }
}

fn sql_error(error: impl Into<SqlError>) -> WyrdErrorResponse {
    let error = error.into();
    tracing::warn!(error = %error, "OIDC callback SQL unavailable");
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({ "retry_after_seconds": 1 }),
    })
}

#[cfg(test)]
mod pg_tests {
    use std::collections::{BTreeSet, HashMap};
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    use axum::http::{HeaderMap, HeaderValue, header};
    use chrono::{Duration as ChronoDuration, Utc};
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use secrecy::SecretString;
    use uuid::Uuid;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_oidc::{ClaimMapping, ClaimPath, ClientAuth, JwksCache, TrustedIssuer};
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_crypt::SecretKey;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerTokenPolicy;
    use wyrd_spec::auth::{IssuerUrl, TokenResponse, TokenType};
    use wyrd_sql::queries::auth::upsert_trusted_issuer;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::auth::permission_resolver::SqlPermissionResolver;
    use crate::auth::pg_resolvers::{PgIssuerResolver, issuer_write_from_trusted};
    use crate::http::error::WyrdErrorResponse;
    use crate::state::AppState;
    use wyrd_auth::callback::{
        AuthorizationCodeExchange, audit_authorization_code_failure, ensure_user_identity,
        role_names_to_refs, verify_nonce,
    };
    use wyrd_auth::login::{LoginStateEntry, PgLoginStateStore};

    use crate::auth::tenant_slug_from_host;

    use super::exchange_authorization_code;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";
    const EXTERNAL_ISSUER: &str = "https://idp.example.com/realms/acme";
    const EXTERNAL_AUDIENCE: &str = "wyrd-client-id";
    const EXTERNAL_KID: &str = "ext-key-1";
    const ED_X: &str = "WhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ-DZ8Vw";

    #[test]
    fn role_names_to_refs_maps_defaults_and_known_groups_only() {
        let trusted = trusted_issuer(
            DataTenantId::new_v7(),
            HashMap::from([("data-science".to_owned(), vec!["data_science".to_owned()])]),
            vec!["viewer".to_owned()],
        );

        let roles = role_set(
            role_names_to_refs(
                &trusted,
                &["data-science".to_owned(), "marketing".to_owned()],
            )
            .expect("roles map"),
        );

        assert_eq!(
            roles,
            BTreeSet::from(["data_science".to_owned(), "viewer".to_owned()])
        );
    }

    #[test]
    fn role_names_to_refs_uses_defaults_when_groups_empty() {
        let trusted = trusted_issuer(
            DataTenantId::new_v7(),
            HashMap::from([("data-science".to_owned(), vec!["data_science".to_owned()])]),
            vec!["viewer".to_owned()],
        );

        let roles = role_set(role_names_to_refs(&trusted, &[]).expect("roles map"));

        assert_eq!(roles, BTreeSet::from(["viewer".to_owned()]));
    }

    #[test]
    fn role_names_to_refs_default_denies_unmapped_groups() {
        let trusted = trusted_issuer(DataTenantId::new_v7(), HashMap::new(), Vec::new());

        let roles = role_names_to_refs(&trusted, &["anything".to_owned()]).expect("roles map");

        assert!(roles.is_empty());
    }

    #[test]
    fn role_names_to_refs_deduplicates_default_and_group_roles() {
        let trusted = trusted_issuer(
            DataTenantId::new_v7(),
            HashMap::from([("data-science".to_owned(), vec!["viewer".to_owned()])]),
            vec!["viewer".to_owned()],
        );

        let roles = role_set(
            role_names_to_refs(&trusted, &["data-science".to_owned()]).expect("roles map"),
        );

        assert_eq!(roles, BTreeSet::from(["viewer".to_owned()]));
    }

    #[test]
    fn verify_nonce_rejects_mismatched_id_token_nonce() {
        let state = LoginStateEntry {
            code_verifier: SecretString::from("verifier".to_owned()),
            nonce: "nonce-a".to_owned(),
            issuer: "https://idp.example.com/realms/acme".to_owned(),
            redirect_uri: "https://test-tenant-1.example.com/auth/callback".to_owned(),
        };

        let error = verify_nonce(&state, &serde_json::json!({ "nonce": "nonce-b" }))
            .expect_err("nonce mismatch rejects");

        assert_eq!(error.code(), "WYRD_AUTH_400_INVALID_NONCE");
    }

    #[test]
    fn callback_tenant_slug_rejects_localhost_hosts() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_static("acme.localhost:8080"),
        );

        assert!(tenant_slug_from_host(&headers).is_none());

        headers.insert(
            header::HOST,
            HeaderValue::from_static("acme.example.com:8080"),
        );

        assert_eq!(
            tenant_slug_from_host(&headers).map(|slug| slug.as_str().to_owned()),
            Some("acme".to_owned())
        );
    }

    #[tokio::test]
    async fn missing_state_writes_failure_audit_with_nil_principal() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state =
            test_state_with_external(&fixture, trusted_issuer(tenant, HashMap::new(), Vec::new()))
                .await;
        let headers = tenant_headers(fixture.tenant_slug());

        let error = exchange_authorization_code(
            &state,
            &headers,
            SecretString::from("code".to_owned()),
            "missing-state",
            "req-missing-state",
        )
        .await
        .expect_err("missing state fails");

        assert_eq!(error.0.code(), "WYRD_AUTH_400_INVALID_STATE");
        let audit = audit_rows(&fixture).await;
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].0, Uuid::nil());
        assert_eq!(audit[0].1, "denied");
        assert_eq!(audit[0].2, auth_failure_detail("INVALID_TOKEN"));
    }

    #[tokio::test]
    async fn stored_invalid_issuer_writes_failure_audit_with_nil_principal() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state =
            test_state_with_external(&fixture, trusted_issuer(tenant, HashMap::new(), Vec::new()))
                .await;
        let headers = tenant_headers(fixture.tenant_slug());
        PgLoginStateStore::new(fixture.app_pool().clone())
            .put(
                tenant,
                "bad-issuer-state",
                LoginStateEntry {
                    code_verifier: SecretString::from("verifier".to_owned()),
                    nonce: "nonce".to_owned(),
                    issuer: "not a url".to_owned(),
                    redirect_uri: "https://test-tenant-1.example.com/auth/callback".to_owned(),
                },
                StdDuration::from_secs(300),
            )
            .await
            .expect("state inserts");

        let error = exchange_authorization_code(
            &state,
            &headers,
            SecretString::from("code".to_owned()),
            "bad-issuer-state",
            "req-bad-issuer",
        )
        .await
        .expect_err("stored issuer rejects");

        assert_eq!(error.0.code(), "WYRD_AUTH_401_INVALID_TOKEN");
        let audit = audit_rows(&fixture).await;
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].0, Uuid::nil());
        assert_eq!(audit[0].1, "denied");
        assert_eq!(audit[0].2, auth_failure_detail("INVALID_TOKEN"));
    }

    #[tokio::test]
    async fn finish_authorization_code_exchange_mints_tokens_and_success_audit() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;
        let jwks_uri = jwks_uri(&server);
        let state = test_state_with_external(
            &fixture,
            trusted_issuer_with_jwks(tenant, jwks_uri.clone(), HashMap::new(), Vec::new()),
        )
        .await;
        let trusted = trusted_issuer_with_jwks(tenant, jwks_uri, HashMap::new(), Vec::new());
        let login_state = login_state_with_nonce("nonce-ok");
        let id_token = encode_external_token(&external_claims(
            EXTERNAL_AUDIENCE,
            "nonce-ok",
            Some("ext@example.com"),
            &[],
        ));
        let (response, audit_principal_id) = finish_id_token_exchange(
            &state,
            tenant,
            &trusted,
            &login_state,
            &id_token,
            "req-success",
        )
        .await
        .expect("callback completion succeeds");

        assert_eq!(response.token_type, TokenType::Bearer);
        assert!(!response.access_token.expose().is_empty());
        assert!(
            !response
                .refresh_token
                .as_ref()
                .expect("human OIDC login issues a refresh token")
                .expose()
                .is_empty()
        );
        assert_ne!(audit_principal_id, Uuid::nil());
        assert_eq!(refresh_token_count(&fixture, audit_principal_id).await, 1);
        let audit = audit_rows(&fixture).await;
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].0, audit_principal_id);
        assert_eq!(audit[0].1, "allowed");
        let user = audit_principal_id.to_string();
        assert_eq!(audit[0].2["subject_principal_id"], user.as_str());
        assert_eq!(audit[0].2["actor_principal_id"], user.as_str());
        assert_eq!(audit[0].2["delegation_chain"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn finish_authorization_code_exchange_wrong_audience_writes_failure_audit() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;
        let jwks_uri = jwks_uri(&server);
        let state = test_state_with_external(
            &fixture,
            trusted_issuer_with_jwks(tenant, jwks_uri.clone(), HashMap::new(), Vec::new()),
        )
        .await;
        let trusted = trusted_issuer_with_jwks(tenant, jwks_uri, HashMap::new(), Vec::new());
        let login_state = login_state_with_nonce("nonce-ok");
        let id_token = encode_external_token(&external_claims(
            "wrong-audience",
            "nonce-ok",
            Some("ext@example.com"),
            &[],
        ));

        let error = finish_with_failure_audit(
            &state,
            tenant,
            &trusted,
            &login_state,
            &id_token,
            "req-wrong-audience",
        )
        .await
        .expect_err("wrong audience rejects");

        assert_eq!(error.0.code(), "WYRD_AUTH_401_INVALID_TOKEN");
        let audit = audit_rows(&fixture).await;
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].0, Uuid::nil());
        assert_eq!(audit[0].1, "denied");
        assert_eq!(audit[0].2, auth_failure_detail("INVALID_TOKEN"));
    }

    #[tokio::test]
    async fn ensure_user_identity_keys_on_issuer_and_subject_not_email() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let trusted = trusted_issuer(tenant, HashMap::new(), Vec::new());
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let first = ensure_user_identity(&mut conn, &trusted, "subject-1", Some("a@example.com"))
            .await
            .expect("first identity upserts");
        let second = ensure_user_identity(&mut conn, &trusted, "subject-1", Some("b@example.com"))
            .await
            .expect("second identity reuses subject");

        assert_eq!(first, second);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.auth_users WHERE auth_type = 'oidc'")
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("user count query runs");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn ensure_user_identity_allows_missing_email() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let trusted = trusted_issuer(tenant, HashMap::new(), Vec::new());
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let user_id = ensure_user_identity(&mut conn, &trusted, "subject-no-email", None)
            .await
            .expect("identity with omitted email inserts");
        let email: Option<String> =
            sqlx::query_scalar("SELECT email FROM wyrd.auth_users WHERE id = $1")
                .bind(user_id)
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("email query runs");

        assert!(email.is_none());
    }

    /// A nonce mismatch is audited as a credential refusal.
    #[test]
    fn callback_nonce_failure_maps_to_invalid_token_code() {
        assert_eq!(
            wyrd_auth::audit::auth_failure_code(&wyrd_spec::error::WyrdError::InvalidNonce {
                message: "nonce".to_owned(),
                details: serde_json::json!({}),
            }),
            wyrd_spec::vala::audit_detail::AuditErrorCode::InvalidToken
        );
    }

    fn trusted_issuer(
        tenant_id: DataTenantId,
        group_role_map: HashMap<String, Vec<String>>,
        default_roles: Vec<String>,
    ) -> TrustedIssuer {
        TrustedIssuer {
            tenant_id,
            issuer: IssuerUrl::new("https://idp.example.com/realms/acme")
                .expect("issuer URL is valid"),
            jwks_uri: "https://idp.example.com/realms/acme/jwks"
                .parse()
                .expect("jwks URL is valid"),
            expected_audience: "wyrd-client-id".to_owned(),
            client_id: "wyrd-client-id".to_owned(),
            client_auth: ClientAuth::SecretPost(SecretString::from("client-secret".to_owned())),
            claim_mapping: ClaimMapping {
                subject: ClaimPath::new("sub"),
                email: Some(ClaimPath::new("email")),
                groups: Some(ClaimPath::new("groups")),
            },
            group_role_map,
            default_roles,
            principal_kind: IssuerTokenPolicy::Human,
            jwks_ttl: StdDuration::from_secs(300),
        }
    }

    fn trusted_issuer_with_jwks(
        tenant_id: DataTenantId,
        jwks_uri: url::Url,
        group_role_map: HashMap<String, Vec<String>>,
        default_roles: Vec<String>,
    ) -> TrustedIssuer {
        TrustedIssuer {
            tenant_id,
            issuer: IssuerUrl::new(EXTERNAL_ISSUER).expect("issuer URL is valid"),
            jwks_uri,
            expected_audience: EXTERNAL_AUDIENCE.to_owned(),
            client_id: EXTERNAL_AUDIENCE.to_owned(),
            client_auth: ClientAuth::SecretPost(SecretString::from("client-secret".to_owned())),
            claim_mapping: ClaimMapping {
                subject: ClaimPath::new("sub"),
                email: Some(ClaimPath::new("email")),
                groups: Some(ClaimPath::new("groups")),
            },
            group_role_map,
            default_roles,
            principal_kind: IssuerTokenPolicy::Human,
            jwks_ttl: StdDuration::from_secs(300),
        }
    }

    fn login_state_with_nonce(nonce: &str) -> LoginStateEntry {
        LoginStateEntry {
            code_verifier: SecretString::from("verifier".to_owned()),
            nonce: nonce.to_owned(),
            issuer: EXTERNAL_ISSUER.to_owned(),
            redirect_uri: "https://test-tenant-1.example.com/auth/callback".to_owned(),
        }
    }

    async fn finish_with_failure_audit(
        state: &AppState,
        tenant_id: DataTenantId,
        trusted: &TrustedIssuer,
        login_state: &LoginStateEntry,
        id_token: &str,
        request_id: &str,
    ) -> Result<TokenResponse, WyrdErrorResponse> {
        let mut audit_principal_id = Uuid::nil();
        let result = authorization_exchange_service(state)
            .finish_id_token_exchange(
                state.postgres.wyrd(),
                tenant_id,
                trusted,
                login_state,
                id_token,
                request_id,
            )
            .await
            .map(|(token, principal_id)| {
                audit_principal_id = principal_id;
                token
            })
            .map_err(WyrdErrorResponse::from);
        if let Err(error) = &result {
            audit_authorization_code_failure(
                state.postgres.wyrd(),
                tenant_id,
                audit_principal_id,
                request_id,
                &error.0,
            )
            .await;
        }
        result
    }

    async fn finish_id_token_exchange(
        state: &AppState,
        tenant_id: DataTenantId,
        trusted: &TrustedIssuer,
        login_state: &LoginStateEntry,
        id_token: &str,
        request_id: &str,
    ) -> Result<(TokenResponse, Uuid), WyrdErrorResponse> {
        authorization_exchange_service(state)
            .finish_id_token_exchange(
                state.postgres.wyrd(),
                tenant_id,
                trusted,
                login_state,
                id_token,
                request_id,
            )
            .await
            .map_err(WyrdErrorResponse::from)
    }

    fn authorization_exchange_service(state: &AppState) -> AuthorizationCodeExchange {
        AuthorizationCodeExchange {
            issuing_key: state
                .auth
                .issuing_key
                .clone()
                .expect("test state has issuing key"),
            verifier: state
                .auth
                .token_verifier
                .clone()
                .expect("test state has verifier"),
            trusted_issuer_resolver: state
                .auth
                .trusted_issuer_resolver
                .clone()
                .expect("test state has issuer resolver"),
            http: state.deployment_profile.screened_http(),
        }
    }

    fn jwks_uri(server: &MockServer) -> url::Url {
        format!("{}/jwks", server.uri())
            .parse()
            .expect("wiremock URI is valid")
    }

    fn ed_jwks_json(kid: &str) -> serde_json::Value {
        serde_json::json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "kid": kid,
                "x": ED_X
            }]
        })
    }

    fn external_claims(
        audience: &str,
        nonce: &str,
        email: Option<&str>,
        groups: &[&str],
    ) -> serde_json::Value {
        let now = Utc::now();
        let mut claims = serde_json::json!({
            "sub": "ext-user@idp.example.com",
            "iss": EXTERNAL_ISSUER,
            "aud": audience,
            "exp": (now + ChronoDuration::hours(1)).timestamp(),
            "iat": now.timestamp(),
            "nonce": nonce,
            "groups": groups,
        });
        if let Some(email) = email {
            claims["email"] = serde_json::json!(email);
        }
        claims
    }

    fn encode_external_token(claims: &serde_json::Value) -> String {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(EXTERNAL_KID.to_owned());
        let key = EncodingKey::from_ed_pem(PRIVATE_KEY_PEM.as_bytes())
            .expect("external private key parses");
        encode(&header, claims, &key).expect("external token signs")
    }

    fn role_set(roles: Vec<wyrd_runtime::RoleRef>) -> BTreeSet<String> {
        roles
            .iter()
            .map(wyrd_runtime::RoleRef::as_str)
            .map(ToOwned::to_owned)
            .collect()
    }

    async fn test_state(fixture: &PgFixture) -> AppState {
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let dir = tempfile::tempdir().expect("callback storage tempdir");
        let storage_root = dir.keep().join("callback-storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        crate::test_support::test_app_state(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
    }

    async fn test_state_with_external(fixture: &PgFixture, trusted: TrustedIssuer) -> AppState {
        // Seed the issuer into Postgres so the production Pg resolver serves it on
        // the verifier's external path. These issuers carry a SecretPost client
        // secret, so a deterministic test sealing key encrypts it on write and
        // decrypts it on read.
        let sealing_key = Arc::new(SecretKey::from_bytes([7_u8; 32]));
        let write =
            issuer_write_from_trusted(&trusted, Some(&sealing_key)).expect("issuer encodes");
        let mut conn = fixture
            .tenant_conn_for(trusted.tenant_id)
            .await
            .expect("tenant conn opens");
        upsert_trusted_issuer(&mut conn, &write)
            .await
            .expect("issuer upsert");
        conn.commit().await.expect("issuer seed commits");

        let issuer_resolver = Arc::new(PgIssuerResolver::new(
            Arc::new(fixture.app_pool().clone()),
            Some(Arc::clone(&sealing_key)),
        ));

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
                wyrd_auth_oidc::ScreenedHttp::allowing_internal(),
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
                sealing_key: Some(sealing_key),
                ..crate::components::auth::ServerAuth::default()
            })
    }

    fn tenant_headers(slug: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_str(&format!("{slug}.example.com")).expect("host is valid"),
        );
        headers
    }

    /// Staged `auth.token.exchange` events as `(principal_id, outcome, detail)`,
    /// oldest first.
    ///
    /// # Panics
    /// Panics when the query fails or a detail is not JSON.
    async fn audit_rows(fixture: &PgFixture) -> Vec<(Uuid, String, serde_json::Value)> {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
            "SELECT principal_id, outcome, detail
               FROM vala.audit_staging
              WHERE operation = 'auth.token.exchange'
              ORDER BY seq ASC",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("audit query runs");
        rows.into_iter()
            .map(|(principal_id, outcome, detail)| {
                let detail = serde_json::from_str(&detail).expect("audit detail is json");
                (principal_id, outcome, detail)
            })
            .collect()
    }

    /// Canonical detail of a refused exchange with `error_code`.
    fn auth_failure_detail(error_code: &str) -> serde_json::Value {
        serde_json::json!({ "kind": "auth_failure", "error_code": error_code })
    }

    async fn refresh_token_count(fixture: &PgFixture, principal_id: Uuid) -> i64 {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        sqlx::query_scalar(
            "SELECT COUNT(*)
               FROM wyrd.auth_refresh_tokens
              WHERE principal_kind = 'user'
                AND principal_id = $1",
        )
        .bind(principal_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("refresh token count query runs")
    }
}
