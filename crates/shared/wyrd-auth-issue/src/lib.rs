//! Server-tier authentication issuance helpers.

#![deny(missing_docs)]

use std::collections::BTreeSet;

use argon2::Argon2;
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use secrecy::{ExposeSecret, SecretString};
use wyrd_auth_verify::AccessTokenClaims;
use wyrd_spec::actor::Actor;
use wyrd_spec::authz::Scope;

/// Server-tier Ed25519 signing key.
pub struct IssuingKey {
    encoding: EncodingKey,
    kid: String,
    issuer: String,
}

/// Authentication issuance errors.
#[derive(Debug, thiserror::Error)]
pub enum IssueError {
    /// Private-key load failed.
    #[error("key load failed: {0}")]
    Key(String),
    /// Token encoding failed.
    #[error("token encode failed: {0}")]
    Encode(String),
    /// API-key hashing failed.
    #[error("api key hash failed: {0}")]
    Hash(String),
}

impl IssuingKey {
    /// Load an Ed25519 private key from PEM bytes.
    ///
    /// # Errors
    /// Returns an error when the PEM is not a valid EdDSA private key.
    pub fn from_ed_pem(
        pem: SecretString,
        kid: impl Into<String>,
        issuer: impl Into<String>,
    ) -> Result<Self, IssueError> {
        let encoding = EncodingKey::from_ed_pem(pem.expose_secret().as_bytes())
            .map_err(|source| IssueError::Key(source.to_string()))?;
        Ok(Self {
            encoding,
            kid: kid.into(),
            issuer: issuer.into(),
        })
    }

    /// Mint an EdDSA access token carrying the resolved actor and scopes.
    ///
    /// # Errors
    /// Returns an error when token encoding fails.
    pub fn issue_access_token(
        &self,
        subject: &str,
        actor: Actor,
        scopes: BTreeSet<Scope>,
        ttl: Duration,
    ) -> Result<String, IssueError> {
        let issued_at = Utc::now();
        let iat = issued_at.timestamp() as usize;
        let exp = (issued_at + ttl).timestamp() as usize;
        let claims = AccessTokenClaims {
            sub: subject.to_owned(),
            actor,
            scopes,
            exp,
            iat,
            iss: self.issuer.clone(),
        };
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(self.kid.clone());

        jsonwebtoken::encode(&header, &claims, &self.encoding)
            .map_err(|source| IssueError::Encode(source.to_string()))
    }
}

/// Hash a raw API key for at-rest storage.
///
/// # Errors
/// Returns an error when Argon2 hashing fails.
pub fn hash_api_key(raw: &SecretString) -> Result<String, IssueError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(raw.expose_secret().as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|source| IssueError::Hash(source.to_string()))
}

/// Verify a raw API key against an Argon2 PHC string.
#[must_use]
pub fn verify_api_key(raw: &SecretString, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };

    Argon2::default()
        .verify_password(raw.expose_secret().as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::Duration;
    use jsonwebtoken::{Algorithm, decode_header};
    use secrecy::SecretString;
    use wyrd_auth_verify::{AccessTokenClaims, decode_kid, public_key_from_pem, verify_eddsa};
    use wyrd_spec::actor::Actor;
    use wyrd_spec::authz::Scope;
    use wyrd_spec::ids::CardUid;

    use super::{IssuingKey, hash_api_key, verify_api_key};

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[test]
    fn issue_then_verify_roundtrip() {
        let actor = test_actor();
        let scopes = BTreeSet::from([Scope::CardRead, Scope::CardWrite]);
        let token = issuing_key()
            .issue_access_token(
                "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00",
                actor.clone(),
                scopes.clone(),
                Duration::minutes(5),
            )
            .expect("token issues");
        let claims = verify_eddsa::<AccessTokenClaims>(&token, &public_key(), Some("wyrd"))
            .expect("issued token verifies");

        assert_eq!(claims.sub, "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00");
        assert_eq!(claims.actor, actor);
        assert_eq!(claims.scopes, scopes);
        assert_eq!(claims.iss, "wyrd");
        assert!(
            claims
                .to_principal()
                .has_all([Scope::CardRead, Scope::CardWrite])
        );
    }

    #[test]
    fn issued_token_header_is_eddsa_with_kid() {
        let token = issue_test_token(Duration::minutes(5));
        let header = decode_header(&token).expect("header decodes");

        assert_eq!(
            decode_kid(&token).expect("kid decodes"),
            Some("k1".to_owned())
        );
        assert_eq!(header.alg, Algorithm::EdDSA);
    }

    #[test]
    fn issued_token_sets_iat_and_exp() {
        let ttl = Duration::minutes(5);
        let token = issue_test_token(ttl);
        let claims = verify_eddsa::<AccessTokenClaims>(&token, &public_key(), Some("wyrd"))
            .expect("issued token verifies");
        let delta = claims.exp - claims.iat;

        assert!(claims.exp > claims.iat);
        assert!((295..=305).contains(&delta));
    }

    #[test]
    fn hash_api_key_verifies() {
        let raw = SecretString::from("wyrd_test_key");
        let hash = hash_api_key(&raw).expect("api key hashes");

        assert!(verify_api_key(&raw, &hash));
    }

    #[test]
    fn verify_api_key_rejects_wrong_key() {
        let raw = SecretString::from("wyrd_test_key");
        let wrong = SecretString::from("wyrd_wrong_key");
        let hash = hash_api_key(&raw).expect("api key hashes");

        assert!(!verify_api_key(&wrong, &hash));
    }

    #[test]
    fn verify_api_key_rejects_garbage_hash() {
        let raw = SecretString::from("wyrd_test_key");

        assert!(!verify_api_key(&raw, "not-a-phc-hash"));
    }

    fn issuing_key() -> IssuingKey {
        IssuingKey::from_ed_pem(SecretString::from(PRIVATE_KEY_PEM), "k1", "wyrd")
            .expect("test private key loads")
    }

    fn public_key() -> jsonwebtoken::DecodingKey {
        public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key loads")
    }

    fn issue_test_token(ttl: Duration) -> String {
        issuing_key()
            .issue_access_token(
                "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00",
                test_actor(),
                BTreeSet::from([Scope::CardRead, Scope::CardWrite]),
                ttl,
            )
            .expect("token issues")
    }

    fn test_actor() -> Actor {
        Actor::User {
            id: CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00")
                .expect("test fixture uses a UUIDv7"),
            email: "user@example.com".to_owned(),
        }
    }
}
