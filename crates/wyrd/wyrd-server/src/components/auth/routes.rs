//! HTTP routes for auth preview surfaces.

use axum::extract::{Extension, Query, State};
use axum::http::HeaderMap;
use axum::{Json, Router};
use std::sync::Arc;

use base64::Engine;
use secrecy::SecretString;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use uuid::Uuid;
use wyrd_auth_verify::AccessTokenClaims;
use wyrd_spec::auth::{CallbackQuery, IssueKeyRequest, TokenRequest, TokenResponse};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::auth::callback::exchange_authorization_code;
use crate::auth::card_scope::{
    MINT_KIND_API_KEY_EXCHANGE, MINT_KIND_DELEGATION, MINT_KIND_REFRESH,
    audit_scope_mint_failure_best_effort,
};
use crate::auth::exchange_api_key::{DelegateToken, ExchangeApiKey, map_exchange_error_to_wyrd};
use crate::auth::issue_api_key::{IssueApiKey, WyrdApiKey};
use crate::auth::jwt_bearer::JwtBearer;
use crate::auth::login::login as login_handler;
use crate::auth::refresh::{RefreshTokens, tenant_from_refresh_jwt};
use crate::components::auth::AuthenticatedPrincipal;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Build auth routes.
pub fn auth_router() -> Router<AppState> {
    let auth_governor = Arc::new(
        GovernorConfigBuilder::default()
            .per_millisecond(100)
            .burst_size(20)
            .finish()
            .expect("static auth governor config is valid"),
    );

    Router::new()
        .route("/auth/login", axum::routing::get(login_handler))
        .route("/auth/callback", axum::routing::get(callback))
        .route("/auth/token", axum::routing::post(token))
        .route("/auth/issue-key", axum::routing::post(issue_key))
        .layer(GovernorLayer::new(auth_governor))
}

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
            let parsed = WyrdApiKey::parse(api_key.expose()).map_err(|_| {
                WyrdErrorResponse::from(WyrdError::ApiKeyInvalid {
                    message: "API key format is invalid".to_owned(),
                    details: serde_json::json!({ "reason": "format" }),
                })
            })?;
            let issuing_key = state
                .auth
                .issuing_key
                .clone()
                .ok_or_else(auth_not_configured)?;
            let mut conn = state
                .postgres
                .tenant_conn(parsed.tenant_id)
                .await
                .map_err(sql_error)?;
            let prefix = parsed.prefix.clone();
            let exchanged = ExchangeApiKey {
                issuing_key,
                settings: state.auth.token_exchange_settings.clone(),
            }
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
            requested_subject,
        } => {
            if !state.auth.allow_preview {
                return Err(WyrdErrorResponse::from(preview_disabled()));
            }
            let issuing_key = state
                .auth
                .issuing_key
                .clone()
                .ok_or_else(auth_not_configured)?;
            let verifier = state
                .auth
                .token_verifier
                .clone()
                .ok_or_else(auth_not_configured)?;
            let tenant_id = tenant_from_unverified_access_token(subject_token.expose())?;
            let mut conn = state
                .postgres
                .tenant_conn(tenant_id)
                .await
                .map_err(sql_error)?;
            let exchanged = DelegateToken {
                issuing_key,
                verifier,
                permission_check: state.authz.permission_check.clone(),
                settings: state.auth.token_exchange_settings.clone(),
            }
            .execute(
                &mut conn,
                SecretString::from(subject_token.expose().to_owned()),
                requested_subject,
                req_id,
            )
            .await;
            let exchanged = match exchanged {
                Ok(exchanged) => exchanged,
                Err(error) => {
                    let wyrd = WyrdError::from(error);
                    audit_scope_mint_failure_best_effort(
                        state.postgres.app_pool(),
                        tenant_id,
                        req_id,
                        MINT_KIND_DELEGATION,
                        &wyrd,
                    )
                    .await;
                    return Err(WyrdErrorResponse::from(wyrd));
                }
            };
            conn.commit().await.map_err(sql_error)?;
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
            let issuing_key = state
                .auth
                .issuing_key
                .clone()
                .ok_or_else(auth_not_configured)?;
            let exchanged = RefreshTokens {
                issuing_key,
                settings: state.auth.token_exchange_settings.clone(),
            }
            .execute(&mut conn, SecretString::from(secret), req_id)
            .await;
            let exchanged = match exchanged {
                Ok(exchanged) => exchanged,
                Err(error) => {
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
            let exchanged = JwtBearer {
                settings: state.auth.token_exchange_settings.clone(),
            }
            .execute(
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

async fn issue_key(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    request_id: Option<Extension<RequestId>>,
    Json(request): Json<IssueKeyRequest>,
) -> Result<Json<wyrd_spec::auth::IssueKeyResponse>, WyrdErrorResponse> {
    wyrd_auth::service_accounts::require_service_accounts_write(
        &caller.principal,
        "issue API keys",
    )
    .map_err(WyrdErrorResponse::from)?;

    let request_id_str: String;
    let req_id = match request_id.as_ref() {
        Some(Extension(id)) => id.as_str(),
        None => {
            request_id_str = Uuid::new_v4().to_string();
            &request_id_str
        }
    };

    let tenant = caller.principal.tenant_id;
    let mut conn = state
        .postgres
        .tenant_conn(tenant)
        .await
        .map_err(sql_error)?;
    let service = IssueApiKey::default();
    let issued = service
        .execute(&mut conn, request, &caller.principal)
        .await
        .map_err(|error| WyrdErrorResponse::from(WyrdError::from(error)))?;
    service
        .audit(&mut conn, &issued, &caller.principal, req_id)
        .await
        .map_err(|error| sql_error(wyrd_sql::SqlError::from(error)))?;
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

fn preview_disabled() -> WyrdError {
    WyrdError::AuthPreviewDisabled {
        message: "preview auth routes are disabled".to_owned(),
        details: serde_json::json!({}),
    }
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
            act: None,
            exp: 9_999_999_999,
            iat: 0,
            iss: "test".to_owned(),
            jti: "01K00000000000000000000000".to_owned(),
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
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    fn caller_with(
        id: Uuid,
        tenant: DataTenantId,
        permissions: PermissionSet,
    ) -> AuthenticatedPrincipal {
        AuthenticatedPrincipal {
            principal: Principal {
                id: PrincipalId::new(id),
                kind: PrincipalKind::User,
                tenant_id: tenant,
                roles: Vec::new(),
                effective_permissions: permissions,
            },
        }
    }

    async fn lazy_state() -> AppState {
        use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None);
        let vala = vala_sql::ValaPostgres::from_pool(app_pool);
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(wyrd, vala));
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        crate::test_support::test_app_state(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
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
        .bind(card_ref.space.as_str())
        .bind(format!("svc-{sa_id}"))
        .bind(card_ref.version.as_str())
        .bind(created_by)
        .execute(&mut **conn.transaction())
        .await
        .expect("service account inserts");
        sa_id
    }

    #[tokio::test]
    async fn issue_key_without_permission_is_rbac_denied() {
        let tenant = DataTenantId::new_v7();
        let state = lazy_state().await;
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
        assert!(!response.0.key_id.is_empty());

        let mut verify_conn = fixture.tenant_conn().await.expect("verify conn opens");
        let row = sqlx::query(
            "SELECT issuer_principal_id, target_sa_id, request_id, data_tenant_id
             FROM wyrd.audit_credential_issuance
             WHERE data_tenant_id = $1",
        )
        .bind(tenant.as_uuid())
        .fetch_one(&mut **verify_conn.transaction())
        .await
        .expect("audit row was written");

        let audit_actor: Uuid = row.get("issuer_principal_id");
        let audit_target: Uuid = row.get("target_sa_id");
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
}
