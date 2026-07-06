//! Boot-time trusted-issuer seeding for self-hosted deployments.
//!
//! Self-hosted Wyrd declares its trusted OIDC issuers in `[[trusted_issuers]]`
//! config. At boot, [`seed_trusted_issuers`] runs OIDC discovery per entry
//! (under a bounded retry schedule), maps each config DTO to the
//! [`TrustedIssuer`] domain type, encrypts any client secret with the sealing
//! key, and upserts the row into `wyrd.auth_trusted_issuers`. Postgres is then
//! the single source of issuer resolution for every request via
//! [`crate::auth::pg_resolvers::PgIssuerResolver`].
//!
//! Issuer trust is tenant-scoped (F02). Boot has no request `Host` to derive
//! the tenant from, so the operator declares the deployment's implicit tenant
//! via `[auth] tenant_slug`. [`resolve_implicit_tenant`] resolves that slug
//! through the SAME `resolve_by_slug_for_app` path the request handlers use, so
//! the bound `DataTenantId` equals what the resolver reads at request time, by
//! construction.
//!
//! Boot fails closed: a discovery that stays unreachable across the retry
//! schedule for an issuer that has never been seeded, or a slug that does not
//! resolve, aborts boot. A discovery that is unreachable for an issuer whose row
//! already exists (a restart while the IdP is briefly down) is non-fatal — the
//! existing row is kept.

use std::time::Duration;

use sqlx::PgPool;
use url::Url;
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, OidcProvider, IssuerTokenPolicy, ProviderMetadata,
    TrustedIssuer, WorkloadBinding,
};
use wyrd_crypt::SecretKey;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::{DataTenantId, TenantSlug};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    trusted_issuer_exists, upsert_trusted_issuer, upsert_workload_binding,
};

use crate::auth::pg_resolvers::{binding_write_from_binding, issuer_write_from_trusted};
use crate::boot::ServerBootError;
use crate::config::{ClaimMappingEntry, ClientAuthEntry, IssuerEntry, IssuerTokenPolicy};

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

/// Seed every `[[trusted_issuers]]` config entry into Postgres for `tenant_id`.
///
/// For each entry: run OIDC discovery (bounded retry), map the config DTO to a
/// [`TrustedIssuer`], encrypt any client secret with `sealing_key`, and upsert
/// the row (`ON CONFLICT DO UPDATE`, so re-seeding from config is idempotent).
/// All writes share one tenant transaction, committed once at the end.
///
/// Discovery that is unreachable for an issuer whose row already exists is
/// non-fatal (a restart while the IdP is briefly down keeps the prior row).
/// Discovery that is unreachable for an issuer that has never been seeded fails
/// boot closed.
///
/// # Errors
/// Returns [`ServerBootError::IssuerDiscoveryUnavailable`] for an unreachable,
/// never-seeded issuer; [`ServerBootError::IssuerSeal`] when a secret-bearing
/// issuer has no sealing key; and [`ServerBootError::Sql`] on a write failure.
pub async fn seed_trusted_issuers(
    pool: &PgPool,
    tenant_id: DataTenantId,
    entries: &[IssuerEntry],
    sealing_key: Option<&SecretKey>,
) -> Result<(), ServerBootError> {
    if entries.is_empty() {
        return Ok(());
    }
    let http = reqwest::Client::new();
    let mut conn = TenantConn::acquire(pool, tenant_id).await?;
    for entry in entries {
        seed_one_trusted_issuer(&mut conn, tenant_id, entry, sealing_key, &http).await?;
    }
    conn.commit().await?;
    Ok(())
}

/// Seed a single trusted issuer, honoring the already-seeded restart exemption.
async fn seed_one_trusted_issuer(
    conn: &mut TenantConn<'_>,
    tenant_id: DataTenantId,
    entry: &IssuerEntry,
    sealing_key: Option<&SecretKey>,
    http: &reqwest::Client,
) -> Result<(), ServerBootError> {
    match discover_with_retry(&entry.issuer, http).await {
        Ok(metadata) => {
            let trusted = build_trusted_issuer(entry, tenant_id, &metadata)?;
            let write = issuer_write_from_trusted(&trusted, sealing_key).map_err(|error| {
                ServerBootError::IssuerSeal {
                    issuer: entry.issuer.clone(),
                    message: error.to_string(),
                }
            })?;
            upsert_trusted_issuer(conn, &write)
                .await
                .map_err(|error| ServerBootError::Sql(error.into()))?;
            Ok(())
        }
        Err(discovery_error) => {
            // A normalized, parseable URL is required even to check existence;
            // a malformed issuer fails closed regardless.
            let Ok(issuer_url) = IssuerUrl::new(entry.issuer.clone()) else {
                return Err(discovery_error);
            };
            let exists = trusted_issuer_exists(conn, issuer_url.as_str())
                .await
                .map_err(|error| ServerBootError::Sql(error.into()))?;
            if exists {
                tracing::warn!(
                    issuer = %issuer_url.as_str(),
                    "OIDC discovery unreachable at boot but trusted issuer is already \
                     seeded; keeping the existing row"
                );
                Ok(())
            } else {
                Err(discovery_error)
            }
        }
    }
}

/// Seed `[[workload_bindings]]` domain bindings into Postgres for `tenant_id`.
///
/// Each binding's structured `card_ref` is encoded to JSONB and upserted
/// (`ON CONFLICT DO UPDATE`). The referenced trusted issuer must already be
/// seeded (FK), so callers seed issuers first. All writes share one tenant
/// transaction.
///
/// # Errors
/// Returns [`ServerBootError::InvalidWorkloadBinding`] when a binding's
/// `card_ref` cannot be serialized, and [`ServerBootError::Sql`] on a write
/// failure (including an FK violation when the issuer was not seeded).
pub async fn seed_workload_bindings(
    pool: &PgPool,
    tenant_id: DataTenantId,
    bindings: &[WorkloadBinding],
) -> Result<(), ServerBootError> {
    if bindings.is_empty() {
        return Ok(());
    }
    let mut conn = TenantConn::acquire(pool, tenant_id).await?;
    for (index, binding) in bindings.iter().enumerate() {
        let write = binding_write_from_binding(binding).map_err(|error| {
            ServerBootError::InvalidWorkloadBinding {
                index,
                message: error.to_string(),
            }
        })?;
        upsert_workload_binding(&mut conn, &write)
            .await
            .map_err(|error| ServerBootError::Sql(error.into()))?;
    }
    conn.commit().await?;
    Ok(())
}

/// Run OIDC discovery for `issuer`, retrying transient failures on the bounded
/// backoff schedule. Returns the discovered metadata or a fail-closed error.
async fn discover_with_retry(
    issuer: &str,
    http: &reqwest::Client,
) -> Result<ProviderMetadata, ServerBootError> {
    let issuer_url =
        Url::parse(issuer).map_err(|e| ServerBootError::IssuerDiscoveryUnavailable {
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
    let issuer = IssuerUrl::new(entry.issuer.clone()).map_err(|e| {
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

/// Map the config principal-kind DTO to the domain [`IssuerTokenPolicy`].
fn map_principal_kind(entry: IssuerTokenPolicy) -> IssuerTokenPolicy {
    match entry {
        IssuerTokenPolicy::Human => IssuerTokenPolicy::Human,
        IssuerTokenPolicy::Workload => IssuerTokenPolicy::Workload,
    }
}

#[cfg(test)]
mod tests {
    use secrecy::{ExposeSecret, SecretString};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_crypt::SecretKey;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_sql::queries::auth::upsert_trusted_issuer;

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
            token_endpoint: Some(
                format!("{issuer}/token")
                    .parse()
                    .expect("token url is valid"),
            ),
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
            principal_kind: IssuerTokenPolicy::Workload,
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
        assert_eq!(trusted.principal_kind, IssuerTokenPolicy::Workload);
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

    // A guaranteed-unreachable issuer (RFC 6761 `.invalid` TLD never resolves) so
    // discovery fails fast without a network dependency.
    const UNREACHABLE_ISSUER: &str = "https://idp.invalid/realms/acme";

    #[tokio::test]
    async fn seed_keeps_existing_row_when_discovery_unreachable_on_restart() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_id = fixture.data_tenant_id();
        let key = SecretKey::from_bytes([3_u8; 32]);

        // Pre-seed the row as a prior successful boot would have.
        let entry = issuer_entry(UNREACHABLE_ISSUER);
        let trusted = build_trusted_issuer(&entry, tenant_id, &fake_metadata(UNREACHABLE_ISSUER))
            .expect("issuer maps");
        let write = crate::auth::pg_resolvers::issuer_write_from_trusted(&trusted, Some(&key))
            .expect("issuer encodes");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn");
        upsert_trusted_issuer(&mut conn, &write)
            .await
            .expect("pre-seed upsert");
        conn.commit().await.expect("pre-seed commits");

        // Boot again with the IdP unreachable: the existing row makes this non-fatal.
        seed_trusted_issuers(fixture.app_pool(), tenant_id, &[entry], Some(&key))
            .await
            .expect("restart with unreachable but already-seeded issuer must not fail boot");
    }

    #[tokio::test]
    async fn seed_fails_closed_for_unreachable_unseeded_issuer() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_id = fixture.data_tenant_id();
        let key = SecretKey::from_bytes([4_u8; 32]);

        let entry = issuer_entry("https://idp.invalid/realms/never-seeded");
        let error = seed_trusted_issuers(fixture.app_pool(), tenant_id, &[entry], Some(&key))
            .await
            .expect_err("a never-seeded unreachable issuer must fail boot closed");
        assert!(
            matches!(error, ServerBootError::IssuerDiscoveryUnavailable { .. }),
            "expected IssuerDiscoveryUnavailable, got {error:?}"
        );
    }
}
