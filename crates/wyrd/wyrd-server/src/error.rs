//! HTTP response mapping for Wyrd errors.

use axum::body::Body;
use axum::http::{StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use wyrd_auth_verify::{AuthError, MAX_DELEGATION_DEPTH};
use wyrd_runtime::PermissionDenyReason;
use wyrd_spec::error::WyrdError;

use crate::auth::roles::RoleAdminError;
use crate::auth::seed::SeedError;

/// Server-owned response wrapper for public Wyrd errors.
///
/// Axum's `IntoResponse` trait and `WyrdError` are both owned outside this
/// crate, so Rust's orphan rules require the local wrapper. All server Wyrd
/// error responses still flow through this one mapper.
#[derive(Debug, Clone)]
pub struct WyrdErrorResponse(pub WyrdError);

impl From<WyrdError> for WyrdErrorResponse {
    fn from(error: WyrdError) -> Self {
        Self(error)
    }
}

impl From<WyrdErrorResponse> for WyrdError {
    fn from(response: WyrdErrorResponse) -> Self {
        response.0
    }
}

impl From<AuthError> for WyrdErrorResponse {
    fn from(error: AuthError) -> Self {
        Self(auth_error_to_wyrd(error))
    }
}

impl From<PermissionDenyReason> for WyrdErrorResponse {
    fn from(reason: PermissionDenyReason) -> Self {
        Self(permission_deny_reason_to_wyrd(reason))
    }
}

impl From<RoleAdminError> for WyrdErrorResponse {
    fn from(error: RoleAdminError) -> Self {
        Self(role_admin_error_to_wyrd(error))
    }
}

impl From<SeedError> for WyrdErrorResponse {
    fn from(error: SeedError) -> Self {
        Self(seed_error_to_wyrd(error))
    }
}

impl IntoResponse for WyrdErrorResponse {
    fn into_response(self) -> Response {
        wyrd_error_response(self.0)
    }
}

/// Render a Wyrd error as an RFC 9457 problem+json response.
#[must_use]
pub fn wyrd_error_response(error: WyrdError) -> Response {
    let status = StatusCode::from_u16(error.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    match serde_json::to_vec(&error.as_problem_json()) {
        Ok(body) => {
            let mut response = response_with_body(status, body);
            if matches!(error, WyrdError::AuthVerifyUnavailable { .. }) {
                response.headers_mut().insert(
                    axum::http::header::RETRY_AFTER,
                    axum::http::HeaderValue::from_static("1"),
                );
            }
            response
        }
        Err(_) => response_with_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            br#"{"type":"https://wyrd.dev/problems/WYRD_SPEC_500_INTERNAL","title":"Internal error","status":500,"detail":"failed to serialize Wyrd error response","code":"WYRD_SPEC_500_INTERNAL","details":{},"remediation":"Retry later or inspect server logs using the request ID."}"#.to_vec(),
        ),
    }
}

fn response_with_body(status: StatusCode, body: Vec<u8>) -> Response {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/problem+json"),
    );
    response
}

/// Convert auth verifier failures into stable public Wyrd errors.
#[must_use]
pub fn auth_error_to_wyrd(error: AuthError) -> WyrdError {
    match error {
        AuthError::Jwt(error) => jwt_error_to_wyrd(error),
        AuthError::InvalidToken => invalid_token("token rejected", serde_json::json!({})),
        AuthError::TokenExpired => WyrdError::TokenExpired {
            message: "token expired".to_owned(),
            details: serde_json::json!({}),
        },
        AuthError::InvalidCardRef => WyrdError::InvalidCardRef {
            message: "non-user token card_ref claim is absent or malformed".to_owned(),
            details: serde_json::json!({}),
        },
        AuthError::DelegationDepthExceeded => WyrdError::DelegationDepthExceededVerify {
            message: format!("delegation chain exceeds MAX_DELEGATION_DEPTH={MAX_DELEGATION_DEPTH}"),
            details: serde_json::json!({ "max": MAX_DELEGATION_DEPTH }),
        },
        AuthError::Revoked => WyrdError::CredentialRevoked {
            message: "credential revoked".to_owned(),
            details: serde_json::json!({}),
        },
        AuthError::BadTokenFormat => bad_token_format("authorization header malformed"),
        AuthError::VerifyUnavailable => WyrdError::AuthVerifyUnavailable {
            message: "auth verify backend unavailable".to_owned(),
            details: serde_json::json!({ "retry_after_seconds": 1 }),
        },
        AuthError::PermissionsCorrupt => WyrdError::RoleCorrupt {
            message: "stored role permissions failed to decode".to_owned(),
            details: serde_json::json!({}),
        },
    }
}

fn jwt_error_to_wyrd(error: jsonwebtoken::errors::Error) -> WyrdError {
    use jsonwebtoken::errors::ErrorKind;

    match error.kind() {
        ErrorKind::ExpiredSignature => WyrdError::TokenExpired {
            message: "token expired".to_owned(),
            details: serde_json::json!({}),
        },
        ErrorKind::Base64(_) | ErrorKind::Json(_) | ErrorKind::Utf8(_) => {
            bad_token_format("token payload is malformed")
        }
        ErrorKind::InvalidSignature
        | ErrorKind::InvalidIssuer
        | ErrorKind::InvalidSubject
        | ErrorKind::InvalidAudience
        | ErrorKind::InvalidAlgorithm
        | ErrorKind::InvalidAlgorithmName => invalid_token(
            "bearer token signature, issuer, subject, audience, or algorithm invalid",
            serde_json::json!({ "jwt_error": format!("{:?}", error.kind()) }),
        ),
        ErrorKind::InvalidToken
        | ErrorKind::InvalidEcdsaKey
        | ErrorKind::InvalidRsaKey(_)
        | ErrorKind::RsaFailedSigning
        | ErrorKind::InvalidKeyFormat
        | ErrorKind::MissingRequiredClaim(_)
        | ErrorKind::ImmatureSignature
        | ErrorKind::MissingAlgorithm
        | ErrorKind::Crypto(_) => invalid_token(
            "bearer token rejected",
            serde_json::json!({ "jwt_error": format!("{:?}", error.kind()) }),
        ),
        _ => invalid_token(
            "bearer token rejected",
            serde_json::json!({ "jwt_error": format!("{:?}", error.kind()) }),
        ),
    }
}

/// Convert an RBAC denial into a stable public Wyrd error.
#[must_use]
pub fn permission_deny_reason_to_wyrd(reason: PermissionDenyReason) -> WyrdError {
    match reason {
        PermissionDenyReason::Rbac {
            required,
            principal,
        } => {
            let resource = format!("{:?}", required.resource);
            let action = format!("{:?}", required.action);
            WyrdError::PermissionDeniedRbac {
                message: format!("principal {principal} lacks {resource}/{action}"),
                details: serde_json::json!({
                    "required": required,
                    "principal": principal.to_string(),
                }),
            }
        }
    }
}

/// Convert role-admin errors at the handler boundary.
#[must_use]
pub fn role_admin_error_to_wyrd(error: RoleAdminError) -> WyrdError {
    match error {
        RoleAdminError::CannotDeleteBuiltin => WyrdError::PermissionDeniedRbac {
            message: "builtin roles cannot be deleted".to_owned(),
            details: serde_json::json!({ "resource": "roles", "action": "delete" }),
        },
        RoleAdminError::NotFound => WyrdError::NotFound {
            message: "role not found".to_owned(),
            details: serde_json::json!({ "resource": "role" }),
        },
        RoleAdminError::Database(error) => sqlx_error_to_wyrd(error),
    }
}

/// Convert seed errors at the handler boundary.
#[must_use]
pub fn seed_error_to_wyrd(error: SeedError) -> WyrdError {
    match error {
        SeedError::Database(error) => sqlx_error_to_wyrd(error),
        SeedError::Serialize(error) => WyrdError::Internal {
            message: "builtin role permissions failed to serialize".to_owned(),
            details: serde_json::json!({ "source": error.to_string() }),
        },
    }
}

/// Convert SQLx database errors that have public auth/RBAC contract meaning.
#[must_use]
pub fn sqlx_error_to_wyrd(error: sqlx::Error) -> WyrdError {
    if let sqlx::Error::Database(db_error) = &error {
        if db_error.code().as_deref() == Some("23514")
            && db_error.constraint() == Some("auth_builtin_role_immutable_name")
        {
            return WyrdError::BuiltinRoleImmutableName {
                message: "builtin role names are immutable".to_owned(),
                details: serde_json::json!({
                    "sqlstate": "23514",
                    "constraint": "auth_builtin_role_immutable_name",
                }),
            };
        }
    }

    tracing::warn!(error = %error, "database operation failed");
    WyrdError::Internal {
        message: "database operation failed".to_owned(),
        details: serde_json::json!({}),
    }
}

fn bad_token_format(message: &str) -> WyrdError {
    WyrdError::BadTokenFormat {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}

fn invalid_token(message: &str, details: serde_json::Value) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details,
    }
}
