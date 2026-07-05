//! `/v1/authz/check` handler.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::http::HeaderMap;
use secrecy::ExposeSecret;
use wyrd_auth_check::guard::{GuardOutcome, guard_reason};
use wyrd_auth_check::{
    AuthzCheckContext, AuthzCheckRequest, AuthzCheckRequestError, AuthzCheckRequestMetadata,
    AuthzCheckResponse,
};
use wyrd_runtime::{Permission, PermissionDenyReason, PermissionVerdict};
use wyrd_spec::card::policy::PolicyDecision;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::auth::token_extract::{
    auth_not_configured, extract_wyrd_access_token, tenant_from_unverified_access_token,
};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Check a delegated Service/Agent invoke request.
#[tracing::instrument(skip(state, headers, body), fields(request_id = %request_id))]
pub async fn check_authz(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<AuthzCheckResponse>, WyrdErrorResponse> {
    let token = extract_wyrd_access_token(&headers)?;
    let expected_tenant = tenant_from_unverified_access_token(token.expose_secret())?;
    let verifier = state
        .auth
        .token_verifier
        .clone()
        .ok_or_else(auth_not_configured)?;
    let verified = verifier
        .verify(&token, &expected_tenant)
        .await
        .map_err(WyrdErrorResponse::from)?;

    match guard_reason(&verified.principal, verified.delegation_chain.len()) {
        GuardOutcome::Allow => {}
        GuardOutcome::Reject(reason) => {
            return Err(WyrdError::AuthzRequiresDelegatedToken {
                message: "delegated-token guard failed".to_owned(),
                details: serde_json::json!({ "reason": reason.as_str() }),
            }
            .into());
        }
    }

    let request = serde_json::from_slice::<AuthzCheckRequest>(&body).map_err(|error| {
        WyrdErrorResponse::from(WyrdError::Validation {
            message: "/v1/authz/check request body is invalid".to_owned(),
            details: serde_json::json!({ "error": error.to_string() }),
        })
    })?;
    let metadata = match AuthzCheckRequestMetadata::from_headers(&headers) {
        Ok(metadata) => Some(metadata),
        Err(AuthzCheckRequestError::MissingHeader { .. }) => None,
        Err(error) => return Err(request_error_to_wyrd(error)),
    };
    let required = permission_for_action(&request.action)?;
    let ctx = AuthzCheckContext::from_verified(&verified, request, metadata, request_id)
        .map_err(WyrdError::from)?;
    let hook_decision = state.authz.policy_hook.evaluate(&ctx).await;
    let decision = match hook_decision {
        PolicyDecision::Allow => match state.authz.permission_check.check(&ctx.callee, &required) {
            PermissionVerdict::Allow => PolicyDecision::Allow,
            PermissionVerdict::Deny {
                reason: PermissionDenyReason::Rbac { .. },
            } => PolicyDecision::Deny {
                reason: "missing_permission".to_owned(),
            },
        },
        PolicyDecision::Deny { reason } => PolicyDecision::Deny { reason },
        _ => PolicyDecision::Deny {
            reason: "unsupported_decision".to_owned(),
        },
    };

    // Skip audit write when using the default stub writer (test/dev environments).
    // build_production() prevents this branch from being reached in production.
    if !state.authz.audit_writer.is_stub_default() {
        let mut conn = state
            .postgres
            .tenant_conn(ctx.callee.tenant_id)
            .await
            .map_err(sql_error)?;
        state
            .authz
            .audit_writer
            .write_authz_check(&mut conn, &ctx, &decision)
            .await?;
        conn.commit().await.map_err(sql_error)?;
    }

    match decision {
        PolicyDecision::Allow => Ok(Json(AuthzCheckResponse::allow())),
        PolicyDecision::Deny { reason } if reason == "missing_permission" => Ok(Json(
            AuthzCheckResponse::missing_permission(required.to_string()),
        )),
        PolicyDecision::Deny { reason } => Err(WyrdError::PolicyDenied {
            message: "policy denied invoke".to_owned(),
            details: serde_json::json!({ "reason": reason }),
        }
        .into()),
        _ => Err(WyrdError::PolicyDenied {
            message: "policy returned unsupported decision".to_owned(),
            details: serde_json::json!({ "reason": "unsupported_decision" }),
        }
        .into()),
    }
}

fn permission_for_action(action: &str) -> Result<Permission, WyrdErrorResponse> {
    match action {
        "card_write" => Ok(Permission::card_write()),
        value => value.parse::<Permission>().map_err(|_| {
            WyrdErrorResponse::from(WyrdError::Validation {
                message: "authz-check action is unknown".to_owned(),
                details: serde_json::json!({ "action": value, "example_actions": ["card_write"] }),
            })
        }),
    }
}

fn request_error_to_wyrd(error: AuthzCheckRequestError) -> WyrdErrorResponse {
    match error {
        AuthzCheckRequestError::MissingHeader { header } => WyrdError::MissingRequiredField {
            message: "authz-check required header is missing".to_owned(),
            details: serde_json::json!({ "field": display_header_name(header) }),
        },
        AuthzCheckRequestError::InvalidUtf8 { header } => WyrdError::Validation {
            message: "authz-check required header is not valid UTF-8".to_owned(),
            details: serde_json::json!({ "field": display_header_name(header) }),
        },
    }
    .into()
}

fn display_header_name(header: &str) -> String {
    header
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

fn sql_error(error: wyrd_sql::SqlError) -> WyrdErrorResponse {
    tracing::warn!(error = %error, "authz-check audit database unavailable");
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "authz-check audit backend unavailable".to_owned(),
        details: serde_json::json!({ "source_code": error.code() }),
    })
}
