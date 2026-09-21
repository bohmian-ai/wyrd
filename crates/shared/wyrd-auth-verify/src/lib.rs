//! Verify-only authentication helpers.

#![deny(missing_docs)]

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode_header};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use wyrd_auth_oidc::{
    IssuerConfigResolver, IssuerVerification, JwksCache, OidcError, OidcKid, map_claims,
};
use wyrd_runtime::{
    DelegationStep, PermissionSet, Principal, PrincipalId, PrincipalKind,
    PrincipalRef as RuntimePrincipalRef, RoleRef,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{IssuerTokenPolicy, IssuerUrl, PrincipalKindTag};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::reference::{CardRef, CardRefScope};

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
    /// Card-bound principal claim is missing or has the wrong card reference kind.
    #[error("invalid card_ref")]
    InvalidCardRef,
    /// Card-bound principal scope is non-empty but does not contain its root card.
    #[error("card_ref_scope is missing the root card_ref")]
    CardScopeMissingRoot,
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

/// The fixed `aud` of every Wyrd tenant access token.
///
/// Issuance stamps it and [`TokenVerifier::verify`] requires it, so a token
/// Wyrd signed for another purpose (a refresh token, a platform session)
/// cannot be presented as a tenant access token.
pub const WYRD_ACCESS_TOKEN_AUDIENCE: &str = "wyrd";

/// Tenant-checked token claims ready to populate request context.
///
/// Built entirely from one verified JWT: the principal's authority is the
/// token's `permissions` claim, fixed when the token was issued.
#[derive(Clone, Debug)]
pub struct VerifiedToken {
    /// Current actor with the token's permission snapshot as its authority.
    pub principal: Principal,
    /// Flattened RFC 8693 actor chain in initiator-first order.
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

/// A federated identity verified against one trusted issuer.
///
/// Deliberately tenant-free: it states what the issuer asserted, not where the
/// identity belongs. [`VerifiedExternalIdentity`] is this plus the tenant the
/// issuer was resolved under; the platform control plane uses this form
/// directly because a platform principal has no tenant.
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

/// Verified identity from an external OIDC issuer.
///
/// This is NOT a [`wyrd_runtime::Principal`] (R03). The server flow (commit 06
/// for human users, commit 07 for workloads) owns constructing the `Principal`
/// by upsert/lookup and RBAC resolution. The verifier is SQL-free and cannot
/// perform those operations here.
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

/// Wyrd-minted access-token verifier.
///
/// Owns only local cryptographic validation state: the deployment's decoding
/// keys, its issuer, the fixed Wyrd audience, and the clock-skew policy.
/// Verification is synchronous and reads no store — a tenant token carries
/// its own authority snapshot, and a platform session's current state is
/// revalidated by the platform plane after this signature check.
#[derive(Clone)]
pub struct TokenVerifier {
    decoding_keys: Arc<HashMap<Kid, Arc<DecodingKey>>>,
    issuer: Arc<String>,
    settings: WyrdAuthVerifySettings,
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

    /// Verify a tenant access token for the active tenant.
    ///
    /// Checks the bearer size, the Ed25519 signature under the named
    /// deployment key, the issuer, the fixed Wyrd audience, and expiry (with
    /// the configured skew), then requires the token's tenant to be the
    /// request's tenant and builds the runtime principal from the claims.
    /// Nothing is read from a store and nothing is cached: the token's
    /// `permissions` claim is its authority until it expires.
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
        ),
        err,
    )]
    pub fn verify(
        &self,
        token: &SecretString,
        expected_tenant: &DataTenantId,
    ) -> Result<VerifiedToken, AuthError> {
        let token = token.expose_secret();
        if token.len() > MAX_BEARER_TOKEN_BYTES {
            return Err(AuthError::BadTokenFormat);
        }
        let key = self.signing_key(token)?;
        let claims = verify_access_token(token, &key, self.validation())?;
        tracing::Span::current().record("principal_id", claims.principal.id.to_string());
        tracing::Span::current().record("jti", claims.jti.as_str());
        if &claims.principal.tenant_id != expected_tenant {
            return Err(AuthError::InvalidToken);
        }
        claims.into_verified()
    }

    fn validation(&self) -> Validation {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[WYRD_ACCESS_TOKEN_AUDIENCE]);
        validation.leeway = self.settings.allowed_clock_skew.as_secs();
        validation
    }
}

/// Issuance-side verifier for tokens minted by a trusted external OIDC issuer.
///
/// Used by OIDC login, workload `jwt-bearer`, and platform federated login to
/// validate the external assertion before a Wyrd token is issued. It never
/// verifies a Wyrd access token. Generic over the issuer resolver `I`
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
    /// over the issuer's JWKS, issuer and audience pinning, clock skew, and
    /// claim mapping. [`Self::verify_external`] is the tenant-scoped entry
    /// point that resolves the issuer from a tenant's configuration and then
    /// delegates here.
    ///
    /// It exists because not every federated identity belongs to a tenant. The
    /// platform control plane resolves its one deployment-owned connection from
    /// the platform store, which has no tenant to key a resolver by, and must
    /// not grow a parallel verification path to compensate. Taking the resolved
    /// issuer as an argument keeps one implementation for both planes; the
    /// caller owns *which* issuer is trusted, this owns *whether* the token is
    /// valid under it.
    ///
    /// # Errors
    /// - [`AuthError::BadTokenFormat`] — the token is oversized or not a JWT.
    /// - [`AuthError::InvalidToken`] — a symmetric algorithm, a missing or
    ///   unknown `kid`, a wrong issuer or audience, or unmappable claims.
    /// - [`AuthError::TokenExpired`] — the token's `exp` has passed.
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
                OidcError::UnknownKid { .. } => AuthError::InvalidToken,
                _ => AuthError::InvalidToken,
            })?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[trusted.issuer.as_str()]);
        validation.set_audience(&[trusted.expected_audience.as_str()]);
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
    /// Ultimate initiator, JWT `sub`.
    pub sub: String,
    /// Current actor whose authority the token carries.
    pub principal: TokenPrincipalRef,
    /// Roles assigned to the current actor at issue time.
    ///
    /// Informational metadata only. No request resolves or authorizes from
    /// it; [`Self::permissions`] is the token's authority.
    pub roles: Vec<RoleRef>,
    /// The current actor's effective permissions, resolved from its grants
    /// when the token was issued. The only tenant authority claim.
    pub permissions: PermissionSet,
    /// RFC 8693 actor chain for delegated tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act: Option<Box<ActClaim>>,
    /// Audience, JWT `aud`; always [`WYRD_ACCESS_TOKEN_AUDIENCE`].
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

/// One layer of an RFC 8693 `act` delegation chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActClaim {
    /// Subject at this delegation layer.
    pub sub: String,
    /// Principal at this delegation layer.
    pub principal: TokenPrincipalRef,
    /// Next older delegation layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act: Option<Box<ActClaim>>,
}

/// Wire-side projection of a runtime principal safe to embed in JWT claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenPrincipalRef {
    /// Stable principal id.
    pub id: PrincipalId,
    /// Principal kind without inline runtime payloads.
    pub kind: PrincipalKindTag,
    /// Tenant isolation key.
    pub tenant_id: DataTenantId,
    /// Bound card reference for Service and Agent principals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_ref: Option<CardRef>,
    /// Transitive card authorization scope.
    #[serde(default, skip_serializing_if = "CardRefScope::is_empty")]
    pub card_ref_scope: CardRefScope,
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
            card_ref: ref_.card_ref().cloned(),
            card_ref_scope: ref_.card_ref_scope().cloned().unwrap_or_default(),
        }
    }
}

impl From<&Principal> for TokenPrincipalRef {
    fn from(principal: &Principal) -> Self {
        let (kind, card_ref, card_ref_scope) = match &principal.kind {
            PrincipalKind::TenantAdmin => {
                (PrincipalKindTag::TenantAdmin, None, CardRefScope::default())
            }
            PrincipalKind::User => (PrincipalKindTag::User, None, CardRefScope::default()),
            PrincipalKind::Service {
                card_ref,
                card_ref_scope,
            } => (
                PrincipalKindTag::Service,
                card_ref.clone(),
                card_ref_scope.clone(),
            ),
            PrincipalKind::Agent {
                card_ref,
                card_ref_scope,
            } => (
                PrincipalKindTag::Agent,
                Some(card_ref.clone()),
                card_ref_scope.clone(),
            ),
        };
        Self {
            id: principal.id,
            kind,
            tenant_id: principal.tenant_id,
            card_ref,
            card_ref_scope,
        }
    }
}

impl AccessTokenClaims {
    /// Convert verified claims into a runtime verified-token envelope.
    ///
    /// The runtime principal is built directly from the claims: its effective
    /// permissions are the `permissions` claim and nothing is read from a
    /// store. `roles` is copied through as informational metadata.
    ///
    /// # Errors
    /// Returns an error when card-bound principal invariants fail, the
    /// delegation chain is too deep or malformed, or the expiry timestamp is
    /// out of range.
    pub fn into_verified(&self) -> Result<VerifiedToken, AuthError> {
        let kind = wire_kind_into_principal_kind(
            self.principal.kind,
            self.principal.card_ref.as_ref(),
            &self.principal.card_ref_scope,
        )?;
        let principal = Principal::new(
            self.principal.id,
            kind,
            self.principal.tenant_id,
            self.roles.clone(),
            self.permissions.clone(),
        )
        .with_credential_id(self.cid.as_deref().and_then(|cid| cid.parse().ok()));
        let delegation_chain = flatten_act_chain(self.act.as_deref())?;
        let exp =
            DateTime::<Utc>::from_timestamp(self.exp as i64, 0).ok_or(AuthError::InvalidToken)?;

        Ok(VerifiedToken {
            principal,
            delegation_chain,
            exp,
        })
    }
}

/// Resolve the wire principal-kind tag and Card binding into a `PrincipalKind`.
///
/// This is where the closed kind set meets the Card-binding rule: Service
/// binding is optional, Agent binding is required, administrative kinds carry
/// no Card, and a platform tag is never admissible on the tenant plane.
///
/// # Errors
/// Returns [`AuthError::InvalidToken`] for a platform-scope tag, which cannot
/// name a tenant principal, and [`AuthError::InvalidCardRef`] when the token's
/// Card binding does not match what the kind allows — a Card on an
/// administrative or User principal, a missing or wrong-kind Card on an Agent,
/// or a non-Service Card on a Service.
fn wire_kind_into_principal_kind(
    wire: PrincipalKindTag,
    card_ref: Option<&CardRef>,
    card_ref_scope: &CardRefScope,
) -> Result<PrincipalKind, AuthError> {
    match (wire, card_ref) {
        // A platform-scope kind can never become a tenant-scope principal. This
        // is the type-level half of the control-plane boundary: even a validly
        // signed token cannot smuggle a platform identity into a tenant.
        (PrincipalKindTag::GlobalAdmin, _) => Err(AuthError::InvalidToken),
        (PrincipalKindTag::TenantAdmin, Some(_)) => Err(AuthError::InvalidCardRef),
        (PrincipalKindTag::TenantAdmin, None) => Ok(PrincipalKind::TenantAdmin),
        (PrincipalKindTag::User, Some(_)) => Err(AuthError::InvalidCardRef),
        (PrincipalKindTag::User, None) => Ok(PrincipalKind::User),
        (PrincipalKindTag::Service, Some(card_ref)) if card_ref.kind == CardKind::Service => {
            Ok(PrincipalKind::Service {
                card_ref: Some(card_ref.clone()),
                card_ref_scope: seed_scope(card_ref, card_ref_scope)?,
            })
        }
        // Card-free tenant automation: a machine principal that holds a
        // credential without being a deployed, Card-bound workload. It carries
        // no emit scope, so an empty scope is the only valid one.
        (PrincipalKindTag::Service, None) if card_ref_scope.is_empty() => {
            Ok(PrincipalKind::Service {
                card_ref: None,
                card_ref_scope: CardRefScope::default(),
            })
        }
        (PrincipalKindTag::Agent, Some(card_ref)) if card_ref.kind == CardKind::Agent => {
            Ok(PrincipalKind::Agent {
                card_ref: card_ref.clone(),
                card_ref_scope: seed_scope(card_ref, card_ref_scope)?,
            })
        }
        (PrincipalKindTag::Service | PrincipalKindTag::Agent, _) => Err(AuthError::InvalidCardRef),
    }
}

fn seed_scope(card_ref: &CardRef, wire_scope: &CardRefScope) -> Result<CardRefScope, AuthError> {
    if wire_scope.is_empty() {
        return Ok(CardRefScope::own(card_ref));
    }
    if !wire_scope.permits_root(card_ref) {
        return Err(AuthError::CardScopeMissingRoot);
    }
    Ok(wire_scope.clone())
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

fn flatten_act_chain(mut act: Option<&ActClaim>) -> Result<Vec<DelegationStep>, AuthError> {
    let mut out = Vec::new();
    while let Some(layer) = act {
        if out.len() >= MAX_DELEGATION_DEPTH {
            return Err(AuthError::DelegationDepthExceeded);
        }
        let kind = wire_kind_into_principal_kind(
            layer.principal.kind,
            layer.principal.card_ref.as_ref(),
            &CardRefScope::default(),
        )?;
        out.push(DelegationStep {
            principal: RuntimePrincipalRef {
                id: layer.principal.id,
                kind,
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

    use jsonwebtoken::{Algorithm, EncodingKey, Header, Validation, encode};
    use secrecy::SecretString;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, IssuerConfigResolver, JwksCache, OidcError,
        TrustedIssuer,
    };
    use wyrd_runtime::{Permission, PermissionSet};
    use wyrd_runtime::{Principal, PrincipalId, PrincipalKind, RoleRef};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerTokenPolicy;
    use wyrd_spec::auth::IssuerUrl;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};

    use super::{
        AccessTokenClaims, ActClaim, AuthError, ExternalVerifier, Kid, MAX_BEARER_TOKEN_BYTES,
        MAX_DELEGATION_DEPTH, PrincipalKindTag, TokenPrincipalRef, TokenVerifier,
        WYRD_ACCESS_TOKEN_AUDIENCE, WyrdAuthVerifySettings, decode_kid, public_key_from_pem,
        verify_eddsa, verify_eddsa_with,
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
        let card_ref = card_ref(CardKind::Service);
        let principal = Principal::new(
            principal_id(),
            PrincipalKind::Service {
                card_ref: Some(card_ref.clone()),
                card_ref_scope: CardRefScope::own(&card_ref),
            },
            tenant_id(),
            vec![role()],
            wyrd_runtime::PermissionSet::new(),
        );

        let projected = TokenPrincipalRef::from(&principal);

        assert_eq!(projected.id, principal.id);
        assert_eq!(projected.kind, PrincipalKindTag::Service);
        assert_eq!(projected.tenant_id, principal.tenant_id);
        assert_eq!(projected.card_ref, Some(card_ref.clone()));
        assert_eq!(projected.card_ref_scope, CardRefScope::own(&card_ref));
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

    #[test]
    fn into_verified_carries_permissions_claim_and_flattens_delegation_initiator_first() {
        let initiator = service_ref("initiator");
        let immediate = service_ref("immediate");
        let claims = AccessTokenClaims {
            principal: service_ref("current"),
            roles: vec![role()],
            act: Some(Box::new(ActClaim {
                sub: immediate.id.to_string(),
                principal: immediate.clone(),
                act: Some(Box::new(ActClaim {
                    sub: initiator.id.to_string(),
                    principal: initiator.clone(),
                    act: None,
                })),
            })),
            ..claims_with_times(now() + 3_600, now())
        };

        let verified = claims.into_verified().expect("claims convert");

        assert!(
            verified
                .principal
                .effective_permissions
                .contains(&Permission::card_read())
        );
        assert_eq!(verified.delegation_chain.len(), 2);
        assert_eq!(
            verified.delegation_chain[0].principal.card_ref(),
            initiator.card_ref.as_ref()
        );
        assert_eq!(
            verified.delegation_chain[1].principal.card_ref(),
            immediate.card_ref.as_ref()
        );
    }

    /// A Card-free service is a valid machine principal: Card binding is a
    /// property of a deployed workload, not a precondition for holding a
    /// credential. It resolves with no bound Card and therefore no emit scope.
    #[test]
    fn into_verified_accepts_card_free_service_with_empty_scope() {
        let claims = AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::Service,
                card_ref: None,
                card_ref_scope: CardRefScope::default(),
                ..user_ref()
            },
            ..claims_with_times(now() + 3_600, now())
        };

        let verified = claims.into_verified().expect("card-free service resolves");

        assert!(matches!(
            verified.principal.kind,
            PrincipalKind::Service { card_ref: None, .. }
        ));
        assert_eq!(verified.principal.card_ref(), None);
        assert!(
            verified
                .principal
                .card_ref_scope()
                .expect("service reports a scope")
                .is_empty(),
            "a Card-free service carries no emit authority"
        );
    }

    /// A Card-free service claiming emit scope is refused: scope is derived
    /// from a bound Card, so a populated scope without one is unattributable.
    #[test]
    fn into_verified_rejects_card_free_service_claiming_scope() {
        let borrowed = card_ref(CardKind::Service);
        let claims = AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::Service,
                card_ref: None,
                card_ref_scope: CardRefScope::own(&borrowed),
                ..user_ref()
            },
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified();

        assert!(matches!(result, Err(AuthError::InvalidCardRef)));
    }

    /// A platform-scope kind can never resolve to a tenant-scope principal,
    /// even from an otherwise valid token.
    #[test]
    fn into_verified_rejects_platform_scope_kind() {
        let claims = AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::GlobalAdmin,
                card_ref: None,
                ..user_ref()
            },
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified();

        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[test]
    fn into_verified_rejects_card_ref_kind_mismatch() {
        let claims = AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::Agent,
                card_ref: Some(card_ref(CardKind::Service)),
                ..user_ref()
            },
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified();

        assert!(matches!(result, Err(AuthError::InvalidCardRef)));
    }

    #[test]
    fn agent_token_with_card_ref_promotes_to_typed_kind() {
        let card_ref = card_ref(CardKind::Agent);
        let claims = AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::Agent,
                card_ref: Some(card_ref.clone()),
                ..user_ref()
            },
            ..claims_with_times(now() + 3_600, now())
        };

        let verified = claims.into_verified().expect("agent claims verify");

        assert!(matches!(
            verified.principal.kind,
            PrincipalKind::Agent { card_ref: ref actual, .. } if actual == &card_ref
        ));
    }

    #[test]
    fn into_verified_rejects_scope_missing_root_card() {
        // Forge a service token whose card_ref_scope does NOT contain the card_ref.
        // The scope is built from a different card ("other-service"), but card_ref
        // is "billing". seed_scope should reject with CardScopeMissingRoot.
        let card_ref = card_ref(CardKind::Service);
        let other_card = named_card_ref(CardKind::Service, "other-service");
        let claims = AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::Service,
                card_ref: Some(card_ref.clone()),
                card_ref_scope: CardRefScope::own(&other_card),
                ..user_ref()
            },
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified();

        assert!(
            matches!(result, Err(AuthError::CardScopeMissingRoot)),
            "scope missing root should fail: {result:?}"
        );
    }

    #[test]
    fn into_verified_accepts_scope_containing_root_card() {
        let card_ref = card_ref(CardKind::Service);
        let claims = AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::Service,
                card_ref: Some(card_ref.clone()),
                card_ref_scope: CardRefScope::own(&card_ref),
                ..user_ref()
            },
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified();

        assert!(
            result.is_ok(),
            "scope containing root should verify: {result:?}"
        );
        if let Ok(verified) = result {
            assert!(matches!(
                verified.principal.kind,
                PrincipalKind::Service { card_ref: Some(ref actual), .. } if actual == &card_ref
            ));
        }
    }

    #[test]
    fn into_verified_rejects_delegation_depth_over_max() {
        let claims = AccessTokenClaims {
            act: Some(Box::new(act_chain(MAX_DELEGATION_DEPTH + 1))),
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified();

        assert!(matches!(result, Err(AuthError::DelegationDepthExceeded)));
    }

    #[test]
    fn into_verified_accepts_delegation_depth_at_max() {
        let claims = AccessTokenClaims {
            act: Some(Box::new(act_chain(MAX_DELEGATION_DEPTH))),
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified();

        assert!(result.is_ok());
    }

    // -------------------------------------------------------------------------
    // F11: principal-epoch revocation
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

    /// An oversized bearer is refused before any decoding.
    #[test]
    fn token_verifier_rejects_oversize_token() {
        let token = SecretString::from("x".repeat(MAX_BEARER_TOKEN_BYTES + 1));

        let result = verifier(WyrdAuthVerifySettings::default()).verify(&token, &tenant_id());

        assert!(matches!(result, Err(AuthError::BadTokenFormat)));
    }

    /// Validation matching [`TokenVerifier`]'s tenant access-token policy.
    fn wyrd_validation() -> Validation {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_issuer(&["wyrd"]);
        validation.set_audience(&[WYRD_ACCESS_TOKEN_AUDIENCE]);
        validation
    }

    fn claims_with_times(exp: usize, iat: usize) -> AccessTokenClaims {
        AccessTokenClaims {
            sub: principal_id().to_string(),
            principal: TokenPrincipalRef {
                id: principal_id(),
                kind: PrincipalKindTag::User,
                tenant_id: tenant_id(),
                card_ref: None,
                card_ref_scope: CardRefScope::default(),
            },
            roles: vec![role()],
            permissions: PermissionSet::from_iter([Permission::card_read()]),
            act: None,
            aud: WYRD_ACCESS_TOKEN_AUDIENCE.to_owned(),
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

    fn card_ref(kind: CardKind) -> CardRef {
        CardRef {
            kind,
            name: CardName::new("billing").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
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
            card_ref: None,
            card_ref_scope: CardRefScope::default(),
        }
    }

    fn service_ref(name: &str) -> TokenPrincipalRef {
        let card_ref = named_card_ref(CardKind::Service, name);
        TokenPrincipalRef {
            id: principal_id(),
            kind: PrincipalKindTag::Service,
            tenant_id: tenant_id(),
            card_ref: Some(card_ref.clone()),
            card_ref_scope: CardRefScope::own(&card_ref),
        }
    }

    fn named_card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    fn tenant_id() -> DataTenantId {
        "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id is valid")
    }

    fn role() -> RoleRef {
        RoleRef::new("runtime_admin").expect("static role is valid")
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
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after unix epoch")
            .as_secs() as usize
    }

    fn act_chain(depth: usize) -> ActClaim {
        let mut act = ActClaim {
            sub: principal_id().to_string(),
            principal: service_ref("layer"),
            act: None,
        };
        for _ in 1..depth {
            act = ActClaim {
                sub: principal_id().to_string(),
                principal: service_ref("layer"),
                act: Some(Box::new(act)),
            };
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
            Duration::from_secs(300),
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
            jwks_ttl: Duration::from_secs(3600),
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
}
