//! Server-tier authentication issuance helpers.

#![deny(missing_docs)]

use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::pkcs8::EncodePrivateKey;
use ed25519_dalek::pkcs8::EncodePublicKey;
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use jsonwebtoken::{EncodingKey, Header};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use ulid::Ulid;
use uuid::Uuid;
use wyrd_auth_verify::{
    AccessTokenClaims, ActClaim, Kid, PLATFORM_TOKEN_SCOPE, PlatformAccessTokenClaims,
    RefreshTokenClaims, TokenAudience, TokenPrincipalRef,
};
use wyrd_runtime::{PermissionSet, PrincipalId, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{GATEWAY_CAPTURE_PRINCIPAL, PrincipalKindTag};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::reference::CardRefScope;

pub use wyrd_auth_verify::{MAX_BEARER_TOKEN_BYTES, MAX_DELEGATION_DEPTH};

/// Plaintext access and refresh tokens returned by server-tier issue paths.
#[derive(Debug)]
pub struct IssuedTokenPair {
    /// Access token `jti`.
    pub jti: String,
    /// Signed access token.
    pub access_token: SecretString,
    /// Signed refresh token.
    pub refresh_token: SecretString,
    /// Access token expiry timestamp.
    pub access_expires_at: DateTime<Utc>,
    /// Refresh token expiry timestamp.
    pub refresh_expires_at: DateTime<Utc>,
}

/// Server-tier Ed25519 signing key.
///
/// The raw PEM is stored as a [`SecretString`] so it is zeroized on drop.
/// The [`jsonwebtoken::EncodingKey`] is constructed per signing call and
/// dropped immediately after use, limiting private key material lifetime.
pub struct IssuingKey {
    pem: SecretString,
    kid: Kid,
    issuer: String,
}

/// Authentication issuance errors.
#[derive(Debug, thiserror::Error)]
pub enum IssueError {
    /// EdDSA key loading or signing failed.
    #[error("EdDSA signing failed")]
    Signing(#[source] jsonwebtoken::errors::Error),
    /// The public verification key could not be derived from the signing key.
    #[error("public key derivation failed: {0}")]
    PublicKeyDerivation(String),
    /// Token TTL was zero or negative.
    #[error("ttl must be > 0")]
    InvalidTtl,
    /// Delegation depth would exceed the configured maximum.
    #[error("delegation depth exceeded (max {max})")]
    DelegationDepthExceeded {
        /// Maximum supported delegation depth.
        max: usize,
    },
    /// Key id was malformed.
    #[error("kid must match ^[A-Za-z0-9._-]{{1,64}}$")]
    InvalidKid,
    /// Principal kind cannot hold a tenant access token.
    #[error("principal kind cannot hold a tenant access token")]
    InvalidPrincipalKind,
    /// Principal card reference was missing or mismatched.
    #[error("principal card_ref is missing or mismatched")]
    InvalidCardRef,
    /// Encoded token would exceed the verifier bearer-token size limit.
    #[error("token too large: encoded length {encoded_len} exceeds limit {limit}")]
    CardScopeTooLarge {
        /// Encoded token byte length.
        encoded_len: usize,
        /// Verifier byte limit.
        limit: usize,
    },
}

/// Everything one tenant access token asserts, resolved by the issuance
/// workflow before signing.
///
/// A plain value: the issuance owner loads the current principal and grants,
/// then hands this to [`IssuingKey::issue_access_token`], which owns claim
/// shape, audience, TTL arithmetic, and signing.
#[derive(Debug)]
pub struct AccessGrant {
    /// Subject the token is minted for: the requesting principal for a direct
    /// token, the party being acted for in an RFC 8693 exchange.
    pub principal: TokenPrincipalRef,
    /// Roles assigned to the subject; informational metadata only.
    pub roles: Vec<RoleRef>,
    /// The token's authority: the subject's effective permissions, already
    /// attenuated to the actor's for a delegated token.
    pub permissions: PermissionSet,
    /// Stored credential the token was exchanged from, when one was presented.
    pub credential_id: Option<Uuid>,
    /// RFC 8693 actor chain for a delegated token, outermost layer the current
    /// actor; `None` for a direct token. Attribution only, never authority.
    pub act: Option<Box<ActClaim>>,
    /// Audience the token is issued for.
    pub audience: TokenAudience,
}

impl IssuingKey {
    /// Load an Ed25519 private key from PEM bytes.
    ///
    /// # Errors
    /// Returns an error when the PEM is not a valid EdDSA private key.
    pub fn from_ed_pem(
        pem: SecretString,
        kid: Kid,
        issuer: impl Into<String>,
    ) -> Result<Self, IssueError> {
        EncodingKey::from_ed_pem(pem.expose_secret().as_bytes()).map_err(IssueError::Signing)?;
        Ok(Self {
            pem,
            kid,
            issuer: issuer.into(),
        })
    }

    /// Derive the SPKI public-key PEM paired with this signing key.
    ///
    /// The Ed25519 public key is computed from the private key, so production
    /// assembles its token verifier from a single environment-provided signing
    /// key — the public verification key is derived here, never supplied
    /// separately. Pass the returned PEM to
    /// [`wyrd_auth_verify::public_key_from_pem`] to build the verifier's
    /// decoding key.
    ///
    /// # Errors
    /// Returns an error when the stored PEM cannot be parsed as a PKCS#8 Ed25519
    /// private key or re-encoded as an SPKI public-key PEM.
    pub fn verifying_key_pem(&self) -> Result<String, IssueError> {
        let signing = SigningKey::from_pkcs8_pem(self.pem.expose_secret())
            .map_err(|e| IssueError::PublicKeyDerivation(e.to_string()))?;
        signing
            .verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .map_err(|e| IssueError::PublicKeyDerivation(e.to_string()))
    }

    /// Generate a fresh, random Ed25519 signing key as a PKCS#8 PEM.
    ///
    /// The development profile calls this to provision an *ephemeral* signing
    /// key when none is configured, so auth works on a fresh `cargo run` without
    /// the operator minting a key first. The key lives only for the process
    /// lifetime — tokens it signs do not survive a restart — and must never be
    /// used in staging or production, which provision a stable key and fail
    /// closed without one. The returned PEM is fed through the same assembly
    /// path as a configured key.
    ///
    /// # Errors
    /// Returns an error when the generated key cannot be encoded as a PKCS#8
    /// PEM (not expected for a freshly generated key).
    pub fn generate_ephemeral_pem() -> Result<SecretString, IssueError> {
        let signing = SigningKey::from_bytes(&rand::random());
        let pem = signing
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|e| IssueError::PublicKeyDerivation(e.to_string()))?;
        Ok(SecretString::from(pem.to_string()))
    }

    /// Mint a tenant access token for one resolved grant.
    ///
    /// The public tenant access-token path. It refuses the reserved
    /// [`GATEWAY_CAPTURE_PRINCIPAL`], which gateway capture stamps on its
    /// server-internal writes and which never appears in a token, then signs
    /// through [`Self::sign_access_token`].
    ///
    /// # Errors
    /// Returns [`IssueError::InvalidPrincipalKind`] for the reserved capture
    /// identity, and otherwise every error of [`Self::sign_access_token`].
    pub fn issue_access_token(
        &self,
        grant: AccessGrant,
        ttl: Duration,
    ) -> Result<String, IssueError> {
        if grant.principal.id == GATEWAY_CAPTURE_PRINCIPAL {
            return Err(IssueError::InvalidPrincipalKind);
        }
        self.sign_access_token(grant, ttl)
    }

    /// Build, without signing, the claims a tenant access token for `grant`
    /// would carry.
    ///
    /// The same validation and claim shaping as [`Self::issue_access_token`],
    /// for a caller that authenticates a durable credential on every request
    /// and needs the verified principal that token would yield, but no token.
    ///
    /// # Errors
    /// As [`Self::issue_access_token`], except the encoded-size and signing
    /// errors, since nothing is encoded.
    pub fn access_claims(
        &self,
        grant: AccessGrant,
        ttl: Duration,
    ) -> Result<AccessTokenClaims, IssueError> {
        if grant.principal.id == GATEWAY_CAPTURE_PRINCIPAL {
            return Err(IssueError::InvalidPrincipalKind);
        }
        self.build_access_claims(grant, ttl)
    }

    /// Sign one tenant access token for a resolved grant.
    ///
    /// Encodes [`Self::build_access_claims`] and requires the token to fit
    /// the verifier's bearer-size limit.
    ///
    /// # Errors
    /// Every [`Self::build_access_claims`] error,
    /// [`IssueError::CardScopeTooLarge`] when the encoded token exceeds
    /// [`MAX_BEARER_TOKEN_BYTES`], and [`IssueError::Signing`] when signing
    /// fails.
    fn sign_access_token(&self, grant: AccessGrant, ttl: Duration) -> Result<String, IssueError> {
        let token = self.encode(&self.build_access_claims(grant, ttl)?)?;
        if token.len() > MAX_BEARER_TOKEN_BYTES {
            return Err(IssueError::CardScopeTooLarge {
                encoded_len: token.len(),
                limit: MAX_BEARER_TOKEN_BYTES,
            });
        }
        Ok(token)
    }

    /// Shape the claims of one tenant access token for a resolved grant.
    ///
    /// The single tenant access-token claim builder. It validates that the
    /// principal's kind and Card binding agree, re-derives a Card-bound
    /// principal's scope from its root so a caller cannot widen it with extra
    /// members, bounds a delegated grant's RFC 8693 `act` chain, and stamps
    /// `sub` as the subject, the grant's audience, its `permissions` authority
    /// snapshot, informational `roles`, and the credential attribution. The
    /// encoded token must fit the verifier's bearer-size limit.
    ///
    /// # Errors
    /// Returns [`IssueError::InvalidPrincipalKind`] for a platform-scope kind
    /// or when the tenant SYSTEM principal is the subject or any delegation
    /// layer, because it never appears in a token,
    /// [`IssueError::InvalidCardRef`] when kind and Card binding disagree,
    /// [`IssueError::DelegationDepthExceeded`] when the resulting chain would
    /// exceed [`MAX_DELEGATION_DEPTH`], and [`IssueError::InvalidTtl`] for a
    /// non-positive TTL.
    #[tracing::instrument(
        level = "debug",
        skip(self, grant),
        fields(
            kid = %self.kid,
            principal_id = %grant.principal.id,
            principal_kind = ?grant.principal.kind,
            tenant_id = %grant.principal.tenant_id,
            delegation_depth = tracing::field::Empty,
            jti = tracing::field::Empty,
        ),
        err,
    )]
    fn build_access_claims(
        &self,
        grant: AccessGrant,
        ttl: Duration,
    ) -> Result<AccessTokenClaims, IssueError> {
        let AccessGrant {
            mut principal,
            roles,
            permissions,
            credential_id,
            act,
            audience,
        } = grant;
        validate_principal_ref(&principal)?;
        if std::iter::successors(act.as_deref(), |claim| claim.act.as_deref())
            .any(|claim| claim.principal.kind == PrincipalKindTag::System)
        {
            return Err(IssueError::InvalidPrincipalKind);
        }
        if let Some(card_ref) = &principal.card_ref {
            principal.card_ref_scope = CardRefScope::from_root_and_members(
                card_ref,
                principal.card_ref_scope.as_slice().iter().cloned(),
            );
        }
        let depth = act_depth(act.as_deref());
        tracing::Span::current().record("delegation_depth", depth);
        if depth > MAX_DELEGATION_DEPTH {
            return Err(IssueError::DelegationDepthExceeded {
                max: MAX_DELEGATION_DEPTH,
            });
        }
        let sub = principal.id.to_string();
        let (iat, exp) = timestamps(Utc::now(), ttl)?;
        let jti = new_jti();
        tracing::Span::current().record("jti", &jti);
        Ok(AccessTokenClaims {
            sub,
            principal,
            roles,
            permissions,
            act,
            aud: audience.as_str().to_owned(),
            exp,
            iat,
            iss: self.issuer.clone(),
            jti,
            cid: credential_id.map(|id| id.to_string()),
        })
    }

    /// Mint a refresh token for a principal that holds a session, from an
    /// explicit instant.
    ///
    /// The tenant SYSTEM principal never appears in a token, so a refresh
    /// token for it is refused.
    ///
    /// `issued_at` is supplied by the caller rather than sampled here so that
    /// the signed `exp` and the durable refresh row that records it are derived
    /// from one clock. Callers that persist the token pass the `PostgreSQL`
    /// issuance instant of their own transaction. The reserved capture
    /// identity is refused before signing: it never appears in a token.
    ///
    /// # Errors
    /// Returns [`IssueError::InvalidPrincipalKind`] for the SYSTEM kind or
    /// [`GATEWAY_CAPTURE_PRINCIPAL`], and an error when TTL is invalid or signing fails.
    #[tracing::instrument(
        level = "debug",
        skip(self),
        fields(
            kid = %self.kid,
            principal_kind = ?principal_kind,
            principal_id = %principal_id,
            tenant_id = %tenant_id,
            jti = tracing::field::Empty,
        ),
        err,
    )]
    pub fn issue_refresh_token(
        &self,
        principal_kind: PrincipalKindTag,
        principal_id: PrincipalId,
        tenant_id: DataTenantId,
        issued_at: DateTime<Utc>,
        ttl: Duration,
    ) -> Result<String, IssueError> {
        if principal_kind == PrincipalKindTag::System || principal_id == GATEWAY_CAPTURE_PRINCIPAL {
            return Err(IssueError::InvalidPrincipalKind);
        }
        let (iat, exp) = timestamps(issued_at, ttl)?;
        let jti = new_jti();
        tracing::Span::current().record("jti", &jti);
        let claims = RefreshTokenClaims {
            sub: principal_id.to_string(),
            principal_kind,
            principal_id,
            tenant_id,
            exp,
            iat,
            iss: self.issuer.clone(),
            jti,
        };
        let token = self.encode(&claims)?;
        if token.len() > MAX_BEARER_TOKEN_BYTES {
            return Err(IssueError::CardScopeTooLarge {
                encoded_len: token.len(),
                limit: MAX_BEARER_TOKEN_BYTES,
            });
        }
        Ok(token)
    }

    /// Mint a platform-scope access token.
    ///
    /// The platform plane has no tenant, so this never goes through the
    /// tenant-scoped claim shape: the minted token cannot name a tenant, carries
    /// no roles, and carries no delegation chain. Authority is resolved from the
    /// principal's grant at verification time rather than frozen here.
    ///
    /// # Errors
    /// Returns [`IssueError::InvalidPrincipalKind`] for the reserved
    /// [`GATEWAY_CAPTURE_PRINCIPAL`], [`IssueError::Signing`] when the token
    /// cannot be signed, and a timestamp error when the issued-at or expiry
    /// cannot be computed.
    pub fn issue_platform_access_token(
        &self,
        principal_id: PrincipalId,
        credential_id: Option<Uuid>,
        ttl: Duration,
    ) -> Result<String, IssueError> {
        if principal_id == GATEWAY_CAPTURE_PRINCIPAL {
            return Err(IssueError::InvalidPrincipalKind);
        }
        let (iat, exp) = timestamps(Utc::now(), ttl)?;
        let claims = PlatformAccessTokenClaims {
            sub: principal_id.to_string(),
            cid: credential_id.map(|id| id.to_string()),
            scope: PLATFORM_TOKEN_SCOPE.to_owned(),
            exp,
            iat,
            iss: self.issuer.clone(),
            jti: new_jti(),
        };
        self.encode(&claims)
    }

    fn encode<T: serde::Serialize>(&self, claims: &T) -> Result<String, IssueError> {
        let encoding = EncodingKey::from_ed_pem(self.pem.expose_secret().as_bytes())
            .map_err(IssueError::Signing)?;
        let mut header = Header::new(jsonwebtoken::Algorithm::EdDSA);
        header.kid = Some(self.kid.to_string());

        jsonwebtoken::encode(&header, claims, &encoding).map_err(IssueError::Signing)
    }
}

/// Hash a server-generated bearer secret for at-rest storage and lookup.
///
/// Every stored Wyrd secret (tenant API keys, platform credentials, refresh
/// tokens) is a long random value the server minted, so a fast SHA-256 is the
/// verifier: there is nothing low-entropy for a slow password hash to protect,
/// and verification stays cheap enough to run on every request. Returns
/// lowercase hex.
#[must_use]
pub fn hash_secret(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}

/// Whether `raw` hashes to `stored`, compared in constant time.
///
/// A stored value in any other format, such as a retired Argon2 verifier,
/// never matches.
#[must_use]
pub fn secret_matches(raw: &str, stored: &str) -> bool {
    hash_secret(raw).as_bytes().ct_eq(stored.as_bytes()).into()
}

fn timestamps(issued_at: DateTime<Utc>, ttl: Duration) -> Result<(usize, usize), IssueError> {
    if ttl <= Duration::zero() {
        return Err(IssueError::InvalidTtl);
    }

    let iat: usize = issued_at
        .timestamp()
        .try_into()
        .map_err(|_| IssueError::InvalidTtl)?;
    let exp: usize = (issued_at + ttl)
        .timestamp()
        .try_into()
        .map_err(|_| IssueError::InvalidTtl)?;
    Ok((iat, exp))
}

fn new_jti() -> String {
    Ulid::new().to_string()
}

/// Count the layers of an `act` chain.
fn act_depth(act: Option<&ActClaim>) -> usize {
    let Some(act) = act else {
        return 0;
    };
    1 + act_depth(act.act.as_deref())
}

/// Reject a principal projection whose kind and Card binding disagree.
///
/// Card binding is a property of a machine principal: an agent must carry an
/// Agent card, a service may carry a Service card or none at all, and an
/// administrative or human principal must carry none. A platform-scope kind
/// and the tenant SYSTEM principal, an attribution identity only, are rejected
/// outright because no tenant-scope token is issued for either.
///
/// # Errors
/// Returns [`IssueError::InvalidCardRef`] when the kind and Card binding
/// disagree, or when the kind is platform-scoped.
fn validate_principal_ref(principal: &TokenPrincipalRef) -> Result<(), IssueError> {
    match (
        principal.kind,
        principal.card_ref.as_ref().map(|card_ref| &card_ref.kind),
    ) {
        // A platform-scope principal is never issued a tenant-scope token. The
        // platform plane has its own credential path, so a token request
        // carrying this kind is malformed rather than merely unauthorized.
        (PrincipalKindTag::GlobalAdmin | PrincipalKindTag::System, _) => {
            Err(IssueError::InvalidPrincipalKind)
        }
        (PrincipalKindTag::TenantAdmin | PrincipalKindTag::User, None)
        | (PrincipalKindTag::Service, Some(CardKind::Service) | None)
        | (PrincipalKindTag::Agent, Some(CardKind::Agent)) => Ok(()),
        (PrincipalKindTag::TenantAdmin | PrincipalKindTag::User, Some(_)) => {
            Err(IssueError::InvalidCardRef)
        }
        (PrincipalKindTag::Service | PrincipalKindTag::Agent, _) => Err(IssueError::InvalidCardRef),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use jsonwebtoken::{Algorithm, decode_header};
    use secrecy::{ExposeSecret, SecretString};
    use wyrd_auth_verify::{
        AccessTokenClaims, ActClaim, PLATFORM_TOKEN_SCOPE, PlatformAccessTokenClaims,
        RefreshTokenClaims, TokenAudience, TokenPrincipalRef, decode_kid, public_key_from_pem,
        verify_eddsa,
    };
    use wyrd_runtime::{Permission, PermissionSet, PrincipalId, RoleRef};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{GATEWAY_CAPTURE_PRINCIPAL, PrincipalKindTag};
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};

    use super::{
        AccessGrant, IssueError, IssuingKey, Kid, MAX_BEARER_TOKEN_BYTES, MAX_DELEGATION_DEPTH,
        hash_secret, secret_matches,
    };

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    /// A direct (undelegated) user grant mints a token whose `sub`, principal,
    /// roles, permission snapshot, default `wyrd` audience, issuer, and ULID-shaped
    /// `jti` all come from the grant, with no Card binding, credential id, or
    /// `act` chain.
    ///
    /// # Panics
    ///
    /// Panics when issuance or verification fails or any verified claim differs
    /// from the grant.
    #[test]
    fn issue_access_token_uses_principal_roles_permissions_audience_and_jti_shape() {
        let token = issuing_key()
            .issue_access_token(
                grant(user_principal(), vec![role("editor")]),
                Duration::minutes(5),
            )
            .expect("token issues");
        let claims = verify_access_token(&token);

        assert_eq!(
            claims.sub,
            principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").to_string()
        );
        assert_eq!(claims.principal.kind, PrincipalKindTag::User);
        assert_eq!(claims.principal.card_ref, None);
        assert_eq!(claims.roles, vec![role("editor")]);
        assert_eq!(claims.permissions, permissions());
        assert_eq!(claims.aud, "wyrd");
        assert_eq!(claims.cid, None);
        assert_eq!(claims.act, None);
        assert_eq!(claims.iss, "wyrd");
        assert_eq!(claims.jti.len(), 26);
    }

    /// Proves no public issue path mints the reserved capture identity, in
    /// any principal kind or delegation shape.
    ///
    /// # Panics
    ///
    /// Panics when any reserved shape yields a token or a different error.
    #[test]
    fn issue_access_token_refuses_the_reserved_capture_identity() {
        let key = issuing_key();
        let caller = ActClaim {
            sub: user_principal().id.to_string(),
            principal: user_principal(),
            act: None,
        };
        for (kind, act) in [
            (PrincipalKindTag::User, None),
            (PrincipalKindTag::Service, None),
            (PrincipalKindTag::Service, Some(Box::new(caller))),
        ] {
            let reserved = TokenPrincipalRef {
                id: GATEWAY_CAPTURE_PRINCIPAL,
                kind,
                ..user_principal()
            };
            assert!(matches!(
                key.issue_access_token(
                    AccessGrant {
                        act,
                        ..grant(reserved, vec![role("editor")])
                    },
                    Duration::minutes(5),
                ),
                Err(IssueError::InvalidPrincipalKind)
            ));
        }
        assert!(matches!(
            key.issue_platform_access_token(GATEWAY_CAPTURE_PRINCIPAL, None, Duration::minutes(5)),
            Err(IssueError::InvalidPrincipalKind)
        ));
    }

    /// Build a Card-bound principal reference for the issuance tests.
    ///
    /// The kind and the Card kind are independent parameters so a test can
    /// present the mismatched pair the issuer must reject.
    fn card_principal(id: &str, kind: PrincipalKindTag, card_kind: CardKind) -> TokenPrincipalRef {
        TokenPrincipalRef {
            id: principal_id(id),
            kind,
            tenant_id: tenant_id(),
            card_ref: Some(card_ref(card_kind)),
            card_ref_scope: CardRefScope::default(),
        }
    }

    /// A Service token carries its Card and the emit scope rooted at it.
    ///
    /// The `card_ref_scope` claim is what later authorizes an observation, so
    /// the issuer must sign it rather than leave a consumer to re-derive it.
    #[test]
    fn issue_access_token_forces_service_card_ref() {
        let card_ref = card_ref(CardKind::Service);
        let token = issuing_key()
            .issue_access_token(
                grant(
                    card_principal(
                        "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02",
                        PrincipalKindTag::Service,
                        CardKind::Service,
                    ),
                    vec![role("service")],
                ),
                Duration::minutes(5),
            )
            .expect("token issues");
        let claims = verify_access_token(&token);

        assert_eq!(claims.principal.kind, PrincipalKindTag::Service);
        assert_eq!(claims.principal.card_ref, Some(card_ref.clone()));
        assert_eq!(
            claims.principal.card_ref_scope,
            CardRefScope::own(&card_ref)
        );
        assert_eq!(claims.sub, claims.principal.id.to_string());
    }

    /// A Service principal may not be signed holding an Agent Card.
    ///
    /// Kind and Card kind are separate inputs, so nothing but this check stops
    /// a caller from minting a token whose claimed kind and Card disagree.
    #[test]
    fn issue_access_token_rejects_agent_card_ref_for_a_service() {
        let result = issuing_key().issue_access_token(
            grant(
                card_principal(
                    "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02",
                    PrincipalKindTag::Service,
                    CardKind::Agent,
                ),
                vec![role("service")],
            ),
            Duration::minutes(5),
        );

        assert!(matches!(result, Err(IssueError::InvalidCardRef)));
    }

    /// An Agent token carries its Card and the emit scope rooted at it.
    ///
    /// The Agent half of the same contract: an Agent is always Card-bound, so
    /// a successful issuance always signs a `card_ref` and its own scope.
    #[test]
    fn issue_access_token_forces_agent_card_ref() {
        let card_ref = card_ref(CardKind::Agent);
        let token = issuing_key()
            .issue_access_token(
                grant(
                    card_principal(
                        "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b04",
                        PrincipalKindTag::Agent,
                        CardKind::Agent,
                    ),
                    vec![role("workload")],
                ),
                Duration::minutes(5),
            )
            .expect("token issues");
        let claims = verify_access_token(&token);

        assert_eq!(claims.principal.kind, PrincipalKindTag::Agent);
        assert_eq!(claims.principal.card_ref, Some(card_ref.clone()));
        assert_eq!(
            claims.principal.card_ref_scope,
            CardRefScope::own(&card_ref)
        );
        assert_eq!(claims.sub, claims.principal.id.to_string());
    }

    /// An Agent principal may not be signed holding a Service Card.
    ///
    /// The mirror of the Service rejection: the check is symmetric, so neither
    /// kind can borrow the other's Card.
    #[test]
    fn issue_access_token_rejects_service_card_ref_for_an_agent() {
        let result = issuing_key().issue_access_token(
            grant(
                card_principal(
                    "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b04",
                    PrincipalKindTag::Agent,
                    CardKind::Service,
                ),
                vec![role("workload")],
            ),
            Duration::minutes(5),
        );

        assert!(matches!(result, Err(IssueError::InvalidCardRef)));
    }

    #[test]
    fn issue_access_token_rejects_user_with_card_ref() {
        let result = issuing_key().issue_access_token(
            grant(
                TokenPrincipalRef {
                    card_ref: Some(card_ref(CardKind::Service)),
                    ..user_principal()
                },
                vec![role("editor")],
            ),
            Duration::minutes(5),
        );

        assert!(matches!(result, Err(IssueError::InvalidCardRef)));
    }

    /// A platform-scope kind can never hold a tenant access token.
    #[test]
    fn issue_access_token_rejects_platform_kind() {
        let result = issuing_key().issue_access_token(
            grant(
                TokenPrincipalRef {
                    kind: PrincipalKindTag::GlobalAdmin,
                    ..user_principal()
                },
                Vec::new(),
            ),
            Duration::minutes(5),
        );

        assert!(matches!(result, Err(IssueError::InvalidPrincipalKind)));
    }

    /// A grant whose encoded token would exceed the verifier's bearer limit is
    /// refused at issuance rather than minted unverifiable.
    #[test]
    fn issue_access_token_rejects_token_over_bearer_limit() {
        let roles = (0..800)
            .map(|index| role(&format!("role_{index:04}")))
            .collect();

        let result =
            issuing_key().issue_access_token(grant(user_principal(), roles), Duration::minutes(5));

        assert!(matches!(
            result,
            Err(IssueError::CardScopeTooLarge {
                limit: MAX_BEARER_TOKEN_BYTES,
                ..
            })
        ));
    }

    /// The credential attribution rides in `cid`.
    #[test]
    fn issue_access_token_carries_credential_id() {
        let credential = uuid::Uuid::now_v7();
        let token = issuing_key()
            .issue_access_token(
                AccessGrant {
                    credential_id: Some(credential),
                    ..grant(service_principal(), vec![role("service")])
                },
                Duration::minutes(5),
            )
            .expect("token issues");

        assert_eq!(
            verify_access_token(&token).cid,
            Some(credential.to_string())
        );
    }

    #[test]
    fn issued_token_header_is_eddsa_with_kid() {
        let token = issue_user_test_token(Duration::minutes(5));
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
        let token = issue_user_test_token(ttl);
        let claims = verify_access_token(&token);
        let delta = claims.exp - claims.iat;

        assert!(claims.exp > claims.iat);
        assert!((295..=305).contains(&delta));
    }

    #[test]
    fn issue_access_token_rejects_zero_ttl() {
        let result = issuing_key().issue_access_token(
            grant(user_principal(), vec![role("editor")]),
            Duration::zero(),
        );
        assert!(matches!(result, Err(IssueError::InvalidTtl)));
    }

    #[test]
    fn issue_access_token_rejects_negative_ttl() {
        let result = issuing_key().issue_access_token(
            grant(user_principal(), vec![role("editor")]),
            Duration::minutes(-1),
        );
        assert!(matches!(result, Err(IssueError::InvalidTtl)));
    }

    /// A delegated grant keeps the subject as `sub` and principal, names the
    /// actor in the outermost `act`, and carries the requested audience.
    ///
    /// # Panics
    ///
    /// Panics when the delegated token fails to issue or verify, the `act` chain
    /// is missing, or the subject, audience, or actor claims differ from the
    /// grant.
    #[test]
    fn issue_access_token_delegated_names_subject_and_outer_actor() {
        let actor = agent_principal();
        let delegated_token = issuing_key()
            .issue_access_token(
                AccessGrant {
                    act: Some(Box::new(ActClaim {
                        sub: actor.id.to_string(),
                        principal: actor.clone(),
                        act: None,
                    })),
                    audience: TokenAudience::Bifrost,
                    ..grant(user_principal(), vec![role("viewer")])
                },
                Duration::minutes(5),
            )
            .expect("delegated token issues");
        let claims = verify_access_token(&delegated_token);
        let act = claims.act.as_ref().expect("act chain is present");

        assert_eq!(claims.sub, user_principal().id.to_string());
        assert_eq!(claims.principal, user_principal());
        assert_eq!(claims.aud, "bifrost");
        assert_eq!(act.sub, actor.id.to_string());
        assert_eq!(act.principal, actor);
        assert_eq!(act.act, None);
    }

    /// An `act` chain one hop deeper than [`MAX_DELEGATION_DEPTH`] is refused
    /// before signing.
    ///
    /// # Panics
    ///
    /// Panics when issuance does not fail with
    /// [`IssueError::DelegationDepthExceeded`] naming the maximum.
    #[test]
    fn issue_access_token_delegated_rejects_depth_over_max() {
        let result = issuing_key().issue_access_token(
            AccessGrant {
                act: Some(Box::new(act_chain(MAX_DELEGATION_DEPTH + 1))),
                ..grant(user_principal(), vec![role("viewer")])
            },
            Duration::minutes(5),
        );

        assert!(matches!(
            result,
            Err(IssueError::DelegationDepthExceeded {
                max: MAX_DELEGATION_DEPTH
            })
        ));
    }

    /// An `act` chain exactly [`MAX_DELEGATION_DEPTH`] hops deep is the
    /// inclusive boundary and still issues.
    ///
    /// # Panics
    ///
    /// Panics when issuance of the at-limit chain fails.
    #[test]
    fn issue_access_token_delegated_accepts_depth_at_max() {
        let result = issuing_key().issue_access_token(
            AccessGrant {
                act: Some(Box::new(act_chain(MAX_DELEGATION_DEPTH))),
                ..grant(user_principal(), vec![role("viewer")])
            },
            Duration::minutes(5),
        );

        assert!(result.is_ok());
    }

    /// The tenant SYSTEM principal never appears in a token: the only signing
    /// path refuses it as the subject in any shape, as a delegation layer, and
    /// as a refresh subject.
    ///
    /// # Panics
    /// Panics when any grant naming the SYSTEM principal is signed.
    #[test]
    fn issue_access_token_refuses_every_system_grant() {
        let system = system_principal();
        let system_actor = || ActClaim {
            sub: system.id.to_string(),
            principal: system.clone(),
            act: None,
        };
        let cases: Vec<(&str, AccessGrant)> = vec![
            (
                "direct result write",
                AccessGrant {
                    permissions: PermissionSet::from_iter([Permission::bifrost_record_write()]),
                    ..grant(system.clone(), Vec::new())
                },
            ),
            (
                "card bound",
                grant(
                    TokenPrincipalRef {
                        card_ref: Some(card_ref(CardKind::Verifier)),
                        ..system.clone()
                    },
                    Vec::new(),
                ),
            ),
            ("role", grant(system.clone(), vec![role("editor")])),
            (
                "credential",
                AccessGrant {
                    credential_id: Some(uuid::Uuid::now_v7()),
                    ..grant(system.clone(), Vec::new())
                },
            ),
            (
                "delegated system token",
                AccessGrant {
                    act: Some(Box::new(ActClaim {
                        sub: user_principal().id.to_string(),
                        principal: user_principal(),
                        act: None,
                    })),
                    ..grant(system.clone(), Vec::new())
                },
            ),
            (
                "system as delegation actor",
                AccessGrant {
                    act: Some(Box::new(system_actor())),
                    ..grant(agent_principal(), Vec::new())
                },
            ),
        ];

        for (label, grant) in cases {
            let result = issuing_key().issue_access_token(grant, Duration::minutes(5));
            assert!(
                matches!(result, Err(IssueError::InvalidPrincipalKind)),
                "{label}: {result:?}"
            );
        }

        let refresh = issuing_key().issue_refresh_token(
            PrincipalKindTag::System,
            system.id,
            tenant_id(),
            Utc::now(),
            Duration::days(30),
        );
        assert!(
            matches!(refresh, Err(IssueError::InvalidPrincipalKind)),
            "{refresh:?}"
        );
    }

    /// A SYSTEM principal projection scoped to one UID-bearing Verifier, the
    /// shape the server once signed for result writes.
    ///
    /// # Panics
    /// Panics when a static identity component is invalid.
    fn system_principal() -> TokenPrincipalRef {
        let verifier = CardRef {
            uid: Some(
                wyrd_spec::ids::CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11")
                    .expect("static uid is valid"),
            ),
            ..card_ref(CardKind::Verifier)
        };
        TokenPrincipalRef {
            id: principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b04"),
            kind: PrincipalKindTag::System,
            tenant_id: tenant_id(),
            card_ref: None,
            card_ref_scope: CardRefScope::own(&verifier),
        }
    }

    #[test]
    fn issue_refresh_token_is_principal_generic() {
        let token = issuing_key()
            .issue_refresh_token(
                PrincipalKindTag::Service,
                principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02"),
                tenant_id(),
                Utc::now(),
                Duration::days(30),
            )
            .expect("refresh token issues");
        let claims = verify_eddsa::<RefreshTokenClaims>(&token, &public_key(), Some("wyrd"))
            .expect("refresh token verifies");

        assert_eq!(claims.principal_kind, PrincipalKindTag::Service);
        assert_eq!(claims.sub, claims.principal_id.to_string());
        assert_eq!(claims.tenant_id, tenant_id());
        assert_eq!(claims.jti.len(), 26);
    }

    /// Refresh issuance refuses the reserved capture identity under every
    /// kind, so no long-lived credential can ever be minted for it.
    ///
    /// # Panics
    ///
    /// Panics when any reserved shape yields a token or a different error.
    #[test]
    fn issue_refresh_token_refuses_the_reserved_capture_identity() {
        for kind in [PrincipalKindTag::Service, PrincipalKindTag::User] {
            let result = issuing_key().issue_refresh_token(
                kind,
                GATEWAY_CAPTURE_PRINCIPAL,
                tenant_id(),
                Utc::now(),
                Duration::days(30),
            );
            assert!(matches!(result, Err(IssueError::InvalidPrincipalKind)));
        }
    }

    #[test]
    fn kid_validation_accepts_locked_shape() {
        let kid = Kid::new("abc.DEF_123-4").expect("kid is valid");

        assert_eq!(kid.as_str(), "abc.DEF_123-4");
    }

    #[test]
    fn kid_validation_rejects_empty_space_and_too_long() {
        assert!(Kid::new("").is_err());
        assert!(Kid::new("bad kid").is_err());
        assert!(Kid::new("a".repeat(65)).is_err());
    }

    /// A secret matches only its own SHA-256 verifier; a wrong secret or a
    /// retired Argon2 verifier never does.
    #[test]
    fn secret_matches_only_its_own_sha256_verifier() {
        let hash = hash_secret("wyrd_test_key");

        assert_eq!(hash.len(), 64);
        assert!(secret_matches("wyrd_test_key", &hash));
        assert!(!secret_matches("wyrd_wrong_key", &hash));
        assert!(!secret_matches(
            "wyrd_test_key",
            "$argon2id$v=19$m=19456,t=2,p=1$x$y"
        ));
    }

    #[test]
    fn no_sqlx_in_crate() {
        assert_no_sqlx_in_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    }

    #[test]
    fn verifying_key_pem_derives_a_key_that_verifies_issued_tokens() {
        let key = issuing_key();
        let derived_pem = key.verifying_key_pem().expect("public key derives");
        let decoding =
            public_key_from_pem(derived_pem.as_bytes()).expect("derived public key loads");

        let token = key
            .issue_access_token(
                grant(user_principal(), vec![role("editor")]),
                Duration::minutes(5),
            )
            .expect("token issues");
        let claims = verify_eddsa::<AccessTokenClaims>(&token, &decoding, Some("wyrd"))
            .expect("token verifies against the derived public key");

        assert_eq!(claims.principal.kind, PrincipalKindTag::User);
    }

    #[test]
    fn generated_ephemeral_key_mints_tokens_verifiable_by_its_derived_key() {
        let pem = IssuingKey::generate_ephemeral_pem().expect("ephemeral key generates");
        let key = IssuingKey::from_ed_pem(pem, Kid::new("k1").expect("kid is valid"), "wyrd")
            .expect("generated key loads as a signing key");
        let decoding = public_key_from_pem(
            key.verifying_key_pem()
                .expect("public key derives")
                .as_bytes(),
        )
        .expect("derived public key loads");

        let token = key
            .issue_access_token(
                grant(user_principal(), vec![role("editor")]),
                Duration::minutes(5),
            )
            .expect("token issues");
        let claims = verify_eddsa::<AccessTokenClaims>(&token, &decoding, Some("wyrd"))
            .expect("token verifies against the generated key's derived public key");

        assert_eq!(claims.principal.kind, PrincipalKindTag::User);
    }

    #[test]
    fn generated_ephemeral_keys_are_distinct() {
        let a = IssuingKey::generate_ephemeral_pem().expect("first key generates");
        let b = IssuingKey::generate_ephemeral_pem().expect("second key generates");
        assert_ne!(a.expose_secret(), b.expose_secret());
    }

    #[test]
    fn from_ed_pem_rejects_invalid_pem() {
        let result = IssuingKey::from_ed_pem(
            SecretString::from("not a pem"),
            Kid::new("k1").expect("kid is valid"),
            "wyrd",
        );
        assert!(matches!(result, Err(IssueError::Signing(_))));
    }

    /// A platform token verifies as platform claims and carries the scope
    /// marker, the principal, and the credential that minted it.
    #[test]
    fn platform_token_carries_scope_principal_and_credential() {
        let principal = PrincipalId::new(uuid::Uuid::now_v7());
        let credential = uuid::Uuid::now_v7();

        let token = issuing_key()
            .issue_platform_access_token(principal, Some(credential), Duration::minutes(15))
            .expect("platform token issues");
        let claims: PlatformAccessTokenClaims =
            verify_eddsa(&token, &public_key(), Some("wyrd")).expect("platform token verifies");

        assert_eq!(claims.scope, PLATFORM_TOKEN_SCOPE);
        assert_eq!(claims.sub, principal.to_string());
        assert_eq!(claims.cid, Some(credential.to_string()));
    }

    /// A tenant token cannot be read as platform claims, and a platform token
    /// cannot be read as tenant claims. Neither plane can replay the other's
    /// token, because the two claim shapes are structurally incompatible rather
    /// than merely differently populated.
    #[test]
    fn the_two_planes_cannot_replay_each_others_tokens() {
        let tenant_token = issue_user_test_token(Duration::minutes(15));
        let platform_token = issuing_key()
            .issue_platform_access_token(
                PrincipalId::new(uuid::Uuid::now_v7()),
                Some(uuid::Uuid::now_v7()),
                Duration::minutes(15),
            )
            .expect("platform token issues");

        assert!(
            verify_eddsa::<PlatformAccessTokenClaims>(&tenant_token, &public_key(), Some("wyrd"))
                .is_err(),
            "a tenant token must not verify as a platform session"
        );
        assert!(
            verify_eddsa::<AccessTokenClaims>(&platform_token, &public_key(), Some("wyrd"))
                .is_err(),
            "a platform token must not verify as a tenant token"
        );
    }

    fn issuing_key() -> IssuingKey {
        IssuingKey::from_ed_pem(
            SecretString::from(PRIVATE_KEY_PEM),
            Kid::new("k1").expect("kid is valid"),
            "wyrd",
        )
        .expect("test private key loads")
    }

    /// A direct, credential-free grant carrying the test permission set.
    fn grant(principal: TokenPrincipalRef, roles: Vec<RoleRef>) -> AccessGrant {
        AccessGrant {
            principal,
            roles,
            permissions: permissions(),
            credential_id: None,
            act: None,
            audience: TokenAudience::Wyrd,
        }
    }

    /// The permission set every test grant carries.
    fn permissions() -> PermissionSet {
        PermissionSet::from_iter([Permission::card_read()])
    }

    fn public_key() -> jsonwebtoken::DecodingKey {
        public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key loads")
    }

    fn issue_user_test_token(ttl: Duration) -> String {
        issuing_key()
            .issue_access_token(grant(user_principal(), vec![role("editor")]), ttl)
            .expect("token issues")
    }

    fn verify_access_token(token: &str) -> AccessTokenClaims {
        verify_eddsa::<AccessTokenClaims>(token, &public_key(), Some("wyrd"))
            .expect("issued token verifies")
    }

    fn user_principal() -> TokenPrincipalRef {
        TokenPrincipalRef {
            id: principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"),
            kind: PrincipalKindTag::User,
            tenant_id: tenant_id(),
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
        }
    }

    fn service_principal() -> TokenPrincipalRef {
        let card_ref = card_ref(CardKind::Service);
        TokenPrincipalRef {
            id: principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02"),
            kind: PrincipalKindTag::Service,
            tenant_id: tenant_id(),
            card_ref: Some(card_ref.clone()),
            card_ref_scope: CardRefScope::own(&card_ref),
        }
    }

    fn agent_principal() -> TokenPrincipalRef {
        let card_ref = card_ref(CardKind::Agent);
        TokenPrincipalRef {
            id: principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b03"),
            kind: PrincipalKindTag::Agent,
            tenant_id: tenant_id(),
            card_ref: Some(card_ref.clone()),
            card_ref_scope: CardRefScope::own(&card_ref),
        }
    }

    fn act_chain(depth: usize) -> ActClaim {
        let next = if depth > 1 {
            Some(Box::new(act_chain(depth - 1)))
        } else {
            None
        };
        ActClaim {
            sub: user_principal().id.to_string(),
            principal: user_principal(),
            act: next,
        }
    }

    fn card_ref(kind: CardKind) -> CardRef {
        CardRef {
            kind,
            name: CardName::new("billing").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    fn principal_id(value: &str) -> PrincipalId {
        value.parse().expect("static principal id is valid")
    }

    fn tenant_id() -> DataTenantId {
        "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id is valid")
    }

    fn role(name: &str) -> RoleRef {
        RoleRef::new(name).expect("static role is valid")
    }

    fn assert_no_sqlx_in_dir(path: impl AsRef<std::path::Path>) {
        for entry in std::fs::read_dir(path).expect("source directory is readable") {
            let entry = entry.expect("source entry is readable");
            let path = entry.path();
            if path.is_dir() {
                assert_no_sqlx_in_dir(path);
                continue;
            }
            if path.extension().and_then(std::ffi::OsStr::to_str) != Some("rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("source file is readable");
            let module_path = ["sql", "x::"].concat();
            let import_path = ["use sql", "x"].concat();
            assert!(
                !source.contains(&module_path) && !source.contains(&import_path),
                "{} must stay sqlx-free",
                path.display()
            );
        }
    }
}
