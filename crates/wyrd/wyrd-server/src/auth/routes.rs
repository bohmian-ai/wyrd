//! HTTP routes for auth preview surfaces.

use axum::extract::{Extension, State};
use axum::{Json, Router};
use base64::Engine;
use secrecy::SecretString;
use uuid::Uuid;
use wyrd_auth_verify::AccessTokenClaims;
use wyrd_spec::auth::{IssueKeyRequest, TokenRequest, TokenResponse};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::auth::Caller;
use crate::auth::exchange_api_key::{DelegateToken, ExchangeApiKey};
use crate::auth::issue_api_key::{IssueApiKey, WyrdApiKey};
use crate::error::WyrdErrorResponse;
use crate::state::AppState;

/// Build auth routes.
#[must_use]
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/token", axum::routing::post(token))
        .route("/auth/issue-key", axum::routing::post(issue_key))
}

async fn token(
    State(state): State<AppState>,
    request_id: Option<Extension<RequestId>>,
    Json(request): Json<TokenRequest>,
) -> Result<Json<TokenResponse>, WyrdErrorResponse> {
    match request {
        TokenRequest::WyrdApiKey { api_key } => {
            let parsed = WyrdApiKey::parse(api_key.expose()).map_err(|_| {
                WyrdErrorResponse::from(WyrdError::ApiKeyInvalid {
                    message: "API key not found, revoked, expired, or hash mismatch".to_owned(),
                    details: serde_json::json!({ "reason": "format" }),
                })
            })?;
            let issuing_key = state.issuing_key.clone().ok_or_else(auth_not_configured)?;
            let mut conn = wyrd_sql::TenantConn::acquire(&state.pool, parsed.tenant_id)
                .await
                .map_err(sql_error)?;
            let exchanged = ExchangeApiKey {
                issuing_key,
                settings: Default::default(),
            }
            .execute(&mut conn, SecretString::from(api_key.expose().to_owned()))
            .await
            .map_err(|error| WyrdErrorResponse::from(WyrdError::from(error)))?;
            conn.commit().await.map_err(sql_error)?;
            Ok(Json(exchanged.into_response()))
        }
        TokenRequest::TokenExchange {
            subject_token,
            subject_token_type: _,
            requested_subject,
        } => {
            if !state.allow_preview_auth {
                return Err(WyrdErrorResponse::from(preview_disabled()));
            }
            let issuing_key = state.issuing_key.clone().ok_or_else(auth_not_configured)?;
            let verifier = state
                .token_verifier
                .clone()
                .ok_or_else(auth_not_configured)?;
            let tenant_id = tenant_from_unverified_access_token(subject_token.expose())?;
            let mut conn = wyrd_sql::TenantConn::acquire(&state.pool, tenant_id)
                .await
                .map_err(sql_error)?;
            let request_id_buf: String;
            let request_id = match request_id.as_ref() {
                Some(Extension(id)) => id.as_str(),
                None => {
                    request_id_buf = Uuid::new_v4().to_string();
                    &request_id_buf
                }
            };
            let exchanged = DelegateToken {
                issuing_key,
                verifier,
                permission_check: state.permission_check.clone(),
                settings: Default::default(),
            }
            .execute(
                &mut conn,
                SecretString::from(subject_token.expose().to_owned()),
                requested_subject,
                request_id,
            )
            .await
            .map_err(|error| WyrdErrorResponse::from(WyrdError::from(error)))?;
            conn.commit().await.map_err(sql_error)?;
            Ok(Json(exchanged.into_response()))
        }
    }
}

async fn issue_key(
    State(_state): State<AppState>,
    caller: Caller,
    Json(_request): Json<IssueKeyRequest>,
) -> Result<Json<wyrd_spec::auth::IssueKeyResponse>, WyrdErrorResponse> {
    // Full route wiring waits for the JWT middleware to expose
    // `wyrd_runtime::Principal`. The service implementation is complete and
    // testable; this skeletal route refuses rather than inventing a lossy
    // conversion from the deprecated scope-based extractor.
    let _ = (caller, IssueApiKey::default());
    Err(WyrdErrorResponse::from(WyrdError::AuthPreviewDisabled {
        message: "issue-key route requires runtime principal extraction".to_owned(),
        details: serde_json::json!({ "phase": "auth-middleware" }),
    }))
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
mod tests {
    use base64::Engine;
    use wyrd_auth_verify::{AccessTokenClaims, PrincipalKindWire, TokenPrincipalRef};
    use wyrd_runtime::PrincipalId;
    use wyrd_spec::DataTenantId;

    use super::{router, tenant_from_unverified_access_token};

    #[test]
    fn mounts_token_and_issue_key_routes() {
        let _router = router();
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
                kind: PrincipalKindWire::User,
                tenant_id,
                card_ref: None,
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
}
