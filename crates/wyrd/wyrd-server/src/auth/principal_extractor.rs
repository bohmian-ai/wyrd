// TODO(auth-wiring): remove when JWT verifier lands

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use wyrd_spec::authz::Principal;
use wyrd_spec::error::WyrdError;

use crate::error::WyrdErrorResponse;

#[cfg(feature = "stub-auth")]
use std::collections::BTreeSet;
#[cfg(feature = "stub-auth")]
use axum::http::header::AUTHORIZATION;
#[cfg(feature = "stub-auth")]
use wyrd_spec::actor::Actor;

/// Server-owned extractor wrapper for an authenticated Wyrd principal.
#[derive(Debug, Clone)]
pub struct AuthenticatedPrincipal {
    /// The authenticated Wyrd principal contract.
    pub principal: Principal,
}

impl From<AuthenticatedPrincipal> for Principal {
    fn from(value: AuthenticatedPrincipal) -> Self {
        value.principal
    }
}

#[cfg(feature = "stub-auth")]
impl<S: Send + Sync> FromRequestParts<S> for AuthenticatedPrincipal {
    type Rejection = WyrdErrorResponse;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = bearer_token(parts).map_err(WyrdErrorResponse::from)?;
        Ok(Self {
            principal: Principal::new(
                Actor::Service {
                    name: token.to_owned(),
                },
                BTreeSet::new(),
            ),
        })
    }
}

#[cfg(not(feature = "stub-auth"))]
impl<S: Send + Sync> FromRequestParts<S> for AuthenticatedPrincipal {
    type Rejection = WyrdErrorResponse;

    async fn from_request_parts(_parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Err(WyrdErrorResponse::from(WyrdError::InvalidToken {
            message: "auth not configured: enable the stub-auth feature in non-production builds, or wire the JWT verifier".to_owned(),
            details: serde_json::json!({}),
        }))
    }
}

#[cfg(feature = "stub-auth")]
fn bearer_token(parts: &Parts) -> Result<&str, WyrdError> {
    let Some(header) = parts.headers.get(AUTHORIZATION) else {
        return Err(WyrdError::Unauthenticated {
            message: "missing bearer token".to_owned(),
            details: serde_json::json!({ "header": "authorization" }),
        });
    };
    let value = header.to_str().map_err(|_| WyrdError::InvalidToken {
        message: "authorization header is not valid UTF-8".to_owned(),
        details: serde_json::json!({ "header": "authorization" }),
    })?;
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(WyrdError::Unauthenticated {
            message: "missing bearer token".to_owned(),
            details: serde_json::json!({ "header": "authorization" }),
        });
    };
    if token.is_empty() {
        return Err(WyrdError::InvalidToken {
            message: "bearer token is empty".to_owned(),
            details: serde_json::json!({ "header": "authorization" }),
        });
    }
    Ok(token)
}
