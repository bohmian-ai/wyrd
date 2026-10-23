//! Verify-only authentication helpers.

#![deny(missing_docs)]

use std::collections::HashMap;
use std::fmt;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode_header};
use lru::LruCache;
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use wyrd_auth_oidc::{
    IssuerConfigResolver, IssuerVerification, JwksCache, OidcError, OidcKid, map_claims,
};
use wyrd_runtime::{
    DelegationStep, PermissionSet, Principal, PrincipalId, PrincipalKind,
    PrincipalRef as RuntimePrincipalRef, RoleRef,
};
use wyrd_spec::DataTenantId;
pub use wyrd_spec::auth::TokenAudience;
use wyrd_spec::auth::{
    CardAccess, IssuerTokenPolicy, IssuerUrl, MAX_SCOPED_CARDS, PrincipalKindTag,
};
use wyrd_spec::ids::CardUid;

/// Hard cap on RFC 8693 delegation depth.
pub const MAX_DELEGATION_DEPTH: usize = 5;

/// Hard cap on a raw bearer token before verifier-side decoding.
pub const MAX_BEARER_TOKEN_BYTES: usize = 8 * 1024;

/// Validated JWT key identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Kid(String);

impl Kid {
    /// Build a validated key id.
    ///
    /// # Errors
    /// Returns an error when the key id does not match `^[A-Za-z0-9._-]{1,64}$`.
    pub fn new(value: impl Into<String>) -> Result<Self, KidError> {
        let value = value.into();
        let ok = !value.is_empty()
            && value.len() <= 64
            && value.bytes().all(|byte| {
                matches!(
                    byte,
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'
                )
            });
        if !ok {
            return Err(KidError);
        }
        Ok(Self(value))
    }

    /// Borrow as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Kid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Invalid JWT key identifier.
#[derive(Debug, thiserror::Error)]
#[error("kid must match ^[A-Za-z0-9._-]{{1,64}}$")]
pub struct KidError;

/// Authentication helper errors.
#[derive(Clone, Debug, thiserror::Error)]
pub enum AuthError {
    /// JWT verification failed.
    #[error("jwt error")]
    Jwt(#[source] jsonwebtoken::errors::Error),
    /// Token shape is invalid or the token does not belong to the expected tenant.
    #[error("invalid token")]
    InvalidToken,
    /// Token is expired.
    #[error("token expired")]
    TokenExpired,
    /// Delegation chain exceeded the supported depth.
    #[error("delegation depth exceeded")]
    DelegationDepthExceeded,
    /// Bearer token format is invalid.
    #[error("bad token format")]
    BadTokenFormat,
    /// An external issuer's trust configuration or JWKS cannot be read.
    #[error("token verification unavailable")]
    VerifyUnavailable,
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(error: jsonwebtoken::errors::Error) -> Self {
        match error.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => Self::TokenExpired,
            _ => Self::Jwt(error),
        }
    }
}

/// Tenant-checked token claims ready to populate request context.
///
/// Built entirely from one verified JWT: the principal's authority is the
/// token's `permissions` claim, fixed when the token was issued.
#[derive(Clone, Debug)]
pub struct VerifiedToken {
    /// The token's subject, carrying the token's permission snapshot as its
    /// authority. For a delegated token this is the party being acted for.
    pub principal: Principal,
    /// Flattened RFC 8693 actor chain, earliest actor first. The last entry is
    /// the current actor; the chain is attribution, never authority.
    pub delegation_chain: Vec<DelegationStep>,
    /// JWT expiry as a UTC timestamp.
    pub exp: DateTime<Utc>,
}

/// Token verification settings.
#[derive(Clone, Debug)]
pub struct WyrdAuthVerifySettings {
    /// Allowed JWT clock skew, applied to `exp` for local and external tokens.
    pub allowed_clock_skew: Duration,
}

impl Default for WyrdAuthVerifySettings {
    fn default() -> Self {
        Self {
            allowed_clock_skew: Duration::from_secs(30),
        }
    }
}

/// A workload assertion verified against one trusted issuer.
///
/// Deliberately tenant-free: it states what the issuer asserted, not where the
/// identity belongs. [`ExternalVerifier::verify_external`] adds the tenant the
/// issuer was resolved under to form a [`VerifiedExternalIdentity`].
#[derive(Debug, Clone)]
pub struct ExternalClaims {
    /// The trusted issuer that signed the token.
    pub issuer: IssuerUrl,
    /// Verified external subject (from `sub` or a configured claim path).
    pub subject: String,
    /// Optional email address; never used as the identity key.
    pub email: Option<String>,
    /// Groups or roles extracted from the token (RBAC resolution input).
    pub groups: Vec<String>,
    /// Whether the matched issuer represents human users or machine workloads.
    pub principal_kind: IssuerTokenPolicy,
    /// The audience the matched issuer expects.
    pub expected_audience: String,
    /// Full verified token claims for downstream assertion checks (e.g. nonce).
    pub raw_claims: JsonValue,
}

/// Verified workload identity from a tenant's trusted external issuer.
///
/// This is NOT a [`wyrd_runtime::Principal`]. The RFC 7523 `jwt-bearer`
/// exchange owns resolving the workload binding and constructing the
/// `Principal`; the verifier is SQL-free and cannot perform those operations
/// here.
#[derive(Debug)]
pub struct VerifiedExternalIdentity {
    /// The trusted issuer that signed the token.
    pub issuer: IssuerUrl,
    /// Tenant the issuer was looked up under (the `(tenant, iss)` key).
    pub tenant_id: DataTenantId,
    /// Verified external subject (from `sub` or a configured claim path).
    pub subject: String,
    /// Optional email address; never used as the identity key.
    pub email: Option<String>,
    /// Groups or roles extracted from the token (RBAC resolution input).
    pub groups: Vec<String>,
    /// Whether the matched issuer represents human users or machine workloads.
    /// Carried out of the trust resolution so callers need not re-resolve it.
    pub principal_kind: IssuerTokenPolicy,
    /// The audience the matched issuer expects, used as the workload binding
    /// audience constraint without a second issuer resolution.
    pub expected_audience: String,
    /// Full verified token claims for downstream assertion checks (e.g. nonce).
    pub raw_claims: JsonValue,
}

/// Most verified tenant tokens one verifier remembers; the least recently
/// used is evicted beyond it.
const VERIFIED_CAPACITY: NonZeroUsize = NonZeroUsize::new(4096).expect("capacity is non-zero");

/// Wyrd-minted access-token verifier.
///
/// Owns only local cryptographic validation state: the deployment's decoding
/// keys, its issuer, the fixed Wyrd audience, the clock-skew policy, and the
/// tenant tokens it has already verified. Verification is synchronous and
/// reads no store — a tenant token carries its own authority snapshot until
/// it expires, and a platform session's current state is revalidated by the
/// platform plane after this signature check. Clones share the remembered
/// tokens.
#[derive(Clone)]
pub struct TokenVerifier {
    /// Public keys by key id; fixed for the verifier's life, so removing a
    /// key builds a new verifier and forgets every token it verified.
    decoding_keys: Arc<HashMap<Kid, Arc<DecodingKey>>>,
    /// The issuer every accepted token names.
    issuer: Arc<String>,
    /// Clock-skew policy.
    settings: WyrdAuthVerifySettings,
    /// Tenant tokens [`Self::verify_on`] accepted, by the SHA-256 of their
    /// surface, tenant, and complete token, each until its own expiry; a
    /// read never extends an entry, and a refusal is never kept.
    verified: Arc<Mutex<LruCache<[u8; 32], VerifiedToken>>>,
}

impl TokenVerifier {
    /// Construct a token verifier.
    ///
    /// # Panics
    /// Panics when no decoding keys are supplied.
    #[must_use]
    pub fn new(
        decoding_keys: HashMap<Kid, Arc<DecodingKey>>,
        issuer: impl Into<String>,
        settings: WyrdAuthVerifySettings,
    ) -> Self {
        assert!(
            !decoding_keys.is_empty(),
            "TokenVerifier requires at least one decoding key"
        );
        Self {
            decoding_keys: Arc::new(decoding_keys),
            issuer: Arc::new(issuer.into()),
            settings,
            verified: Arc::new(Mutex::new(LruCache::new(VERIFIED_CAPACITY))),
        }
    }

    /// Resolve the deployment key that signed one compact token.
    ///
    /// Reads the `kid` from the token's unverified header and looks it up in
    /// this verifier's configured key set, recording the key id on the current
    /// span so a verification failure names the key it was attempted against.
    /// Nothing about the token is trusted beyond the key id, which is a lookup
    /// key and not an assertion: the signature check the caller performs next is
    /// what makes the header credible.
    ///
    /// Both internal verify paths — the tenant access token and the platform
    /// session — resolve their key here so one deployment cannot end up with two
    /// notions of which keys it trusts. `verify_external_against` is deliberately
    /// not a caller: it resolves against a tenant's external JWKS, which is a
    /// different trust anchor.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] when the token is not a decodable
    /// JWT, carries no `kid`, carries a malformed one, or names a key this
    /// deployment does not hold.
    fn signing_key(&self, token: &str) -> Result<Arc<DecodingKey>, AuthError> {
        let header = decode_header(token).map_err(AuthError::from)?;
        let kid = header
            .kid
            .ok_or(AuthError::InvalidToken)
            .and_then(|kid| Kid::new(kid).map_err(|_| AuthError::InvalidToken))?;
        tracing::Span::current().record("kid", tracing::field::display(&kid));
        Ok(Arc::clone(
            self.decoding_keys
                .get(&kid)
                .ok_or(AuthError::InvalidToken)?,
        ))
    }

    /// Verify a platform-scope access token.
    ///
    /// Shares this verifier's key set and issuer policy with the tenant path so
    /// one deployment has one signing trust anchor, but decodes the platform
    /// claim shape, which carries no tenant, no roles, and no delegation chain.
    /// A token whose scope marker is anything other than the platform marker is
    /// rejected, so a tenant token can never be replayed here.
    ///
    /// Results are not cached: a platform session's continued validity depends
    /// on credential state the caller re-reads, and caching the claims would
    /// only invite treating that check as optional.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] for an unknown key id, a failed
    /// signature, a wrong issuer, or a non-platform scope marker, and
    /// [`AuthError::TokenExpired`] for an expired token.
    pub fn verify_platform(&self, token: &str) -> Result<PlatformAccessTokenClaims, AuthError> {
        let key = self.signing_key(token)?;
        let claims: PlatformAccessTokenClaims =
            verify_eddsa(token, &key, Some(self.issuer.as_str()))?;
        if claims.scope != PLATFORM_TOKEN_SCOPE {
            return Err(AuthError::InvalidToken);
        }
        Ok(claims)
    }

    /// Verify a tenant access token presented to a general Wyrd surface.
    ///
    /// Equivalent to [`Self::verify_on`] with [`TokenAudience::Wyrd`]: only a
    /// `wyrd`-audience token is accepted, so a token delegated for a narrower
    /// resource such as Bifrost cannot be replayed here.
    ///
    /// # Errors
    /// Returns the errors of [`Self::verify_on`].
    pub fn verify(
        &self,
        token: &SecretString,
        expected_tenant: &DataTenantId,
    ) -> Result<VerifiedToken, AuthError> {
        self.verify_on(token, expected_tenant, TokenAudience::Wyrd)
    }

    /// Verify a tenant access token presented to the `surface` audience.
    ///
    /// Checks the bearer size, the Ed25519 signature under the named
    /// deployment key, the issuer, the audience, and expiry (with the
    /// configured skew), then requires the token's tenant to be the request's
    /// tenant and builds the runtime principal from the claims. The accepted
    /// audiences are `wyrd` — which every tenant surface honors — and
    /// `surface` itself. Nothing is read from a store: the token's
    /// `permissions` claim is its authority until it expires, so a token this
    /// verifier already accepted for the same tenant and surface is accepted
    /// again from memory until its `exp`, without re-checking its signature.
    /// That is the same decision the full check would make, since no key of
    /// this verifier can be removed and no tenant token is revoked before it
    /// expires; the configured skew past `exp` is left to the full check.
    ///
    /// # Errors
    /// Returns [`AuthError::BadTokenFormat`] for an oversized bearer,
    /// [`AuthError::InvalidToken`] for an unknown key, a wrong issuer or
    /// audience, or a token belonging to another tenant,
    /// [`AuthError::Jwt`] for a failed signature or undecodable claims
    /// (including malformed permissions), [`AuthError::TokenExpired`] for an
    /// expired token, and the claim-shape errors of
    /// [`AccessTokenClaims::into_verified`].
    #[tracing::instrument(
        level = "debug",
        skip(self, token),
        fields(
            kid = tracing::field::Empty,
            principal_id = tracing::field::Empty,
            jti = tracing::field::Empty,
            tenant_id = %expected_tenant,
            surface = surface.as_str(),
        ),
        err,
    )]
    pub fn verify_on(
        &self,
        token: &SecretString,
        expected_tenant: &DataTenantId,
        surface: TokenAudience,
    ) -> Result<VerifiedToken, AuthError> {
        let token = token.expose_secret();
        if token.len() > MAX_BEARER_TOKEN_BYTES {
            return Err(AuthError::BadTokenFormat);
        }
        let digest = Self::digest(token, expected_tenant, surface);
        if let Some(verified) = self.remembered(&digest, Utc::now()) {
            tracing::Span::current().record("principal_id", verified.principal.id.to_string());
            return Ok(verified);
        }
        let key = self.signing_key(token)?;
        let claims = verify_access_token(token, &key, self.validation(surface))?;
        tracing::Span::current().record("principal_id", claims.principal.id.to_string());
        tracing::Span::current().record("jti", claims.jti.as_str());
        if &claims.principal.tenant_id != expected_tenant {
            return Err(AuthError::InvalidToken);
        }
        let verified = claims.into_verified()?;
        self.remember(digest, &verified);
        Ok(verified)
    }

    /// The SHA-256 of `surface`, `tenant`, and the complete `token`, the only
    /// form a verified token is remembered under.
    fn digest(token: &str, tenant: &DataTenantId, surface: TokenAudience) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(surface.as_str().as_bytes());
        digest.update([0]);
        digest.update(tenant.as_uuid().as_bytes());
        digest.update(token.as_bytes());
        digest.finalize().into()
    }

    /// The token remembered under `digest` when it has not expired at `now`;
    /// an expired entry is removed.
    fn remembered(&self, digest: &[u8; 32], now: DateTime<Utc>) -> Option<VerifiedToken> {
        let mut verified = self.verified.lock().unwrap_or_else(PoisonError::into_inner);
        match verified.get(digest) {
            Some(token) if now < token.exp => Some(token.clone()),
            Some(_) => {
                verified.pop(digest);
                None
            }
            None => None,
        }
    }

    /// Remembers `token` under `digest` until its expiry.
    fn remember(&self, digest: [u8; 32], token: &VerifiedToken) {
        self.verified
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .put(digest, token.clone());
    }

    /// Access-token validation accepting `wyrd` and the `surface` audience.
    fn validation(&self, surface: TokenAudience) -> Validation {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[TokenAudience::Wyrd.as_str(), surface.as_str()]);
        validation.leeway = self.settings.allowed_clock_skew.as_secs();
        validation
    }
}

/// Issuance-side verifier for tokens minted by a trusted external OIDC issuer.
///
/// Used only by the workload RFC 7523 `jwt-bearer` exchange to validate the
/// external assertion before a Wyrd token is issued. Human ID tokens are
/// verified by the OpenID Connect relying party in `wyrd-auth-oidc`, and this
/// never verifies a Wyrd access token. Generic over the issuer resolver `I`
/// (an RPITIT trait, not dyn-compatible), so trust lookups happen per call
/// against the live config store.
pub struct ExternalVerifier<I> {
    jwks: Arc<JwksCache>,
    trusted: Arc<I>,
    settings: WyrdAuthVerifySettings,
}

impl<I> Clone for ExternalVerifier<I> {
    fn clone(&self) -> Self {
        Self {
            jwks: Arc::clone(&self.jwks),
            trusted: Arc::clone(&self.trusted),
            settings: self.settings.clone(),
        }
    }
}

impl<I: IssuerConfigResolver> ExternalVerifier<I> {
    /// Construct an external verifier over a JWKS cache and an issuer-config
    /// resolver.
    #[must_use]
    pub fn new(jwks: Arc<JwksCache>, trusted: Arc<I>, settings: WyrdAuthVerifySettings) -> Self {
        Self {
            jwks,
            trusted,
            settings,
        }
    }

    /// Verify a token issued by a trusted external OIDC issuer.
    ///
    /// Returns a [`VerifiedExternalIdentity`] with the verified subject, optional
    /// profile claims, and the raw JSON claims for downstream assertion checks
    /// (e.g. nonce). Does **not** build a `Principal`; the issuance flow owns
    /// identity lookup and grant resolution.
    ///
    /// The lookup is keyed by `(tenant, iss)`, so only an issuer the tenant
    /// trusts can vouch for an identity.
    ///
    /// # Errors
    /// - `AuthError::BadTokenFormat` — the token is oversized or not a JWT.
    /// - `AuthError::InvalidToken` — untrusted issuer, bad token shape, or
    ///   wrong audience.
    /// - `AuthError::TokenExpired` — the token's `exp` has passed.
    /// - `AuthError::VerifyUnavailable` — the JWKS endpoint is unreachable.
    #[tracing::instrument(
        level = "debug",
        skip(self, token),
        fields(tenant_id = %tenant),
        err,
    )]
    pub async fn verify_external(
        &self,
        tenant: &DataTenantId,
        token: &str,
    ) -> Result<VerifiedExternalIdentity, AuthError> {
        if token.len() > MAX_BEARER_TOKEN_BYTES {
            return Err(AuthError::BadTokenFormat);
        }

        // Read the unverified `iss` from the JWT payload to look up the trusted
        // issuer. JWTs are three base64url-no-padding parts: header.claims.sig.
        let iss_str = {
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
            let payload_b64 = token.split('.').nth(1).ok_or(AuthError::BadTokenFormat)?;
            let payload = URL_SAFE_NO_PAD
                .decode(payload_b64)
                .map_err(|_| AuthError::BadTokenFormat)?;
            let claims_value: serde_json::Value =
                serde_json::from_slice(&payload).map_err(|_| AuthError::InvalidToken)?;
            claims_value
                .get("iss")
                .and_then(serde_json::Value::as_str)
                .ok_or(AuthError::InvalidToken)?
                .to_owned()
        };

        // Parse the issuer string into the typed form for the trust lookup.
        let iss_url = IssuerUrl::new(iss_str).map_err(|_| AuthError::InvalidToken)?;

        // Look up this tenant's trusted issuer for the unverified `iss` in the
        // live config store. A resolver outage fails closed as
        // VerifyUnavailable; no matching issuer fails closed as InvalidToken
        // (untrusted or cross-tenant).
        let trusted = self
            .trusted
            .trusted_issuer(tenant, &iss_url)
            .await
            .map_err(|_| AuthError::VerifyUnavailable)?
            .ok_or(AuthError::InvalidToken)?;

        let claims = self
            .verify_external_against(&trusted.verification(), token)
            .await?;

        Ok(VerifiedExternalIdentity {
            issuer: claims.issuer,
            tenant_id: *tenant,
            subject: claims.subject,
            email: claims.email,
            groups: claims.groups,
            principal_kind: claims.principal_kind,
            expected_audience: claims.expected_audience,
            raw_claims: claims.raw_claims,
        })
    }

    /// Verify a federated token against an issuer the caller already resolved.
    ///
    /// This is the single authoritative external-token verification: signature
    /// over the issuer's JWKS, required and pinned issuer and audience, expiry
    /// and a present `nbf` under clock skew, and claim mapping. [`Self::verify_external`] is the tenant-scoped entry
    /// point that resolves the issuer from a tenant's configuration and then
    /// delegates here.
    ///
    /// The caller owns *which* issuer is trusted; this owns *whether* the
    /// token is valid under it.
    ///
    /// # Errors
    /// - [`AuthError::BadTokenFormat`] — the token is oversized or not a JWT.
    /// - [`AuthError::InvalidToken`] — a symmetric algorithm, a missing or
    ///   unknown `kid`, a wrong issuer or audience, or unmappable claims.
    /// - [`AuthError::TokenExpired`] — the token's `exp` has passed.
    /// - [`AuthError::Jwt`] — a missing `exp`, `iss`, or `aud`, a future
    ///   `nbf`, or another signature or decoding failure.
    /// - [`AuthError::VerifyUnavailable`] — the JWKS endpoint is unreachable.
    pub async fn verify_external_against(
        &self,
        trusted: &IssuerVerification,
        token: &str,
    ) -> Result<ExternalClaims, AuthError> {
        if token.len() > MAX_BEARER_TOKEN_BYTES {
            return Err(AuthError::BadTokenFormat);
        }
        let header = decode_header(token).map_err(AuthError::from)?;

        // Reject symmetric algorithms. Only asymmetric keys appear in JWKS.
        if matches!(
            header.alg,
            Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512
        ) {
            return Err(AuthError::InvalidToken);
        }

        // F09 — single Kid → OidcKid conversion site.
        let kid_str = header.kid.ok_or(AuthError::InvalidToken)?;
        let oidc_kid = OidcKid::new(kid_str);

        let decoding_key = self
            .jwks
            .key(trusted.issuer.as_str(), &trusted.jwks_uri, &oidc_kid)
            .await
            .map_err(|e| match e {
                OidcError::JwksUnavailable { .. } => AuthError::VerifyUnavailable,
                OidcError::UnknownKid { .. } | OidcError::ClaimMissing { .. } => {
                    AuthError::InvalidToken
                }
            })?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[trusted.issuer.as_str()]);
        validation.set_audience(&[trusted.expected_audience.as_str()]);
        // Pinning a claim the token omits proves nothing, so both pinned
        // claims must be present, and a present `nbf` must have passed.
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        validation.validate_nbf = true;
        validation.leeway = self.settings.allowed_clock_skew.as_secs();

        let raw_claims: serde_json::Value = jsonwebtoken::decode(token, &decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                jsonwebtoken::errors::ErrorKind::InvalidAudience
                | jsonwebtoken::errors::ErrorKind::InvalidIssuer => AuthError::InvalidToken,
                _ => AuthError::Jwt(e),
            })?;

        let mapped =
            map_claims(&trusted.claim_mapping, &raw_claims).map_err(|_| AuthError::InvalidToken)?;

        Ok(ExternalClaims {
            issuer: trusted.issuer.clone(),
            subject: mapped.subject,
            email: mapped.email,
            groups: mapped.groups,
            principal_kind: trusted.principal_kind,
            expected_audience: trusted.expected_audience.clone(),
            raw_claims,
        })
    }
}

/// Resolved Wyrd access-token claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessTokenClaims {
    /// Subject id, JWT `sub`; always `principal.id`.
    pub sub: String,
    /// Subject whose attenuated authority the token carries.
    pub principal: TokenPrincipalRef,
    /// Roles assigned to the subject at issue time.
    ///
    /// Informational metadata only. No request resolves or authorizes from
    /// it; [`Self::permissions`] is the token's authority.
    pub roles: Vec<RoleRef>,
    /// Effective permissions resolved when the token was issued — for a
    /// delegated token, the intersection of the subject's and the actor's.
    /// The only tenant authority claim.
    pub permissions: PermissionSet,
    /// The subject's exact selected Service or Agent root, when one was
    /// selected. Execution context, never account identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workload_uid: Option<CardUid>,
    /// Required explicit limit on which Cards the token may attribute
    /// evidence to; checked independently of `permissions`.
    pub card_access: CardAccess,
    /// RFC 8693 actor chain for delegated tokens: the outermost layer is the
    /// current actor, and earlier actors nest inside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act: Option<Box<ActClaim>>,
    /// Audience, JWT `aud`: a [`TokenAudience`] value.
    pub aud: String,
    /// Expiry as Unix seconds, JWT `exp`.
    pub exp: usize,
    /// Issued-at as Unix seconds, JWT `iat`.
    pub iat: usize,
    /// Issuer, JWT `iss`.
    pub iss: String,
    /// Token identifier.
    pub jti: String,
    /// Non-secret id of the credential this token was exchanged from, when a
    /// credential was presented.
    ///
    /// Carried so audit can name which of a principal's several live
    /// credentials made a decision — the one thing needed to revoke the right
    /// key after a leak. Absent for a federated human session and for tokens
    /// the server mints internally. It is attribution, not authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
}

/// Claims carried by a platform-scope access token.
///
/// Deliberately smaller than [`AccessTokenClaims`] and structurally unable to
/// name a tenant: a platform token has no tenancy, no roles, and no delegation
/// chain. It carries only who is acting and which credential minted it, so the
/// verifier resolves current authority from the store rather than trusting a
/// permission snapshot that a later grant change would leave stale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformAccessTokenClaims {
    /// Platform principal id, JWT `sub`.
    pub sub: String,
    /// Credential that minted this token, when one did.
    ///
    /// Re-read on every platform request so revoking a credential stops the
    /// sessions it issued on the next request, and so audit can name which
    /// credential was used.
    ///
    /// Absent for a session established by federated login: a human presents an
    /// identity, not a credential, so there is no credential to name or revoke.
    /// Such a session is anchored on the principal instead, which is re-read on
    /// every verification for the same reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    /// Control-plane marker. Always `platform`; any other value fails
    /// verification, so a tenant token cannot be replayed as a platform one.
    pub scope: String,
    /// Expiry as Unix seconds, JWT `exp`.
    pub exp: usize,
    /// Issued-at as Unix seconds, JWT `iat`.
    pub iat: usize,
    /// Issuer, JWT `iss`.
    pub iss: String,
    /// Token identifier.
    pub jti: String,
}

/// The only accepted value of [`PlatformAccessTokenClaims::scope`].
pub const PLATFORM_TOKEN_SCOPE: &str = "platform";

/// One layer of an RFC 8693 `act` delegation chain: one actor, with the
/// actor before it nested in [`Self::act`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActClaim {
    /// Actor id at this delegation layer.
    pub sub: String,
    /// Actor principal at this delegation layer.
    pub principal: TokenPrincipalRef,
    /// The actor's exact root; every actor runs under a resolved root.
    pub workload_uid: CardUid,
    /// Non-secret id of the credential the actor authenticated with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    /// The actor that acted before this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act: Option<Box<ActClaim>>,
}

/// Wire-side account reference embedded in JWT claims.
///
/// Account identity only: execution context and Card access are separate
/// claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenPrincipalRef {
    /// Stable principal id.
    pub id: PrincipalId,
    /// Account kind.
    pub kind: PrincipalKindTag,
    /// Tenant isolation key.
    pub tenant_id: DataTenantId,
}

/// Refresh-token claims for any principal kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshTokenClaims {
    /// Subject id, equal to `principal_id`.
    pub sub: String,
    /// Principal kind.
    pub principal_kind: PrincipalKindTag,
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

impl From<(&RuntimePrincipalRef, DataTenantId)> for TokenPrincipalRef {
    fn from((ref_, tenant_id): (&RuntimePrincipalRef, DataTenantId)) -> Self {
        Self {
            id: ref_.id,
            kind: ref_.kind.tag(),
            tenant_id,
        }
    }
}

impl From<&Principal> for TokenPrincipalRef {
    fn from(principal: &Principal) -> Self {
        Self {
            id: principal.id,
            kind: principal.kind.tag(),
            tenant_id: principal.tenant_id,
        }
    }
}

impl AccessTokenClaims {
    /// Convert verified claims into a runtime verified-token envelope.
    ///
    /// The runtime principal is built directly from the claims: its effective
    /// permissions are the `permissions` claim, its Card limit the
    /// `card_access` claim, and nothing is read from a store. `roles` is
    /// copied through as informational metadata.
    ///
    /// Fails closed on every inconsistent shape: a subject other than the
    /// account id, a kind other than `tenant_admin`, `user`, or `service`, a
    /// duplicate or oversized scope, a direct scope without its root, a root
    /// beside unscoped access, an unresolved `root_required` on a non-Service
    /// account, or a delegated token that is not scoped, carries a top-level
    /// credential, or names an actor that is not a Service in the same tenant.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] for any inconsistent claim shape,
    /// [`AuthError::DelegationDepthExceeded`] past [`MAX_DELEGATION_DEPTH`]
    /// actors, and [`AuthError::InvalidToken`] for an out-of-range expiry.
    pub fn into_verified(&self) -> Result<VerifiedToken, AuthError> {
        let kind = tenant_account_kind(self.principal.kind)?;
        if self.sub != self.principal.id.to_string() {
            return Err(AuthError::InvalidToken);
        }
        let direct = self.act.is_none();
        let consistent = match (&self.card_access, &self.workload_uid) {
            (CardAccess::Scoped { cards }, root) => {
                scope_is_canonical(cards)
                    && (!direct || root.as_ref().is_some_and(|root| cards.contains(root)))
            }
            (CardAccess::AnyRegistered, None) => direct,
            (CardAccess::RootRequired { .. }, None) => direct && kind == PrincipalKind::Service,
            (CardAccess::AnyRegistered | CardAccess::RootRequired { .. }, Some(_)) => false,
        };
        if !consistent || (!direct && self.cid.is_some()) {
            return Err(AuthError::InvalidToken);
        }
        let principal = Principal::new(
            self.principal.id,
            kind,
            self.principal.tenant_id,
            self.roles.clone(),
            self.permissions.clone(),
        )
        .with_credential_id(parse_cid(self.cid.as_deref())?)
        .with_card_access(self.card_access.clone(), self.workload_uid.clone());
        let delegation_chain = flatten_act_chain(self.act.as_deref(), self.principal.tenant_id)?;
        let exp = i64::try_from(self.exp)
            .ok()
            .and_then(|exp| DateTime::<Utc>::from_timestamp(exp, 0))
            .ok_or(AuthError::InvalidToken)?;

        Ok(VerifiedToken {
            principal,
            delegation_chain,
            exp,
        })
    }
}

/// The tenant account kind a signed claim may name.
///
/// A platform kind can never become a tenant principal, and the tenant
/// SYSTEM writer is never issued a token, so neither is admissible.
///
/// # Errors
/// Returns [`AuthError::InvalidToken`] for `global_admin` and `system`.
fn tenant_account_kind(wire: PrincipalKindTag) -> Result<PrincipalKind, AuthError> {
    match wire {
        PrincipalKindTag::TenantAdmin => Ok(PrincipalKind::TenantAdmin),
        PrincipalKindTag::User => Ok(PrincipalKind::User),
        PrincipalKindTag::Service => Ok(PrincipalKind::Service),
        PrincipalKindTag::GlobalAdmin | PrincipalKindTag::System => Err(AuthError::InvalidToken),
    }
}

/// Whether a signed scope is bounded and free of duplicates.
fn scope_is_canonical(cards: &[CardUid]) -> bool {
    cards.len() <= MAX_SCOPED_CARDS
        && cards
            .iter()
            .enumerate()
            .all(|(index, card)| !cards[..index].contains(card))
}

/// Parse an optional non-secret credential id claim.
///
/// # Errors
/// Returns [`AuthError::InvalidToken`] when present but not a credential id.
fn parse_cid<T: std::str::FromStr>(cid: Option<&str>) -> Result<Option<T>, AuthError> {
    cid.map(|cid| cid.parse().map_err(|_| AuthError::InvalidToken))
        .transpose()
}

/// Verify and decode EdDSA access-token claims.
///
/// # Errors
/// Returns [`AuthError`] when the signature or registered claims fail
/// validation, or [`AuthError::InvalidToken`] when the claims do not decode.
fn verify_access_token(
    token: &str,
    public_key: &DecodingKey,
    mut validation: Validation,
) -> Result<AccessTokenClaims, AuthError> {
    validation.algorithms = vec![Algorithm::EdDSA];
    jsonwebtoken::decode::<AccessTokenClaims>(token, public_key, &validation)
        .map(|data| data.claims)
        .map_err(AuthError::from)
}

/// Flatten a nested `act` claim into actors ordered earliest first.
///
/// The outermost layer is the current actor, so the walk collects newest
/// first and reverses; the current actor is therefore the last entry. Every
/// layer keeps its root and credential provenance.
///
/// # Errors
/// Returns [`AuthError::DelegationDepthExceeded`] past
/// [`MAX_DELEGATION_DEPTH`] layers, and [`AuthError::InvalidToken`] for a
/// layer that is not a Service account of `tenant` with `sub` equal to its id.
fn flatten_act_chain(
    mut act: Option<&ActClaim>,
    tenant: DataTenantId,
) -> Result<Vec<DelegationStep>, AuthError> {
    let mut out = Vec::new();
    while let Some(layer) = act {
        if out.len() >= MAX_DELEGATION_DEPTH {
            return Err(AuthError::DelegationDepthExceeded);
        }
        if layer.principal.kind != PrincipalKindTag::Service
            || layer.principal.tenant_id != tenant
            || layer.sub != layer.principal.id.to_string()
        {
            return Err(AuthError::InvalidToken);
        }
        out.push(DelegationStep {
            principal: RuntimePrincipalRef {
                id: layer.principal.id,
                kind: PrincipalKind::Service,
                workload_uid: Some(layer.workload_uid.clone()),
                credential_id: parse_cid(layer.cid.as_deref())?,
            },
        });
        act = layer.act.as_deref();
    }
    out.reverse();
    Ok(out)
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
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::Utc;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, Validation, encode};
    use secrecy::{ExposeSecret, SecretString};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, IssuerConfigResolver, JwksCache, OidcError,
        TrustedIssuer,
    };
    use wyrd_runtime::{
        BifrostPermissionScope, BifrostTableScope, Permission, PermissionScope, PermissionSet,
    };
    use wyrd_runtime::{CardAttribution, Principal, PrincipalId, PrincipalKind, RoleRef};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerTokenPolicy;
    use wyrd_spec::auth::IssuerUrl;
    use wyrd_spec::auth::{
        CardAccess, FamilyRestriction, MAX_SCOPED_CARDS, WorkloadFamily, WorkloadKind,
    };
    use wyrd_spec::ids::{CardName, CardUid};

    use super::{
        AccessTokenClaims, ActClaim, AuthError, ExternalVerifier, Kid, MAX_BEARER_TOKEN_BYTES,
        MAX_DELEGATION_DEPTH, PrincipalKindTag, TokenAudience, TokenPrincipalRef, TokenVerifier,
        WyrdAuthVerifySettings, decode_kid, public_key_from_pem, verify_eddsa, verify_eddsa_with,
    };

    const PRIVATE_KEY_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";
    /// A second Ed25519 key the verifier does not trust.
    const OTHER_PRIVATE_KEY_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIByme/fpiJQ3GvvAlTUcSGe2JZmngm9PVGO0YGL1dMM/\n-----END PRIVATE KEY-----\n";

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
        let principal = Principal::new(
            principal_id(),
            PrincipalKind::Service,
            tenant_id(),
            vec![role()],
            wyrd_runtime::PermissionSet::new(),
        );

        let projected = TokenPrincipalRef::from(&principal);

        assert_eq!(projected.id, principal.id);
        assert_eq!(projected.kind, PrincipalKindTag::Service);
        assert_eq!(projected.tenant_id, principal.tenant_id);
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

    #[test]
    fn auth_error_is_clone() {
        fn assert_clone<T: Clone>() {}

        assert_clone::<AuthError>();
    }

    #[test]
    fn no_sqlx_in_crate() {
        assert_no_sqlx_in_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    }

    /// The outermost `act` is the current actor and earlier actors nest inside
    /// it; verification keeps the subject as the principal and flattens the
    /// actors earliest first, current actor last, each with its own root and
    /// credential provenance.
    ///
    /// # Panics
    ///
    /// Panics when claim conversion fails, the verified principal is not the
    /// subject or lacks its permission, or the delegation chain is not the two
    /// actors in earliest-first order.
    #[test]
    fn into_verified_keeps_subject_and_orders_actors_earliest_first() {
        let (earlier_root, current_root) = (uid(1), uid(2));
        let credential = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b0c";
        let claims = AccessTokenClaims {
            principal: service_ref(),
            act: Some(Box::new(ActClaim {
                cid: Some(credential.to_owned()),
                act: Some(Box::new(act_layer(earlier_root.clone(), None))),
                ..act_layer(current_root.clone(), None)
            })),
            ..delegated_claims(vec![current_root.clone()])
        };

        let verified = claims.into_verified().expect("claims convert");

        assert_eq!(verified.principal.id, principal_id());
        assert!(
            verified
                .principal
                .effective_permissions
                .contains(&Permission::card_read())
        );
        assert_eq!(verified.delegation_chain.len(), 2);
        assert_eq!(
            verified.delegation_chain[0].principal.workload_uid,
            Some(earlier_root)
        );
        let current = &verified.delegation_chain[1].principal;
        assert_eq!(current.workload_uid, Some(current_root.clone()));
        assert_eq!(
            current.credential_id.map(|id| id.to_string()),
            Some(credential.to_owned())
        );
        assert_eq!(
            wyrd_runtime::owning_root(&verified.principal, &verified.delegation_chain),
            Some(&current_root)
        );
    }

    /// Every combination of account kind, Card access, workload root, and
    /// delegation that the contract does not define is refused, and every one
    /// it defines verifies to exactly the signed limit.
    ///
    /// # Panics
    /// Panics when a malformed shape verifies or a valid one is refused or
    /// projects another attribution.
    #[test]
    fn explicit_card_access_claims_are_fail_closed() {
        let (root, member, other) = (uid(1), uid(2), uid(3));
        let scoped = |cards: Vec<CardUid>| CardAccess::Scoped { cards };
        let root_required = CardAccess::RootRequired {
            restriction: FamilyRestriction::Family(WorkloadFamily {
                kind: WorkloadKind::Service,
                name: CardName::new("checkout").expect("static name"),
            }),
        };
        let direct = |principal: TokenPrincipalRef, card_access: CardAccess, workload_uid| {
            AccessTokenClaims {
                principal,
                workload_uid,
                card_access,
                ..claims_with_times(now() + 3_600, now())
            }
        };
        let delegated = |card_access: CardAccess, act: ActClaim| AccessTokenClaims {
            card_access,
            act: Some(Box::new(act)),
            ..delegated_claims(Vec::new())
        };
        let accepted = [
            (
                "rootless user",
                direct(user_ref(), CardAccess::AnyRegistered, None),
            ),
            (
                "rootless service",
                direct(service_ref(), CardAccess::AnyRegistered, None),
            ),
            (
                "unresolved family",
                direct(service_ref(), root_required.clone(), None),
            ),
            (
                "direct root",
                direct(
                    service_ref(),
                    scoped(vec![root.clone(), member.clone()]),
                    Some(root.clone()),
                ),
            ),
            (
                "user selected root",
                direct(user_ref(), scoped(vec![root.clone()]), Some(root.clone())),
            ),
            (
                "empty delegated",
                delegated(scoped(Vec::new()), act_layer(root.clone(), None)),
            ),
            (
                "delegated without roots",
                delegated(scoped(vec![member.clone()]), act_layer(root.clone(), None)),
            ),
        ];
        for (label, claims) in accepted {
            let verified = claims
                .into_verified()
                .unwrap_or_else(|error| panic!("{label}: {error:?}"));
            assert_eq!(
                verified.principal.card_access, claims.card_access,
                "{label}"
            );
            assert_eq!(
                verified.principal.workload_uid, claims.workload_uid,
                "{label}"
            );
        }
        assert_eq!(
            direct(
                service_ref(),
                scoped(vec![root.clone()]),
                Some(root.clone())
            )
            .into_verified()
            .expect("direct root")
            .principal
            .card_attribution(),
            CardAttribution::Scoped(std::slice::from_ref(&root))
        );

        let too_many: Vec<CardUid> = (0..=MAX_SCOPED_CARDS)
            .map(|n| uid(100 + n as u64))
            .collect();
        let refused = [
            (
                "any with root",
                direct(service_ref(), CardAccess::AnyRegistered, Some(root.clone())),
            ),
            (
                "root_required with root",
                direct(service_ref(), root_required.clone(), Some(root.clone())),
            ),
            (
                "root_required user",
                direct(user_ref(), root_required.clone(), None),
            ),
            (
                "scoped without root",
                direct(service_ref(), scoped(vec![root.clone()]), None),
            ),
            (
                "root outside scope",
                direct(
                    service_ref(),
                    scoped(vec![member.clone()]),
                    Some(root.clone()),
                ),
            ),
            (
                "duplicate members",
                direct(
                    service_ref(),
                    scoped(vec![root.clone(), root.clone()]),
                    Some(root.clone()),
                ),
            ),
            (
                "oversized scope",
                direct(
                    service_ref(),
                    scoped(too_many.clone()),
                    Some(too_many[0].clone()),
                ),
            ),
            (
                "sub mismatch",
                AccessTokenClaims {
                    sub: other.to_string(),
                    ..direct(user_ref(), CardAccess::AnyRegistered, None)
                },
            ),
            (
                "system kind",
                direct(
                    TokenPrincipalRef {
                        kind: PrincipalKindTag::System,
                        ..user_ref()
                    },
                    scoped(Vec::new()),
                    None,
                ),
            ),
            (
                "platform kind",
                direct(
                    TokenPrincipalRef {
                        kind: PrincipalKindTag::GlobalAdmin,
                        ..user_ref()
                    },
                    CardAccess::AnyRegistered,
                    None,
                ),
            ),
            (
                "delegated any",
                delegated(CardAccess::AnyRegistered, act_layer(root.clone(), None)),
            ),
            (
                "delegated root_required",
                delegated(root_required, act_layer(root.clone(), None)),
            ),
            (
                "delegated top-level cid",
                AccessTokenClaims {
                    cid: Some("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b0c".to_owned()),
                    ..delegated(scoped(Vec::new()), act_layer(root.clone(), None))
                },
            ),
            (
                "user actor",
                delegated(
                    scoped(Vec::new()),
                    ActClaim {
                        principal: user_ref(),
                        ..act_layer(root.clone(), None)
                    },
                ),
            ),
            (
                "foreign actor",
                delegated(
                    scoped(Vec::new()),
                    ActClaim {
                        principal: TokenPrincipalRef {
                            tenant_id: DataTenantId::new_v7(),
                            ..service_ref()
                        },
                        ..act_layer(root.clone(), None)
                    },
                ),
            ),
            (
                "actor sub mismatch",
                delegated(
                    scoped(Vec::new()),
                    ActClaim {
                        sub: other.to_string(),
                        ..act_layer(root.clone(), None)
                    },
                ),
            ),
        ];
        for (label, claims) in refused {
            assert!(
                matches!(claims.into_verified(), Err(AuthError::InvalidToken)),
                "{label} must be refused"
            );
        }

        let mut wire = serde_json::to_value(claims_with_times(now() + 3_600, now()))
            .expect("claims serialize");
        for removed in [
            serde_json::json!({"card_access": null}),
            serde_json::json!({"principal": {"card_ref_scope": []}}),
            serde_json::json!({"principal": {"kind": "agent"}}),
            serde_json::json!({"card_access": {"mode": "scoped", "cards": ["not-a-uid"]}}),
        ] {
            let mut candidate = wire.clone();
            merge(&mut candidate, &removed);
            assert!(
                serde_json::from_value::<AccessTokenClaims>(candidate).is_err(),
                "{removed} must not decode"
            );
        }
        wire.as_object_mut().expect("object").remove("card_access");
        assert!(serde_json::from_value::<AccessTokenClaims>(wire).is_err());
    }

    /// Merge `patch` into `target`, recursing into objects.
    fn merge(target: &mut serde_json::Value, patch: &serde_json::Value) {
        match (target, patch) {
            (serde_json::Value::Object(target), serde_json::Value::Object(patch)) => {
                for (key, value) in patch {
                    merge(
                        target.entry(key.clone()).or_insert(serde_json::Value::Null),
                        value,
                    );
                }
            }
            (target, patch) => *target = patch.clone(),
        }
    }

    #[test]
    fn into_verified_rejects_delegation_depth_over_max() {
        let claims = AccessTokenClaims {
            act: Some(Box::new(act_chain(MAX_DELEGATION_DEPTH + 1))),
            ..delegated_claims(Vec::new())
        };

        let result = claims.into_verified();

        assert!(matches!(result, Err(AuthError::DelegationDepthExceeded)));
    }

    #[test]
    fn into_verified_accepts_delegation_depth_at_max() {
        let claims = AccessTokenClaims {
            act: Some(Box::new(act_chain(MAX_DELEGATION_DEPTH))),
            ..delegated_claims(Vec::new())
        };

        let result = claims.into_verified();

        assert!(result.is_ok());
    }

    // -------------------------------------------------------------------------
    // Claim-built principals
    // -------------------------------------------------------------------------

    /// The verifier builds the runtime principal from the token alone: its
    /// authority is the signed `permissions` claim, `roles` rides along as
    /// metadata, and the credential attribution survives.
    #[test]
    fn token_verifier_builds_principal_from_permissions_claim() {
        let credential = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b0c";
        let claims = AccessTokenClaims {
            cid: Some(credential.to_owned()),
            ..claims_with_times(now() + 3_600, now())
        };
        let token = SecretString::from(encode_eddsa_with_kid(&claims));

        let verified = verifier(WyrdAuthVerifySettings::default())
            .verify(&token, &tenant_id())
            .expect("valid token verifies");

        assert_eq!(verified.principal.id, principal_id());
        assert_eq!(verified.principal.tenant_id, tenant_id());
        assert_eq!(verified.principal.roles, vec![role()]);
        assert_eq!(verified.principal.effective_permissions, claims.permissions);
        assert_eq!(
            verified.principal.credential_id.map(|id| id.to_string()),
            Some(credential.to_owned())
        );
    }

    /// `roles` carries no authority: a token naming a role but granting no
    /// permissions yields a principal with no permissions.
    #[test]
    fn token_verifier_treats_roles_as_informational() {
        let claims = AccessTokenClaims {
            permissions: PermissionSet::new(),
            ..claims_with_times(now() + 3_600, now())
        };
        let token = SecretString::from(encode_eddsa_with_kid(&claims));

        let verified = verifier(WyrdAuthVerifySettings::default())
            .verify(&token, &tenant_id())
            .expect("valid token verifies");

        assert_eq!(verified.principal.roles, vec![role()]);
        assert!(verified.principal.effective_permissions.is_empty());
    }

    /// The `permissions` claim round-trips through signing and decoding.
    #[test]
    fn access_token_claims_round_trip_permissions() {
        let claims = claims_with_times(now() + 3_600, now());
        let decoded = verify_eddsa_with::<AccessTokenClaims>(
            &encode_eddsa(&claims),
            &public_key(),
            wyrd_validation(),
        )
        .expect("valid token decodes");

        assert_eq!(decoded, claims);
    }

    /// A token signed by a key other than the named deployment key fails.
    #[test]
    fn token_verifier_rejects_wrong_signature() {
        let other = EncodingKey::from_ed_pem(OTHER_PRIVATE_KEY_PEM).expect("other key parses");
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some("k1".to_owned());
        let token = encode(&header, &claims_with_times(now() + 3_600, now()), &other)
            .expect("test token signs");

        let result = verifier(WyrdAuthVerifySettings::default())
            .verify(&SecretString::from(token), &tenant_id());

        assert!(matches!(result, Err(AuthError::Jwt(_))), "{result:?}");
    }

    /// A token from another issuer fails.
    #[test]
    fn token_verifier_rejects_wrong_issuer() {
        let claims = AccessTokenClaims {
            iss: "other".to_owned(),
            ..claims_with_times(now() + 3_600, now())
        };
        let token = SecretString::from(encode_eddsa_with_kid(&claims));

        let result = verifier(WyrdAuthVerifySettings::default()).verify(&token, &tenant_id());

        assert!(matches!(result, Err(AuthError::Jwt(_))), "{result:?}");
    }

    /// A token for another audience fails.
    #[test]
    fn token_verifier_rejects_wrong_audience() {
        let claims = AccessTokenClaims {
            aud: "not-wyrd".to_owned(),
            ..claims_with_times(now() + 3_600, now())
        };
        let token = SecretString::from(encode_eddsa_with_kid(&claims));

        let result = verifier(WyrdAuthVerifySettings::default()).verify(&token, &tenant_id());

        assert!(matches!(result, Err(AuthError::Jwt(_))), "{result:?}");
    }

    /// A Bifrost-audience token verifies only on the Bifrost surface, while a
    /// `wyrd` token verifies on both, so a delegated Bifrost token cannot be
    /// replayed against a general Wyrd surface.
    ///
    /// # Panics
    ///
    /// Panics when the Bifrost token verifies on the `wyrd` surface, or either
    /// token fails to verify on the Bifrost surface.
    #[test]
    fn bifrost_audience_is_accepted_only_on_the_bifrost_surface() {
        let bifrost = SecretString::from(encode_eddsa_with_kid(&AccessTokenClaims {
            aud: TokenAudience::Bifrost.as_str().to_owned(),
            ..claims_with_times(now() + 3_600, now())
        }));
        let wyrd = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            now(),
        )));
        let verifier = verifier(WyrdAuthVerifySettings::default());

        assert!(matches!(
            verifier.verify(&bifrost, &tenant_id()),
            Err(AuthError::Jwt(_))
        ));
        assert!(
            verifier
                .verify_on(&bifrost, &tenant_id(), TokenAudience::Bifrost)
                .is_ok()
        );
        assert!(
            verifier
                .verify_on(&wyrd, &tenant_id(), TokenAudience::Bifrost)
                .is_ok()
        );
    }

    /// An expired token fails as expired, with no skew allowance.
    #[test]
    fn token_verifier_rejects_expired_token() {
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() - 1,
            now() - 301,
        )));
        let settings = WyrdAuthVerifySettings {
            allowed_clock_skew: Duration::ZERO,
        };

        let result = verifier(settings).verify(&token, &tenant_id());

        assert!(matches!(result, Err(AuthError::TokenExpired)), "{result:?}");
    }

    /// A signed token whose `permissions` claim is not a permission set fails.
    #[test]
    fn token_verifier_rejects_malformed_permissions() {
        let mut claims = serde_json::to_value(claims_with_times(now() + 3_600, now()))
            .expect("claims serialize");
        claims["permissions"] = serde_json::json!([{ "resource": "nope" }]);
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some("k1".to_owned());
        let token = encode(&header, &claims, &private_key()).expect("test token signs");

        let result = verifier(WyrdAuthVerifySettings::default())
            .verify(&SecretString::from(token), &tenant_id());

        assert!(matches!(result, Err(AuthError::Jwt(_))), "{result:?}");
    }

    /// A valid token for one tenant is refused under another.
    #[test]
    fn token_verifier_rejects_cross_tenant_token() {
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            now(),
        )));
        let wrong_tenant = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b09"
            .parse()
            .expect("static tenant id is valid");

        let result = verifier(WyrdAuthVerifySettings::default()).verify(&token, &wrong_tenant);

        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    /// An accepted token is remembered for its tenant and surface until its
    /// `exp`; at `exp` the entry is no longer used and is removed.
    #[test]
    fn an_accepted_token_is_remembered_until_it_expires() {
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            now(),
        )));
        let verifier = verifier(WyrdAuthVerifySettings::default());
        let digest =
            TokenVerifier::digest(token.expose_secret(), &tenant_id(), TokenAudience::Wyrd);

        let verified = verifier
            .verify(&token, &tenant_id())
            .expect("valid token verifies");

        let remembered = verifier
            .remembered(&digest, Utc::now())
            .expect("an accepted token is remembered");
        assert_eq!(remembered.principal.id, verified.principal.id);
        assert_eq!(
            verifier
                .verify(&token, &tenant_id())
                .expect("a remembered token verifies")
                .exp,
            verified.exp
        );
        assert!(verifier.remembered(&digest, verified.exp).is_none());
        assert!(verifier.verified.lock().expect("unpoisoned").is_empty());
    }

    /// A token remembered for one tenant or surface does not answer for
    /// another, and a refusal is never remembered.
    #[test]
    fn a_remembered_token_answers_only_its_tenant_and_surface() {
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            now(),
        )));
        let wrong_tenant = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b09"
            .parse()
            .expect("static tenant id is valid");
        let verifier = verifier(WyrdAuthVerifySettings::default());
        verifier
            .verify(&token, &tenant_id())
            .expect("valid token verifies");

        assert!(matches!(
            verifier.verify(&token, &wrong_tenant),
            Err(AuthError::InvalidToken)
        ));
        assert!(
            verifier
                .remembered(
                    &TokenVerifier::digest(
                        token.expose_secret(),
                        &tenant_id(),
                        TokenAudience::Bifrost
                    ),
                    Utc::now()
                )
                .is_none()
        );
        assert_eq!(verifier.verified.lock().expect("unpoisoned").len(), 1);
    }

    /// An oversized bearer is refused before any decoding.
    #[test]
    fn token_verifier_rejects_oversize_token() {
        let token = SecretString::from("x".repeat(MAX_BEARER_TOKEN_BYTES + 1));

        let result = verifier(WyrdAuthVerifySettings::default()).verify(&token, &tenant_id());

        assert!(matches!(result, Err(AuthError::BadTokenFormat)));
    }

    /// Every signed `system` claim set is refused, including the two shapes
    /// the server once minted for result writes and Drift reads, because the
    /// SYSTEM principal is an attribution identity that never holds a token.
    ///
    /// # Panics
    /// Panics when any `system` claim set verifies.
    #[test]
    fn token_verifier_refuses_every_system_claim_set() {
        let write = system_claims();
        let read = AccessTokenClaims {
            permissions: PermissionSet::from_iter([Permission {
                scope: PermissionScope::Bifrost(BifrostPermissionScope::Table(BifrostTableScope {
                    catalog: "vala".to_owned(),
                    schema: "drift".to_owned(),
                    table_uid: uuid::Uuid::now_v7(),
                })),
                ..Permission::bifrost_query_read()
            }]),
            ..write.clone()
        };
        let unscoped = AccessTokenClaims {
            card_access: CardAccess::Scoped { cards: Vec::new() },
            workload_uid: None,
            ..write.clone()
        };
        for (label, claims) in [("write", write), ("read", read), ("unscoped", unscoped)] {
            let token = SecretString::from(encode_eddsa_with_kid(&claims));
            let result = verifier(WyrdAuthVerifySettings::default()).verify(&token, &tenant_id());
            assert!(
                matches!(result, Err(AuthError::InvalidToken)),
                "{label}: {result:?}"
            );
        }
    }

    /// A SYSTEM identity can never appear as a delegation layer: it holds no
    /// credential and takes part in no delegation or impersonation flow.
    ///
    /// # Panics
    /// Panics when a chain naming a SYSTEM layer resolves.
    #[test]
    fn into_verified_rejects_system_delegation_layer() {
        let claims = AccessTokenClaims {
            act: Some(Box::new(ActClaim {
                principal: system_claims().principal,
                ..act_layer(uid(1), None)
            })),
            ..delegated_claims(Vec::new())
        };

        let result = claims.into_verified();

        assert!(matches!(result, Err(AuthError::InvalidToken)), "{result:?}");
    }

    /// Build the claim set the server once minted for the SYSTEM result
    /// writer: no roles, credential, or delegation; exactly
    /// `bifrost_record:write`; and a scope of exactly one Verifier.
    fn system_claims() -> AccessTokenClaims {
        let verifier = uid(11);
        AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::System,
                ..user_ref()
            },
            roles: Vec::new(),
            permissions: PermissionSet::from_iter([Permission::bifrost_record_write()]),
            card_access: CardAccess::Scoped {
                cards: vec![verifier.clone()],
            },
            workload_uid: Some(verifier),
            ..claims_with_times(now() + 300, now())
        }
    }

    /// A deterministic UUIDv7 Card UID distinguished by `n`.
    ///
    /// # Panics
    /// Panics when the static UID is invalid.
    fn uid(n: u64) -> CardUid {
        CardUid::new(format!("01890f28-7c4a-7cc3-98e7-{n:012x}")).expect("static uid is valid")
    }

    /// A direct Service subject delegated to by one current actor, with
    /// `cards` as the effective intersection.
    fn delegated_claims(cards: Vec<CardUid>) -> AccessTokenClaims {
        AccessTokenClaims {
            principal: service_ref(),
            card_access: CardAccess::Scoped { cards },
            act: Some(Box::new(act_layer(uid(9), None))),
            ..claims_with_times(now() + 3_600, now())
        }
    }

    /// One Service actor layer running under `root`.
    fn act_layer(root: CardUid, act: Option<Box<ActClaim>>) -> ActClaim {
        ActClaim {
            sub: principal_id().to_string(),
            principal: service_ref(),
            workload_uid: root,
            cid: None,
            act,
        }
    }

    /// Validation matching [`TokenVerifier`]'s tenant access-token policy.
    fn wyrd_validation() -> Validation {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_issuer(&["wyrd"]);
        validation.set_audience(&[TokenAudience::Wyrd.as_str()]);
        validation
    }

    /// Build valid `wyrd`-audience user access-token claims for this tenant with
    /// the given `exp` and `iat` (Unix seconds).
    ///
    /// Tests override single fields with struct-update syntax to present exactly
    /// the defect under test; everything else stays verifiable.
    fn claims_with_times(exp: usize, iat: usize) -> AccessTokenClaims {
        AccessTokenClaims {
            sub: principal_id().to_string(),
            principal: user_ref(),
            roles: vec![role()],
            permissions: PermissionSet::from_iter([Permission::card_read()]),
            workload_uid: None,
            card_access: CardAccess::AnyRegistered,
            act: None,
            aud: TokenAudience::Wyrd.as_str().to_owned(),
            exp,
            iat,
            iss: "wyrd".to_owned(),
            jti: "01K00000000000000000000000".to_owned(),
            cid: None,
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

    fn encode_eddsa_with_kid(claims: &AccessTokenClaims) -> String {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some("k1".to_owned());
        encode(&header, claims, &private_key()).expect("test token signs")
    }

    fn encode_hs256(claims: &AccessTokenClaims) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(b"secret"),
        )
        .expect("test token signs")
    }

    fn principal_id() -> PrincipalId {
        "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"
            .parse()
            .expect("static principal id is valid")
    }

    fn user_ref() -> TokenPrincipalRef {
        TokenPrincipalRef {
            id: principal_id(),
            kind: PrincipalKindTag::User,
            tenant_id: tenant_id(),
        }
    }

    fn service_ref() -> TokenPrincipalRef {
        TokenPrincipalRef {
            kind: PrincipalKindTag::Service,
            ..user_ref()
        }
    }

    fn tenant_id() -> DataTenantId {
        "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id is valid")
    }

    fn role() -> RoleRef {
        RoleRef::new("editor").expect("static role is valid")
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

    fn now() -> usize {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after unix epoch")
            .as_secs();
        usize::try_from(secs).expect("unix seconds fit in usize")
    }

    fn act_chain(depth: usize) -> ActClaim {
        let mut act = act_layer(uid(1), None);
        for _ in 1..depth {
            act = act_layer(uid(1), Some(Box::new(act)));
        }
        act
    }

    fn verifier(settings: WyrdAuthVerifySettings) -> TokenVerifier {
        let mut keys = HashMap::new();
        keys.insert(
            Kid::new("k1").expect("kid is valid"),
            Arc::new(public_key()),
        );
        TokenVerifier::new(keys, "wyrd", settings)
    }

    /// DB-free stub `IssuerConfigResolver` for the crate's own unit tests.
    ///
    /// Holds a fixed set of trusted issuers and filters them by tenant, mirroring
    /// the tenant-scoping that the production Postgres resolver enforces via RLS.
    #[derive(Debug, Default)]
    struct StubIssuerResolver {
        issuers: Vec<TrustedIssuer>,
    }

    impl StubIssuerResolver {
        fn new(issuers: Vec<TrustedIssuer>) -> Self {
            Self { issuers }
        }
    }

    impl IssuerConfigResolver for StubIssuerResolver {
        fn trusted_issuers(
            &self,
            tenant: &DataTenantId,
        ) -> impl std::future::Future<Output = Result<Vec<TrustedIssuer>, OidcError>> + Send
        {
            let issuers: Vec<TrustedIssuer> = self
                .issuers
                .iter()
                .filter(|ti| &ti.tenant_id == tenant)
                .cloned()
                .collect();
            async move { Ok(issuers) }
        }
    }

    // -------------------------------------------------------------------------
    // External verify helpers
    // -------------------------------------------------------------------------

    // Ed25519 public key x-component matching PRIVATE_KEY_PEM / PUBLIC_KEY_PEM.
    const ED_X: &str = "WhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ-DZ8Vw";
    const EXTERNAL_ISSUER: &str = "https://test-idp.example.com";
    const EXTERNAL_AUDIENCE: &str = "wyrd-client-id";
    const EXTERNAL_KID: &str = "ext-key-1";

    fn ed_jwks_json(kid: &str) -> serde_json::Value {
        serde_json::json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "kid": kid,
                "x": ED_X
            }]
        })
    }

    fn external_claims(iss: &str, aud: &str, exp: usize, iat: usize) -> serde_json::Value {
        serde_json::json!({
            "sub": "ext-user@idp.example.com",
            "email": "ext@example.com",
            "groups": ["viewer"],
            "iss": iss,
            "aud": aud,
            "exp": exp,
            "iat": iat,
        })
    }

    fn encode_external_token(claims: &serde_json::Value, kid: &str) -> String {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(kid.to_owned());
        encode(&header, claims, &private_key()).expect("external test token signs")
    }

    fn make_jwks_cache() -> Arc<JwksCache> {
        // The workspace reqwest has no built-in Rustls provider; production
        // installs Wyrd's before building clients, so each test process must too.
        wyrd_tls::install_crypto_provider().expect("Wyrd owns the Rustls provider");
        Arc::new(JwksCache::new(
            wyrd_auth_oidc::ScreenedHttp::allowing_internal(),
            Duration::from_mins(5),
            Duration::from_secs(5),
        ))
    }

    fn make_claim_mapping() -> ClaimMapping {
        ClaimMapping {
            subject: ClaimPath::new("sub"),
            email: Some(ClaimPath::new("email")),
            groups: Some(ClaimPath::new("groups")),
        }
    }

    fn make_trusted_issuer(
        tenant_id: DataTenantId,
        issuer: IssuerUrl,
        audience: &str,
        jwks_uri: url::Url,
    ) -> TrustedIssuer {
        TrustedIssuer {
            tenant_id,
            issuer: issuer.clone(),
            jwks_uri,
            expected_audience: audience.to_owned(),
            client_id: "wyrd".to_owned(),
            client_auth: ClientAuth::PrivateKeyJwt,
            claim_mapping: make_claim_mapping(),
            group_role_map: std::collections::HashMap::new(),
            default_roles: Vec::new(),
            principal_kind: IssuerTokenPolicy::Human,
            jwks_ttl: Duration::from_hours(1),
        }
    }

    fn with_external_issuer(
        trusted: TrustedIssuer,
        jwks: Arc<JwksCache>,
    ) -> ExternalVerifier<StubIssuerResolver> {
        let stub = Arc::new(StubIssuerResolver::new(vec![trusted]));
        ExternalVerifier::new(jwks, stub, WyrdAuthVerifySettings::default())
    }

    // -------------------------------------------------------------------------
    // External verify tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn verify_external_happy_path_returns_verified_identity_not_principal() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;

        let jwks_uri: url::Url = format!("{}/jwks", server.uri())
            .parse()
            .expect("wiremock uri is valid");
        let issuer = IssuerUrl::new(EXTERNAL_ISSUER).expect("test issuer is valid");
        let tid = tenant_id();
        let trusted = make_trusted_issuer(tid, issuer.clone(), EXTERNAL_AUDIENCE, jwks_uri);
        let v = with_external_issuer(trusted, make_jwks_cache());

        let claims = external_claims(EXTERNAL_ISSUER, EXTERNAL_AUDIENCE, now() + 3_600, now());
        let token = encode_external_token(&claims, EXTERNAL_KID);

        let identity = v
            .verify_external(&tid, &token)
            .await
            .expect("external happy path should succeed");

        assert_eq!(identity.issuer, issuer);
        assert_eq!(identity.tenant_id, tid);
        assert_eq!(identity.subject, "ext-user@idp.example.com");
        assert_eq!(identity.email.as_deref(), Some("ext@example.com"));
        assert_eq!(identity.groups, vec!["viewer"]);
    }

    #[tokio::test]
    async fn verify_external_tenant_isolation_wrong_audience_is_invalid_token() {
        // Same issuer registered under two tenants with different audiences.
        // Token signed for tenant A (aud-for-a) presented under tenant B → InvalidToken.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;

        let jwks_uri: url::Url = format!("{}/jwks", server.uri())
            .parse()
            .expect("wiremock uri is valid");
        let issuer = IssuerUrl::new(EXTERNAL_ISSUER).expect("test issuer is valid");
        let tenant_a = tenant_id();
        let tenant_b: DataTenantId = "01890f28-7c4a-7001-98e7-4f4a3c2d1b02"
            .parse()
            .expect("static tenant id is valid");

        let trusted_a =
            make_trusted_issuer(tenant_a, issuer.clone(), "aud-for-a", jwks_uri.clone());
        let trusted_b = make_trusted_issuer(tenant_b, issuer.clone(), "aud-for-b", jwks_uri);

        let stub = Arc::new(StubIssuerResolver::new(vec![trusted_a, trusted_b]));
        let jwks = make_jwks_cache();
        let v = ExternalVerifier::new(jwks, stub, WyrdAuthVerifySettings::default());

        // Token signed with aud-for-a.
        let claims_a = external_claims(EXTERNAL_ISSUER, "aud-for-a", now() + 3_600, now());
        let token = encode_external_token(&claims_a, EXTERNAL_KID);

        // Presented under tenant A → ok.
        v.verify_external(&tenant_a, &token)
            .await
            .expect("tenant A with its own audience should succeed");

        // Presented under tenant B → InvalidToken (audience mismatch).
        let result = v.verify_external(&tenant_b, &token).await;
        assert!(
            matches!(result, Err(AuthError::InvalidToken)),
            "wrong tenant presentation should be InvalidToken"
        );
    }

    #[tokio::test]
    async fn verify_external_unknown_tenant_issuer_pair_returns_invalid_token() {
        // Empty resolver — the (tenant, iss) pair is not trusted.
        let stub = Arc::new(StubIssuerResolver::default());
        let jwks = make_jwks_cache();
        let v = ExternalVerifier::new(jwks, stub, WyrdAuthVerifySettings::default());

        let claims = external_claims(EXTERNAL_ISSUER, EXTERNAL_AUDIENCE, now() + 3_600, now());
        let token = encode_external_token(&claims, EXTERNAL_KID);

        let result = v.verify_external(&tenant_id(), &token).await;
        assert!(
            matches!(result, Err(AuthError::InvalidToken)),
            "untrusted issuer should be InvalidToken"
        );
    }

    #[tokio::test]
    async fn verify_external_wrong_audience_returns_invalid_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;

        let jwks_uri: url::Url = format!("{}/jwks", server.uri())
            .parse()
            .expect("wiremock uri is valid");
        let issuer = IssuerUrl::new(EXTERNAL_ISSUER).expect("test issuer is valid");
        let tid = tenant_id();
        let trusted = make_trusted_issuer(tid, issuer, EXTERNAL_AUDIENCE, jwks_uri);
        let v = with_external_issuer(trusted, make_jwks_cache());

        let claims = external_claims(EXTERNAL_ISSUER, "wrong-audience", now() + 3_600, now());
        let token = encode_external_token(&claims, EXTERNAL_KID);

        let result = v.verify_external(&tid, &token).await;
        assert!(
            matches!(result, Err(AuthError::InvalidToken)),
            "wrong audience should be InvalidToken"
        );
    }

    #[tokio::test]
    async fn verify_external_expired_token_returns_token_expired() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;

        let jwks_uri: url::Url = format!("{}/jwks", server.uri())
            .parse()
            .expect("wiremock uri is valid");
        let issuer = IssuerUrl::new(EXTERNAL_ISSUER).expect("test issuer is valid");
        let tid = tenant_id();
        let settings = WyrdAuthVerifySettings {
            allowed_clock_skew: Duration::ZERO,
        };
        let trusted = make_trusted_issuer(tid, issuer, EXTERNAL_AUDIENCE, jwks_uri);
        let stub = Arc::new(StubIssuerResolver::new(vec![trusted]));
        let v = ExternalVerifier::new(make_jwks_cache(), stub, settings);

        let claims = external_claims(
            EXTERNAL_ISSUER,
            EXTERNAL_AUDIENCE,
            now() - 3_600,
            now() - 7_200,
        );
        let token = encode_external_token(&claims, EXTERNAL_KID);

        let result = v.verify_external(&tid, &token).await;
        assert!(
            matches!(result, Err(AuthError::TokenExpired)),
            "expired token should be TokenExpired"
        );
    }

    #[tokio::test]
    async fn verify_external_jwks_unreachable_returns_verify_unavailable() {
        let jwks_uri: url::Url = "http://127.0.0.1:1/jwks"
            .parse()
            .expect("static uri is valid");
        let issuer = IssuerUrl::new(EXTERNAL_ISSUER).expect("test issuer is valid");
        let tid = tenant_id();
        let trusted = make_trusted_issuer(tid, issuer, EXTERNAL_AUDIENCE, jwks_uri);
        let v = with_external_issuer(trusted, make_jwks_cache());

        let claims = external_claims(EXTERNAL_ISSUER, EXTERNAL_AUDIENCE, now() + 3_600, now());
        let token = encode_external_token(&claims, EXTERNAL_KID);

        let result = v.verify_external(&tid, &token).await;
        assert!(
            matches!(result, Err(AuthError::VerifyUnavailable)),
            "unreachable JWKS endpoint should be VerifyUnavailable"
        );
    }

    #[tokio::test]
    async fn verify_external_unknown_kid_refetches_and_succeeds_after_rotation() {
        let server = MockServer::start().await;
        let old_kid = "old-ext-key";
        let new_kid = "new-ext-key";

        // First fetch: serve old key set.
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(old_kid)))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Second fetch (refetch after unknown kid): serve new key set.
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(new_kid)))
            .mount(&server)
            .await;

        let jwks_uri: url::Url = format!("{}/jwks", server.uri())
            .parse()
            .expect("wiremock uri is valid");
        let issuer = IssuerUrl::new(EXTERNAL_ISSUER).expect("test issuer is valid");
        let tid = tenant_id();

        // Populate the cache with the old key set by requesting the old kid first.
        let jwks = make_jwks_cache();
        let trusted_for_old =
            make_trusted_issuer(tid, issuer.clone(), EXTERNAL_AUDIENCE, jwks_uri.clone());
        let stub = Arc::new(StubIssuerResolver::new(vec![trusted_for_old]));
        {
            let v = ExternalVerifier::new(
                Arc::clone(&jwks),
                Arc::clone(&stub),
                WyrdAuthVerifySettings::default(),
            );
            let old_claims =
                external_claims(EXTERNAL_ISSUER, EXTERNAL_AUDIENCE, now() + 3_600, now());
            let old_token = encode_external_token(&old_claims, old_kid);
            v.verify_external(&tid, &old_token)
                .await
                .expect("old kid should resolve");
        }

        // Now create a new verifier sharing the same JWKS cache, but the token
        // uses new_kid. The cache has the old key set; new_kid is unknown →
        // triggers one refetch → found in the new key set → success.
        let trusted_for_new = make_trusted_issuer(tid, issuer, EXTERNAL_AUDIENCE, jwks_uri);
        let stub2 = Arc::new(StubIssuerResolver::new(vec![trusted_for_new]));
        let v2 = ExternalVerifier::new(Arc::clone(&jwks), stub2, WyrdAuthVerifySettings::default());
        let new_claims = external_claims(EXTERNAL_ISSUER, EXTERNAL_AUDIENCE, now() + 3_600, now());
        let new_token = encode_external_token(&new_claims, new_kid);
        v2.verify_external(&tid, &new_token)
            .await
            .expect("new kid should resolve after one JWKS refetch");
    }

    /// A workload assertion needs no `iat`, while the shared workload entry
    /// still refuses a missing `iss` or `aud` (RFC 7523 §3) or a future `nbf`.
    #[tokio::test]
    async fn workload_assertion_requires_binding_claims() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;
        let jwks_uri: url::Url = format!("{}/jwks", server.uri())
            .parse()
            .expect("wiremock uri is valid");
        let issuer = IssuerUrl::new(EXTERNAL_ISSUER).expect("test issuer is valid");
        let tid = tenant_id();
        let trusted = make_trusted_issuer(tid, issuer, EXTERNAL_AUDIENCE, jwks_uri);
        let v = with_external_issuer(trusted, make_jwks_cache());
        let valid = external_claims(EXTERNAL_ISSUER, EXTERNAL_AUDIENCE, now() + 3_600, now());
        // Re-signs the valid claims with `key` set to `value`, or removed.
        let variant = |key: &str, value: Option<serde_json::Value>| {
            let mut claims = valid.clone();
            let object = claims.as_object_mut().expect("claims are an object");
            match value {
                Some(value) => object.insert(key.to_owned(), value),
                None => object.remove(key),
            };
            encode_external_token(&claims, EXTERNAL_KID)
        };
        let future = Some((now() + 3_000).into());

        v.verify_external(&tid, &variant("iat", None))
            .await
            .expect("a workload assertion without iat stays valid");
        assert!(
            v.verify_external(&tid, &variant("nbf", future))
                .await
                .is_err(),
            "a workload assertion with a future nbf is refused"
        );
        for claim in ["iss", "aud"] {
            assert!(
                v.verify_external(&tid, &variant(claim, None))
                    .await
                    .is_err(),
                "a workload assertion missing {claim} is refused (RFC 7523 §3)"
            );
        }
    }
}
