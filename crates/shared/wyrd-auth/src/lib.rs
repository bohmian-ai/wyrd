//! Verify-only authentication helpers.

#![deny(missing_docs)]

use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use wyrd_spec::actor::Actor;

/// Verified identity claim set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject.
    pub sub: String,
    /// Email address.
    pub email: Option<String>,
    /// Expiration timestamp.
    pub exp: usize,
}

/// Verify a JWT and map it to an Actor.
///
/// # Errors
/// Returns an error when token verification fails.
pub fn verify_jwt(
    token: &str,
    key: &DecodingKey,
    validation: &Validation,
) -> Result<Claims, AuthError> {
    decode::<Claims>(token, key, validation)
        .map(|data| data.claims)
        .map_err(|source| AuthError::Jwt(source.to_string()))
}

/// Pair an Actor with verified claims.
#[derive(Debug, Clone)]
pub struct VerifiedActor {
    /// Actor identity.
    pub actor: Actor,
    /// JWT claims.
    pub claims: Claims,
}

/// Authentication helper errors.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// JWT verification failed.
    #[error("jwt verification failed: {0}")]
    Jwt(String),
}
