//! Verify-only authentication helpers.

#![deny(missing_docs)]

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode_header};
use moka::future::Cache;
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wyrd_auth_oidc::{IssuerConfigResolver, JwksCache, OidcError, OidcKid, map_claims};
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
    /// Token was revoked.
    #[error("credential revoked")]
    Revoked,
    /// Bearer token format is invalid.
    #[error("bad token format")]
    BadTokenFormat,
    /// Permission resolution store is unavailable.
    #[error("token verification unavailable")]
    VerifyUnavailable,
    /// Stored role permissions are corrupt.
    #[error("role permissions are corrupt")]
    PermissionsCorrupt,
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(error: jsonwebtoken::errors::Error) -> Self {
        match error.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => Self::TokenExpired,
            _ => Self::Jwt(error),
        }
    }
}

/// Resolve role refs to an effective permission set.
pub trait PermissionResolver: Send + Sync + fmt::Debug {
    /// Resolve role permissions for a tenant.
    fn resolve<'a>(
        &'a self,
        tenant_id: &'a DataTenantId,
        roles: &'a [RoleRef],
    ) -> impl std::future::Future<Output = Result<PermissionSet, ResolveError>> + Send + 'a;
}

/// Permission resolution failure.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// The backing permission store is unavailable.
    #[error("permission store unavailable: {0}")]
    Unavailable(String),
    /// A stored role permission document is malformed.
    #[error("permissions JSONB malformed for role {role}: {source}")]
    BadPermissionsJson {
        /// Role whose permissions failed to decode.
        role: String,
        /// JSON decode error.
        #[source]
        source: serde_json::Error,
    },
}

/// Future returned by [`RevocationCheck::epoch`].
pub type RevocationEpochFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Option<DateTime<Utc>>, ResolveError>> + Send + 'a>,
>;

/// Check whether a principal's revocation epoch has been bumped.
///
/// The verifier calls this on every verify path — both cache hits and fresh
/// verifies — so a revoked principal's cached tokens are rejected before the
/// positive-cache early return (F11). Implementations are expected to serve
/// the result from a short-TTL in-process cache; the verifier does not add
/// network IO to the hot path.
///
/// Object-safe (returns a boxed `Future`) so `TokenVerifier<R>` can hold
/// `Option<Arc<dyn RevocationCheck>>` without an extra type parameter.
pub trait RevocationCheck: Send + Sync + fmt::Debug {
    /// Return the principal's revocation epoch, if any.
    ///
    /// `None` means the principal has never been revoked. An access token whose
    /// `iat < epoch` is dead.
    fn epoch<'a>(
        &'a self,
        tenant: &'a DataTenantId,
        principal: PrincipalId,
        kind: PrincipalKindTag,
    ) -> RevocationEpochFuture<'a>;
}

/// Zero-cost no-op revocation check used when revocation is not configured.
#[derive(Debug, Clone, Copy)]
pub struct NoRevocation;

impl RevocationCheck for NoRevocation {
    fn epoch<'a>(
        &'a self,
        _tenant: &'a DataTenantId,
        _principal: PrincipalId,
        _kind: PrincipalKindTag,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Option<DateTime<Utc>>, ResolveError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(std::future::ready(Ok(None)))
    }
}

/// Resolved, tenant-checked token ready to populate request context.
#[derive(Clone, Debug)]
pub struct VerifiedToken {
    /// Current actor with effective permissions filled.
    pub principal: Principal,
    /// Flattened RFC 8693 actor chain in initiator-first order.
    pub delegation_chain: Vec<DelegationStep>,
    /// JWT expiry as UTC timestamp for cache-hit lifetime checks.
    pub exp: DateTime<Utc>,
    /// JWT issued-at as UTC timestamp, used for epoch revocation checks (F11).
    pub iat: DateTime<Utc>,
}

/// Token verification settings.
#[derive(Clone, Debug)]
pub struct WyrdAuthVerifySettings {
    /// Token-hash cache TTL.
    pub cache_ttl: Duration,
    /// Maximum token-hash cache entries.
    pub max_cache_entries: u64,
    /// Allowed JWT clock skew.
    pub allowed_clock_skew: Duration,
}

impl Default for WyrdAuthVerifySettings {
    fn default() -> Self {
        Self {
            cache_ttl: Duration::from_secs(60),
            max_cache_entries: 10_000,
            allowed_clock_skew: Duration::from_secs(30),
        }
    }
}

/// Opaque hash of a presented bearer token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TokenHash([u8; 32]);

impl TokenHash {
    fn of(token: &str) -> Self {
        let mut hash = Sha256::new();
        hash.update(token.as_bytes());
        Self(hash.finalize().into())
    }
}

/// External OIDC verify path. `None` until `with_external` is called.
///
/// Generic over the issuer resolver `I` (RPITIT trait — not dyn-compatible),
/// so trust lookups happen per-request against the live config store.
struct ExternalVerify<I> {
    jwks: Arc<JwksCache>,
    trusted: Arc<I>,
}

impl<I> Clone for ExternalVerify<I> {
    fn clone(&self) -> Self {
        Self {
            jwks: Arc::clone(&self.jwks),
            trusted: Arc::clone(&self.trusted),
        }
    }
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
    pub raw_claims: serde_json::Value,
}

/// Stateful verifier with token-hash cache and resolver-backed permission refresh.
///
/// Parametrized by the permission resolver `R` and the issuer-config resolver
/// `I`. Both traits are RPITIT (not dyn-compatible), so the verifier holds them
/// generically behind trait bounds rather than as trait objects.
pub struct TokenVerifier<R: PermissionResolver, I: IssuerConfigResolver> {
    decoding_keys: Arc<HashMap<Kid, Arc<DecodingKey>>>,
    issuer: Arc<String>,
    resolver: Arc<R>,
    revocation: Option<Arc<dyn RevocationCheck>>,
    cache: Cache<TokenHash, Arc<VerifiedToken>>,
    settings: Arc<WyrdAuthVerifySettings>,
    external: Option<ExternalVerify<I>>,
}

impl<R: PermissionResolver, I: IssuerConfigResolver> Clone for TokenVerifier<R, I> {
    fn clone(&self) -> Self {
        Self {
            decoding_keys: Arc::clone(&self.decoding_keys),
            issuer: Arc::clone(&self.issuer),
            resolver: Arc::clone(&self.resolver),
            revocation: self.revocation.clone(),
            cache: self.cache.clone(),
            settings: Arc::clone(&self.settings),
            external: self.external.clone(),
        }
    }
}

impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> TokenVerifier<R, I> {
    /// Construct a token verifier.
    ///
    /// # Panics
    /// Panics when no decoding keys are supplied.
    #[must_use]
    pub fn new(
        decoding_keys: HashMap<Kid, Arc<DecodingKey>>,
        issuer: impl Into<String>,
        resolver: Arc<R>,
        settings: WyrdAuthVerifySettings,
    ) -> Self {
        assert!(
            !decoding_keys.is_empty(),
            "TokenVerifier requires at least one decoding key"
        );
        let cache = Cache::builder()
            .max_capacity(settings.max_cache_entries)
            .time_to_live(settings.cache_ttl)
            .build();
        Self {
            decoding_keys: Arc::new(decoding_keys),
            issuer: Arc::new(issuer.into()),
            resolver,
            revocation: None,
            cache,
            settings: Arc::new(settings),
            external: None,
        }
    }

    /// Attach an external OIDC verification path backed by a JWKS cache and an
    /// issuer-config resolver. Until this is called, `verify_external` always
    /// returns `AuthError::InvalidToken`.
    #[must_use]
    pub fn with_external(mut self, jwks: Arc<JwksCache>, trusted: Arc<I>) -> Self {
        self.external = Some(ExternalVerify { jwks, trusted });
        self
    }

    /// Attach a principal-epoch revocation resolver (F11).
    ///
    /// Any type implementing `RevocationCheck` is accepted. In production this
    /// is `SqlRevocationCheck`; in tests `NoRevocation` is the default when
    /// this method is never called.
    #[must_use]
    pub fn with_revocation(mut self, revocation: Arc<dyn RevocationCheck>) -> Self {
        self.revocation = Some(revocation);
        self
    }

    /// Verify a bearer token for the active tenant.
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
    pub async fn verify(
        &self,
        token: &SecretString,
        expected_tenant: &DataTenantId,
    ) -> Result<Arc<VerifiedToken>, AuthError> {
        let token = token.expose_secret();
        if token.len() > MAX_BEARER_TOKEN_BYTES {
            return Err(AuthError::BadTokenFormat);
        }

        let hash = TokenHash::of(token);
        if let Some(cached) = self.cache.get(&hash).await {
            if &cached.principal.tenant_id != expected_tenant {
                self.cache.invalidate(&hash).await;
                return Err(AuthError::InvalidToken);
            }
            if self.is_expired(cached.exp) {
                self.cache.invalidate(&hash).await;
                return Err(AuthError::TokenExpired);
            }
            // F11: check revocation epoch BEFORE returning the positive cache hit.
            // A principal revoked after this token was cached must be rejected here.
            if let Some(ref rev) = self.revocation {
                let kind = match &cached.principal.kind {
                    PrincipalKind::User => PrincipalKindTag::User,
                    PrincipalKind::Service { .. } => PrincipalKindTag::Service,
                    PrincipalKind::Agent { .. } => PrincipalKindTag::Agent,
                };
                match rev
                    .epoch(&cached.principal.tenant_id, cached.principal.id, kind)
                    .await
                {
                    Ok(Some(epoch)) if cached.iat < epoch => {
                        self.cache.invalidate(&hash).await;
                        return Err(AuthError::Revoked);
                    }
                    Err(err) => {
                        tracing::warn!(
                            error = %err,
                            "revocation resolver unavailable; failing open (token may be revoked)"
                        );
                    }
                    _ => {}
                }
            }
            return Ok(cached);
        }

        let result = self
            .cache
            .try_get_with(hash, async {
                let header = decode_header(token).map_err(AuthError::from)?;
                let kid = header
                    .kid
                    .ok_or(AuthError::InvalidToken)
                    .and_then(|kid| Kid::new(kid).map_err(|_| AuthError::InvalidToken))?;
                tracing::Span::current().record("kid", tracing::field::display(&kid));
                let key = Arc::clone(
                    self.decoding_keys
                        .get(&kid)
                        .ok_or(AuthError::InvalidToken)?,
                );

                let claims = verify_access_token(token, &key, self.validation())?;
                tracing::Span::current().record("principal_id", claims.principal.id.to_string());
                tracing::Span::current().record("jti", claims.jti.as_str());
                if &claims.principal.tenant_id != expected_tenant {
                    return Err(AuthError::InvalidToken);
                }
                let verified = claims.into_verified(&*self.resolver).await?;
                Ok::<_, AuthError>(Arc::new(verified))
            })
            .await
            .map_err(|error| AuthError::clone(&error))?;

        // Also check revocation for fresh (cache-miss) verifies.
        if let Some(ref rev) = self.revocation {
            let kind = match &result.principal.kind {
                PrincipalKind::User => PrincipalKindTag::User,
                PrincipalKind::Service { .. } => PrincipalKindTag::Service,
                PrincipalKind::Agent { .. } => PrincipalKindTag::Agent,
            };
            match rev
                .epoch(&result.principal.tenant_id, result.principal.id, kind)
                .await
            {
                Ok(Some(epoch)) if result.iat < epoch => {
                    self.cache.invalidate(&hash).await;
                    return Err(AuthError::Revoked);
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "revocation resolver unavailable; failing open (token may be revoked)"
                    );
                }
                _ => {}
            }
        }

        Ok(result)
    }

    /// Remove a token from the cache.
    #[tracing::instrument(level = "debug", skip(self, token))]
    pub async fn invalidate(&self, token: &SecretString) {
        self.cache
            .invalidate(&TokenHash::of(token.expose_secret()))
            .await;
    }

    /// Remove cached tokens for a principal from the cache.
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn invalidate_principal(&self, principal_id: PrincipalId) {
        let hashes = self
            .cache
            .iter()
            .filter_map(|(hash, verified)| (verified.principal.id == principal_id).then_some(*hash))
            .collect::<Vec<_>>();
        for hash in hashes {
            self.cache.invalidate(&hash).await;
        }
    }

    /// Verify a token issued by a trusted external OIDC issuer.
    ///
    /// Returns a [`VerifiedExternalIdentity`] with the verified subject, optional
    /// profile claims, and the raw JSON claims for downstream assertion checks
    /// (e.g. nonce). Does **not** build a `Principal` or call the
    /// `PermissionResolver` (R03 — the server flow owns that step).
    ///
    /// The lookup is keyed by `(tenant, iss)` (F02). Calling this with a
    /// Wyrd-minted local token (i.e. `iss == self.issuer`) returns an error;
    /// use `verify()` for those.
    ///
    /// # Errors
    /// - `AuthError::InvalidToken` — unknown issuer, bad token shape, wrong
    ///   audience, or the external path was not configured.
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

        let header = decode_header(token).map_err(AuthError::from)?;

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

        // Local tokens must go through verify(), not here.
        if iss_str == self.issuer.as_str() {
            return Err(AuthError::InvalidToken);
        }

        let ext = self.external.as_ref().ok_or(AuthError::InvalidToken)?;

        // Parse the issuer string into the typed form for the trust lookup.
        let iss_url = IssuerUrl::new(iss_str).map_err(|_| AuthError::InvalidToken)?;

        // Resolve this tenant's trusted issuers from the live config store, then
        // filter by the unverified `iss`. A resolver outage fails closed as
        // VerifyUnavailable; no matching issuer fails closed as InvalidToken
        // (untrusted or cross-tenant).
        let trusted = ext
            .trusted
            .trusted_issuers(tenant)
            .await
            .map_err(|_| AuthError::VerifyUnavailable)?
            .into_iter()
            .find(|ti| ti.issuer == iss_url)
            .ok_or(AuthError::InvalidToken)?;

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

        let decoding_key = ext
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

        Ok(VerifiedExternalIdentity {
            issuer: trusted.issuer.clone(),
            tenant_id: *tenant,
            subject: mapped.subject,
            email: mapped.email,
            groups: mapped.groups,
            principal_kind: trusted.principal_kind,
            expected_audience: trusted.expected_audience.clone(),
            raw_claims,
        })
    }

    fn validation(&self) -> Validation {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.validate_aud = false;
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.leeway = self.settings.allowed_clock_skew.as_secs();
        validation
    }

    fn is_expired(&self, exp: DateTime<Utc>) -> bool {
        let Ok(skew) = chrono::Duration::from_std(self.settings.allowed_clock_skew) else {
            return true;
        };
        Utc::now() > exp + skew
    }
}

/// Resolved Wyrd access-token claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessTokenClaims {
    /// Ultimate initiator, JWT `sub`.
    pub sub: String,
    /// Current actor whose roles are evaluated for authorization.
    pub principal: TokenPrincipalRef,
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
            kind: match &ref_.kind {
                PrincipalKind::User => PrincipalKindTag::User,
                PrincipalKind::Service { .. } => PrincipalKindTag::Service,
                PrincipalKind::Agent { .. } => PrincipalKindTag::Agent,
            },
            tenant_id,
            card_ref: ref_.card_ref().cloned(),
            card_ref_scope: ref_.card_ref_scope().cloned().unwrap_or_default(),
        }
    }
}

impl From<&Principal> for TokenPrincipalRef {
    fn from(principal: &Principal) -> Self {
        let (kind, card_ref, card_ref_scope) = match &principal.kind {
            PrincipalKind::User => (PrincipalKindTag::User, None, CardRefScope::default()),
            PrincipalKind::Service {
                card_ref,
                card_ref_scope,
            } => (
                PrincipalKindTag::Service,
                Some(card_ref.clone()),
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
    /// # Errors
    /// Returns an error when card-bound principal invariants fail, role resolution fails, the
    /// delegation chain is too deep, or the expiry timestamp is invalid.
    pub async fn into_verified<R: PermissionResolver>(
        &self,
        resolver: &R,
    ) -> Result<VerifiedToken, AuthError> {
        let kind = wire_kind_into_principal_kind(
            self.principal.kind,
            self.principal.card_ref.as_ref(),
            &self.principal.card_ref_scope,
        )?;
        let effective_permissions = resolver
            .resolve(&self.principal.tenant_id, &self.roles)
            .await
            .map_err(|error| match error {
                ResolveError::Unavailable(_) => AuthError::VerifyUnavailable,
                ResolveError::BadPermissionsJson { .. } => AuthError::PermissionsCorrupt,
            })?;
        let principal = Principal::new(
            self.principal.id,
            kind,
            self.principal.tenant_id,
            self.roles.clone(),
            effective_permissions,
        );
        let delegation_chain = flatten_act_chain(self.act.as_deref())?;
        let exp =
            DateTime::<Utc>::from_timestamp(self.exp as i64, 0).ok_or(AuthError::InvalidToken)?;
        let iat =
            DateTime::<Utc>::from_timestamp(self.iat as i64, 0).ok_or(AuthError::InvalidToken)?;

        Ok(VerifiedToken {
            principal,
            delegation_chain,
            exp,
            iat,
        })
    }
}

fn wire_kind_into_principal_kind(
    wire: PrincipalKindTag,
    card_ref: Option<&CardRef>,
    card_ref_scope: &CardRefScope,
) -> Result<PrincipalKind, AuthError> {
    match (wire, card_ref) {
        (PrincipalKindTag::User, Some(_)) => Err(AuthError::InvalidCardRef),
        (PrincipalKindTag::User, None) => Ok(PrincipalKind::User),
        (PrincipalKindTag::Service, Some(card_ref)) if card_ref.kind == CardKind::Service => {
            Ok(PrincipalKind::Service {
                card_ref: card_ref.clone(),
                card_ref_scope: seed_scope(card_ref, card_ref_scope)?,
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

/// Verify and decode access-token claims while preserving the private system-owner sentinel.
///
/// [`DataTenantId::new`] intentionally rejects non-v7 UUIDs, while the wire token format still
/// carries the nil UUID for the internal system owner. This helper keeps that exception private
/// to local-token verification instead of weakening the public tenant-id constructor.
fn verify_access_token(
    token: &str,
    public_key: &DecodingKey,
    mut validation: Validation,
) -> Result<AccessTokenClaims, AuthError> {
    validation.algorithms = vec![Algorithm::EdDSA];
    let mut raw = jsonwebtoken::decode::<serde_json::Value>(token, public_key, &validation)
        .map(|data| data.claims)
        .map_err(AuthError::from)?;
    let marker = DataTenantId::new_v7();
    replace_system_owner_tenant_ids(&mut raw, marker);
    let mut claims =
        serde_json::from_value::<AccessTokenClaims>(raw).map_err(|_| AuthError::InvalidToken)?;
    restore_system_owner_tenant_ids(&mut claims, marker);
    Ok(claims)
}

/// Replace nil tenant ids only in the access-token principal chain before strict
/// [`DataTenantId`] deserialization. Other claim fields remain strict so a nil
/// value cannot silently become a valid public tenant identifier.
fn replace_system_owner_tenant_ids(value: &mut serde_json::Value, marker: DataTenantId) {
    let Some(fields) = value.as_object_mut() else {
        return;
    };
    replace_principal_tenant_id(fields.get_mut("principal"), marker);
    let mut act = fields.get_mut("act");
    while let Some(layer) = act {
        let Some(layer_fields) = layer.as_object_mut() else {
            break;
        };
        replace_principal_tenant_id(layer_fields.get_mut("principal"), marker);
        act = layer_fields.get_mut("act");
    }
}

/// Rewrites one token principal's private system-owner sentinel for decoding.
fn replace_principal_tenant_id(value: Option<&mut serde_json::Value>, marker: DataTenantId) {
    let Some(serde_json::Value::Object(fields)) = value else {
        return;
    };
    if fields.get("tenant_id").and_then(serde_json::Value::as_str)
        == Some("00000000-0000-0000-0000-000000000000")
    {
        fields.insert(
            "tenant_id".to_owned(),
            serde_json::Value::String(marker.to_string()),
        );
    }
}

/// Restore marker tenant ids to the internal system-owner sentinel after decoding.
fn restore_system_owner_tenant_ids(claims: &mut AccessTokenClaims, marker: DataTenantId) {
    if claims.principal.tenant_id == marker {
        claims.principal.tenant_id = DataTenantId::SYSTEM_OWNER;
    }
    let mut layer = claims.act.as_deref_mut();
    while let Some(current) = layer {
        if current.principal.tenant_id == marker {
            current.principal.tenant_id = DataTenantId::SYSTEM_OWNER;
        }
        layer = current.act.as_deref_mut();
    }
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use chrono::{DateTime, Utc};

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
        AccessTokenClaims, ActClaim, AuthError, Kid, MAX_BEARER_TOKEN_BYTES, MAX_DELEGATION_DEPTH,
        PermissionResolver, PrincipalKindTag, ResolveError, RevocationCheck, TokenPrincipalRef,
        TokenVerifier, WyrdAuthVerifySettings, decode_kid, public_key_from_pem, verify_eddsa,
        verify_eddsa_with,
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

    #[tokio::test]
    async fn into_verified_resolves_permissions_and_flattens_delegation_initiator_first() {
        let resolver = TestResolver::default();
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

        let verified = claims
            .into_verified(&resolver)
            .await
            .expect("claims convert");

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
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn into_verified_rejects_non_user_without_card_ref() {
        let claims = AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::Service,
                card_ref: None,
                ..user_ref()
            },
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified(&TestResolver::default()).await;

        assert!(matches!(result, Err(AuthError::InvalidCardRef)));
    }

    #[tokio::test]
    async fn into_verified_rejects_card_ref_kind_mismatch() {
        let claims = AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::Agent,
                card_ref: Some(card_ref(CardKind::Service)),
                ..user_ref()
            },
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified(&TestResolver::default()).await;

        assert!(matches!(result, Err(AuthError::InvalidCardRef)));
    }

    #[tokio::test]
    async fn agent_token_with_card_ref_promotes_to_typed_kind() {
        let card_ref = card_ref(CardKind::Agent);
        let claims = AccessTokenClaims {
            principal: TokenPrincipalRef {
                kind: PrincipalKindTag::Agent,
                card_ref: Some(card_ref.clone()),
                ..user_ref()
            },
            ..claims_with_times(now() + 3_600, now())
        };

        let verified = claims
            .into_verified(&TestResolver::default())
            .await
            .expect("agent claims verify");

        assert!(matches!(
            verified.principal.kind,
            PrincipalKind::Agent { card_ref: ref actual, .. } if actual == &card_ref
        ));
    }

    #[tokio::test]
    async fn into_verified_rejects_scope_missing_root_card() {
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

        let result = claims.into_verified(&TestResolver::default()).await;

        assert!(
            matches!(result, Err(AuthError::CardScopeMissingRoot)),
            "scope missing root should fail: {result:?}"
        );
    }

    #[tokio::test]
    async fn into_verified_accepts_scope_containing_root_card() {
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

        let result = claims.into_verified(&TestResolver::default()).await;

        assert!(
            result.is_ok(),
            "scope containing root should verify: {result:?}"
        );
        if let Ok(verified) = result {
            assert!(matches!(
                verified.principal.kind,
                PrincipalKind::Service { card_ref: ref actual, .. } if actual == &card_ref
            ));
        }
    }

    #[tokio::test]
    async fn into_verified_rejects_delegation_depth_over_max() {
        let claims = AccessTokenClaims {
            act: Some(Box::new(act_chain(MAX_DELEGATION_DEPTH + 1))),
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified(&TestResolver::default()).await;

        assert!(matches!(result, Err(AuthError::DelegationDepthExceeded)));
    }

    #[tokio::test]
    async fn into_verified_accepts_delegation_depth_at_max() {
        let claims = AccessTokenClaims {
            act: Some(Box::new(act_chain(MAX_DELEGATION_DEPTH))),
            ..claims_with_times(now() + 3_600, now())
        };

        let result = claims.into_verified(&TestResolver::default()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn into_verified_maps_resolver_unavailable_to_verify_unavailable() {
        let resolver = TestResolver {
            unavailable: true,
            ..TestResolver::default()
        };
        let claims = claims_with_times(now() + 3_600, now());

        let result = claims.into_verified(&resolver).await;

        assert!(matches!(result, Err(AuthError::VerifyUnavailable)));
    }

    #[tokio::test]
    async fn token_verifier_caches_by_token_hash() {
        let resolver = Arc::new(TestResolver::default());
        let verifier = verifier(Arc::clone(&resolver), WyrdAuthVerifySettings::default());
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            now(),
        )));

        verifier
            .verify(&token, &tenant_id())
            .await
            .expect("first verify succeeds");
        verifier
            .verify(&token, &tenant_id())
            .await
            .expect("second verify succeeds from cache");

        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn token_verifier_rejects_cross_tenant_hit_and_evicts_cached_token() {
        let resolver = Arc::new(TestResolver::default());
        let verifier = verifier(Arc::clone(&resolver), WyrdAuthVerifySettings::default());
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            now(),
        )));

        verifier
            .verify(&token, &tenant_id())
            .await
            .expect("first verify succeeds");
        let wrong_tenant = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b09"
            .parse()
            .expect("static tenant id is valid");
        let result = verifier.verify(&token, &wrong_tenant).await;
        assert!(matches!(result, Err(AuthError::InvalidToken)));

        verifier
            .verify(&token, &tenant_id())
            .await
            .expect("verify after tenant mismatch re-resolves");
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn token_verifier_rejects_cross_tenant_miss_before_resolver() {
        let resolver = Arc::new(TestResolver::default());
        let verifier = verifier(Arc::clone(&resolver), WyrdAuthVerifySettings::default());
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            now(),
        )));
        let wrong_tenant = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b09"
            .parse()
            .expect("static tenant id is valid");

        let result = verifier.verify(&token, &wrong_tenant).await;

        assert!(matches!(result, Err(AuthError::InvalidToken)));
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn token_verifier_rejects_oversize_token() {
        let verifier = verifier(
            Arc::new(TestResolver::default()),
            WyrdAuthVerifySettings::default(),
        );
        let token = SecretString::from("x".repeat(MAX_BEARER_TOKEN_BYTES + 1));

        let result = verifier.verify(&token, &tenant_id()).await;

        assert!(matches!(result, Err(AuthError::BadTokenFormat)));
    }

    #[tokio::test]
    async fn invalidate_removes_entry_and_forces_re_resolve() {
        let resolver = Arc::new(TestResolver::default());
        let verifier = verifier(Arc::clone(&resolver), WyrdAuthVerifySettings::default());
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            now(),
        )));

        verifier
            .verify(&token, &tenant_id())
            .await
            .expect("first verify succeeds");
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);

        verifier.invalidate(&token).await;

        verifier
            .verify(&token, &tenant_id())
            .await
            .expect("verify after invalidate succeeds");
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn invalidate_principal_removes_matching_entries_and_forces_re_resolve() {
        let resolver = Arc::new(TestResolver::default());
        let verifier = verifier(Arc::clone(&resolver), WyrdAuthVerifySettings::default());
        let claims = claims_with_times(now() + 3_600, now());
        let principal_id = claims.principal.id;
        let token = SecretString::from(encode_eddsa_with_kid(&claims));

        verifier
            .verify(&token, &tenant_id())
            .await
            .expect("first verify succeeds");
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);

        verifier.invalidate_principal(principal_id).await;

        verifier
            .verify(&token, &tenant_id())
            .await
            .expect("verify after principal invalidate succeeds");
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 2);
    }

    // -------------------------------------------------------------------------
    // F11: principal-epoch revocation
    // -------------------------------------------------------------------------

    #[derive(Debug)]
    struct TestRevocation {
        epoch: Option<DateTime<Utc>>,
        unavailable: bool,
    }

    impl RevocationCheck for TestRevocation {
        fn epoch<'a>(
            &'a self,
            _tenant: &'a DataTenantId,
            _principal: PrincipalId,
            _kind: PrincipalKindTag,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Option<DateTime<Utc>>, ResolveError>>
                    + Send
                    + 'a,
            >,
        > {
            if self.unavailable {
                Box::pin(std::future::ready(Err(ResolveError::Unavailable(
                    "test outage".to_owned(),
                ))))
            } else {
                Box::pin(std::future::ready(Ok(self.epoch)))
            }
        }
    }

    fn verifier_with_revocation(
        resolver: Arc<TestResolver>,
        epoch: Option<DateTime<Utc>>,
    ) -> TokenVerifier<TestResolver, StubIssuerResolver> {
        let check = Arc::new(TestRevocation {
            epoch,
            unavailable: false,
        });
        verifier(resolver, WyrdAuthVerifySettings::default()).with_revocation(check)
    }

    #[tokio::test]
    async fn revocation_epoch_rejects_cache_hit_when_iat_predates_epoch() {
        let resolver = Arc::new(TestResolver::default());
        let iat_unix = now() - 10;
        let epoch =
            DateTime::from_timestamp(iat_unix as i64 + 1, 0).expect("static epoch is valid");
        let verifier = verifier_with_revocation(Arc::clone(&resolver), Some(epoch));
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            iat_unix,
        )));

        verifier
            .verify(&token, &tenant_id())
            .await
            .expect_err("fresh verify with iat < epoch must fail");

        let err = verifier.verify(&token, &tenant_id()).await;
        assert!(
            matches!(err, Err(AuthError::Revoked)),
            "both fresh and cached verify must return Revoked, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn revocation_epoch_passes_when_iat_at_or_after_epoch() {
        let resolver = Arc::new(TestResolver::default());
        let iat_unix = now() - 5;
        let epoch =
            DateTime::from_timestamp(iat_unix as i64 - 1, 0).expect("static epoch is valid");
        let verifier = verifier_with_revocation(Arc::clone(&resolver), Some(epoch));
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            iat_unix,
        )));

        verifier
            .verify(&token, &tenant_id())
            .await
            .expect("token issued after epoch must pass");
    }

    #[tokio::test]
    async fn revocation_unavailable_fails_open_and_allows_token() {
        let resolver = Arc::new(TestResolver::default());
        let check = Arc::new(TestRevocation {
            epoch: None,
            unavailable: true,
        });
        let verifier = verifier(resolver, WyrdAuthVerifySettings::default()).with_revocation(check);
        let token = SecretString::from(encode_eddsa_with_kid(&claims_with_times(
            now() + 3_600,
            now() - 5,
        )));

        verifier
            .verify(&token, &tenant_id())
            .await
            .expect("unavailable revocation resolver must fail open");
    }

    #[tokio::test]
    async fn is_expired_with_zero_skew_returns_true_for_past_timestamp() {
        let settings = WyrdAuthVerifySettings {
            allowed_clock_skew: Duration::ZERO,
            ..WyrdAuthVerifySettings::default()
        };
        let v = verifier(Arc::new(TestResolver::default()), settings);
        let past = DateTime::<Utc>::from_timestamp((now() as i64) - 3_600, 0)
            .expect("static past timestamp is valid");
        let future = DateTime::<Utc>::from_timestamp((now() as i64) + 3_600, 0)
            .expect("static future timestamp is valid");

        assert!(v.is_expired(past));
        assert!(!v.is_expired(future));
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
            space: SpaceName::new("prod").expect("static space is valid"),
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
            space: SpaceName::new("prod").expect("static space is valid"),
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

    fn verifier(
        resolver: Arc<TestResolver>,
        settings: WyrdAuthVerifySettings,
    ) -> TokenVerifier<TestResolver, StubIssuerResolver> {
        let mut keys = HashMap::new();
        keys.insert(
            Kid::new("k1").expect("kid is valid"),
            Arc::new(public_key()),
        );
        TokenVerifier::new(keys, "wyrd", resolver, settings)
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

    #[derive(Debug, Default)]
    struct TestResolver {
        calls: AtomicUsize,
        unavailable: bool,
        bad_json: bool,
    }

    impl PermissionResolver for TestResolver {
        #[allow(clippy::manual_async_fn)]
        fn resolve<'a>(
            &'a self,
            _tenant_id: &'a DataTenantId,
            _roles: &'a [RoleRef],
        ) -> impl std::future::Future<Output = Result<PermissionSet, ResolveError>> + Send + 'a
        {
            async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                if self.unavailable {
                    return Err(ResolveError::Unavailable("test outage".to_owned()));
                }
                if self.bad_json {
                    let source = serde_json::from_str::<serde_json::Value>("not json")
                        .expect_err("bad json is not valid");
                    return Err(ResolveError::BadPermissionsJson {
                        role: "test_role".to_owned(),
                        source,
                    });
                }
                Ok(PermissionSet::from_iter([Permission::card_read()]))
            }
        }
    }

    #[tokio::test]
    async fn into_verified_maps_bad_permissions_json_to_permissions_corrupt() {
        let resolver = TestResolver {
            bad_json: true,
            ..TestResolver::default()
        };
        let result = claims_with_times(now() + 3_600, now())
            .into_verified(&resolver)
            .await;
        assert!(matches!(result, Err(AuthError::PermissionsCorrupt)));
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
        Arc::new(JwksCache::new(
            reqwest::Client::new(),
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
        resolver: Arc<TestResolver>,
        trusted: TrustedIssuer,
        jwks: Arc<JwksCache>,
    ) -> TokenVerifier<TestResolver, StubIssuerResolver> {
        let stub = Arc::new(StubIssuerResolver::new(vec![trusted]));
        verifier(resolver, WyrdAuthVerifySettings::default()).with_external(jwks, stub)
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
        let resolver = Arc::new(TestResolver::default());
        let v = with_external_issuer(Arc::clone(&resolver), trusted, make_jwks_cache());

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
        // R03: no PermissionResolver call on the external path.
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
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
        let v = verifier(
            Arc::new(TestResolver::default()),
            WyrdAuthVerifySettings::default(),
        )
        .with_external(jwks, stub);

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
        let v = verifier(
            Arc::new(TestResolver::default()),
            WyrdAuthVerifySettings::default(),
        )
        .with_external(jwks, stub);

        let claims = external_claims(EXTERNAL_ISSUER, EXTERNAL_AUDIENCE, now() + 3_600, now());
        let token = encode_external_token(&claims, EXTERNAL_KID);

        let result = v.verify_external(&tenant_id(), &token).await;
        assert!(
            matches!(result, Err(AuthError::InvalidToken)),
            "untrusted issuer should be InvalidToken"
        );
    }

    #[tokio::test]
    async fn verify_external_without_external_configured_returns_invalid_token() {
        // No with_external() call → external path is None.
        let v = verifier(
            Arc::new(TestResolver::default()),
            WyrdAuthVerifySettings::default(),
        );
        let claims = external_claims(EXTERNAL_ISSUER, EXTERNAL_AUDIENCE, now() + 3_600, now());
        let token = encode_external_token(&claims, EXTERNAL_KID);

        let result = v.verify_external(&tenant_id(), &token).await;
        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[tokio::test]
    async fn verify_external_rejects_local_issuer_token() {
        // Token with iss == local issuer ("wyrd") must go through verify(), not verify_external().
        let v = verifier(
            Arc::new(TestResolver::default()),
            WyrdAuthVerifySettings::default(),
        );
        let claims = claims_with_times(now() + 3_600, now()); // iss: "wyrd"
        let token = encode_eddsa_with_kid(&claims);

        let result = v.verify_external(&tenant_id(), &token).await;
        assert!(
            matches!(result, Err(AuthError::InvalidToken)),
            "local-issuer token should be rejected by verify_external"
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
        let v = with_external_issuer(
            Arc::new(TestResolver::default()),
            trusted,
            make_jwks_cache(),
        );

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
            ..WyrdAuthVerifySettings::default()
        };
        let trusted = make_trusted_issuer(tid, issuer, EXTERNAL_AUDIENCE, jwks_uri);
        let stub = Arc::new(StubIssuerResolver::new(vec![trusted]));
        let v = verifier(Arc::new(TestResolver::default()), settings)
            .with_external(make_jwks_cache(), stub);

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
        let v = with_external_issuer(
            Arc::new(TestResolver::default()),
            trusted,
            make_jwks_cache(),
        );

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
            let v = verifier(
                Arc::new(TestResolver::default()),
                WyrdAuthVerifySettings::default(),
            )
            .with_external(Arc::clone(&jwks), Arc::clone(&stub));
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
        let v2 = verifier(
            Arc::new(TestResolver::default()),
            WyrdAuthVerifySettings::default(),
        )
        .with_external(Arc::clone(&jwks), stub2);
        let new_claims = external_claims(EXTERNAL_ISSUER, EXTERNAL_AUDIENCE, now() + 3_600, now());
        let new_token = encode_external_token(&new_claims, new_kid);
        v2.verify_external(&tid, &new_token)
            .await
            .expect("new kid should resolve after one JWKS refetch");
    }
}
