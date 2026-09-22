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
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::request_id::RequestId;

use crate::components::auth::token_extract::{
    auth_not_configured, extract_wyrd_access_token, tenant_from_unverified_access_token,
};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Check whether a Service/Agent actor's call on a subject's behalf may proceed.
///
/// The delegated token's subject carries the attenuated authority the required
/// permission is checked against; its current actor must be a Card-bound
/// Service or Agent, and the policy sees the full actor chain.
///
/// The policy hook runs first; only its `Allow` falls through to the RBAC
/// check on the subject. Outside the stub writer, the resulting decision is
/// audited and committed in a tenant transaction before the response is sent,
/// so a failed audit returns an error and no decision.
///
/// # Errors
/// Returns `401`/`400` when the access token is missing, malformed, invalid,
/// or expired; `503` when no verifier is configured or the audit database is
/// unavailable; `403 WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN` when the token has
/// no eligible Card-bound actor; `400` when the body is invalid, a check header
/// is not UTF-8, or the action is unknown; `500` when the check context cannot
/// be assembled or the audit write fails; and `403 WYRD_AUTHZ_403_POLICY_DENIED`
/// when policy denies for any reason other than a missing permission.
#[tracing::instrument(skip(state, headers, body), fields(request_id = %request_id))]
#[utoipa::path(
    post,
    path = "/authz/check",
    request_body(content = String, content_type = "application/json",
      description = "`AuthzCheckRequest`: the action the delegated caller wants to perform"),
    responses(
        (status = 200, description = "`AuthzCheckResponse`: allowed, or denied for a missing \
          permission", content_type = "application/json"),
        (status = 400, description = "The token is not a compact JWT, the body is not a valid \
          check request, a required check header is missing, or the action is unknown \
          (WYRD_AUTH_400_BAD_TOKEN_FORMAT, WYRD_SPEC_400_VALIDATION, \
          WYRD_VALIDATION_400_MISSING_REQUIRED_FIELD)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The token is not a delegated invoke token, or policy \
          refused the invocation (WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN, \
          WYRD_AUTHZ_403_POLICY_DENIED)", body = WyrdProblem),
        (status = 500, description = "The request context could not be assembled, or the \
          decision could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "Authentication is unconfigured, or no verifier is \
          configured for the access token \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Authz"
)]
pub async fn check_authz(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<AuthzCheckResponse>, WyrdErrorResponse> {
    let token = extract_wyrd_access_token(&headers)?;
    let expected_tenant = tenant_from_unverified_access_token(token.expose_secret())?;
    let verified = state
        .auth
        .token_verifier
        .as_deref()
        .ok_or_else(auth_not_configured)?
        .verify(&token, &expected_tenant)
        .map_err(WyrdErrorResponse::from)?;

    let actor = verified.delegation_chain.last().map(|step| &step.principal);
    match guard_reason(actor) {
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
        PolicyDecision::Allow => {
            match state.authz.permission_check.check(&ctx.subject, &required) {
                PermissionVerdict::Allow => PolicyDecision::Allow,
                PermissionVerdict::Deny {
                    reason: PermissionDenyReason::Rbac { .. },
                } => PolicyDecision::Deny {
                    reason: "missing_permission".to_owned(),
                },
            }
        }
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
            .tenant_conn(ctx.subject.tenant_id)
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
