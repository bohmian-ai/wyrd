//! Cross-cutting auth error converters shared by all three token-issuing flows.

use serde_json::json;
use wyrd_auth_oidc::ScreenError;
use wyrd_auth_verify::{AuthError, MAX_DELEGATION_DEPTH};
use wyrd_spec::error::WyrdError;

/// Convert an outbound address-screening refusal to the public catalog.
///
/// Every cause is one public error: a caller learns that the provider could not
/// be reached, never which address range it resolved to, because that answer is
/// a probe of the deployment's internal network.
pub(crate) fn screen_error(error: &ScreenError) -> WyrdError {
    tracing::warn!(%error, "identity provider request refused by address screening");
    WyrdError::DiscoveryUnavailable {
        message: "identity provider could not be reached".to_owned(),
        details: json!({}),
    }
}

/// Convert a token-verifier [`AuthError`] to the public [`WyrdError`] catalog.
pub(crate) fn auth_error_to_wyrd(error: AuthError) -> WyrdError {
    match error {
        AuthError::Jwt(error) => jwt_error_to_wyrd(&error),
        AuthError::InvalidToken => invalid_token("token rejected"),
        AuthError::TokenExpired => WyrdError::TokenExpired {
            message: "token expired".to_owned(),
            details: json!({}),
        },
        AuthError::InvalidCardRef => WyrdError::InvalidCardRef {
            message: "non-user token card_ref claim is absent or malformed".to_owned(),
            details: json!({}),
        },
        AuthError::CardScopeMissingRoot => WyrdError::InvalidCardRef {
            message: "card-bound token scope is missing its root card_ref".to_owned(),
            details: json!({ "field": "card_ref_scope" }),
        },
        AuthError::DelegationDepthExceeded => WyrdError::DelegationDepthExceededVerify {
            message: format!(
                "delegation chain exceeds MAX_DELEGATION_DEPTH={MAX_DELEGATION_DEPTH}"
            ),
            details: json!({ "max": MAX_DELEGATION_DEPTH }),
        },
        AuthError::BadTokenFormat => WyrdError::BadTokenFormat {
            message: "X-Wyrd-Access-Token is not a compact Wyrd JWT".to_owned(),
            details: json!({}),
        },
        AuthError::VerifyUnavailable => WyrdError::AuthVerifyUnavailable {
            message: "auth verify backend unavailable".to_owned(),
            details: json!({ "retry_after_seconds": 1 }),
        },
    }
}

pub(crate) fn jwt_error_to_wyrd(error: &jsonwebtoken::errors::Error) -> WyrdError {
    use jsonwebtoken::errors::ErrorKind;

    tracing::debug!(jwt_error = ?error.kind(), "JWT validation failed");

    match error.kind() {
        ErrorKind::ExpiredSignature => WyrdError::TokenExpired {
            message: "token expired".to_owned(),
            details: json!({}),
        },
        ErrorKind::Base64(_) | ErrorKind::Json(_) | ErrorKind::Utf8(_) => {
            WyrdError::BadTokenFormat {
                message: "token payload is malformed".to_owned(),
                details: json!({}),
            }
        }
        ErrorKind::InvalidSignature
        | ErrorKind::InvalidIssuer
        | ErrorKind::InvalidSubject
        | ErrorKind::InvalidAudience
        | ErrorKind::InvalidAlgorithm
        | ErrorKind::InvalidAlgorithmName => {
            invalid_token("bearer token signature, issuer, subject, audience, or algorithm invalid")
        }
        _ => invalid_token("bearer token rejected"),
    }
}

fn invalid_token(message: &str) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details: json!({}),
    }
}
