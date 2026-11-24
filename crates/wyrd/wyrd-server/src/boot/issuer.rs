//! Boot-time trusted-issuer seeding for self-hosted deployments.
//!
//! Self-hosted Wyrd declares its trusted workload OIDC issuers in
//! `[[trusted_issuers]]` config; a `principal_kind = human` entry is refused,
//! because human login trust lives only in the tenant OIDC connection store. At boot, [`seed_trusted_issuers`] runs OIDC discovery per entry
//! (under a bounded retry schedule), maps each config DTO to the
//! [`TrustedIssuer`] domain type, encrypts any client secret with the sealing
//! key (keeping an unchanged committed envelope), and upserts the row into `wyrd.auth_trusted_issuers`. Postgres is then
//! the single source of issuer resolution for every request via
//! [`crate::auth::pg_resolvers::PgIssuerResolver`].
//!
//! Issuer trust is tenant-scoped (F02). Boot has no request `Host` to derive
//! the tenant from, so the operator declares the deployment's implicit tenant
//! via `[auth] tenant_slug`. [`resolve_implicit_tenant`] resolves that slug
//! through the SAME `WyrdPostgres::resolve_tenant_slug` the request handlers use, so
//! the bound `DataTenantId` equals what the resolver reads at request time, by
//! construction.
//!
//! Boot fails closed: a discovery that stays unreachable across the retry
//! schedule for an issuer that has never been seeded, or a slug that does not
//! resolve, aborts boot. A discovery that is unreachable for an issuer whose row
//! already exists (a restart while the IdP is briefly down) is non-fatal — the
//! existing row is kept.

use std::time::Duration;

use url::Url;
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, ScreenedHttp, TrustedIssuer, WorkloadBinding,
};
use wyrd_kms::WyrdKms;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::{DataTenantId, TenantSlug};
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::auth::{
    TrustedIssuerWrite, trusted_issuer_by_url, trusted_issuer_exists, upsert_trusted_issuer,
    upsert_workload_binding,
};
use wyrd_sql::row_types::auth::TrustedIssuerRow;

use crate::auth::pg_resolvers::{
    IssuerSealError, binding_write_from_binding, client_auth_label, issuer_write_from_trusted,
    issuer_write_with_envelope, trusted_issuer_secret_matches,
};
use crate::boot::ServerBootError;
use wyrd_spec::auth::IssuerTokenPolicy;

use crate::config::{ClaimMappingEntry, ClientAuthEntry, IssuerEntry};

/// Default JWKS key-cache TTL applied when an entry omits `jwks_ttl_secs`.
const DEFAULT_JWKS_TTL: Duration = Duration::from_hours(1);

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
/// Reuses [`WyrdPostgres::resolve_tenant_slug`], the one resolver tenant
/// login and workload exchange use, so the boot tenant matches request-time
/// lookups by construction.
///
/// # Errors
/// Returns [`ServerBootError::Sql`] when the resolver query fails or no
/// operator pool is configured, and
/// [`ServerBootError::TenantSlugUnresolved`] when the slug does not resolve to
/// an active tenant.
pub async fn resolve_implicit_tenant(
    postgres: &WyrdPostgres,
    slug: &TenantSlug,
) -> Result<DataTenantId, ServerBootError> {
    match postgres.resolve_tenant_slug(slug).await? {
        Some(tenant_id) => Ok(tenant_id),
        None => Err(ServerBootError::TenantSlugUnresolved {
            slug: slug.to_string(),
        }),
    }
}

/// Seed every `[[trusted_issuers]]` config entry into Postgres for `tenant_id`.
///
/// First read each entry's committed envelope in a short tenant transaction.
/// Then, with no transaction open: run OIDC discovery (bounded retry), map
/// the config DTO to a [`TrustedIssuer`], and seal any client secret through
/// `kms`, keeping the committed envelope byte-for-byte when it already seals
/// the configured secret for the configured client id and method, so a
/// restart under a newly active key moves no unchanged issuer. Then upsert every row (`ON CONFLICT DO UPDATE`, so
/// re-seeding from config is idempotent) in one tenant transaction opened
/// through [`WyrdPostgres::tenant_conn`], committed once at the end.
///
/// Discovery that is unreachable for an issuer whose row already exists is
/// non-fatal (a restart while the IdP is briefly down keeps the prior row).
/// Discovery that is unreachable for an issuer that has never been seeded fails
/// boot closed.
///
/// # Errors
/// Returns [`ServerBootError::HumanIssuerSeed`] before any IO when an entry is
/// `principal_kind = human`: human login trust lives only in the tenant OIDC
/// connection store. Returns [`ServerBootError::IssuerDiscoveryUnavailable`] when another Rustls
/// provider already owns the process or for an unreachable, never-seeded
/// issuer; [`ServerBootError::IssuerSeal`] when a secret-bearing issuer has no
/// `[kms]` or cannot be sealed; and [`ServerBootError::Sql`] on a write failure. Cancellation
/// may leave earlier issuer entries committed while later entries remain
/// unprocessed.
pub async fn seed_trusted_issuers(
    postgres: &WyrdPostgres,
    tenant_id: DataTenantId,
    entries: &[IssuerEntry],
    kms: Option<&WyrdKms>,
) -> Result<(), ServerBootError> {
    if entries.is_empty() {
        return Ok(());
    }
    if let Some(human) = entries
        .iter()
        .find(|entry| entry.principal_kind == IssuerTokenPolicy::Human)
    {
        return Err(ServerBootError::HumanIssuerSeed {
            issuer: human.issuer.clone(),
        });
    }
    wyrd_tls::install_crypto_provider().map_err(|error| {
        ServerBootError::IssuerDiscoveryUnavailable {
            issuer: "configured issuer".to_owned(),
            message: error.to_string(),
        }
    })?;
    // Boot issuers are operator configuration, so internal addresses stay
    // reachable; requests still go through the redirect-disabled transport.
    let discovery = ScreenedHttp::allowing_internal();
    let mut conn = postgres.tenant_conn(tenant_id).await?;
    let mut prior = Vec::with_capacity(entries.len());
    for entry in entries {
        let row = match IssuerUrl::new(entry.issuer.clone()) {
            Ok(issuer_url) => trusted_issuer_by_url(&mut conn, issuer_url.as_str())
                .await
                .map_err(|error| ServerBootError::Sql(error.into()))?,
            Err(_) => None,
        };
        prior.push(row);
    }
    conn.commit().await?;
    // Discovery and sealing wait on remote services, so both run between the
    // read and the seeding transaction.
    let mut writes = Vec::with_capacity(entries.len());
    for (entry, prior) in entries.iter().zip(&prior) {
        writes
            .push(prepare_trusted_issuer(tenant_id, entry, prior.as_ref(), kms, &discovery).await?);
    }
    let mut conn = postgres.tenant_conn(tenant_id).await?;
    for prepared in writes {
        match prepared {
            Prepared::Write(write) => upsert_trusted_issuer(&mut conn, &write)
                .await
                .map_err(|error| ServerBootError::Sql(error.into()))?,
            Prepared::KeepExisting {
                issuer_url,
                discovery_error,
            } => {
                let exists = trusted_issuer_exists(&mut conn, issuer_url.as_str())
                    .await
                    .map_err(|error| ServerBootError::Sql(error.into()))?;
                if !exists {
                    return Err(discovery_error);
                }
                tracing::warn!(
                    issuer = %issuer_url.as_str(),
                    "OIDC discovery unreachable at boot but trusted issuer is already \
                     seeded; keeping the existing row"
                );
            }
        }
    }
    conn.commit().await?;
    Ok(())
}

/// One configured issuer, prepared for the seeding transaction.
enum Prepared {
    /// Discovery succeeded; upsert this sealed row.
    Write(Box<TrustedIssuerWrite>),
    /// Discovery failed; keep an already-seeded row, else fail with
    /// `discovery_error`.
    KeepExisting {
        /// The normalized issuer whose row must already exist.
        issuer_url: IssuerUrl,
        /// The discovery failure boot returns when no row exists.
        discovery_error: ServerBootError,
    },
}

/// Discover one issuer and build its row against its committed `prior` row
/// (see [`reconcile_trusted_issuer`]), honoring the already-seeded restart
/// exemption; runs with no transaction open.
///
/// # Errors
/// Returns the discovery error for a malformed issuer URL, and
/// [`ServerBootError::IssuerSeal`] when its secret cannot be sealed.
async fn prepare_trusted_issuer(
    tenant_id: DataTenantId,
    entry: &IssuerEntry,
    prior: Option<&TrustedIssuerRow>,
    kms: Option<&WyrdKms>,
    discovery: &ScreenedHttp,
) -> Result<Prepared, ServerBootError> {
    match discover_with_retry(&entry.issuer, discovery).await {
        Ok(jwks_uri) => {
            let trusted = build_trusted_issuer(entry, tenant_id, jwks_uri)?;
            reconcile_trusted_issuer(&trusted, prior, kms)
                .await
                .map(|write| Prepared::Write(Box::new(write)))
                .map_err(|error| ServerBootError::IssuerSeal {
                    issuer: entry.issuer.clone(),
                    message: error.to_string(),
                })
        }
        // A normalized, parseable URL is required even to check existence;
        // a malformed issuer fails closed regardless.
        Err(discovery_error) => match IssuerUrl::new(entry.issuer.clone()) {
            Ok(issuer_url) => Ok(Prepared::KeepExisting {
                issuer_url,
                discovery_error,
            }),
            Err(_) => Err(discovery_error),
        },
    }
}

/// Build the configured `trusted` issuer's write against its committed
/// `prior` row, sealing a client secret only when the configuration
/// establishes a new one.
///
/// A new issuer, a row without a secret, or a changed client id or method (a
/// new binding the old envelope cannot serve) is sealed afresh. Under an
/// unchanged binding the committed envelope stays byte-for-byte when it opens
/// to the configured secret, and is resealed when it opens to another one.
/// When it cannot be opened for any reason, nothing establishes a change, so
/// it also stays: the issuer keeps refusing as before, and the administrator
/// restores the old key or replaces the secret through its owner. A restart
/// therefore never renews or heals an existing credential.
///
/// # Errors
/// Returns [`IssuerSealError`] when a needed seal fails or a JSONB column
/// cannot be serialized.
async fn reconcile_trusted_issuer(
    trusted: &TrustedIssuer,
    prior: Option<&TrustedIssuerRow>,
    kms: Option<&WyrdKms>,
) -> Result<TrustedIssuerWrite, IssuerSealError> {
    let same_binding = prior.and_then(|row| {
        let unchanged = row.client_id == trusted.client_id
            && row.client_auth == client_auth_label(&trusted.client_auth);
        row.client_secret_enc.as_deref().filter(|_| unchanged)
    });
    let Some(stored) = same_binding else {
        return issuer_write_from_trusted(trusted, kms).await;
    };
    match trusted_issuer_secret_matches(trusted, stored, kms).await {
        Some(false) => issuer_write_from_trusted(trusted, kms).await,
        Some(true) => issuer_write_with_envelope(trusted, Some(stored.to_vec())),
        None => {
            tracing::warn!(
                issuer = %trusted.issuer.as_str(),
                "configured issuer secret could not be compared with its stored envelope; \
                 keeping the stored envelope"
            );
            issuer_write_with_envelope(trusted, Some(stored.to_vec()))
        }
    }
}

/// Seed `[[workload_bindings]]` domain bindings into Postgres for `tenant_id`.
///
/// Each binding is upserted (`ON CONFLICT DO UPDATE`). The referenced trusted issuer must already be
/// seeded (FK), so callers seed issuers first. All writes share one tenant
/// transaction opened through [`WyrdPostgres::tenant_conn`].
///
/// # Errors
/// Returns [`ServerBootError::InvalidWorkloadBinding`] when a binding names
/// no active Service account, and [`ServerBootError::Sql`] on a write
/// failure (including an FK violation when the issuer was not seeded).
pub async fn seed_workload_bindings(
    postgres: &WyrdPostgres,
    tenant_id: DataTenantId,
    bindings: &[WorkloadBinding],
) -> Result<(), ServerBootError> {
    if bindings.is_empty() {
        return Ok(());
    }
    let mut conn = postgres.tenant_conn(tenant_id).await?;
    for (index, binding) in bindings.iter().enumerate() {
        let written = upsert_workload_binding(&mut conn, &binding_write_from_binding(binding))
            .await
            .map_err(|error| ServerBootError::Sql(error.into()))?;
        if !written {
            return Err(ServerBootError::InvalidWorkloadBinding {
                index,
                message: format!(
                    "principal_id {} is not an active Service account",
                    binding.principal_id
                ),
            });
        }
    }
    conn.commit().await?;
    Ok(())
}

/// Read `issuer`'s discovery document through `discovery`, retrying
/// transient failures on the bounded backoff schedule, and return the key-set
/// URI the provider published.
///
/// Only the document is read ([`ScreenedHttp::provider_metadata`]); the
/// workload verifier fetches the keys when an assertion first needs them, so
/// a key set that is briefly unavailable does not block seeding.
///
/// # Errors
/// Returns [`ServerBootError::IssuerDiscoveryUnavailable`] when `issuer` is
/// not a valid issuer URL (without retrying) or every attempt fails.
async fn discover_with_retry(
    issuer: &str,
    discovery: &ScreenedHttp,
) -> Result<Url, ServerBootError> {
    let issuer_url =
        IssuerUrl::new(issuer).map_err(|e| ServerBootError::IssuerDiscoveryUnavailable {
            issuer: issuer.to_owned(),
            message: format!("issuer URL could not be parsed: {e}"),
        })?;

    let mut last_message = String::from("discovery did not complete");
    for attempt in 0..DISCOVERY_MAX_ATTEMPTS {
        match discovery.provider_metadata(&issuer_url).await {
            Ok(metadata) => return Ok(metadata.jwks_uri().url().clone()),
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

/// Map one config DTO plus its discovered key-set URI to a [`TrustedIssuer`].
///
/// `jwks_uri` comes solely from discovery (F04: never hand-authored from
/// config, no `card_ref` on this path).
fn build_trusted_issuer(
    entry: &IssuerEntry,
    tenant_id: DataTenantId,
    jwks_uri: Url,
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
        jwks_uri,
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

fn map_principal_kind(entry: IssuerTokenPolicy) -> IssuerTokenPolicy {
    entry
}

#[cfg(test)]
mod pg_tests {
    use secrecy::{ExposeSecret, SecretString};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_kms::{KmsConfig, WyrdKms};
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
            "response_types_supported": ["id_token"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256", "EdDSA"]
        })
    }

    /// Make `server`'s `/jwks` unavailable: workload seeding reads only the
    /// discovery document, so it must succeed without the key set.
    async fn mount_unavailable_key_set(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(503))
            .mount(server)
            .await;
    }

    /// Requests `server` received for its key set.
    ///
    /// # Panics
    /// Panics when the mock server does not record requests.
    async fn key_set_requests(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .expect("requests are recorded")
            .iter()
            .filter(|request| request.url.path() == "/jwks")
            .count()
    }

    /// The discovery transport the boot tests use.
    fn discovery() -> ScreenedHttp {
        ScreenedHttp::allowing_internal()
    }

    /// The conventional key-set URI of `issuer`.
    ///
    /// # Panics
    /// Panics when `issuer` does not form a valid URL.
    fn fake_jwks_uri(issuer: &str) -> Url {
        format!("{issuer}/jwks").parse().expect("jwks url is valid")
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

        mount_unavailable_key_set(&server).await;

        let jwks_uri = discover_with_retry(&issuer, &discovery())
            .await
            .expect("discovery should succeed while the key set is unavailable");

        assert!(jwks_uri.as_str().ends_with("/jwks"));
        assert_eq!(
            key_set_requests(&server).await,
            0,
            "seeding never fetches keys"
        );
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

        let error = discover_with_retry(&issuer, &discovery())
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

        mount_unavailable_key_set(&server).await;

        let jwks_uri = discover_with_retry(&issuer, &discovery())
            .await
            .expect("discovery should recover after a transient failure");

        assert!(jwks_uri.as_str().ends_with("/jwks"));
    }

    #[test]
    fn issuer_boot_build_trusted_issuer_maps_dto_fields() {
        let issuer = "https://idp.example.com/realms/acme";
        let entry = issuer_entry(issuer);
        let trusted = build_trusted_issuer(&entry, tenant(), fake_jwks_uri(issuer))
            .expect("mapping should succeed");

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
        let error = build_trusted_issuer(&entry, tenant(), fake_jwks_uri(issuer))
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
        let key = WyrdKms::new(&KmsConfig::memory("test"), false).expect("memory facade");

        // Pre-seed the row as a prior successful boot would have.
        let entry = issuer_entry(UNREACHABLE_ISSUER);
        let trusted = build_trusted_issuer(&entry, tenant_id, fake_jwks_uri(UNREACHABLE_ISSUER))
            .expect("issuer maps");
        let write = crate::auth::pg_resolvers::issuer_write_from_trusted(&trusted, Some(&key))
            .await
            .expect("issuer encodes");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn");
        upsert_trusted_issuer(&mut conn, &write)
            .await
            .expect("pre-seed upsert");
        conn.commit().await.expect("pre-seed commits");

        // Boot again with the IdP unreachable: the existing row makes this non-fatal.
        seed_trusted_issuers(fixture.wyrd_postgres(), tenant_id, &[entry], Some(&key))
            .await
            .expect("restart with unreachable but already-seeded issuer must not fail boot");
    }

    #[tokio::test]
    async fn seed_fails_closed_for_unreachable_unseeded_issuer() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant_id = fixture.data_tenant_id();
        let key = WyrdKms::new(&KmsConfig::memory("test"), false).expect("memory facade");

        let entry = issuer_entry("https://idp.invalid/realms/never-seeded");
        let error = seed_trusted_issuers(fixture.wyrd_postgres(), tenant_id, &[entry], Some(&key))
            .await
            .expect_err("a never-seeded unreachable issuer must fail boot closed");
        assert!(
            matches!(error, ServerBootError::IssuerDiscoveryUnavailable { .. }),
            "expected IssuerDiscoveryUnavailable, got {error:?}"
        );
    }

    /// A human `[[trusted_issuers]]` entry is refused before any IO and names
    /// the connection API; nothing is seeded that login would ignore.
    #[tokio::test]
    async fn seed_refuses_a_human_issuer_before_discovery() {
        let mut entry = issuer_entry("https://idp.invalid/realms/human");
        entry.principal_kind = IssuerTokenPolicy::Human;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused.invalid/none")
            .expect("lazy pool builds without connecting");

        let postgres = WyrdPostgres::from_pools(pool, None);

        let error = seed_trusted_issuers(&postgres, tenant(), &[entry], None)
            .await
            .expect_err("a human issuer must not seed");

        assert!(matches!(error, ServerBootError::HumanIssuerSeed { .. }));
        assert!(error.to_string().contains("/v1/identity/oidc/candidate"));
    }
}
