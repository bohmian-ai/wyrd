//! HTTP routes for the tenant auth surfaces: token exchange, OIDC callback,
//! and Card-bound API key issuance.

use axum::Json;
use axum::extract::{Extension, Query, State};
use axum::http::HeaderMap;
use std::sync::Arc;

use base64::Engine;
use secrecy::SecretString;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use uuid::Uuid;
use wyrd_auth_verify::AccessTokenClaims;
use wyrd_spec::auth::{CallbackQuery, IssueKeyRequest, TokenRequest, TokenResponse};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::request_id::RequestId;

use crate::auth::callback::exchange_authorization_code;
use crate::auth::card_scope::{
    MINT_KIND_API_KEY_EXCHANGE, MINT_KIND_REFRESH, audit_scope_mint_failure_best_effort,
};
use crate::auth::credential_verify::verify_presented;
use crate::auth::exchange_api_key::{
    DelegateToken, ExchangeApiKey, api_key_invalid, map_exchange_error_to_wyrd,
};
use crate::auth::issue_api_key::{IssueApiKey, WyrdApiKey};
use crate::auth::jwt_bearer::exchange_jwt_bearer;
use crate::auth::refresh::{RefreshError, RefreshTokens, tenant_from_refresh_jwt};
use crate::components::auth::{AuthenticatedPrincipal, Caller};
use crate::http::error::WyrdErrorResponse;
use crate::http::error::internal_failure;
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build auth routes.
pub fn auth_router() -> OpenApiRouter<AppState> {
    let auth_governor = Arc::new(
        GovernorConfigBuilder::default()
            .per_millisecond(100)
            .burst_size(20)
            .finish()
            .expect("static auth governor config is valid"),
    );

    OpenApiRouter::new()
        .routes(routes!(crate::auth::login::login))
        .routes(routes!(callback))
        .routes(routes!(token))
        .routes(routes!(issue_key))
        .layer(GovernorLayer::new(auth_governor))
}

/// `POST /auth/token` — exchange a credential for a short-lived access token.
///
/// The one tenant-plane entry point: a Wyrd API key, an external JWT bearer
/// bound to a workload, a refresh token, or an RFC 8693 delegation all arrive
/// here and leave with the same [`TokenResponse`]. Tenant and principal are
/// derived from the verified credential, never from a client-supplied header.
///
/// Every invalid-credential condition returns one indistinguishable `401` so
/// the response cannot be used to probe which part was wrong.
///
/// # Errors
/// Returns a `400` when a token exchange's identity input is malformed or its
/// delegation would exceed the configured chain depth, a `401` for every
/// unusable credential — including a reused, revoked, or expired refresh token
/// and an invalid subject or actor token — a `403` when the invoke policy does
/// not let the actor act for the subject, a `404` when the actor's principal or
/// the presented workload assertion matches no principal, and a `503` when the
/// auth backend or the audit path is unavailable. The grant and its exchange
/// audit commit together, so a refusal serves no token.
#[utoipa::path(
    post,
    path = "/auth/token",
    request_body = TokenRequest,
    responses(
        (status = 200, description = "Access token issued", body = TokenResponse),
        (status = 400, description = "The token exchange's identity input is malformed — a \
          delegated or Card-free actor token, or a party exchanging with itself \
          (WYRD_SPEC_400_VALIDATION) — or the delegation would exceed the configured chain \
          depth (WYRD_AUTH_400_DELEGATION_DEPTH_EXCEEDED)", body = WyrdProblem),
        (status = 401, description = "The presented credential is not usable. Every \
          invalid-credential condition renders one indistinguishable refusal \
          (WYRD_AUTH_401_API_KEY_INVALID); a refresh token that was already consumed reports \
          the reuse it contained (WYRD_AUTH_401_REFRESH_REUSED), one that is revoked, expired, \
          or malformed reports that (WYRD_AUTH_401_REFRESH_REVOKED), and a subject token whose \
          delegation chain is already at the limit reports that \
          (WYRD_AUTH_401_DELEGATION_DEPTH_EXCEEDED)", body = WyrdProblem),
        (status = 403, description = "Both exchange tokens are valid but the invoke policy \
          does not let the actor act for the subject (WYRD_AUTHZ_403_POLICY_DENIED)",
          body = WyrdProblem),
        (status = 404, description = "No principal in this tenant matches the token \
          exchange's actor or the presented workload assertion \
          (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "Token issuance or the server's own auth configuration \
          failed, so no token was served (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "The auth backend is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE), or the exchange audit could not be staged, which \
          fails the grant closed (WYRD_AUDIT_503_UNAVAILABLE)", body = WyrdProblem)
    ),
    // No session exists yet at this operation, so it clears the document-wide
    // requirement instead of inheriting it.
    security(()),
    tag = "Auth"
)]
async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    request_id: Option<Extension<RequestId>>,
    Json(request): Json<TokenRequest>,
) -> Result<Json<TokenResponse>, WyrdErrorResponse> {
    let request_id_str: String;
    let req_id = match request_id.as_ref() {
        Some(Extension(id)) => id.as_str(),
        None => {
            request_id_str = Uuid::new_v4().to_string();
            &request_id_str
        }
    };
    match request {
        TokenRequest::WyrdApiKey { api_key } => {
            // A key that does not parse names no tenant, so there is no
            // connection to reach and no row to verify against — and returning
            // here for free is exactly what makes a malformed key
            // distinguishable by clock from a live prefix with a wrong tail.
            // One verification against the fixed dummy costs what the real
            // comparison costs, and the refusal is the same one every invalid
            // key earns.
            let parsed = match WyrdApiKey::parse(api_key.expose()) {
                Ok(parsed) => parsed,
                Err(_) => {
                    verify_presented(&SecretString::from(api_key.expose().to_owned()), None)
                        .await
                        .map_err(|error| {
                            WyrdErrorResponse::from(internal_failure(
                                "api key verification failed",
                                &error,
                            ))
                        })?;
                    return Err(WyrdErrorResponse::from(api_key_invalid()));
                }
            };
            let issuer = state.auth.tenant_issuer().ok_or_else(auth_not_configured)?;
            let mut conn = state
                .postgres
                .tenant_conn(parsed.tenant_id)
                .await
                .map_err(sql_error)?;
            let prefix = parsed.prefix.clone();
            let exchanged = ExchangeApiKey { issuer }
                .execute(
                    &mut conn,
                    SecretString::from(api_key.expose().to_owned()),
                    req_id,
                )
                .await;
            let exchanged = match exchanged {
                Ok(exchanged) => exchanged,
                Err(error) => {
                    let wyrd = map_exchange_error_to_wyrd(&mut conn, &prefix, error).await;
                    audit_scope_mint_failure_best_effort(
                        state.postgres.app_pool(),
                        parsed.tenant_id,
                        req_id,
                        MINT_KIND_API_KEY_EXCHANGE,
                        &wyrd,
                    )
                    .await;
                    return Err(WyrdErrorResponse::from(wyrd));
                }
            };
            conn.commit().await.map_err(sql_error)?;
            Ok(Json(exchanged.into_response()))
        }
        TokenRequest::TokenExchange {
            subject_token,
            subject_token_type: _,
            actor_token,
            actor_token_type: _,
            audience,
        } => {
            let issuer = state.auth.tenant_issuer().ok_or_else(auth_not_configured)?;
            let verifier = state
                .auth
                .token_verifier
                .clone()
                .ok_or_else(auth_not_configured)?;
            // Both tokens are verified against this tenant, so an actor token
            // from any other tenant is refused.
            let tenant_id = tenant_from_unverified_access_token(subject_token.expose())?;
            let conn = state
                .postgres
                .tenant_conn(tenant_id)
                .await
                .map_err(sql_error)?;
            // The exchange commits its own authorization decision, so a
            // refusal after the policy decision is still durably audited.
            let exchanged = DelegateToken {
                issuer,
                verifier,
                policy: state.authz.policy_hook.clone(),
            }
            .execute(
                conn,
                SecretString::from(subject_token.expose().to_owned()),
                SecretString::from(actor_token.expose().to_owned()),
                audience,
                req_id,
            )
            .await
            .map_err(|error| WyrdErrorResponse::from(WyrdError::from(error)))?;
            Ok(Json(exchanged.into_response()))
        }
        TokenRequest::RefreshToken { refresh_token } => {
            let secret = refresh_token.expose().to_owned();
            let tenant_id = tenant_from_refresh_jwt(&secret)?;
            let mut conn = state
                .postgres
                .tenant_conn(tenant_id)
                .await
                .map_err(sql_error)?;
            let issuer = state.auth.tenant_issuer().ok_or_else(auth_not_configured)?;
            let exchanged = RefreshTokens { issuer }
                .execute(&mut conn, SecretString::from(secret), req_id)
                .await;
            let exchanged = match exchanged {
                Ok(exchanged) => exchanged,
                Err(error) => {
                    // Replay is the one refusal that also writes. Detection
                    // revoked the whole family and appended the canonical
                    // revocation event on this transaction; rolling that back
                    // with every other error would tell the legitimate holder
                    // the theft was contained while leaving the attacker's
                    // successor usable. Committing first makes the containment
                    // durable, and the 401 is rendered from the typed outcome
                    // exactly as before.
                    if matches!(error, RefreshError::Reused) {
                        conn.commit().await.map_err(sql_error)?;
                    }
                    let wyrd = WyrdError::from(error);
                    audit_scope_mint_failure_best_effort(
                        state.postgres.app_pool(),
                        tenant_id,
                        req_id,
                        MINT_KIND_REFRESH,
                        &wyrd,
                    )
                    .await;
                    return Err(WyrdErrorResponse::from(wyrd));
                }
            };
            conn.commit().await.map_err(sql_error)?;
            Ok(Json(exchanged.into_response()))
        }
        TokenRequest::AuthorizationCode {
            code,
            state: login_state,
        } => {
            let request_id = req_id.to_owned();
            let exchanged = exchange_authorization_code(
                &state,
                &headers,
                code.into_secret_string(),
                &login_state,
                &request_id,
            )
            .await?;
            Ok(Json(exchanged))
        }
        TokenRequest::JwtBearer { assertion, tenant } => {
            let exchanged = exchange_jwt_bearer(
                &state,
                &headers,
                assertion.into_secret_string(),
                tenant,
                req_id,
            )
            .await?;
            Ok(Json(exchanged.into_response()))
        }
    }
}

/// `GET /auth/callback` — complete an OIDC login and return the session.
///
/// Anonymous by construction: the caller is mid-login and has no Wyrd session
/// yet. The opaque `state` generated at initiation is what binds the callback
/// to that login, so a code presented without it is refused.
#[utoipa::path(
    get,
    path = "/auth/callback",
    params(
        ("code" = String, Query, description = "Authorization code from the identity provider"),
        ("state" = String, Query, description = "Opaque login state Wyrd generated at initiation")
    ),
    responses(
        (status = 200, description = "Login completed and a session issued", body = TokenResponse),
        (status = 401, description = "The code or login state is not usable \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 503, description = "The identity provider or auth backend is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    // No session exists yet at this operation, so it clears the document-wide
    // requirement instead of inheriting it.
    security(()),
    tag = "Auth"
)]
async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    request_id: Option<Extension<RequestId>>,
    Query(query): Query<CallbackQuery>,
) -> Result<Json<TokenResponse>, WyrdErrorResponse> {
    let request_id_str;
    let req_id = match request_id.as_ref() {
        Some(Extension(id)) => id.as_str(),
        None => {
            request_id_str = Uuid::new_v4().to_string();
            &request_id_str
        }
    };
    let exchanged = exchange_authorization_code(
        &state,
        &headers,
        query.code.into_secret_string(),
        &query.state,
        req_id,
    )
    .await?;
    Ok(Json(exchanged))
}

/// `POST /auth/issue-key` — mint a Card-bound API key for an existing principal.
///
/// Gated on `service_accounts:write`. The principal must already exist — a
/// credential is issued against an identity, never in place of one — and the
/// plaintext is returned exactly once.
///
/// # Errors
/// Returns a `401` without a usable token, a `403` without
/// `service_accounts:write`, a `404` when the named principal does not exist in
/// this tenant, a `500` when the write or its audit fails, and a `503` when the
/// store is unavailable or no token verifier is configured.
#[utoipa::path(
    post,
    path = "/auth/issue-key",
    request_body = IssueKeyRequest,
    responses(
        (status = 200, description = "Key issued, plaintext returned once",
         body = wyrd_spec::auth::IssueKeyResponse),
        (status = 401, description = "An access token is required \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 403, description = "Caller lacks service_accounts:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No principal is bound to the named Card \
          (WYRD_AUTH_404_ADMIN_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable, or the credential's own audit \
          row could not be staged (WYRD_AUDIT_503_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Auth"
)]
async fn issue_key(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    request_id: Option<Extension<RequestId>>,
    Json(request): Json<IssueKeyRequest>,
) -> Result<Json<wyrd_spec::auth::IssueKeyResponse>, WyrdErrorResponse> {
    let request_id_str: String;
    let req_id = match request_id.as_ref() {
        Some(Extension(id)) => id.as_str(),
        None => {
            request_id_str = Uuid::new_v4().to_string();
            &request_id_str
        }
    };
    let audited_caller = Caller::from_authenticated(
        &caller,
        RequestId::parse(req_id).unwrap_or_else(|_| RequestId::now_v7()),
    );
    let decision = crate::audit::authorize_service_accounts_write(
        &state,
        &audited_caller,
        "issue API keys",
        "auth.api_key.issue",
        &format!("card:{}", request.card_ref.name),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;

    // The allowance rides the same transaction as the key it permits. A
    // refusal is durable on its own — a denied attempt is evidence whether or
    // not anything followed it — but an allowance is not: committing it first
    // would leave a record permitting a key that a later failure never issued.
    let tenant = caller.principal().tenant_id;
    let mut conn = state
        .postgres
        .tenant_conn(tenant)
        .await
        .map_err(sql_error)?;
    crate::audit::append_on(&mut conn, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let service = IssueApiKey::default();
    let issued = service
        .execute(&mut conn, request, caller.principal())
        .await
        .map_err(|error| WyrdErrorResponse::from(WyrdError::from(error)))?;
    service
        .audit(&mut conn, &issued, caller.principal(), req_id)
        .await
        .map_err(WyrdErrorResponse::from)?;
    conn.commit().await.map_err(sql_error)?;

    Ok(Json(issued.response))
}

fn auth_not_configured() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Internal {
        message: "auth signing or verification handle is not configured".to_owned(),
        details: serde_json::json!({}),
    })
}

fn sql_error(error: wyrd_sql::SqlError) -> WyrdErrorResponse {
    tracing::warn!(error = %error, "auth db unavailable");
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({}),
    })
}

fn tenant_from_unverified_access_token(
    token: &str,
) -> Result<wyrd_spec::DataTenantId, WyrdErrorResponse> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(bad_subject_token_format)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| bad_subject_token_format())?;
    let claims: AccessTokenClaims =
        serde_json::from_slice(&bytes).map_err(|_| bad_subject_token_format())?;
    Ok(claims.principal.tenant_id)
}

fn bad_subject_token_format() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::BadTokenFormat {
        message: "subject_token is not a compact JWT".to_owned(),
        details: serde_json::json!({}),
    })
}

#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;

    use axum::Json;
    use axum::extract::State;
    use base64::Engine;
    use sqlx::types::Json as SqlxJson;
    use uuid::Uuid;
    use wyrd_auth_verify::{AccessTokenClaims, TokenPrincipalRef};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalId, PrincipalKind};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssueKeyRequest;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::components::auth::AuthenticatedPrincipal;
    use crate::state::AppState;
    use axum::Extension;
    use sqlx::Row;
    use wyrd_spec::request_id::RequestId;
    use wyrd_sql::TenantConn;

    use super::{auth_router, issue_key, tenant_from_unverified_access_token};

    #[test]
    fn mounts_token_and_issue_key_routes() {
        let _router = auth_router();
    }

    #[test]
    fn extracts_tenant_from_unverified_access_token() {
        let tenant_id: DataTenantId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id is valid");
        let principal_id: PrincipalId = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"
            .parse()
            .expect("static principal id is valid");
        let claims = AccessTokenClaims {
            sub: principal_id.to_string(),
            principal: TokenPrincipalRef {
                id: principal_id,
                kind: PrincipalKindTag::User,
                tenant_id,
                card_ref: None,
                card_ref_scope: Default::default(),
            },
            roles: vec![],
            permissions: Default::default(),
            act: None,
            aud: "wyrd".to_owned(),
            exp: 9_999_999_999,
            iat: 0,
            iss: "test".to_owned(),
            jti: "01K00000000000000000000000".to_owned(),
            cid: None,
        };
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            serde_json::to_string(&claims)
                .expect("claims serialize")
                .as_bytes(),
        );
        let fake_jwt = format!("header.{payload}.sig");

        let result = tenant_from_unverified_access_token(&fake_jwt).expect("extracts tenant");
        assert_eq!(result, tenant_id);
    }

    #[test]
    fn rejects_bad_base64_payload() {
        let result = tenant_from_unverified_access_token("not.a.jwt");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_wrong_segment_count() {
        let result = tenant_from_unverified_access_token("onlyone");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_non_json_payload() {
        let garbage = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"not json at all");
        let fake_jwt = format!("header.{garbage}.sig");

        let result = tenant_from_unverified_access_token(&fake_jwt);
        assert!(result.is_err());
    }

    fn service_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("issue-key-target").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    fn caller_with(
        id: Uuid,
        tenant: DataTenantId,
        permissions: PermissionSet,
    ) -> AuthenticatedPrincipal {
        AuthenticatedPrincipal::from_verified(Arc::new(wyrd_auth_verify::VerifiedToken {
            principal: Principal {
                id: PrincipalId::new(id),
                kind: PrincipalKind::User,
                tenant_id: tenant,
                roles: Vec::new(),
                effective_permissions: permissions,
                credential_id: None,
            },
            delegation_chain: Vec::new(),
            exp: chrono::Utc::now() + chrono::Duration::minutes(5),
        }))
    }

    async fn fixture_state(fixture: &PgFixture) -> AppState {
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let dir = tempfile::tempdir().expect("routes storage tempdir");
        let storage_root = dir.keep().join("issue-key-storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        crate::test_support::test_app_state(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
    }

    async fn insert_test_user(conn: &mut TenantConn<'_>, tenant: DataTenantId) -> Uuid {
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_users (id, data_tenant_id, email, auth_type, status)
             VALUES ($1, $2, $3, 'password', 'active')",
        )
        .bind(user_id)
        .bind(tenant.as_uuid())
        .bind(format!("test-{user_id}@example.com"))
        .execute(&mut **conn.transaction())
        .await
        .expect("test user inserts");
        user_id
    }

    async fn insert_test_service_account(
        conn: &mut TenantConn<'_>,
        tenant: DataTenantId,
        created_by: Uuid,
        card_ref: &CardRef,
    ) -> Uuid {
        let sa_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO wyrd.auth_service_accounts
                 (id, data_tenant_id, principal_kind, card_kind, card_uid, card_ref, space, name, version, status, created_by)
             VALUES ($1, $2, 'service', 'Service', $3, $4, $5, $6, $7, 'active', $8)",
        )
        .bind(sa_id)
        .bind(tenant.as_uuid())
        .bind(Uuid::new_v4())
        .bind(SqlxJson(card_ref.clone()))
        .bind(
            card_ref
                .space
                .as_ref()
                .expect("fixture service card ref has a resolved space")
                .as_str(),
        )
        .bind(format!("svc-{sa_id}"))
        .bind(card_ref.version.as_str())
        .bind(created_by)
        .execute(&mut **conn.transaction())
        .await
        .expect("service account inserts");
        sa_id
    }

    #[tokio::test]
    /// A caller without `service_accounts:write` is refused with the RBAC
    /// denial. The denial row commits to the real fixture database, so the
    /// refusal is not masked by an unreachable audit store.
    async fn issue_key_without_permission_is_rbac_denied() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = fixture_state(&fixture).await;
        let caller = caller_with(Uuid::new_v4(), tenant, PermissionSet::new());
        let request = IssueKeyRequest {
            card_ref: service_card_ref(),
            label: None,
            expires_in_seconds: None,
        };

        let error = issue_key(State(state), caller, None, Json(request))
            .await
            .expect_err("missing permission is denied");

        assert!(
            matches!(error.0, WyrdError::PermissionDeniedRbac { .. }),
            "expected an RBAC denial, got {:?}",
            error.0
        );
    }

    #[tokio::test]
    async fn issue_key_with_permission_mints_response() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let card_ref = service_card_ref();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = insert_test_user(&mut conn, tenant).await;
        let sa_id = insert_test_service_account(&mut conn, tenant, creator, &card_ref).await;
        conn.commit().await.expect("seed commits");

        let state = fixture_state(&fixture).await;
        let caller = caller_with(
            creator,
            tenant,
            PermissionSet::from_iter([Permission::service_accounts_write()]),
        );
        let request = IssueKeyRequest {
            card_ref: card_ref.clone(),
            label: None,
            expires_in_seconds: None,
        };

        let request_id = RequestId::parse("01890f28-7c4a-7cc3-98e7-4f4a3c2d1bff")
            .expect("static request id is valid");
        let expected_request_id = request_id.as_str().to_owned();

        let response = issue_key(
            State(state),
            caller,
            Some(Extension(request_id)),
            Json(request),
        )
        .await
        .expect("issue key succeeds");

        assert_eq!(response.0.card_ref, card_ref);
        assert!(!response.0.prefix.is_empty());

        let mut verify_conn = fixture.tenant_conn().await.expect("verify conn opens");
        let stored_key_id: Uuid =
            sqlx::query_scalar("SELECT id FROM wyrd.auth_api_keys WHERE prefix = $1")
                .bind(&response.0.prefix)
                .fetch_one(&mut **verify_conn.transaction())
                .await
                .expect("issued key row reads");
        assert_eq!(
            response.0.key_id, stored_key_id,
            "the response names the issued credential's UUID"
        );
        let row = sqlx::query(
            "SELECT principal_id, detail, request_id, data_tenant_id
             FROM vala.audit_staging
             WHERE data_tenant_id = $1
               AND operation = 'auth.api_key.issue'
               AND detail LIKE '%credential_issuance%'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(&mut **verify_conn.transaction())
        .await
        .expect("credential issuance audit event was staged");

        let audit_actor: Uuid = row.get("principal_id");
        let detail: serde_json::Value =
            serde_json::from_str(row.get("detail")).expect("audit detail is json");
        let audit_target: Uuid = detail["target_principal_id"]
            .as_str()
            .expect("target principal id is present")
            .parse()
            .expect("target principal id is a uuid");
        let audit_request_id: String = row.get("request_id");
        let audit_tenant: Uuid = row.get("data_tenant_id");
        assert_eq!(audit_actor, creator, "audit records the issuing actor");
        assert_eq!(
            audit_target, sa_id,
            "audit records the target service account"
        );
        assert_eq!(audit_tenant, tenant.as_uuid(), "audit is tenant-scoped");
        assert_eq!(
            audit_request_id, expected_request_id,
            "audit records the request id"
        );
    }

    /// A failure after authorization commits neither the key nor the allowance.
    ///
    /// The caller holds `service_accounts:write`, so the decision is an
    /// allowance — but the named Card binds no principal, so issuance fails
    /// after it. The allowance rides the issuing transaction, so it rolls back
    /// with the effect it was permitting: a record saying a key was allowed,
    /// with no key anywhere, is the mismatch the canonical audit rule forbids.
    #[tokio::test]
    async fn a_failed_issue_commits_neither_the_key_nor_its_allowance() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let creator = insert_test_user(&mut conn, tenant).await;
        conn.commit().await.expect("seed commits");

        let state = fixture_state(&fixture).await;
        let caller = caller_with(
            creator,
            tenant,
            PermissionSet::from_iter([Permission::service_accounts_write()]),
        );
        let request = IssueKeyRequest {
            card_ref: service_card_ref(),
            label: None,
            expires_in_seconds: None,
        };

        let error = issue_key(State(state), caller, None, Json(request))
            .await
            .expect_err("an unbound card cannot be issued a key");
        assert!(
            matches!(error.0, WyrdError::PrincipalNotFound { .. }),
            "expected the unbound-card refusal, got {:?}",
            error.0
        );

        let mut verify_conn = fixture.tenant_conn().await.expect("verify conn opens");
        let staged: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_staging
              WHERE data_tenant_id = $1
                AND operation = 'auth.api_key.issue'
                AND outcome = 'allowed'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(&mut **verify_conn.transaction())
        .await
        .expect("allowance count reads");
        assert_eq!(staged, 0, "a failed issue leaves no committed allowance");

        let keys: i64 =
            sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_api_keys WHERE data_tenant_id = $1")
                .bind(tenant.as_uuid())
                .fetch_one(&mut **verify_conn.transaction())
                .await
                .expect("key count reads");
        assert_eq!(keys, 0, "a failed issue leaves no key");
    }
}
