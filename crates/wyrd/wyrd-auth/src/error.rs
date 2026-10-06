//! Cross-cutting auth error converters shared by all three token-issuing flows.

use std::fmt::Display;

use serde_json::json;
use wyrd_auth_oidc::{RelyingPartyError, ScreenError};
use wyrd_auth_verify::{AuthError, MAX_DELEGATION_DEPTH};
use wyrd_spec::error::WyrdError;
use wyrd_sql::SqlError;

/// Convert an outbound address-screening refusal to the public catalog.
///
/// Every cause is one public error: a caller learns that the provider could not
/// be reached, never which address range it resolved to, because that answer is
/// a probe of the deployment's internal network.
pub(crate) fn screen_error(error: &ScreenError) -> WyrdError {
    provider_unreachable(format_args!(
        "request refused by address screening: {error}"
    ))
}

/// The identity provider could not be reached, failed, or answered unusably.
///
/// Logs `cause` server-side and returns the one public refusal every provider
/// transport failure shares, naming no address, status, or body detail.
pub(crate) fn provider_unreachable(cause: impl Display) -> WyrdError {
    tracing::warn!(error = %cause, "identity provider unavailable");
    WyrdError::DiscoveryUnavailable {
        message: "identity provider could not be reached".to_owned(),
        details: json!({}),
    }
}

/// Convert a relying-party refusal to the public catalog.
///
/// Screening refusals and provider outages are the one provider-unreachable
/// refusal; an issuer mismatch keeps its stable
/// `details.reason = "issuer_mismatch"`; a token-endpoint outage is a
/// retryable `503`; every refused code or ID token is
/// [`WyrdError::InvalidToken`] with its cause logged server-side only.
pub(crate) fn relying_party_error(error: RelyingPartyError) -> WyrdError {
    match error {
        RelyingPartyError::Screened(error) => screen_error(&error),
        RelyingPartyError::IssuerMismatch => {
            tracing::warn!("OIDC discovery names a different issuer");
            WyrdError::DiscoveryUnavailable {
                message: "the provider discovery document names a different issuer".to_owned(),
                details: json!({ "reason": "issuer_mismatch" }),
            }
        }
        RelyingPartyError::DiscoveryUnavailable(cause) => {
            provider_unreachable(format_args!("OIDC discovery failed: {cause}"))
        }
        RelyingPartyError::TokenEndpointUnavailable(cause) => {
            tracing::warn!(error = %cause, "OIDC token endpoint unavailable");
            WyrdError::AuthVerifyUnavailable {
                message: "OIDC token endpoint unavailable".to_owned(),
                details: json!({ "retry_after_seconds": 1 }),
            }
        }
        RelyingPartyError::InvalidNonce => WyrdError::InvalidNonce {
            message: "id token nonce is missing or mismatched".to_owned(),
            details: json!({}),
        },
        RelyingPartyError::Configuration(cause) => WyrdError::Internal {
            message: format!("relying-party configuration is unusable: {cause}"),
            details: json!({}),
        },
        error @ (RelyingPartyError::TokenRejected(_)
        | RelyingPartyError::UnknownKey
        | RelyingPartyError::InvalidIdToken(_)) => {
            tracing::warn!(error = %error, "OIDC sign-in refused");
            invalid_token("sign-in was refused")
        }
    }
}

/// Map an auth store failure to the fail-closed backend error, logging the
/// cause server-side only.
///
/// Every auth workflow that reads or writes the tenant auth tables answers a
/// store outage the same way: a retryable `503` that names no table, query, or
/// driver detail.
pub(crate) fn store_error(error: impl Into<SqlError>) -> WyrdError {
    let error = error.into();
    tracing::warn!(error = %error, "auth store unavailable");
    WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: json!({ "retry_after_seconds": 1 }),
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
