//! Verify-only authentication helpers.

#![deny(missing_docs)]

use std::collections::BTreeSet;

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use wyrd_spec::actor::Actor;
use wyrd_spec::authz::{Principal, Scope};

/// Authentication helper errors.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// JWT verification failed.
    #[error("jwt error")]
    Jwt(#[source] jsonwebtoken::errors::Error),
}

/// Resolved Wyrd access-token claims.
///
/// The token carries the authenticated subject and its resolved scopes so
/// verification can reconstruct a principal without a runtime lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    /// Subject id, JWT `sub`.
    pub sub: String,
    /// Full authenticated identity.
    pub actor: Actor,
    /// Scopes resolved from the subject's roles at issue time.
    pub scopes: BTreeSet<Scope>,
    /// Expiry as Unix seconds, JWT `exp`.
    pub exp: usize,
    /// Issued-at as Unix seconds, JWT `iat`.
    pub iat: usize,
    /// Issuer, JWT `iss`.
    pub iss: String,
}

impl AccessTokenClaims {
    /// Build a runtime principal from verified claims.
    #[must_use]
    pub fn to_principal(&self) -> Principal {
        Principal::new(self.actor.clone(), self.scopes.clone())
    }
}

/// Verify an EdDSA JWT with caller-supplied validation policy.
///
/// # Errors
/// Returns an error when signature verification, header validation, or claim
/// validation fails.
pub fn verify_eddsa_with<C: DeserializeOwned>(
    token: &str,
    public_key: &DecodingKey,
    mut validation: Validation,
) -> Result<C, AuthError> {
    validation.algorithms = vec![Algorithm::EdDSA];
    jsonwebtoken::decode::<C>(token, public_key, &validation)
        .map(|data| data.claims)
        .map_err(AuthError::Jwt)
}

/// Verify an EdDSA Wyrd token with the standard access-token policy.
///
/// Expiration is enforced, audience is unused, and issuer is enforced when
/// supplied.
///
/// # Errors
/// Returns an error when signature verification, header validation, or claim
/// validation fails.
pub fn verify_eddsa<C: DeserializeOwned>(
    token: &str,
    public_key: &DecodingKey,
    issuer: Option<&str>,
) -> Result<C, AuthError> {
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_aud = false;
    if let Some(issuer) = issuer {
        validation.set_issuer(&[issuer]);
    }
    verify_eddsa_with(token, public_key, validation)
}

/// Decode the JWT header `kid` without verifying the signature.
///
/// # Errors
/// Returns an error when the token header cannot be decoded.
pub fn decode_kid(token: &str) -> Result<Option<String>, AuthError> {
    jsonwebtoken::decode_header(token)
        .map(|header| header.kid)
        .map_err(AuthError::Jwt)
}

/// Build an EdDSA public decoding key from PEM bytes.
///
/// # Errors
/// Returns an error when the PEM bytes are not a valid EdDSA public key.
pub fn public_key_from_pem(pem: &[u8]) -> Result<DecodingKey, AuthError> {
    DecodingKey::from_ed_pem(pem).map_err(AuthError::Jwt)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use jsonwebtoken::{Algorithm, EncodingKey, Header, Validation, encode};
    use wyrd_spec::authz::Scope;
    use wyrd_spec::ids::CardUid;

    use super::verify_eddsa_with;
    use super::{AccessTokenClaims, decode_kid, public_key_from_pem, verify_eddsa};

    const PRIVATE_KEY_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[test]
    fn verify_eddsa_roundtrips_access_token() {
        let claims = claims_with_times(now() + 3_600, now());
        let token = encode_eddsa(&claims);
        let decoded = verify_eddsa::<AccessTokenClaims>(&token, &public_key(), Some("wyrd"))
            .expect("valid token verifies");

        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.actor, claims.actor);
        assert_eq!(decoded.scopes, claims.scopes);
    }

    #[test]
    fn to_principal_carries_actor_and_scopes() {
        let claims = claims_with_times(now() + 3_600, now());
        let principal = claims.to_principal();

        assert_eq!(principal.actor, claims.actor);
        assert_eq!(principal.scopes, claims.scopes);
        assert!(principal.has_all(claims.scopes));
    }

    #[test]
    fn verify_eddsa_rejects_hs256_token() {
        let claims = claims_with_times(now() + 3_600, now());
        let token = encode_hs256(&claims);

        assert!(verify_eddsa::<AccessTokenClaims>(&token, &public_key(), Some("wyrd")).is_err());
    }

    #[test]
    fn verify_eddsa_with_overrides_caller_algorithms() {
        let claims = claims_with_times(now() + 3_600, now());
        let token = encode_hs256(&claims);
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_aud = false;
        validation.algorithms = vec![Algorithm::HS256];

        assert!(verify_eddsa_with::<AccessTokenClaims>(&token, &public_key(), validation).is_err());
    }

    #[test]
    fn verify_eddsa_rejects_expired() {
        let claims = claims_with_times(now() - 3_600, now() - 7_200);
        let token = encode_eddsa(&claims);

        assert!(verify_eddsa::<AccessTokenClaims>(&token, &public_key(), Some("wyrd")).is_err());
    }

    #[test]
    fn verify_eddsa_enforces_issuer() {
        let claims = claims_with_times(now() + 3_600, now());
        let token = encode_eddsa(&claims);

        assert!(verify_eddsa::<AccessTokenClaims>(&token, &public_key(), Some("other")).is_err());
    }

    #[test]
    fn decode_kid_reads_header() {
        let claims = claims_with_times(now() + 3_600, now());
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some("k1".to_owned());
        let token = encode(&header, &claims, &private_key()).expect("test token signs");

        assert_eq!(
            decode_kid(&token).expect("kid decodes"),
            Some("k1".to_owned())
        );
    }

    #[test]
    fn public_key_from_pem_rejects_garbage() {
        assert!(public_key_from_pem(b"not a pem").is_err());
    }

    fn claims_with_times(exp: usize, iat: usize) -> AccessTokenClaims {
        let actor = wyrd_spec::actor::Actor::User {
            id: CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00")
                .expect("test fixture uses a UUIDv7"),
            email: "user@example.com".to_owned(),
        };

        AccessTokenClaims {
            sub: "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00".to_owned(),
            actor,
            scopes: BTreeSet::from([Scope::CardRead, Scope::CardWrite]),
            exp,
            iat,
            iss: "wyrd".to_owned(),
        }
    }

    fn private_key() -> EncodingKey {
        EncodingKey::from_ed_pem(PRIVATE_KEY_PEM).expect("test private key parses")
    }

    fn public_key() -> jsonwebtoken::DecodingKey {
        public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key parses")
    }

    fn encode_eddsa(claims: &AccessTokenClaims) -> String {
        encode(&Header::new(Algorithm::EdDSA), claims, &private_key()).expect("test token signs")
    }

    fn encode_hs256(claims: &AccessTokenClaims) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(b"secret"),
        )
        .expect("test token signs")
    }

    fn now() -> usize {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after unix epoch")
            .as_secs() as usize
    }
}
