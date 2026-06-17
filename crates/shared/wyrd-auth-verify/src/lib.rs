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
use wyrd_runtime::{
    DelegationStep, PermissionSet, Principal, PrincipalId, PrincipalKind,
    PrincipalRef as RuntimePrincipalRef, RoleRef,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::reference::CardRef;

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

/// Resolved, tenant-checked token ready to populate request context.
#[derive(Clone, Debug)]
pub struct VerifiedToken {
    /// Current actor with effective permissions filled.
    pub principal: Principal,
    /// Flattened RFC 8693 actor chain in initiator-first order.
    pub delegation_chain: Vec<DelegationStep>,
    /// JWT expiry as UTC timestamp for cache-hit lifetime checks.
    pub exp: DateTime<Utc>,
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

/// Stateful verifier with token-hash cache and resolver-backed permission refresh.
#[derive(Clone)]
pub struct TokenVerifier<R: PermissionResolver> {
    decoding_keys: Arc<HashMap<Kid, Arc<DecodingKey>>>,
    issuer: Arc<String>,
    resolver: Arc<R>,
    cache: Cache<TokenHash, Arc<VerifiedToken>>,
    settings: Arc<WyrdAuthVerifySettings>,
}

impl<R: PermissionResolver + 'static> TokenVerifier<R> {
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
            cache,
            settings: Arc::new(settings),
        }
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

                let claims: AccessTokenClaims = verify_eddsa_with(token, &key, self.validation())?;
                tracing::Span::current().record("principal_id", claims.principal.id.to_string());
                tracing::Span::current().record("jti", claims.jti.as_str());
                if &claims.principal.tenant_id != expected_tenant {
                    return Err(AuthError::InvalidToken);
                }
                let verified = claims.into_verified(&*self.resolver).await?;
                Ok::<_, AuthError>(Arc::new(verified))
            })
            .await;

        result.map_err(|error| AuthError::clone(&error))
    }

    /// Remove a token from the cache.
    #[tracing::instrument(level = "debug", skip(self, token))]
    pub async fn invalidate(&self, token: &SecretString) {
        self.cache
            .invalidate(&TokenHash::of(token.expose_secret()))
            .await;
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

impl From<(&RuntimePrincipalRef, DataTenantId)> for TokenPrincipalRef {
    fn from((ref_, tenant_id): (&RuntimePrincipalRef, DataTenantId)) -> Self {
        Self {
            id: ref_.id,
            kind: match &ref_.kind {
                PrincipalKind::User => PrincipalKindWire::User,
                PrincipalKind::Service { .. } => PrincipalKindWire::Service,
                PrincipalKind::Agent { .. } => PrincipalKindWire::Agent,
            },
            tenant_id,
            card_ref: ref_.card_ref().cloned(),
        }
    }
}

impl From<&Principal> for TokenPrincipalRef {
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
        let kind =
            wire_kind_into_principal_kind(self.principal.kind, self.principal.card_ref.as_ref())?;
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

        Ok(VerifiedToken {
            principal,
            delegation_chain,
            exp,
        })
    }
}

fn wire_kind_into_principal_kind(
    wire: PrincipalKindWire,
    card_ref: Option<&CardRef>,
) -> Result<PrincipalKind, AuthError> {
    match (wire, card_ref) {
        (PrincipalKindWire::User, Some(_)) => Err(AuthError::InvalidCardRef),
        (PrincipalKindWire::User, None) => Ok(PrincipalKind::User),
        (PrincipalKindWire::Service, Some(card_ref)) if card_ref.kind == CardKind::Service => {
            Ok(PrincipalKind::Service {
                card_ref: card_ref.clone(),
            })
        }
        (PrincipalKindWire::Agent, Some(card_ref)) if card_ref.kind == CardKind::Agent => {
            Ok(PrincipalKind::Agent {
                card_ref: card_ref.clone(),
            })
        }
        (PrincipalKindWire::Service | PrincipalKindWire::Agent, _) => {
            Err(AuthError::InvalidCardRef)
        }
    }
}

fn flatten_act_chain(mut act: Option<&ActClaim>) -> Result<Vec<DelegationStep>, AuthError> {
    let mut out = Vec::new();
    while let Some(layer) = act {
        if out.len() >= MAX_DELEGATION_DEPTH {
            return Err(AuthError::DelegationDepthExceeded);
        }
        let kind =
            wire_kind_into_principal_kind(layer.principal.kind, layer.principal.card_ref.as_ref())?;
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
    use wyrd_runtime::{Permission, PermissionSet};
    use wyrd_runtime::{Principal, PrincipalId, PrincipalKind, RoleRef};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::version::VersionBlock;

    use super::{
        AccessTokenClaims, ActClaim, AuthError, Kid, MAX_BEARER_TOKEN_BYTES, MAX_DELEGATION_DEPTH,
        PermissionResolver, PrincipalKindWire, ResolveError, TokenPrincipalRef, TokenVerifier,
        WyrdAuthVerifySettings, decode_kid, public_key_from_pem, verify_eddsa, verify_eddsa_with,
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

        let projected = TokenPrincipalRef::from(&principal);

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
                kind: PrincipalKindWire::Service,
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
                kind: PrincipalKindWire::Agent,
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
                kind: PrincipalKindWire::Agent,
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
            PrincipalKind::Agent { card_ref: ref actual } if actual == &card_ref
        ));
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
            kind: PrincipalKindWire::User,
            tenant_id: tenant_id(),
            card_ref: None,
        }
    }

    fn service_ref(name: &str) -> TokenPrincipalRef {
        TokenPrincipalRef {
            id: principal_id(),
            kind: PrincipalKindWire::Service,
            tenant_id: tenant_id(),
            card_ref: Some(named_card_ref(CardKind::Service, name)),
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
    ) -> TokenVerifier<TestResolver> {
        let mut keys = HashMap::new();
        keys.insert(
            Kid::new("k1").expect("kid is valid"),
            Arc::new(public_key()),
        );
        TokenVerifier::new(keys, "wyrd", resolver, settings)
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
}
