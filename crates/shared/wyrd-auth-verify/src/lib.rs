//! Verify-only authentication helpers.

#![deny(missing_docs)]

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use wyrd_runtime::{Principal, PrincipalId, PrincipalKind, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::reference::CardRef;

/// Authentication helper errors.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// JWT verification failed.
    #[error("jwt error")]
    Jwt(#[source] jsonwebtoken::errors::Error),
}

/// Resolved Wyrd access-token claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessTokenClaims {
    /// Ultimate initiator, JWT `sub`.
    pub sub: String,
    /// Current actor whose roles are evaluated for authorization.
    pub principal: PrincipalRef,
    /// Roles assigned to the current actor at issue time.
    pub roles: Vec<RoleRef>,
    /// RFC 8693 actor chain for delegated tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act: Option<Box<ActClaim>>,
    /// Expiry as Unix seconds, JWT `exp`.
    pub exp: usize,
    /// Issued-at as Unix seconds, JWT `iat`.
    pub iat: usize,
    /// Issuer, JWT `iss`.
    pub iss: String,
    /// Token identifier.
    pub jti: String,
}

/// One layer of an RFC 8693 `act` delegation chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActClaim {
    /// Subject at this delegation layer.
    pub sub: String,
    /// Principal at this delegation layer.
    pub principal: PrincipalRef,
    /// Next older delegation layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act: Option<Box<ActClaim>>,
}

/// Wire-side projection of a runtime principal safe to embed in JWT claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrincipalRef {
    /// Stable principal id.
    pub id: PrincipalId,
    /// Principal kind without inline runtime payloads.
    pub kind: PrincipalKindWire,
    /// Tenant isolation key.
    pub tenant_id: DataTenantId,
    /// Bound card reference for Service and Agent principals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_ref: Option<CardRef>,
}

/// Principal kind discriminant used in token claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKindWire {
    /// Human user identity.
    User,
    /// Card-bound service identity.
    Service,
    /// Card-bound agent identity.
    Agent,
}

/// Refresh-token claims for any principal kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshTokenClaims {
    /// Subject id, equal to `principal_id`.
    pub sub: String,
    /// Principal kind.
    pub principal_kind: PrincipalKindWire,
    /// Stable principal id.
    pub principal_id: PrincipalId,
    /// Tenant isolation key.
    pub tenant_id: DataTenantId,
    /// Expiry as Unix seconds, JWT `exp`.
    pub exp: usize,
    /// Issued-at as Unix seconds, JWT `iat`.
    pub iat: usize,
    /// Issuer, JWT `iss`.
    pub iss: String,
    /// Token identifier.
    pub jti: String,
}

impl From<&Principal> for PrincipalRef {
    fn from(principal: &Principal) -> Self {
        let (kind, card_ref) = match &principal.kind {
            PrincipalKind::User => (PrincipalKindWire::User, None),
            PrincipalKind::Service { card_ref } => {
                (PrincipalKindWire::Service, Some(card_ref.clone()))
            }
            PrincipalKind::Agent { card_ref } => (PrincipalKindWire::Agent, Some(card_ref.clone())),
        };
        Self {
            id: principal.id,
            kind,
            tenant_id: principal.tenant_id,
            card_ref,
        }
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
    use jsonwebtoken::{Algorithm, EncodingKey, Header, Validation, encode};
    use wyrd_runtime::{Principal, PrincipalId, PrincipalKind, RoleRef};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::version::VersionBlock;

    use super::{
        AccessTokenClaims, PrincipalKindWire, PrincipalRef, decode_kid, public_key_from_pem,
        verify_eddsa, verify_eddsa_with,
    };

    const PRIVATE_KEY_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[test]
    fn verify_eddsa_roundtrips_access_token() {
        let claims = claims_with_times(now() + 3_600, now());
        let token = encode_eddsa(&claims);
        let decoded = verify_eddsa::<AccessTokenClaims>(&token, &public_key(), Some("wyrd"))
            .expect("valid token verifies");

        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.principal, claims.principal);
        assert_eq!(decoded.roles, claims.roles);
        assert_eq!(decoded.jti, claims.jti);
    }

    #[test]
    fn principal_ref_projects_runtime_principal() {
        let card_ref = card_ref(CardKind::Service);
        let principal = Principal::new(
            principal_id(),
            PrincipalKind::Service {
                card_ref: card_ref.clone(),
            },
            tenant_id(),
            vec![role()],
            wyrd_runtime::PermissionSet::new(),
        );

        let projected = PrincipalRef::from(&principal);

        assert_eq!(projected.id, principal.id);
        assert_eq!(projected.kind, PrincipalKindWire::Service);
        assert_eq!(projected.tenant_id, principal.tenant_id);
        assert_eq!(projected.card_ref, Some(card_ref));
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
        AccessTokenClaims {
            sub: principal_id().to_string(),
            principal: PrincipalRef {
                id: principal_id(),
                kind: PrincipalKindWire::User,
                tenant_id: tenant_id(),
                card_ref: None,
            },
            roles: vec![role()],
            act: None,
            exp,
            iat,
            iss: "wyrd".to_owned(),
            jti: "01K00000000000000000000000".to_owned(),
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

    fn card_ref(kind: CardKind) -> CardRef {
        CardRef {
            kind,
            name: CardName::new("billing").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("prod").expect("static space is valid"),
            uid: None,
        }
    }

    fn principal_id() -> PrincipalId {
        "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"
            .parse()
            .expect("static principal id is valid")
    }

    fn tenant_id() -> DataTenantId {
        "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id is valid")
    }

    fn role() -> RoleRef {
        RoleRef::new("runtime_admin").expect("static role is valid")
    }

    fn now() -> usize {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after unix epoch")
            .as_secs() as usize
    }
}
