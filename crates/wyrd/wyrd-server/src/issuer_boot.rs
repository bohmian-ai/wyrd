//! Boot-time trusted-issuer resolution for self-hosted deployments.
//!
//! Self-hosted Wyrd declares its trusted OIDC issuers in `[[trusted_issuers]]`
//! config. At boot, [`ConfigFileIssuerResolver`] runs OIDC discovery per entry
//! (under a bounded retry schedule), maps each config DTO to the
//! [`TrustedIssuer`] domain type, and yields the issuers used to build the
//! static [`wyrd_auth_oidc::TrustedIssuerRegistry`].
//!
//! The registry is tenant-scoped (F02). Boot has no request `Host` to derive
//! the tenant from, so the operator declares the deployment's implicit tenant
//! via `[auth] tenant_slug`. [`resolve_implicit_tenant`] resolves that slug
//! through the SAME `resolve_by_slug_for_app` path the request handlers use, so
//! the bound `DataTenantId` equals what `TrustedIssuerRegistry::get` is keyed by
//! at request time, by construction.
//!
//! Boot fails closed: a discovery that stays unreachable across the retry
//! schedule, or a slug that does not resolve, aborts boot rather than starting
//! with a silently empty or mis-keyed registry.

use std::time::Duration;

use sqlx::PgPool;
use url::Url;
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, OidcProvider, PrincipalKindPolicy, ProviderMetadata,
    TrustedIssuer,
};
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::{DataTenantId, TenantSlug};

use crate::boot::ServerBootError;
use crate::config::{ClaimMappingEntry, ClientAuthEntry, IssuerEntry, PrincipalKindEntry};

/// Default JWKS key-cache TTL applied when an entry omits `jwks_ttl_secs`.
const DEFAULT_JWKS_TTL: Duration = Duration::from_secs(3600);

/// Maximum number of OIDC discovery attempts per issuer (initial try + retries).
const DISCOVERY_MAX_ATTEMPTS: u32 = 3;

/// Exponential backoff delay in milliseconds for a 0-based retry `attempt`.
///
/// Mirrors the `wyrd-client` transport schedule (attempt 0 → 100 ms, 1 → 1 s,
/// ≥2 → 5 s cap) so boot discovery backs off on the same curve. It is
/// `pub(crate)` to that crate, so this is an intentional copy, not an import.
fn backoff_ms(attempt: u32) -> u64 {
    const DELAYS: &[u64] = &[100, 1_000];
    DELAYS.get(attempt as usize).copied().unwrap_or(5_000)
}

/// Resolve the deployment's implicit tenant slug to its `DataTenantId`.
///
/// Reuses [`resolve_by_slug_for_app`](wyrd_sql::queries::platform::tenant_resolver::resolve_by_slug_for_app),
/// the same resolver the request handlers (`callback.rs`/`login.rs`/
/// `jwt_bearer.rs`) use, so the boot tenant matches request-time lookups.
/// Commit 03 reuses this helper for workload bindings.
///
/// # Errors
/// Returns [`ServerBootError::Sql`] when the resolver query fails, and
/// [`ServerBootError::TenantSlugUnresolved`] when the slug does not resolve to
/// an active tenant.
pub async fn resolve_implicit_tenant(
    pool: &PgPool,
    slug: &TenantSlug,
) -> Result<DataTenantId, ServerBootError> {
    match wyrd_sql::queries::platform::tenant_resolver::resolve_by_slug_for_app(pool, slug).await? {
        Some(tenant_id) => Ok(tenant_id),
        None => Err(ServerBootError::TenantSlugUnresolved {
            slug: slug.to_string(),
        }),
    }
}

/// Boot-time resolver that turns `[[trusted_issuers]]` config into the domain
/// [`TrustedIssuer`] list, filling each `jwks_uri` from OIDC discovery.
///
/// Lives in `wyrd-server` (it needs the server config DTOs) and is consumed
/// only at boot; the `IssuerConfigResolver` trait stays SQL-free in
/// `wyrd-auth-oidc`.
#[derive(Debug)]
pub struct ConfigFileIssuerResolver {
    issuers: Vec<IssuerEntry>,
    tenant_id: DataTenantId,
}

impl ConfigFileIssuerResolver {
    /// Build a resolver for the given config issuers, all bound to `tenant_id`.
    #[must_use]
    pub fn new(issuers: Vec<IssuerEntry>, tenant_id: DataTenantId) -> Self {
        Self { issuers, tenant_id }
    }

    /// Resolve every configured issuer into a [`TrustedIssuer`].
    ///
    /// Runs OIDC discovery per entry under the bounded retry schedule and maps
    /// the config DTO to the domain type. The returned issuers are all keyed to
    /// the resolver's implicit tenant.
    ///
    /// # Errors
    /// Returns [`ServerBootError::IssuerDiscoveryUnavailable`] when an issuer's
    /// discovery stays unreachable across the retry schedule, or when its
    /// configured URL is not a valid `https` issuer.
    pub async fn resolve(&self) -> Result<Vec<TrustedIssuer>, ServerBootError> {
        let http = reqwest::Client::new();
        let mut resolved = Vec::with_capacity(self.issuers.len());
        for entry in &self.issuers {
            let metadata = discover_with_retry(&entry.issuer, &http).await?;
            resolved.push(build_trusted_issuer(entry, self.tenant_id, &metadata)?);
        }
        Ok(resolved)
    }
}

/// Run OIDC discovery for `issuer`, retrying transient failures on the bounded
/// backoff schedule. Returns the discovered metadata or a fail-closed error.
async fn discover_with_retry(
    issuer: &str,
    http: &reqwest::Client,
) -> Result<ProviderMetadata, ServerBootError> {
    let issuer_url = Url::parse(issuer).map_err(|e| ServerBootError::IssuerDiscoveryUnavailable {
        issuer: issuer.to_owned(),
        message: format!("issuer URL could not be parsed: {e}"),
    })?;

    let mut last_message = String::from("discovery did not complete");
    for attempt in 0..DISCOVERY_MAX_ATTEMPTS {
        match OidcProvider::discover(issuer_url.clone(), http.clone()).await {
            Ok(provider) => return Ok(provider.metadata),
            Err(error) => {
                tracing::warn!(
                    issuer,
                    attempt,
                    error = %error,
                    "OIDC discovery attempt failed during boot"
                );
                last_message = error.to_string();
                if attempt + 1 < DISCOVERY_MAX_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(backoff_ms(attempt))).await;
                }
            }
        }
    }

    Err(ServerBootError::IssuerDiscoveryUnavailable {
        issuer: issuer.to_owned(),
        message: last_message,
    })
}

/// Map one config DTO plus its discovered metadata to a [`TrustedIssuer`].
///
/// `jwks_uri` comes solely from discovery (F04: never hand-authored from
/// config, no `card_ref` on this path).
fn build_trusted_issuer(
    entry: &IssuerEntry,
    tenant_id: DataTenantId,
    metadata: &ProviderMetadata,
) -> Result<TrustedIssuer, ServerBootError> {
    let issuer =
        IssuerUrl::new(entry.issuer.clone()).map_err(|e| {
            ServerBootError::IssuerDiscoveryUnavailable {
                issuer: entry.issuer.clone(),
                message: format!("issuer URL is not a valid https issuer: {e}"),
            }
        })?;

    Ok(TrustedIssuer {
        tenant_id,
        issuer,
        jwks_uri: metadata.jwks_uri.clone(),
        expected_audience: entry.expected_audience.clone(),
        client_id: entry.client_id.clone(),
        client_auth: map_client_auth(&entry.client_auth),
        claim_mapping: map_claim_mapping(&entry.claim_mapping),
        group_role_map: entry.group_role_map.clone(),
        default_roles: entry.default_roles.clone(),
        principal_kind: map_principal_kind(entry.principal_kind),
        jwks_ttl: entry
            .jwks_ttl_secs
            .map_or(DEFAULT_JWKS_TTL, Duration::from_secs),
    })
}

/// Map the config `client_auth` DTO to the domain enum, moving secrets across.
fn map_client_auth(entry: &ClientAuthEntry) -> ClientAuth {
    match entry {
        ClientAuthEntry::SecretBasic(secret) => ClientAuth::SecretBasic(secret.clone()),
        ClientAuthEntry::SecretPost(secret) => ClientAuth::SecretPost(secret.clone()),
        ClientAuthEntry::PrivateKeyJwt => ClientAuth::PrivateKeyJwt,
        ClientAuthEntry::Public => ClientAuth::Public,
    }
}

/// Map the config claim-mapping DTO to the domain [`ClaimMapping`].
fn map_claim_mapping(entry: &ClaimMappingEntry) -> ClaimMapping {
    ClaimMapping {
        subject: ClaimPath::new(entry.subject.clone()),
        email: entry.email.clone().map(ClaimPath::new),
        groups: entry.groups.clone().map(ClaimPath::new),
    }
}

/// Map the config principal-kind DTO to the domain [`PrincipalKindPolicy`].
fn map_principal_kind(entry: PrincipalKindEntry) -> PrincipalKindPolicy {
    match entry {
        PrincipalKindEntry::Human => PrincipalKindPolicy::Human,
        PrincipalKindEntry::Workload => PrincipalKindPolicy::Workload,
    }
}

#[cfg(test)]
mod tests {
    use secrecy::{ExposeSecret, SecretString};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_oidc::TrustedIssuerRegistry;

    use super::*;

    fn tenant() -> DataTenantId {
        "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant id is valid")
    }

    fn discovery_body(issuer: &str) -> serde_json::Value {
        serde_json::json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{issuer}/authorize"),
            "token_endpoint": format!("{issuer}/token"),
            "jwks_uri": format!("{issuer}/jwks"),
            "id_token_signing_alg_values_supported": ["RS256", "EdDSA"]
        })
    }

    fn fake_metadata(issuer: &str) -> ProviderMetadata {
        ProviderMetadata {
            issuer: issuer.to_owned(),
            authorization_endpoint: format!("{issuer}/authorize")
                .parse()
                .expect("authorize url is valid"),
            token_endpoint: Some(format!("{issuer}/token").parse().expect("token url is valid")),
            jwks_uri: format!("{issuer}/jwks").parse().expect("jwks url is valid"),
            id_token_signing_alg_values_supported: vec!["RS256".to_owned()],
        }
    }

    fn issuer_entry(issuer: &str) -> IssuerEntry {
        IssuerEntry {
            issuer: issuer.to_owned(),
            client_id: "wyrd-client".to_owned(),
            expected_audience: "wyrd-aud".to_owned(),
            client_auth: ClientAuthEntry::SecretPost(SecretString::from("top-secret".to_owned())),
            claim_mapping: ClaimMappingEntry {
                subject: "sub".to_owned(),
                email: Some("email".to_owned()),
                groups: Some("realm_access.roles".to_owned()),
            },
            group_role_map: std::collections::HashMap::new(),
            default_roles: vec!["viewer".to_owned()],
            principal_kind: PrincipalKindEntry::Workload,
            jwks_ttl_secs: None,
        }
    }

    #[tokio::test]
    async fn issuer_boot_discovery_happy_path_fills_jwks_uri() {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(discovery_body(&issuer)))
            .mount(&server)
            .await;

        let metadata = discover_with_retry(&issuer, &reqwest::Client::new())
            .await
            .expect("discovery should succeed");

        assert!(metadata.jwks_uri.as_str().ends_with("/jwks"));
    }

    #[tokio::test]
    async fn issuer_boot_discovery_fails_closed_after_retries() {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let error = discover_with_retry(&issuer, &reqwest::Client::new())
            .await
            .expect_err("unreachable issuer must fail closed");

        assert!(
            matches!(error, ServerBootError::IssuerDiscoveryUnavailable { .. }),
            "expected IssuerDiscoveryUnavailable, got {error:?}"
        );
        let rendered = error.to_string();
        assert!(
            rendered.contains("WYRD_AUTH_503_DISCOVERY_UNAVAILABLE"),
            "error must surface the stable 503 signal: {rendered}"
        );
        assert!(
            rendered.contains(&issuer),
            "error must name the offending issuer: {rendered}"
        );
    }

    #[tokio::test]
    async fn issuer_boot_discovery_recovers_after_transient_blip() {
        let server = MockServer::start().await;
        let issuer = server.uri();
        // First attempt fails, then discovery succeeds on retry.
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(discovery_body(&issuer)))
            .mount(&server)
            .await;

        let metadata = discover_with_retry(&issuer, &reqwest::Client::new())
            .await
            .expect("discovery should recover after a transient failure");

        assert!(metadata.jwks_uri.as_str().ends_with("/jwks"));
    }

    #[test]
    fn issuer_boot_build_trusted_issuer_maps_dto_fields() {
        let issuer = "https://idp.example.com/realms/acme";
        let entry = issuer_entry(issuer);
        let metadata = fake_metadata(issuer);

        let trusted =
            build_trusted_issuer(&entry, tenant(), &metadata).expect("mapping should succeed");

        assert_eq!(trusted.tenant_id, tenant());
        assert_eq!(trusted.issuer.as_str(), issuer);
        // jwks_uri comes from discovery metadata, never from config (F04).
        assert_eq!(trusted.jwks_uri.as_str(), format!("{issuer}/jwks"));
        assert_eq!(trusted.expected_audience, "wyrd-aud");
        assert_eq!(trusted.client_id, "wyrd-client");
        assert_eq!(trusted.principal_kind, PrincipalKindPolicy::Workload);
        assert_eq!(trusted.default_roles, vec!["viewer".to_owned()]);
        assert_eq!(trusted.jwks_ttl, DEFAULT_JWKS_TTL);
        assert_eq!(trusted.claim_mapping.subject.as_str(), "sub");
        assert_eq!(
            trusted.claim_mapping.groups.as_ref().map(ClaimPath::as_str),
            Some("realm_access.roles")
        );
        match trusted.client_auth {
            ClientAuth::SecretPost(secret) => {
                assert_eq!(secret.expose_secret(), "top-secret");
            }
            other => panic!("expected SecretPost, got {other:?}"),
        }
    }

    #[test]
    fn issuer_boot_build_trusted_issuer_rejects_non_https_issuer() {
        let issuer = "http://insecure.example.com";
        let entry = issuer_entry(issuer);
        let metadata = fake_metadata(issuer);

        let error = build_trusted_issuer(&entry, tenant(), &metadata)
            .expect_err("non-https issuer must fail closed");

        assert!(
            matches!(error, ServerBootError::IssuerDiscoveryUnavailable { .. }),
            "expected IssuerDiscoveryUnavailable, got {error:?}"
        );
    }

    #[test]
    fn issuer_boot_registry_is_keyed_by_implicit_tenant() {
        let issuer = "https://idp.example.com/realms/acme";
        let entry = issuer_entry(issuer);
        let metadata = fake_metadata(issuer);
        let trusted =
            build_trusted_issuer(&entry, tenant(), &metadata).expect("mapping should succeed");

        let registry = TrustedIssuerRegistry::from_issuers(vec![trusted]);

        let key = IssuerUrl::new(issuer).expect("issuer url is valid https");
        let looked_up = registry
            .get(&tenant(), &key)
            .expect("issuer must be reachable under the implicit tenant");
        assert_eq!(looked_up.client_id, "wyrd-client");

        let other_tenant: DataTenantId = "01890f28-7c4a-7000-98e7-4f4a3c2d1b02"
            .parse()
            .expect("static tenant id is valid");
        assert!(
            registry.get(&other_tenant, &key).is_none(),
            "issuer must not leak across tenants (F02)"
        );
    }
}
