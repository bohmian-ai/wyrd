//! Server-tier authentication issuance helpers.

#![deny(missing_docs)]

use argon2::{Algorithm, Argon2, Params, Version};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::pkcs8::EncodePrivateKey;
use ed25519_dalek::pkcs8::EncodePublicKey;
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use jsonwebtoken::{EncodingKey, Header};
use password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use secrecy::{ExposeSecret, SecretString};
use ulid::Ulid;
use wyrd_auth_verify::{AccessTokenClaims, ActClaim, Kid, RefreshTokenClaims, TokenPrincipalRef};
use wyrd_runtime::{PrincipalId, RoleRef};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::reference::{CardRef, CardRefScope};

pub use wyrd_auth_verify::{MAX_BEARER_TOKEN_BYTES, MAX_DELEGATION_DEPTH};

/// OWASP-recommended Argon2id memory cost in KiB.
pub const ARGON2_M_COST_KIB: u32 = 19_456;

/// OWASP-recommended Argon2id iteration count.
pub const ARGON2_T_COST: u32 = 2;

/// OWASP-recommended Argon2id lane count.
pub const ARGON2_P_COST: u32 = 1;

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
    /// API-key hashing failed.
    #[error("Argon2 hashing failed")]
    Hashing(password_hash::Error),
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
    /// Principal kind did not match the requested issue helper.
    #[error("principal kind does not match token issue helper")]
    InvalidPrincipalKind,
    /// Principal card reference was missing or mismatched.
    #[error("principal card_ref is missing or mismatched")]
    InvalidCardRef,
    /// Encoded token would exceed the verifier bearer-token size limit.
    #[error("card_ref_scope token too large: encoded length {encoded_len} exceeds limit {limit}")]
    CardScopeTooLarge {
        /// Encoded token byte length.
        encoded_len: usize,
        /// Verifier byte limit.
        limit: usize,
    },
}

/// Minimal caller context for RFC 8693 token delegation.
///
/// Contains only the three fields read by [`IssuingKey::issue_delegated_access_token`],
/// avoiding fabrication of unused fields such as `iss`, `jti`, `roles`, and timestamps.
pub struct DelegationCaller {
    /// Subject identifier of the original caller (the `sub` claim of their token).
    pub sub: String,
    /// Principal reference of the caller.
    pub principal: TokenPrincipalRef,
    /// Act chain from the caller's token, if any.
    pub act: Option<Box<ActClaim>>,
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
            .map(|pem| pem.to_string())
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
        let signing = SigningKey::generate(&mut OsRng);
        let pem = signing
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|e| IssueError::PublicKeyDerivation(e.to_string()))?;
        Ok(SecretString::from(pem.to_string()))
    }

    /// Mint an access token for a user principal.
    ///
    /// # Errors
    /// Returns an error when the principal is not a user, TTL is invalid, or signing fails.
    #[tracing::instrument(
        level = "debug",
        skip(self, principal),
        fields(
            kid = %self.kid,
            principal_id = %principal.id,
            principal_kind = ?principal.kind,
            jti = tracing::field::Empty,
        ),
        err,
    )]
    pub fn issue_user_access_token(
        &self,
        principal: TokenPrincipalRef,
        roles: Vec<RoleRef>,
        ttl: Duration,
    ) -> Result<String, IssueError> {
        if principal.kind != PrincipalKindTag::User {
            return Err(IssueError::InvalidPrincipalKind);
        }
        validate_principal_ref(&principal)?;
        self.issue_access_token_with_claims(principal.id.to_string(), principal, roles, None, ttl)
    }

    /// Mint an access token for a Service principal.
    ///
    /// # Errors
    /// Returns an error when the card reference is not a Service, TTL is invalid, or signing fails.
    #[tracing::instrument(
        level = "debug",
        skip(self, card_ref),
        fields(
            kid = %self.kid,
            sa_id = %sa_id,
            tenant_id = %tenant_id,
            jti = tracing::field::Empty,
        ),
        err,
    )]
    pub fn issue_service_access_token(
        &self,
        sa_id: PrincipalId,
        tenant_id: DataTenantId,
        card_ref: CardRef,
        card_ref_scope: CardRefScope,
        roles: Vec<RoleRef>,
        ttl: Duration,
    ) -> Result<String, IssueError> {
        if card_ref.kind != CardKind::Service {
            return Err(IssueError::InvalidCardRef);
        }
        let card_ref_scope = CardRefScope::from_root_and_members(
            &card_ref,
            card_ref_scope.as_slice().iter().cloned(),
        );
        let principal = TokenPrincipalRef {
            id: sa_id,
            kind: PrincipalKindTag::Service,
            tenant_id,
            card_ref: Some(card_ref),
            card_ref_scope,
        };
        self.issue_access_token_with_claims(sa_id.to_string(), principal, roles, None, ttl)
    }

    /// Mint an access token for an Agent principal.
    ///
    /// # Errors
    /// Returns an error when the card reference is not an Agent, TTL is invalid, or signing fails.
    #[tracing::instrument(
        level = "debug",
        skip(self, card_ref),
        fields(
            kid = %self.kid,
            agent_id = %agent_id,
            tenant_id = %tenant_id,
            jti = tracing::field::Empty,
        ),
        err,
    )]
    pub fn issue_agent_access_token(
        &self,
        agent_id: PrincipalId,
        tenant_id: DataTenantId,
        card_ref: CardRef,
        card_ref_scope: CardRefScope,
        roles: Vec<RoleRef>,
        ttl: Duration,
    ) -> Result<String, IssueError> {
        if card_ref.kind != CardKind::Agent {
            return Err(IssueError::InvalidCardRef);
        }
        let card_ref_scope = CardRefScope::from_root_and_members(
            &card_ref,
            card_ref_scope.as_slice().iter().cloned(),
        );
        let principal = TokenPrincipalRef {
            id: agent_id,
            kind: PrincipalKindTag::Agent,
            tenant_id,
            card_ref: Some(card_ref),
            card_ref_scope,
        };
        self.issue_access_token_with_claims(agent_id.to_string(), principal, roles, None, ttl)
    }

    /// Mint a delegated access token via RFC 8693 token exchange.
    ///
    /// # Errors
    /// Returns an error when the requested subject is invalid, TTL is invalid, delegation depth is
    /// exceeded, or signing fails.
    #[tracing::instrument(
        level = "debug",
        skip(self, caller, requested_subject),
        fields(
            kid = %self.kid,
            requested_subject = %requested_subject.id,
            delegation_depth = tracing::field::Empty,
            jti = tracing::field::Empty,
        ),
        err,
    )]
    pub fn issue_delegated_access_token(
        &self,
        caller: &DelegationCaller,
        requested_subject: TokenPrincipalRef,
        requested_roles: Vec<RoleRef>,
        ttl: Duration,
    ) -> Result<String, IssueError> {
        validate_principal_ref(&requested_subject)?;
        let resulting_depth = act_depth(caller.act.as_deref()) + 1;
        tracing::Span::current().record("delegation_depth", resulting_depth);
        if resulting_depth > MAX_DELEGATION_DEPTH {
            return Err(IssueError::DelegationDepthExceeded {
                max: MAX_DELEGATION_DEPTH,
            });
        }

        let act = Some(Box::new(ActClaim {
            sub: caller.sub.clone(),
            principal: caller.principal.clone(),
            act: caller.act.clone(),
        }));

        self.issue_access_token_with_claims(
            caller.sub.clone(),
            requested_subject,
            requested_roles,
            act,
            ttl,
        )
    }

    /// Mint a refresh token for any principal kind.
    ///
    /// # Errors
    /// Returns an error when TTL is invalid or signing fails.
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
        ttl: Duration,
    ) -> Result<String, IssueError> {
        let (iat, exp) = timestamps(ttl)?;
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

    fn issue_access_token_with_claims(
        &self,
        sub: String,
        principal: TokenPrincipalRef,
        roles: Vec<RoleRef>,
        act: Option<Box<ActClaim>>,
        ttl: Duration,
    ) -> Result<String, IssueError> {
        let (iat, exp) = timestamps(ttl)?;
        let jti = new_jti();
        tracing::Span::current().record("jti", &jti);
        let claims = AccessTokenClaims {
            sub,
            principal,
            roles,
            act,
            exp,
            iat,
            iss: self.issuer.clone(),
            jti,
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

/// Domain separator for audit-seal Ed25519 signatures.
///
/// Distinct from the JWT issuing key: the audit-seal key is provisioned
/// separately and signs only audit-log checkpoint ranges.
pub const AUDIT_SEAL_DOMAIN: &[u8] = b"wyrd.audit.seal.v1\0";

/// Dedicated Ed25519 signing and verification key for audit log sealing.
///
/// Not the JWT issuing key — a separate key provisioned for the `vala_audit_seal`
/// role. Loaded from the `WYRD_AUDIT_SEAL_KEY` environment variable (PKCS#8 PEM).
/// Sign with [`AuditSealKey::sign`]; verify with [`AuditSealKey::verify`].
pub struct AuditSealKey {
    signing: SigningKey,
}

/// Audit seal error.
#[derive(Debug, thiserror::Error)]
pub enum AuditSealError {
    /// PEM decode or key parse failed.
    #[error("audit seal key invalid: {0}")]
    InvalidKey(String),
    /// Signature verification failed.
    #[error("audit seal signature invalid")]
    InvalidSignature,
}

impl AuditSealKey {
    /// Load an Ed25519 audit-seal key from PKCS#8 PEM bytes.
    ///
    /// # Errors
    /// Returns [`AuditSealError::InvalidKey`] when the PEM is not a valid
    /// PKCS#8 Ed25519 private key.
    pub fn from_pkcs8_pem(pem: &str) -> Result<Self, AuditSealError> {
        SigningKey::from_pkcs8_pem(pem)
            .map(|signing| Self { signing })
            .map_err(|e| AuditSealError::InvalidKey(e.to_string()))
    }

    /// Generate an ephemeral key for tests.
    ///
    /// # Errors
    /// Returns [`AuditSealError::InvalidKey`] if encoding fails (not expected
    /// for a freshly generated key).
    pub fn generate() -> Result<Self, AuditSealError> {
        let pem = SigningKey::generate(&mut OsRng)
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|e| AuditSealError::InvalidKey(e.to_string()))?;
        Self::from_pkcs8_pem(&pem)
    }

    /// Sign a message with the domain separator prepended.
    ///
    /// `msg` is the range hash (SHA256 over ordered audit log column bytes).
    #[must_use]
    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        use ed25519_dalek::Signer as _;
        let mut payload = Vec::with_capacity(AUDIT_SEAL_DOMAIN.len() + msg.len());
        payload.extend_from_slice(AUDIT_SEAL_DOMAIN);
        payload.extend_from_slice(msg);
        self.signing.sign(&payload).to_bytes().to_vec()
    }

    /// Verify a signature produced by [`AuditSealKey::sign`].
    ///
    /// # Errors
    /// Returns [`AuditSealError::InvalidSignature`] when the signature does
    /// not verify, is the wrong length, or was produced by a different key.
    pub fn verify(&self, msg: &[u8], sig_bytes: &[u8]) -> Result<(), AuditSealError> {
        use ed25519_dalek::Verifier as _;
        let sig_arr: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| AuditSealError::InvalidSignature)?;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
        let mut payload = Vec::with_capacity(AUDIT_SEAL_DOMAIN.len() + msg.len());
        payload.extend_from_slice(AUDIT_SEAL_DOMAIN);
        payload.extend_from_slice(msg);
        self.signing
            .verifying_key()
            .verify(&payload, &sig)
            .map_err(|_| AuditSealError::InvalidSignature)
    }
}

/// Hash a raw API key for at-rest storage.
///
/// # Errors
/// Returns an error when Argon2 hashing fails.
pub fn hash_api_key(raw: &SecretString) -> Result<String, IssueError> {
    let salt = SaltString::generate(&mut OsRng);
    argon2()
        .hash_password(raw.expose_secret().as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(IssueError::Hashing)
}

/// Verify a raw API key against an Argon2 PHC string.
#[must_use]
pub fn verify_api_key(raw: &SecretString, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };

    argon2()
        .verify_password(raw.expose_secret().as_bytes(), &parsed)
        .is_ok()
}

fn argon2() -> Argon2<'static> {
    let params = Params::new(ARGON2_M_COST_KIB, ARGON2_T_COST, ARGON2_P_COST, None)
        .expect("OWASP Argon2id params are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

fn timestamps(ttl: Duration) -> Result<(usize, usize), IssueError> {
    if ttl <= Duration::zero() {
        return Err(IssueError::InvalidTtl);
    }

    let issued_at = Utc::now();
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

fn act_depth(act: Option<&ActClaim>) -> usize {
    let Some(act) = act else {
        return 0;
    };
    1 + act_depth(act.act.as_deref())
}

fn validate_principal_ref(principal: &TokenPrincipalRef) -> Result<(), IssueError> {
    match (
        principal.kind,
        principal.card_ref.as_ref().map(|card_ref| &card_ref.kind),
    ) {
        (PrincipalKindTag::User, None) => Ok(()),
        (PrincipalKindTag::User, Some(_)) => Err(IssueError::InvalidCardRef),
        (PrincipalKindTag::Service, Some(CardKind::Service)) => Ok(()),
        (PrincipalKindTag::Agent, Some(CardKind::Agent)) => Ok(()),
        (PrincipalKindTag::Service | PrincipalKindTag::Agent, _) => Err(IssueError::InvalidCardRef),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use jsonwebtoken::{Algorithm, decode_header};
    use secrecy::{ExposeSecret, SecretString};
    use wyrd_auth_verify::{
        AccessTokenClaims, ActClaim, RefreshTokenClaims, TokenPrincipalRef, decode_kid,
        public_key_from_pem, verify_eddsa,
    };
    use wyrd_runtime::{PrincipalId, RoleRef};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};

    use super::{
        ARGON2_M_COST_KIB, AUDIT_SEAL_DOMAIN, AuditSealKey, DelegationCaller, IssueError,
        IssuingKey, Kid, MAX_DELEGATION_DEPTH, hash_api_key, verify_api_key,
    };

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    #[test]
    fn issue_user_access_token_uses_principal_roles_act_and_jti_shape() {
        let token = issuing_key()
            .issue_user_access_token(
                user_principal(),
                vec![role("runtime_admin")],
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
        assert_eq!(claims.roles, vec![role("runtime_admin")]);
        assert_eq!(claims.act, None);
        assert_eq!(claims.iss, "wyrd");
        assert_eq!(claims.jti.len(), 26);
    }

    #[test]
    fn issue_service_access_token_forces_service_card_ref() {
        let card_ref = card_ref(CardKind::Service);
        let token = issuing_key()
            .issue_service_access_token(
                principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02"),
                tenant_id(),
                card_ref.clone(),
                CardRefScope::default(),
                vec![role("service")],
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

    #[test]
    fn issue_service_access_token_rejects_agent_card_ref() {
        let result = issuing_key().issue_service_access_token(
            principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02"),
            tenant_id(),
            card_ref(CardKind::Agent),
            CardRefScope::default(),
            vec![role("service")],
            Duration::minutes(5),
        );

        assert!(matches!(result, Err(IssueError::InvalidCardRef)));
    }

    #[test]
    fn issue_agent_access_token_forces_agent_card_ref() {
        let card_ref = card_ref(CardKind::Agent);
        let token = issuing_key()
            .issue_agent_access_token(
                principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b04"),
                tenant_id(),
                card_ref.clone(),
                CardRefScope::default(),
                vec![role("agent")],
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

    #[test]
    fn issue_agent_access_token_rejects_service_card_ref() {
        let result = issuing_key().issue_agent_access_token(
            principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b04"),
            tenant_id(),
            card_ref(CardKind::Service),
            CardRefScope::default(),
            vec![role("agent")],
            Duration::minutes(5),
        );

        assert!(matches!(result, Err(IssueError::InvalidCardRef)));
    }

    #[test]
    fn issue_user_access_token_rejects_non_user_principal() {
        let result = issuing_key().issue_user_access_token(
            service_principal(),
            vec![role("service")],
            Duration::minutes(5),
        );

        assert!(matches!(result, Err(IssueError::InvalidPrincipalKind)));
    }

    #[test]
    fn issue_user_access_token_rejects_user_with_card_ref() {
        let result = issuing_key().issue_user_access_token(
            TokenPrincipalRef {
                card_ref: Some(card_ref(CardKind::Service)),
                ..user_principal()
            },
            vec![role("runtime_admin")],
            Duration::minutes(5),
        );

        assert!(matches!(result, Err(IssueError::InvalidCardRef)));
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
    fn issue_user_access_token_rejects_zero_ttl() {
        let result = issuing_key().issue_user_access_token(
            user_principal(),
            vec![role("runtime_admin")],
            Duration::zero(),
        );
        assert!(matches!(result, Err(IssueError::InvalidTtl)));
    }

    #[test]
    fn issue_user_access_token_rejects_negative_ttl() {
        let result = issuing_key().issue_user_access_token(
            user_principal(),
            vec![role("runtime_admin")],
            Duration::minutes(-1),
        );
        assert!(matches!(result, Err(IssueError::InvalidTtl)));
    }

    #[test]
    fn issue_delegated_access_token_extends_act_chain() {
        let caller_token = issuing_key()
            .issue_user_access_token(
                user_principal(),
                vec![role("runtime_admin")],
                Duration::minutes(5),
            )
            .expect("caller token issues");
        let raw = verify_access_token(&caller_token);
        let caller = DelegationCaller {
            sub: raw.sub.clone(),
            principal: raw.principal.clone(),
            act: raw.act.clone(),
        };
        let delegated_token = issuing_key()
            .issue_delegated_access_token(
                &caller,
                agent_principal(),
                vec![role("agent")],
                Duration::minutes(5),
            )
            .expect("delegated token issues");
        let delegated_claims = verify_access_token(&delegated_token);
        let act = delegated_claims.act.as_ref().expect("act chain is present");

        assert_eq!(delegated_claims.sub, raw.sub);
        assert_eq!(delegated_claims.principal.kind, PrincipalKindTag::Agent);
        assert_eq!(delegated_claims.roles, vec![role("agent")]);
        assert_eq!(act.sub, raw.sub);
        assert_eq!(act.principal, raw.principal);
        assert_eq!(act.act, None);
    }

    #[test]
    fn issue_delegated_access_token_rejects_depth_over_max() {
        let caller = DelegationCaller {
            sub: user_principal().id.to_string(),
            principal: user_principal(),
            act: Some(Box::new(act_chain(MAX_DELEGATION_DEPTH))),
        };

        let result = issuing_key().issue_delegated_access_token(
            &caller,
            agent_principal(),
            vec![role("agent")],
            Duration::minutes(5),
        );

        assert!(matches!(
            result,
            Err(IssueError::DelegationDepthExceeded {
                max: MAX_DELEGATION_DEPTH
            })
        ));
    }

    #[test]
    fn issue_delegated_access_token_accepts_depth_at_max() {
        let caller = DelegationCaller {
            sub: user_principal().id.to_string(),
            principal: user_principal(),
            act: Some(Box::new(act_chain(MAX_DELEGATION_DEPTH - 1))),
        };

        let result = issuing_key().issue_delegated_access_token(
            &caller,
            agent_principal(),
            vec![role("agent")],
            Duration::minutes(5),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn issue_refresh_token_is_principal_generic() {
        let token = issuing_key()
            .issue_refresh_token(
                PrincipalKindTag::Service,
                principal_id("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02"),
                tenant_id(),
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

    #[test]
    fn hash_api_key_verifies_with_pinned_argon2_params() {
        let raw = SecretString::from("wyrd_test_key");
        let hash = hash_api_key(&raw).expect("api key hashes");

        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify_api_key(&raw, &hash));
        assert_eq!(ARGON2_M_COST_KIB, 19_456);
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

    /// Nested module so the gate filter
    /// `wyrd_auth_issue::audit_seal_sign_verifies_and_is_domain_separated`
    /// is a substring of the full test path
    /// `tests::wyrd_auth_issue::audit_seal_sign_verifies_and_is_domain_separated`.
    mod wyrd_auth_issue {
        use super::{AUDIT_SEAL_DOMAIN, AuditSealKey};

        #[test]
        fn audit_seal_sign_verifies_and_is_domain_separated() {
            let key = AuditSealKey::generate().expect("ephemeral key generates");
            let msg = b"sha256-range-hash-bytes-32-padded!";

            let sig = key.sign(msg);
            key.verify(msg, &sig).expect("valid signature verifies");

            // Wrong message fails.
            let tampered = b"sha256-range-hash-bytes-32-padded?";
            assert!(
                key.verify(tampered, &sig).is_err(),
                "tampered message must not verify"
            );

            // Wrong key fails.
            let other_key = AuditSealKey::generate().expect("second key generates");
            assert!(
                other_key.verify(msg, &sig).is_err(),
                "signature must not verify under a different key"
            );

            // Domain separation: signing `domain || msg` double-applies the domain
            // (stored payload is `domain || domain || msg`); the resulting signature
            // must not verify for the original `msg` payload.
            let domain_prepended_msg: Vec<u8> = AUDIT_SEAL_DOMAIN
                .iter()
                .chain(msg.iter())
                .copied()
                .collect();
            let double_domain_sig = key.sign(&domain_prepended_msg);
            assert!(
                key.verify(msg, &double_domain_sig).is_err(),
                "double-domain payload must not verify as single-domain message"
            );
        }
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
            .issue_user_access_token(
                user_principal(),
                vec![role("runtime_admin")],
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
            .issue_user_access_token(
                user_principal(),
                vec![role("runtime_admin")],
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

    fn issuing_key() -> IssuingKey {
        IssuingKey::from_ed_pem(
            SecretString::from(PRIVATE_KEY_PEM),
            Kid::new("k1").expect("kid is valid"),
            "wyrd",
        )
        .expect("test private key loads")
    }

    fn public_key() -> jsonwebtoken::DecodingKey {
        public_key_from_pem(PUBLIC_KEY_PEM).expect("test public key loads")
    }

    fn issue_user_test_token(ttl: Duration) -> String {
        issuing_key()
            .issue_user_access_token(user_principal(), vec![role("runtime_admin")], ttl)
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
