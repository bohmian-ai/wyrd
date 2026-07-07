//! Trusted issuer registry and workload binding resolver.

use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use secrecy::SecretString;
use url::Url;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{IssuerTokenPolicy, IssuerUrl};
use wyrd_spec::reference::CardRef;

use crate::error::OidcError;

// --------------------------------------------------------------------------
// ClientAuth
// --------------------------------------------------------------------------

/// How this Wyrd deployment authenticates to the OIDC provider.
#[derive(Debug, Clone)]
pub enum ClientAuth {
    /// HTTP Basic auth with a shared secret (RFC 6749 §2.3.1).
    SecretBasic(SecretString),
    /// Secret sent in the POST body (RFC 6749 §2.3.1).
    SecretPost(SecretString),
    /// Private-key JWT (RFC 7523). Key reference resolved at runtime.
    PrivateKeyJwt,
    /// Public PKCE-only client — no client secret or assertion.
    /// The PKCE code verifier sent in the token exchange is the sole proof
    /// of possession. Used for browser/mobile flows where storing a secret
    /// is not possible.
    Public,
}

// --------------------------------------------------------------------------
// ClaimMapping
// --------------------------------------------------------------------------

/// Dotted JSON path into a token claims object (e.g. `realm_access.roles`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimPath(String);

impl ClaimPath {
    /// Wrap any string as a claim path. No format validation is imposed;
    /// segments are split on `.` at resolution time.
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Borrow as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ClaimPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Mapping from standard OIDC claim paths to Wyrd's normalized claim names.
///
/// # F04 — no `card_ref` here
///
/// External token claims identify the external subject only. A Wyrd `CardRef`
/// is never injected from token claims; it is resolved server-side via
/// [`WorkloadBindingResolver`] using the verified subject as the lookup key.
#[derive(Debug, Clone)]
pub struct ClaimMapping {
    /// Path to the external subject claim. Defaults to `"sub"` for standard
    /// OIDC tokens.
    pub subject: ClaimPath,
    /// Optional path to an email claim. Absent → no error; email is optional.
    pub email: Option<ClaimPath>,
    /// Optional path to a groups/roles array claim
    /// (e.g. `"realm_access.roles"` for Keycloak).
    pub groups: Option<ClaimPath>,
}

// --------------------------------------------------------------------------
// TrustedIssuer
// --------------------------------------------------------------------------

/// Configuration for a single trusted OIDC issuer within a tenant.
///
/// Keyed by `(tenant_id, issuer)` in [`TrustedIssuerRegistry`] (F02).
#[derive(Debug, Clone)]
pub struct TrustedIssuer {
    /// Tenant that trusts this issuer.
    pub tenant_id: DataTenantId,
    /// Normalized OIDC issuer URL.
    pub issuer: IssuerUrl,
    /// JWKS endpoint URL for fetching signing keys. Populated from OIDC
    /// discovery (`jwks_uri` field) or supplied directly for providers that do
    /// not publish a discovery document.
    pub jwks_uri: Url,
    /// The audience value (`aud` claim) expected in tokens from this issuer.
    /// Usually Wyrd's `client_id` registered at the IdP.
    pub expected_audience: String,
    /// Wyrd's OAuth 2.0 client identifier at this IdP.
    pub client_id: String,
    /// How Wyrd authenticates to this IdP when using the token endpoint.
    pub client_auth: ClientAuth,
    /// How to extract normalized claims from a verified token.
    pub claim_mapping: ClaimMapping,
    /// Per-issuer mapping from IdP group strings to Wyrd role names.
    pub group_role_map: HashMap<String, Vec<String>>,
    /// Baseline Wyrd role names granted to every federated user.
    pub default_roles: Vec<String>,
    /// Whether tokens represent humans or machine workloads.
    pub principal_kind: IssuerTokenPolicy,
    /// TTL for the JWKS key cache for this issuer.
    pub jwks_ttl: Duration,
}

// --------------------------------------------------------------------------
// WorkloadBinding
// --------------------------------------------------------------------------

/// Server-owned mapping from a verified external subject to a registered card.
///
/// # F04 — authoritative source of card binding
///
/// A workload token carries `sub` (the external subject identity, e.g.
/// a Kubernetes service account or SPIFFE ID). The server looks up the
/// corresponding registered card via [`WorkloadBindingResolver`]. The token
/// itself never carries a `card_ref`; allowing it to do so would let a
/// compromised workload claim any card's permissions.
#[derive(Debug, Clone)]
pub struct WorkloadBinding {
    /// Tenant that owns this binding.
    pub tenant_id: DataTenantId,
    /// Issuer that issued the token whose subject is bound here.
    pub issuer: IssuerUrl,
    /// The verified external subject from the token (the lookup key).
    pub subject: String,
    /// Optional audience constraint for additional precision.
    pub audience: Option<String>,
    /// The registered Wyrd card (Service or Agent) that this subject maps to.
    pub card_ref: CardRef,
}

/// Resolves a verified external subject to its registered Wyrd card.
///
/// Implementations are provided by the server layer: a config-file impl for
/// self-hosted deployments, a per-tenant SQL impl for cloud multi-tenant.
/// This crate stays SQL-free.
pub trait WorkloadBindingResolver: Send + Sync + std::fmt::Debug {
    /// Look up the registered card for a verified external subject.
    ///
    /// Returns `Ok(None)` when no binding exists for this subject (the server
    /// should respond with a `PrincipalNotFound` error). The token cannot
    /// supply or influence this mapping.
    fn binding(
        &self,
        tenant: &DataTenantId,
        issuer: &IssuerUrl,
        subject: &str,
        audience: Option<&str>,
    ) -> impl Future<Output = Result<Option<CardRef>, OidcError>> + Send;
}

// --------------------------------------------------------------------------
// TrustedIssuerRegistry
// --------------------------------------------------------------------------

/// In-memory registry of trusted issuers, keyed by `(DataTenantId, IssuerUrl)`.
///
/// Issuer trust is tenant-scoped (F02): the same issuer URL trusted by two
/// different tenants produces two independent [`TrustedIssuer`] entries. A
/// lookup on one tenant cannot resolve into another tenant's issuer config.
pub struct TrustedIssuerRegistry {
    inner: HashMap<(DataTenantId, IssuerUrl), TrustedIssuer>,
}

impl TrustedIssuerRegistry {
    /// Build a registry from an iterator of trusted issuers.
    pub fn from_issuers(issuers: impl IntoIterator<Item = TrustedIssuer>) -> Self {
        issuers.into_iter().collect()
    }

    /// Look up a trusted issuer by tenant and issuer URL.
    ///
    /// Returns `None` when the issuer is not trusted for this tenant.
    pub fn get(&self, tenant: &DataTenantId, iss: &IssuerUrl) -> Option<&TrustedIssuer> {
        self.inner.get(&(*tenant, iss.clone()))
    }
}

impl FromIterator<TrustedIssuer> for TrustedIssuerRegistry {
    fn from_iter<T: IntoIterator<Item = TrustedIssuer>>(issuers: T) -> Self {
        let inner = issuers
            .into_iter()
            .map(|ti| ((ti.tenant_id, ti.issuer.clone()), ti))
            .collect();
        Self { inner }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerUrl;

    use super::*;

    fn tenant(s: &str) -> DataTenantId {
        s.parse().expect("static tenant id is valid")
    }

    fn issuer(s: &str) -> IssuerUrl {
        IssuerUrl::new(s).expect("static issuer url is valid")
    }

    fn trusted_issuer(tenant_id: DataTenantId, issuer: IssuerUrl, audience: &str) -> TrustedIssuer {
        TrustedIssuer {
            tenant_id,
            issuer: issuer.clone(),
            jwks_uri: format!("{issuer}/.well-known/keys")
                .parse()
                .expect("static jwks uri is valid"),
            expected_audience: audience.to_owned(),
            client_id: "wyrd".to_owned(),
            client_auth: ClientAuth::PrivateKeyJwt,
            claim_mapping: ClaimMapping {
                subject: ClaimPath::new("sub"),
                email: Some(ClaimPath::new("email")),
                groups: Some(ClaimPath::new("realm_access.roles")),
            },
            group_role_map: HashMap::new(),
            default_roles: Vec::new(),
            principal_kind: IssuerTokenPolicy::Human,
            jwks_ttl: Duration::from_secs(3600),
        }
    }

    // Tenant IDs must be UUIDv7. These are valid v7 values.
    const TENANT_A: &str = "01890f28-7c4a-7000-98e7-4f4a3c2d1b01";
    const TENANT_B: &str = "01890f28-7c4a-7001-98e7-4f4a3c2d1b02";
    const ISSUER_URL: &str = "https://idp.example.com/realms/wyrd";

    #[test]
    fn same_issuer_different_tenants_resolves_independently() {
        let tenant_a = tenant(TENANT_A);
        let tenant_b = tenant(TENANT_B);
        let iss = issuer(ISSUER_URL);

        let registry = TrustedIssuerRegistry::from_issuers([
            trusted_issuer(tenant_a, iss.clone(), "aud-tenant-a"),
            trusted_issuer(tenant_b, iss.clone(), "aud-tenant-b"),
        ]);

        let a = registry
            .get(&tenant(TENANT_A), &iss)
            .expect("tenant A should resolve");
        let b = registry
            .get(&tenant(TENANT_B), &iss)
            .expect("tenant B should resolve");

        assert_eq!(a.expected_audience, "aud-tenant-a");
        assert_eq!(b.expected_audience, "aud-tenant-b");
        assert_ne!(a.expected_audience, b.expected_audience);
    }

    #[test]
    fn unknown_issuer_returns_none() {
        let tenant_a = tenant(TENANT_A);
        let iss = issuer(ISSUER_URL);
        let registry = TrustedIssuerRegistry::from_issuers([trusted_issuer(tenant_a, iss, "aud")]);

        let other_iss = issuer("https://other.example.com");
        assert!(
            registry.get(&tenant(TENANT_A), &other_iss).is_none(),
            "unknown issuer should return None"
        );
    }

    #[test]
    fn unknown_tenant_returns_none() {
        let tenant_a = tenant(TENANT_A);
        let iss = issuer(ISSUER_URL);
        let registry =
            TrustedIssuerRegistry::from_issuers([trusted_issuer(tenant_a, iss.clone(), "aud")]);

        assert!(
            registry.get(&tenant(TENANT_B), &iss).is_none(),
            "unknown tenant should return None even for known issuer"
        );
    }

    #[test]
    fn claim_mapping_has_no_card_ref_field() {
        // Compile-time regression guard for F04: ClaimMapping must not have a
        // card_ref field. If someone adds one, this test is a reminder that the
        // field must be removed.
        let mapping = ClaimMapping {
            subject: ClaimPath::new("sub"),
            email: None,
            groups: None,
        };
        // Verify that ClaimMapping only has the expected fields by exhaustive construction.
        let ClaimMapping {
            subject,
            email,
            groups,
        } = mapping;
        assert_eq!(subject.as_str(), "sub");
        assert!(email.is_none());
        assert!(groups.is_none());
    }
}
