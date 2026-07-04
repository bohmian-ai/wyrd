//! Human OIDC callback and authorization-code exchange.

use axum::Json;
use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, header};
use chrono::{Duration as ChronoDuration, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
use wyrd_auth_oidc::{ClientAuth, OidcProvider, TrustedIssuer};
use wyrd_auth_verify::PrincipalKindWire;
use wyrd_auth_verify::TokenPrincipalRef;
use wyrd_runtime::{PermissionSet, Principal, PrincipalId, PrincipalKind, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{CallbackQuery, TokenResponse, TokenType};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_sql::queries::auth::{
    delete_user, insert_audit_token_exchange, insert_refresh_token, insert_user,
    upsert_user_identity, user_id_by_identity,
};
use wyrd_sql::{SqlError, TenantConn};

use crate::auth::exchange_api_key::{ExchangedToken, role_refs, token_hash};
use crate::auth::login::{LoginStateEntry, PgLoginStateStore};
use crate::error::WyrdErrorResponse;
use crate::state::AppState;

const ACCESS_TTL: ChronoDuration = ChronoDuration::seconds(15 * 60);
const REFRESH_TTL: ChronoDuration = ChronoDuration::seconds(30 * 24 * 60 * 60);

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
///
/// This entrypoint owns the external OIDC round trip: tenant resolution,
/// consumed login-state lookup, trusted-issuer lookup, discovery, and token
/// endpoint exchange. Once an ID token is available, Wyrd-side verification and
/// durable session writes are delegated to `finish_authorization_code_exchange`.
pub async fn exchange_authorization_code(
    state: &AppState,
    headers: &HeaderMap,
    code: SecretString,
    state_key: &str,
    request_id: &str,
) -> Result<TokenResponse, WyrdErrorResponse> {
    let tenant_id = resolve_callback_tenant(state, headers).await?;
    let store = PgLoginStateStore::new(state.pool.clone());
    let mut audit_principal_id = Uuid::nil();
    let result = async {
        let Some(login_state) = store.take(tenant_id, state_key).await.map_err(sql_error)? else {
            return Err(invalid_state(
                "login state is missing, expired, or already consumed",
            ));
        };
        let issuer = wyrd_spec::auth::IssuerUrl::new(login_state.issuer.clone())
            .map_err(|_| invalid_token("stored issuer URL is invalid"))?;
        let trusted = crate::auth::trusted_issuer(state, tenant_id, &issuer).await?;
        let provider = discover_provider(&trusted).await?;
        let id_token = exchange_code_for_id_token(&provider, &trusted, &login_state, code).await?;
        finish_authorization_code_exchange(
            state,
            tenant_id,
            &trusted,
            &login_state,
            &id_token,
            request_id,
            &mut audit_principal_id,
        )
        .await
    }
    .await;

    match result {
        Ok(token) => Ok(token),
        Err(error) => {
            audit_authorization_code_failure(
                state,
                tenant_id,
                audit_principal_id,
                request_id,
                &error,
            )
            .await;
            Err(error)
        }
    }
}

/// Complete a verified human OIDC callback inside Wyrd.
///
/// The external authorization code has already been exchanged for an ID token
/// by the caller. This function handles only Wyrd-owned durable behavior:
/// verifying the external ID token against the tenant's trusted issuer,
/// enforcing the stored nonce, resolving or creating the local user identity,
/// mapping trusted groups to local roles, issuing Wyrd tokens, and recording
/// refresh-token plus audit rows.
///
/// `audit_principal_id` is updated as soon as the user identity is known so the
/// outer failure auditor can attribute later failures to the resolved user.
async fn finish_authorization_code_exchange(
    state: &AppState,
    tenant_id: DataTenantId,
    trusted: &TrustedIssuer,
    login_state: &LoginStateEntry,
    id_token: &str,
    request_id: &str,
    audit_principal_id: &mut Uuid,
) -> Result<TokenResponse, WyrdErrorResponse> {
    let verifier = state
        .token_verifier
        .as_ref()
        .ok_or_else(auth_not_configured)?;
    let verified = verifier
        .verify_external(&tenant_id, id_token)
        .await
        .map_err(WyrdErrorResponse::from)?;
    verify_nonce(login_state, &verified.raw_claims)?;

    let mut conn = TenantConn::acquire(&state.pool, tenant_id)
        .await
        .map_err(sql_error)?;
    let principal_id = ensure_user_identity(
        &mut conn,
        trusted,
        &verified.subject,
        verified.email.as_deref(),
    )
    .await
    .map_err(sql_error)?;
    *audit_principal_id = principal_id;
    let roles = role_names_to_refs(trusted, &verified.groups)?;
    let issuing_key = state.issuing_key.as_ref().ok_or_else(auth_not_configured)?;
    let exchanged = issue_and_record_user_session(
        &mut conn,
        issuing_key,
        tenant_id,
        principal_id,
        roles,
        request_id,
    )
    .await?;
    conn.commit().await.map_err(sql_error)?;

    Ok(exchanged.into_response())
}

/// Issue user access/refresh tokens and persist their tenant-scoped side effects.
///
/// The caller owns the tenant transaction. Keeping refresh-token insertion and
/// success audit insertion on that transaction ensures Wyrd does not return a
/// refresh token unless the corresponding server-side state and audit row commit
/// together.
async fn issue_and_record_user_session(
    conn: &mut TenantConn<'_>,
    issuing_key: &wyrd_auth_issue::IssuingKey,
    tenant_id: DataTenantId,
    principal_id: Uuid,
    roles: Vec<RoleRef>,
    request_id: &str,
) -> Result<ExchangedToken, WyrdErrorResponse> {
    let principal = Principal::new(
        PrincipalId::new(principal_id),
        PrincipalKind::User,
        tenant_id,
        roles.clone(),
        PermissionSet::default(),
    );
    let access_token = issuing_key
        .issue_user_access_token(
            TokenPrincipalRef::from(&principal),
            roles.clone(),
            ACCESS_TTL,
        )
        .map_err(issue_error)?;
    let refresh_token = issuing_key
        .issue_refresh_token(
            PrincipalKindWire::User,
            PrincipalId::new(principal_id),
            tenant_id,
            REFRESH_TTL,
        )
        .map_err(issue_error)?;
    let now = Utc::now();
    let expires_at = now + ACCESS_TTL;
    insert_refresh_token(
        conn,
        Uuid::new_v4(),
        "user",
        principal_id,
        &token_hash(&refresh_token),
        now + REFRESH_TTL,
    )
    .await
    .map_err(sql_error)?;
    insert_audit_token_exchange(
        conn,
        Uuid::new_v4(),
        principal_id,
        principal_id,
        serde_json::json!([]),
        request_id,
        expires_at,
    )
    .await
    .map_err(sql_error)?;

    Ok(ExchangedToken {
        access_token: SecretString::from(access_token),
        refresh_token: Some(SecretString::from(refresh_token)),
        token_type: TokenType::Bearer,
        expires_at,
    })
}

async fn resolve_callback_tenant(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<DataTenantId, WyrdErrorResponse> {
    let Some(slug) = tenant_slug_from_host(headers) else {
        return Err(invalid_token("request host does not encode a tenant"));
    };
    match wyrd_sql::queries::platform::tenant_resolver::resolve_by_slug_for_app(&state.pool, &slug)
        .await
        .map_err(sql_error)?
    {
        Some(tenant) => Ok(tenant),
        None => Err(invalid_token("request tenant could not be resolved")),
    }
}

/// Best-effort audit for authorization-code failures after tenant resolution.
///
/// Authentication failure must still return the original error if auditing
/// fails, so audit write failures are logged and not surfaced to the caller.
async fn audit_authorization_code_failure(
    state: &AppState,
    tenant_id: DataTenantId,
    principal_id: Uuid,
    request_id: &str,
    error: &WyrdErrorResponse,
) {
    let mut conn = match TenantConn::acquire(&state.pool, tenant_id).await {
        Ok(conn) => conn,
        Err(audit_error) => {
            tracing::warn!(
                error = %audit_error,
                original_error = %error.0,
                "OIDC authorization-code failure audit could not acquire tenant connection"
            );
            return;
        }
    };
    if let Err(audit_error) = insert_audit_token_exchange(
        &mut conn,
        Uuid::new_v4(),
        principal_id,
        principal_id,
        serde_json::json!([{ "error": audit_error_tag(&error.0) }]),
        request_id,
        Utc::now() + ACCESS_TTL,
    )
    .await
    {
        tracing::warn!(
            error = %audit_error,
            original_error = %error.0,
            "OIDC authorization-code failure audit insert failed"
        );
        return;
    }
    if let Err(audit_error) = conn.commit().await {
        tracing::warn!(
            error = %audit_error,
            original_error = %error.0,
            "OIDC authorization-code failure audit commit failed"
        );
    }
}

fn audit_error_tag(error: &WyrdError) -> &'static str {
    match error {
        WyrdError::InvalidState { .. } => "InvalidState",
        WyrdError::InvalidToken { .. } => "InvalidToken",
        WyrdError::DiscoveryUnavailable { .. } => "DiscoveryUnavailable",
        WyrdError::AuthVerifyUnavailable { .. } => "AuthVerifyUnavailable",
        WyrdError::InvalidNonce { .. } => "InvalidNonce",
        WyrdError::Internal { .. } => "Internal",
        WyrdError::TokenExpired { .. } => "TokenExpired",
        WyrdError::BadTokenFormat { .. } => "BadTokenFormat",
        WyrdError::CredentialRevoked { .. } => "CredentialRevoked",
        WyrdError::RoleCorrupt { .. } => "RoleCorrupt",
        _ => "WyrdError",
    }
}

fn tenant_slug_from_host(headers: &HeaderMap) -> Option<wyrd_spec::ids::TenantSlug> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    let host = host.split(':').next().unwrap_or(host);
    let mut segments = host.split('.').filter(|segment| !segment.is_empty());
    let first = segments.next()?;
    let second = segments.next()?;
    if first == "localhost" || second == "localhost" {
        return None;
    }
    wyrd_spec::ids::TenantSlug::new(first.to_owned()).ok()
}

async fn discover_provider(trusted: &TrustedIssuer) -> Result<OidcProvider, WyrdErrorResponse> {
    let issuer_url = url::Url::parse(trusted.issuer.as_str()).map_err(|_| {
        WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
            message: "trusted issuer URL could not be parsed".to_owned(),
            details: serde_json::json!({}),
        })
    })?;
    OidcProvider::discover(issuer_url, reqwest::Client::new())
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "OIDC discovery failed");
            WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
                message: "OIDC discovery unavailable".to_owned(),
                details: serde_json::json!({}),
            })
        })
}

async fn exchange_code_for_id_token(
    provider: &OidcProvider,
    trusted: &TrustedIssuer,
    state: &LoginStateEntry,
    code: SecretString,
) -> Result<String, WyrdErrorResponse> {
    let Some(token_endpoint) = provider.metadata.token_endpoint.clone() else {
        return Err(WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
            message: "OIDC discovery document did not advertise a token endpoint".to_owned(),
            details: serde_json::json!({}),
        }));
    };

    let client = reqwest::Client::new();
    let mut request = client.post(token_endpoint);
    let mut form = vec![
        ("grant_type", "authorization_code".to_owned()),
        ("code", code.expose_secret().to_owned()),
        ("client_id", trusted.client_id.clone()),
        ("redirect_uri", state.redirect_uri.clone()),
        (
            "code_verifier",
            state.code_verifier.expose_secret().to_owned(),
        ),
    ];

    match &trusted.client_auth {
        ClientAuth::SecretBasic(secret) => {
            request = request.basic_auth(
                trusted.client_id.clone(),
                Some(secret.expose_secret().to_owned()),
            );
        }
        ClientAuth::SecretPost(secret) => {
            form.push(("client_secret", secret.expose_secret().to_owned()));
        }
        ClientAuth::PrivateKeyJwt => {
            return Err(WyrdErrorResponse::from(WyrdError::Internal {
                message: "private_key_jwt client authentication is not implemented".to_owned(),
                details: serde_json::json!({ "client_auth": "private_key_jwt" }),
            }));
        }
        ClientAuth::Public => {}
    }

    let response = request.form(&form).send().await.map_err(|error| {
        tracing::warn!(error = %error, "OIDC token endpoint unavailable");
        WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
            message: "OIDC token endpoint unavailable".to_owned(),
            details: serde_json::json!({ "retry_after_seconds": 1 }),
        })
    })?;
    if !response.status().is_success() {
        return if response.status().is_server_error() {
            Err(WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
                message: "OIDC token endpoint unavailable".to_owned(),
                details: serde_json::json!({ "retry_after_seconds": 1 }),
            }))
        } else {
            Err(invalid_token("authorization code exchange was rejected"))
        };
    }

    #[derive(Debug, Deserialize)]
    struct TokenEndpointResponse {
        id_token: String,
    }

    response
        .json::<TokenEndpointResponse>()
        .await
        .map(|body| body.id_token)
        .map_err(|error| {
            tracing::warn!(error = %error, "OIDC token response decode failed");
            WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
                message: "OIDC token response decode failed".to_owned(),
                details: serde_json::json!({ "retry_after_seconds": 1 }),
            })
        })
}

/// Resolve the local user for a trusted external identity or create it once.
///
/// The `(issuer, subject)` pair is the identity key. Email is optional profile
/// data and is never used to decide identity ownership.
async fn ensure_user_identity(
    conn: &mut TenantConn<'_>,
    trusted: &TrustedIssuer,
    subject: &str,
    email: Option<&str>,
) -> Result<Uuid, SqlError> {
    let issuer = trusted.issuer.as_str();
    if let Some(user_id) = user_id_by_identity(conn, issuer, subject).await? {
        return Ok(user_id);
    }

    let user_id = Uuid::new_v4();
    insert_user(conn, user_id, email, "oidc", None).await?;
    let canonical = upsert_user_identity(conn, issuer, subject, user_id).await?;
    if canonical != user_id {
        let _ = delete_user(conn, user_id).await?;
    }
    Ok(canonical)
}

fn verify_nonce(state: &LoginStateEntry, claims: &Value) -> Result<(), WyrdErrorResponse> {
    let Some(nonce) = claims.get("nonce").and_then(Value::as_str) else {
        return Err(invalid_nonce("id token nonce is missing"));
    };
    if nonce != state.nonce {
        return Err(invalid_nonce("id token nonce mismatch"));
    }
    Ok(())
}

fn role_names_to_refs(
    trusted: &TrustedIssuer,
    groups: &[String],
) -> Result<Vec<RoleRef>, WyrdErrorResponse> {
    let mut names = trusted.default_roles.clone();
    for group in groups {
        if let Some(mapped) = trusted.group_role_map.get(group) {
            names.extend(mapped.iter().cloned());
        }
    }
    names.sort_unstable();
    names.dedup();
    role_refs(names).map_err(|_| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "trusted issuer role mapping is invalid".to_owned(),
            details: serde_json::json!({}),
        })
    })
}

fn invalid_state(message: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::InvalidState {
        message: message.to_owned(),
        details: serde_json::json!({}),
    })
}

fn invalid_nonce(message: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::InvalidNonce {
        message: message.to_owned(),
        details: serde_json::json!({}),
    })
}

fn invalid_token(message: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::InvalidToken {
        message: message.to_owned(),
        details: serde_json::json!({}),
    })
}

fn auth_not_configured() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Internal {
        message: "auth signing or verification handle is not configured".to_owned(),
        details: serde_json::json!({}),
    })
}

fn sql_error(error: impl Into<SqlError>) -> WyrdErrorResponse {
    let error = error.into();
    tracing::warn!(error = %error, "OIDC callback SQL unavailable");
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({ "retry_after_seconds": 1 }),
    })
}

fn issue_error(error: wyrd_auth_issue::IssueError) -> WyrdErrorResponse {
    tracing::warn!(error = %error, "OIDC token issue failed");
    WyrdErrorResponse::from(WyrdError::Internal {
        message: "token issue failed".to_owned(),
        details: serde_json::json!({}),
    })
}

#[cfg(test)]
mod tests {
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
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, JwksCache, PrincipalKindPolicy, TrustedIssuer,
    };
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_crypt::SecretKey;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{IssuerUrl, TokenResponse, TokenType};
    use wyrd_sql::queries::auth::upsert_trusted_issuer;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::auth::login::{LoginStateEntry, PgLoginStateStore};
    use crate::auth::permission_resolver::SqlPermissionResolver;
    use crate::auth::pg_resolvers::{PgIssuerResolver, issuer_write_from_trusted};
    use crate::error::WyrdErrorResponse;
    use crate::state::AppState;

    use super::{
        audit_authorization_code_failure, audit_error_tag, ensure_user_identity,
        exchange_authorization_code, finish_authorization_code_exchange, role_names_to_refs,
        tenant_slug_from_host, verify_nonce,
    };

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

        assert_eq!(error.0.code(), "WYRD_AUTH_400_INVALID_NONCE");
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
        let state = test_state(&fixture);
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
        assert_eq!(audit[0].1, Uuid::nil());
        assert_eq!(audit[0].2, serde_json::json!([{ "error": "InvalidState" }]));
    }

    #[tokio::test]
    async fn stored_invalid_issuer_writes_failure_audit_with_nil_principal() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture);
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
        assert_eq!(audit[0].2, serde_json::json!([{ "error": "InvalidToken" }]));
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
        let mut audit_principal_id = Uuid::nil();

        let response = finish_authorization_code_exchange(
            &state,
            tenant,
            &trusted,
            &login_state,
            &id_token,
            "req-success",
            &mut audit_principal_id,
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
        assert_eq!(audit[0].1, audit_principal_id);
        assert_eq!(audit[0].2, serde_json::json!([]));
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
        assert_eq!(audit[0].1, Uuid::nil());
        assert_eq!(audit[0].2, serde_json::json!([{ "error": "InvalidToken" }]));
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

    #[test]
    fn audit_error_tags_use_variant_names_for_callback_failures() {
        assert_eq!(
            audit_error_tag(&wyrd_spec::error::WyrdError::InvalidNonce {
                message: "nonce".to_owned(),
                details: serde_json::json!({}),
            }),
            "InvalidNonce"
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
            principal_kind: PrincipalKindPolicy::Human,
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
            principal_kind: PrincipalKindPolicy::Human,
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
        let result = finish_authorization_code_exchange(
            state,
            tenant_id,
            trusted,
            login_state,
            id_token,
            request_id,
            &mut audit_principal_id,
        )
        .await;
        if let Err(error) = &result {
            audit_authorization_code_failure(
                state,
                tenant_id,
                audit_principal_id,
                request_id,
                error,
            )
            .await;
        }
        result
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

    fn test_state(fixture: &PgFixture) -> AppState {
        let dir = tempfile::tempdir().expect("callback storage tempdir");
        let storage_root = dir.keep().join("callback-storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        AppState::new(
            fixture.app_pool().clone(),
            None,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
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
                reqwest::Client::new(),
                StdDuration::from_secs(300),
                StdDuration::from_secs(5),
            )),
            Arc::clone(&issuer_resolver),
        );
        test_state(fixture)
            .with_auth_handles(issuing_key, Arc::new(verifier))
            .with_trusted_issuer_resolver(issuer_resolver)
            .with_sealing_key(sealing_key)
    }

    fn tenant_headers(slug: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_str(&format!("{slug}.example.com")).expect("host is valid"),
        );
        headers
    }

    async fn audit_rows(fixture: &PgFixture) -> Vec<(Uuid, Uuid, serde_json::Value)> {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        sqlx::query_as(
            "SELECT subject_principal_id, actor_principal_id, act_chain
               FROM wyrd.audit_token_exchange
              ORDER BY issued_at ASC",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("audit query runs")
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
