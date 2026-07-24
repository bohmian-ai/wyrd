//! Tenant-admin CRUD for trusted OIDC issuers and workload bindings.
//!
//! These routes are the authoring control plane for the cloud-issuer trust
//! store: `POST/GET/DELETE /v1/admin/trusted-issuers` and
//! `/v1/admin/workload-bindings`. Every mutation is permission-gated
//! (`service_accounts:write`) and writes through a [`TenantConn`] on the
//! `wyrd_app` RLS pool, so Postgres row-level security is the load-bearing
//! tenant boundary — no per-query tenant filtering.
//!
//! Create resolves OIDC discovery exactly once to fill `jwks_uri`, encrypts the
//! client secret with the process sealing key before insert, and never lets a
//! plaintext secret reach a column, log, or response (GET redacts it). Reads and
//! deletes never touch discovery.
//!
//! No audit row is written here, and that is deliberate: admin-mutation audit is
//! deferred to the audit→Vala/Iceberg consolidation rather than a per-feature
//! Postgres table (the `revoke_principal` precedent writes no audit row either).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::post;
use secrecy::SecretString;
use serde::Deserialize;
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, OidcProvider, TrustedIssuer, WorkloadBinding,
};
use wyrd_spec::auth::{
    ClaimMappingPayload, ClientAuthKind, CreateTrustedIssuerRequest, CreateWorkloadBindingRequest,
    IssuerTokenPolicy, IssuerUrl, TrustedIssuerView, WorkloadBindingView,
};
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::auth::{
    TrustedIssuerWrite, WorkloadBindingWrite, delete_trusted_issuer, delete_workload_binding,
    delete_workload_bindings_for_issuer, insert_trusted_issuer, insert_workload_binding,
    trusted_issuers_for_tenant, workload_bindings_for_tenant,
};
use wyrd_sql::row_types::auth::{TrustedIssuerRow, WorkloadBindingRow};
use wyrd_sql::{SqlError, TenantConn};

use crate::audit;
use crate::auth::pg_resolvers::{binding_write_from_binding, issuer_write_from_trusted};
use crate::components::auth::Caller;
use crate::config::DeploymentProfile;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Default JWKS key-cache TTL when a create request omits `jwks_ttl_secs`.
const DEFAULT_JWKS_TTL: Duration = Duration::from_secs(3600);

/// Build the tenant-admin CRUD routes for the `/v1` group.
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/trusted-issuers",
            post(create_trusted_issuer)
                .get(list_trusted_issuers)
                .delete(delete_trusted_issuer_route),
        )
        .route(
            "/admin/workload-bindings",
            post(create_workload_binding)
                .get(list_workload_bindings)
                .delete(delete_workload_binding_route),
        )
}

// --------------------------------------------------------------------------
// Wire-DTO conversions
//
// The request/response DTOs themselves live in `wyrd_spec::auth::admin`. The
// orphan rule forbids `impl From<wyrd_auth_oidc::_> for wyrd_spec::_` here
// (neither type is local to this crate), so every mapping to or from the
// domain and row types is a free function below.
// --------------------------------------------------------------------------

/// Build the domain [`ClaimMapping`] from its serde mirror; [`ClaimPath`] is
/// not directly (de)serializable.
fn claim_mapping_into_domain(payload: ClaimMappingPayload) -> ClaimMapping {
    ClaimMapping {
        subject: ClaimPath::new(payload.subject),
        email: payload.email.map(ClaimPath::new),
        groups: payload.groups.map(ClaimPath::new),
    }
}

fn principal_kind_policy(payload: IssuerTokenPolicy) -> IssuerTokenPolicy {
    payload
}

/// Build the domain [`ClientAuth`], requiring a secret for the secret-bearing
/// variants. A missing secret on `SecretBasic`/`SecretPost` is a client error.
fn request_client_auth(
    request: &CreateTrustedIssuerRequest,
) -> Result<ClientAuth, WyrdErrorResponse> {
    match request.client_auth {
        ClientAuthKind::SecretBasic => Ok(ClientAuth::SecretBasic(required_secret(request)?)),
        ClientAuthKind::SecretPost => Ok(ClientAuth::SecretPost(required_secret(request)?)),
        ClientAuthKind::PrivateKeyJwt => Ok(ClientAuth::PrivateKeyJwt),
        ClientAuthKind::Public => Ok(ClientAuth::Public),
    }
}

/// Extract a non-empty `client_secret` from the create request, or fail with a
/// `400` [`WyrdError::MissingRequiredField`]. Called only for the secret-bearing
/// client-auth variants (`SecretBasic`/`SecretPost`).
fn required_secret(
    request: &CreateTrustedIssuerRequest,
) -> Result<SecretString, WyrdErrorResponse> {
    match request.client_secret.as_deref() {
        Some(secret) if !secret.is_empty() => Ok(SecretString::from(secret.to_owned())),
        _ => Err(WyrdErrorResponse::from(WyrdError::MissingRequiredField {
            message: "client_secret is required for SecretBasic and SecretPost client auth"
                .to_owned(),
            details: serde_json::json!({ "field": "client_secret" }),
        })),
    }
}

/// Redacted issuer projection from a write row. Never carries the client secret.
fn trusted_issuer_view_from_write(write: &TrustedIssuerWrite) -> TrustedIssuerView {
    TrustedIssuerView {
        issuer: write.issuer_url.clone(),
        jwks_uri: write.jwks_uri.clone(),
        expected_audience: write.expected_audience.clone(),
        client_id: write.client_id.clone(),
        client_auth: write.client_auth.clone(),
        principal_kind: write.principal_kind.clone(),
        jwks_ttl_secs: write.jwks_ttl_secs,
        claim_mapping: write.claim_mapping.clone(),
        group_role_map: write.group_role_map.clone(),
        default_roles: write.default_roles.clone(),
    }
}

/// Redacted issuer projection from a stored row.
fn trusted_issuer_view_from_row(row: TrustedIssuerRow) -> TrustedIssuerView {
    TrustedIssuerView {
        issuer: row.issuer_url,
        jwks_uri: row.jwks_uri,
        expected_audience: row.expected_audience,
        client_id: row.client_id,
        client_auth: row.client_auth,
        principal_kind: row.principal_kind,
        jwks_ttl_secs: row.jwks_ttl_secs,
        claim_mapping: row.claim_mapping,
        group_role_map: row.group_role_map,
        default_roles: row.default_roles,
    }
}

/// Workload-binding projection from a write row.
fn workload_binding_view_from_write(write: &WorkloadBindingWrite) -> WorkloadBindingView {
    WorkloadBindingView {
        issuer: write.issuer_url.clone(),
        subject: write.subject.clone(),
        audience: write.audience.clone(),
        card_ref: write.card_ref.clone(),
    }
}

/// Workload-binding projection from a stored row.
fn workload_binding_view_from_row(
    row: WorkloadBindingRow,
) -> Result<WorkloadBindingView, WyrdErrorResponse> {
    Ok(WorkloadBindingView {
        issuer: row.issuer_url,
        subject: row.subject,
        audience: row.audience,
        card_ref: serde_json::to_value(row.card_ref).map_err(internal_error)?,
    })
}

/// Query for deleting one issuer, with the cascade flag.
#[derive(Debug, Deserialize)]
struct DeleteIssuerQuery {
    issuer: String,
    #[serde(default)]
    cascade: bool,
}

/// Query for addressing one workload binding by `(issuer, subject)`.
#[derive(Debug, Deserialize)]
struct BindingQuery {
    issuer: String,
    subject: String,
}

/// Optional exact-match filters for the workload-binding list. Both default to
/// `None`, which lists every binding for the caller's tenant.
#[derive(Debug, Default, Deserialize)]
struct BindingFilter {
    #[serde(default)]
    issuer: Option<String>,
    #[serde(default)]
    subject: Option<String>,
}

// --------------------------------------------------------------------------
// Handlers — trusted issuers
// --------------------------------------------------------------------------

/// `POST /v1/admin/trusted-issuers` — register a trusted OIDC issuer for the
/// caller's tenant.
///
/// Gated on `service_accounts:write`. Resolves OIDC discovery once to fill
/// `jwks_uri`, encrypts any client secret with the process sealing key before
/// insert (fails closed if a secret is supplied without a key), and writes
/// through the RLS `TenantConn`. Returns the redacted [`TrustedIssuerView`]
/// (never the secret).
async fn create_trusted_issuer(
    State(state): State<AppState>,
    caller: Caller,
    Json(request): Json<CreateTrustedIssuerRequest>,
) -> Result<Json<TrustedIssuerView>, WyrdErrorResponse> {
    wyrd_auth::service_accounts::require_service_accounts_write(
        &caller.principal,
        "manage trusted issuers",
    )
    .map_err(WyrdErrorResponse::from)?;

    // The only network call on any admin path, and only at create: discovery
    // resolves jwks_uri. A runtime read must never re-discover.
    let jwks_uri = discover_jwks_uri(&request.issuer, state.deployment_profile).await?;

    let trusted = TrustedIssuer {
        tenant_id: caller.principal.tenant_id,
        issuer: request.issuer.clone(),
        jwks_uri,
        expected_audience: request.expected_audience.clone(),
        client_id: request.client_id.clone(),
        client_auth: request_client_auth(&request)?,
        claim_mapping: claim_mapping_into_domain(request.claim_mapping.clone()),
        group_role_map: request.group_role_map.clone(),
        default_roles: request.default_roles.clone(),
        principal_kind: principal_kind_policy(request.principal_kind),
        jwks_ttl: request
            .jwks_ttl_secs
            .map_or(DEFAULT_JWKS_TTL, Duration::from_secs),
    };

    // Encrypt before insert. A secret-bearing issuer with no sealing key fails
    // closed here — plaintext never lands in a column.
    let write = issuer_write_from_trusted(&trusted, state.auth.sealing_key.as_deref())
        .map_err(seal_error)?;

    let mut conn = acquire_conn(&state, &caller).await?;
    insert_trusted_issuer(&mut conn, &write)
        .await
        .map_err(map_write_error)?;
    audit::append_on(
        &mut conn,
        &audit::audit_event(
            &caller,
            "admin.trusted_issuer.create",
            &format!("trusted_issuer:{}", trusted.issuer),
            "service_accounts:write",
            wyrd_spec::vala::api::AuditDecision::Allow,
            wyrd_spec::vala::api::AuditResult::Success,
            "trusted issuer registered",
        ),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(Json(trusted_issuer_view_from_write(&write)))
}

/// `GET /v1/admin/trusted-issuers` — list the caller tenant's trusted issuers as
/// redacted [`TrustedIssuerView`]s. Gated on `service_accounts:write`; no
/// discovery or secret ever leaves the store.
async fn list_trusted_issuers(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<Vec<TrustedIssuerView>>, WyrdErrorResponse> {
    wyrd_auth::service_accounts::require_service_accounts_write(
        &caller.principal,
        "read trusted issuers",
    )
    .map_err(WyrdErrorResponse::from)?;

    let mut conn = acquire_conn(&state, &caller).await?;
    let rows = trusted_issuers_for_tenant(&mut conn)
        .await
        .map_err(sql_unavailable)?;
    conn.commit().await.map_err(sql_unavailable)?;

    let views = rows.into_iter().map(trusted_issuer_view_from_row).collect();
    Ok(Json(views))
}

/// `DELETE /v1/admin/trusted-issuers?issuer=&cascade=` — remove one issuer.
///
/// Gated on `service_accounts:write`. With `cascade=true` the referencing
/// workload bindings are deleted first so the FK `ON DELETE RESTRICT` does not
/// block the issuer delete; without it, a live binding fails the delete closed
/// (`409`). A missing issuer is a `404`. Returns `204 No Content`.
async fn delete_trusted_issuer_route(
    State(state): State<AppState>,
    caller: Caller,
    Query(query): Query<DeleteIssuerQuery>,
) -> Result<StatusCode, WyrdErrorResponse> {
    wyrd_auth::service_accounts::require_service_accounts_write(
        &caller.principal,
        "delete trusted issuers",
    )
    .map_err(WyrdErrorResponse::from)?;

    let issuer = normalize_issuer(&query.issuer);
    let mut conn = acquire_conn(&state, &caller).await?;

    // --cascade removes the referencing bindings first so the issuer delete is
    // not blocked by the FK ON DELETE RESTRICT. Without it, a live binding makes
    // the delete fail closed (23503 -> 409).
    if query.cascade {
        delete_workload_bindings_for_issuer(&mut conn, &issuer)
            .await
            .map_err(map_write_error)?;
    }

    let removed = delete_trusted_issuer(&mut conn, &issuer)
        .await
        .map_err(map_write_error)?;
    if removed == 0 {
        return Err(issuer_not_found(&issuer));
    }
    audit::append_on(
        &mut conn,
        &audit::audit_event(
            &caller,
            "admin.trusted_issuer.delete",
            &format!("trusted_issuer:{issuer}"),
            "service_accounts:write",
            wyrd_spec::vala::api::AuditDecision::Allow,
            wyrd_spec::vala::api::AuditResult::Success,
            "trusted issuer deleted",
        ),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// Handlers — workload bindings
// --------------------------------------------------------------------------

/// `POST /v1/admin/workload-bindings` — bind a workload identity
/// `(issuer, subject, audience)` to a server-owned `CardRef`.
///
/// Gated on `service_accounts:write`. The referenced issuer must already be
/// registered in this tenant; an FK violation maps to a `404` (create the issuer
/// first), a duplicate binding to a `409`. Writes through the RLS `TenantConn`.
async fn create_workload_binding(
    State(state): State<AppState>,
    caller: Caller,
    Json(request): Json<CreateWorkloadBindingRequest>,
) -> Result<Json<WorkloadBindingView>, WyrdErrorResponse> {
    wyrd_auth::service_accounts::require_service_accounts_write(
        &caller.principal,
        "manage workload bindings",
    )
    .map_err(WyrdErrorResponse::from)?;

    let binding = WorkloadBinding {
        tenant_id: caller.principal.tenant_id,
        issuer: request.issuer.clone(),
        subject: request.subject.clone(),
        audience: request.audience.clone(),
        card_ref: request.card_ref.clone(),
    };
    let write = binding_write_from_binding(&binding).map_err(internal_error)?;

    let mut conn = acquire_conn(&state, &caller).await?;
    insert_workload_binding(&mut conn, &write)
        .await
        .map_err(|error| map_binding_write_error(error, &binding.issuer))?;
    audit::append_on(
        &mut conn,
        &audit::audit_event(
            &caller,
            "admin.workload_binding.create",
            &format!("workload_binding:{}:{}", binding.issuer, binding.subject),
            "service_accounts:write",
            wyrd_spec::vala::api::AuditDecision::Allow,
            wyrd_spec::vala::api::AuditResult::Success,
            "workload binding registered",
        ),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(Json(workload_binding_view_from_write(&write)))
}

/// `GET /v1/admin/workload-bindings?issuer=&subject=` — list the caller tenant's
/// workload bindings, optionally filtered by exact issuer and/or subject. Gated
/// on `service_accounts:write`. The issuer filter is normalized to the stored
/// form so a trailing slash does not silently miss.
async fn list_workload_bindings(
    State(state): State<AppState>,
    caller: Caller,
    Query(filter): Query<BindingFilter>,
) -> Result<Json<Vec<WorkloadBindingView>>, WyrdErrorResponse> {
    wyrd_auth::service_accounts::require_service_accounts_write(
        &caller.principal,
        "read workload bindings",
    )
    .map_err(WyrdErrorResponse::from)?;

    // Normalize the issuer filter to the stored form so a trailing slash does
    // not silently miss; the subject is matched verbatim.
    let issuer = filter.issuer.as_deref().map(normalize_issuer);
    let mut conn = acquire_conn(&state, &caller).await?;
    let rows =
        workload_bindings_for_tenant(&mut conn, issuer.as_deref(), filter.subject.as_deref())
            .await
            .map_err(sql_unavailable)?;
    conn.commit().await.map_err(sql_unavailable)?;

    let views = rows
        .into_iter()
        .map(workload_binding_view_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(views))
}

/// `DELETE /v1/admin/workload-bindings?issuer=&subject=` — remove one binding
/// addressed by `(issuer, subject)`. Gated on `service_accounts:write`. A missing
/// binding is a `404`. Returns `204 No Content`.
async fn delete_workload_binding_route(
    State(state): State<AppState>,
    caller: Caller,
    Query(query): Query<BindingQuery>,
) -> Result<StatusCode, WyrdErrorResponse> {
    wyrd_auth::service_accounts::require_service_accounts_write(
        &caller.principal,
        "delete workload bindings",
    )
    .map_err(WyrdErrorResponse::from)?;

    let issuer = normalize_issuer(&query.issuer);
    let mut conn = acquire_conn(&state, &caller).await?;
    let removed = delete_workload_binding(&mut conn, &issuer, &query.subject)
        .await
        .map_err(map_write_error)?;
    if removed == 0 {
        return Err(binding_not_found(&issuer, &query.subject));
    }
    audit::append_on(
        &mut conn,
        &audit::audit_event(
            &caller,
            "admin.workload_binding.delete",
            &format!("workload_binding:{issuer}:{}", query.subject),
            "service_accounts:write",
            wyrd_spec::vala::api::AuditDecision::Allow,
            wyrd_spec::vala::api::AuditResult::Success,
            "workload binding deleted",
        ),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;
    conn.commit().await.map_err(sql_unavailable)?;

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// Shared helpers
// --------------------------------------------------------------------------

/// Acquire an RLS [`TenantConn`] scoped to the caller's tenant, mapping an
/// acquisition failure to a `503`. Every admin handler mutates/reads through this
/// so Postgres row-level security is the load-bearing tenant boundary.
async fn acquire_conn<'a>(
    state: &'a AppState,
    caller: &Caller,
) -> Result<TenantConn<'a>, WyrdErrorResponse> {
    state
        .postgres
        .tenant_conn(caller.principal.tenant_id)
        .await
        .map_err(sql_unavailable)
}

/// Build a reqwest client with SSRF mitigations: 10 s timeout, no redirect-following.
fn discovery_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("discovery client config is valid")
}

/// Collapse an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`) to its IPv4 form so a
/// single classifier catches metadata/internal ranges written in either family.
/// Bare `::1`/`::` are left as IPv6 and handled by the IPv6 arms.
fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    }
}

/// `100.64.0.0/10` — RFC 6598 carrier-grade NAT shared address space.
fn is_cgnat(v4: Ipv4Addr) -> bool {
    let [a, b, ..] = v4.octets();
    a == 100 && (b & 0xc0) == 0x40
}

/// `fc00::/7` — RFC 4193 unique local addresses.
fn is_unique_local_v6(v6: Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xfe00) == 0xfc00
}

/// Addresses that are never a legitimate IdP and are blocked in **every**
/// deployment profile. Link-local (`169.254.0.0/16`) covers the cloud instance
/// metadata endpoint (`169.254.169.254`); even self-hosted and development must
/// never let an issuer URL reach it.
fn is_always_blocked(ip: IpAddr) -> bool {
    match normalize_ip(ip) {
        IpAddr::V4(v4) => v4.is_link_local() || v4.is_broadcast() || v4.is_documentation(),
        // fe80::/10 link-local.
        IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// Internal ranges a `Production` (multi-tenant SaaS) deployment must not let a
/// semi-untrusted tenant admin's issuer reach. Self-hosted / enterprise
/// single-tenant and development legitimately run the IdP on these ranges, so
/// this set is gated on the profile by [`is_blocked_addr`].
fn is_internal(ip: IpAddr) -> bool {
    match normalize_ip(ip) {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_unspecified() || is_cgnat(v4)
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified() || is_unique_local_v6(v6),
    }
}

/// The SSRF address policy: block metadata/link-local everywhere, and block the
/// broader internal ranges only under `Production`.
fn is_blocked_addr(ip: IpAddr, profile: DeploymentProfile) -> bool {
    is_always_blocked(ip) || (profile.is_production() && is_internal(ip))
}

/// The rejection an issuer earns when it resolves to a blocked address.
fn blocked_issuer_error() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::MissingRequiredField {
        message: "issuer resolves to a blocked address range".to_owned(),
        details: serde_json::json!({ "field": "issuer" }),
    })
}

/// Resolve `host:port` and reject if **any** resolved address is blocked for the
/// profile, returning the screened addresses. Rejecting on any blocked record
/// defeats split-horizon DNS that mixes one public and one internal answer.
async fn resolve_and_screen(
    host: &str,
    port: u16,
    profile: DeploymentProfile,
) -> Result<Vec<SocketAddr>, WyrdErrorResponse> {
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| {
            tracing::warn!(%host, %error, "issuer host resolution failed");
            WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
                message: "issuer host could not be resolved".to_owned(),
                details: serde_json::json!({ "field": "issuer" }),
            })
        })?
        .collect();

    if addrs.is_empty() || addrs.iter().any(|addr| is_blocked_addr(addr.ip(), profile)) {
        return Err(blocked_issuer_error());
    }
    Ok(addrs)
}

/// Build a discovery client pinned to the pre-screened addresses so the fetch
/// connects to a validated IP and cannot be re-pointed at an internal address by
/// a DNS-rebinding answer between the screen and the connect.
fn pinned_discovery_client(
    host: &str,
    addrs: &[SocketAddr],
) -> Result<reqwest::Client, WyrdErrorResponse> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, addrs)
        .build()
        .map_err(|error| {
            tracing::error!(%error, "failed to build pinned discovery client");
            WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
                message: "discovery client could not be constructed".to_owned(),
                details: serde_json::json!({ "field": "issuer" }),
            })
        })
}

/// Resolve the issuer's `jwks_uri` via OIDC discovery. The issuer URL is already
/// validated by [`IssuerUrl`]; a discovery failure is a `503`.
///
/// SSRF policy (see [`is_blocked_addr`]): the cloud metadata / link-local range
/// is blocked in every profile; the broader internal ranges (loopback, private,
/// CGNAT, ULA) are blocked only in `Production`, because self-hosted /
/// enterprise-single-tenant and development legitimately run the IdP on a
/// private or loopback address. A literal-IP host is screened directly; a
/// hostname is resolved, every resolved address is screened, and the discovery
/// client is pinned to those addresses so a DNS-rebinding answer cannot redirect
/// the connect to an internal address after the screen.
async fn discover_jwks_uri(
    issuer: &IssuerUrl,
    deployment_profile: DeploymentProfile,
) -> Result<url::Url, WyrdErrorResponse> {
    let url = url::Url::parse(issuer.as_str()).map_err(|error| {
        WyrdErrorResponse::from(WyrdError::MissingRequiredField {
            message: format!("issuer is not a valid URL: {error}"),
            details: serde_json::json!({ "field": "issuer" }),
        })
    })?;

    let client = match url.host() {
        Some(url::Host::Ipv4(addr)) => {
            if is_blocked_addr(IpAddr::V4(addr), deployment_profile) {
                return Err(blocked_issuer_error());
            }
            discovery_client()
        }
        Some(url::Host::Ipv6(addr)) => {
            if is_blocked_addr(IpAddr::V6(addr), deployment_profile) {
                return Err(blocked_issuer_error());
            }
            discovery_client()
        }
        Some(url::Host::Domain(domain)) => {
            let port = url.port_or_known_default().unwrap_or(443);
            let addrs = resolve_and_screen(domain, port, deployment_profile).await?;
            pinned_discovery_client(domain, &addrs)?
        }
        None => return Err(blocked_issuer_error()),
    };

    let provider = OidcProvider::discover(url, client).await.map_err(|error| {
        tracing::warn!(
            error = %error,
            issuer = issuer.as_str(),
            "OIDC discovery failed for admin create"
        );
        WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
            message: "OIDC discovery failed for issuer".to_owned(),
            details: serde_json::json!({ "issuer": issuer.as_str() }),
        })
    })?;
    Ok(provider.metadata.jwks_uri)
}

/// Normalize a query-supplied issuer to match the stored form (trailing slash
/// trimmed, as [`IssuerUrl::new`] does on the create path).
fn normalize_issuer(value: &str) -> String {
    value.trim_end_matches('/').to_owned()
}

/// Map a write-path SQLx error to an admin response.
///
/// A unique violation (duplicate create) and an FK violation (deleting an issuer
/// that still has live bindings, `ON DELETE RESTRICT`) are both `409` conflicts.
/// Anything else is a backend-unavailable `503`.
///
/// The binding-insert path does not use this mapper: there an FK violation means
/// the referenced issuer is missing, which is a `404`, not a conflict. See
/// [`map_binding_write_error`].
fn map_write_error(error: sqlx::Error) -> WyrdErrorResponse {
    match SqlError::from(error) {
        SqlError::UniqueViolation { constraint } | SqlError::FkViolation { constraint } => {
            WyrdErrorResponse::from(WyrdError::AdminConflict {
                message: format!("admin mutation conflicted on constraint {constraint}"),
                details: serde_json::json!({ "constraint": constraint }),
            })
        }
        other => sql_unavailable(other),
    }
}

/// Map a workload-binding insert error to an admin response.
///
/// The two shared composite-FK constraints (`auth_workload_bindings` →
/// `auth_trusted_issuers`) report the same constraint name whether they fire on
/// a binding insert or on a blocked issuer delete, so the constraint name cannot
/// distinguish them; the call site can. On insert the only reachable FK
/// violation is a reference to a trusted issuer that is not registered in this
/// tenant (the `platform.tenants` FK is unreachable under the RLS `WITH CHECK`),
/// so it maps to a `404` [`WyrdError::AdminNotFound`] rather than a conflict. A
/// unique violation is still a duplicate-binding `409`.
fn map_binding_write_error(error: sqlx::Error, issuer: &IssuerUrl) -> WyrdErrorResponse {
    match SqlError::from(error) {
        SqlError::FkViolation { .. } => WyrdErrorResponse::from(WyrdError::AdminNotFound {
            message: format!(
                "trusted issuer {} is not registered in this tenant; create it first",
                issuer.as_str()
            ),
            details: serde_json::json!({ "issuer": issuer.as_str() }),
        }),
        SqlError::UniqueViolation { constraint } => {
            WyrdErrorResponse::from(WyrdError::AdminConflict {
                message: format!("admin mutation conflicted on constraint {constraint}"),
                details: serde_json::json!({ "constraint": constraint }),
            })
        }
        other => sql_unavailable(other),
    }
}

/// Map a client-secret sealing failure to an admin response. A missing sealing
/// key is a fail-closed `503` (never persist plaintext); an encrypt/serialize
/// failure is a `500`.
fn seal_error(error: crate::auth::pg_resolvers::IssuerSealError) -> WyrdErrorResponse {
    use crate::auth::pg_resolvers::IssuerSealError;
    match error {
        IssuerSealError::SealingKeyMissing => {
            WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
                message: "a client secret was supplied but no sealing key is configured; \
                          refusing to store a plaintext secret"
                    .to_owned(),
                details: serde_json::json!({ "reason": "sealing_key_missing" }),
            })
        }
        IssuerSealError::Encrypt | IssuerSealError::Serialize(_) => internal_error(error),
    }
}

/// `404` for a delete that matched no issuer in the caller's tenant.
fn issuer_not_found(issuer: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::AdminNotFound {
        message: format!("trusted issuer {issuer} not found in tenant"),
        details: serde_json::json!({ "issuer": issuer }),
    })
}

/// `404` for a delete that matched no binding for `(issuer, subject)` in the
/// caller's tenant.
fn binding_not_found(issuer: &str, subject: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::AdminNotFound {
        message: format!(
            "workload binding for issuer {issuer} subject {subject} not found in tenant"
        ),
        details: serde_json::json!({ "issuer": issuer, "subject": subject }),
    })
}

/// Map any backing-store failure to a fail-closed `503` and log the cause. Admin
/// paths never surface the raw SQL error to the client.
fn sql_unavailable(error: impl std::fmt::Display) -> WyrdErrorResponse {
    tracing::warn!(error = %error, "admin db unavailable");
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({}),
    })
}

/// Map an unexpected server-side failure (serialization, encryption internals)
/// to a `500`.
fn internal_error(error: impl std::fmt::Display) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Internal {
        message: error.to_string(),
        details: serde_json::Value::Null,
    })
}

#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::Json;
    use axum::extract::{Query, State};
    use axum::http::StatusCode;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, IssuerConfigResolver, TrustedIssuer,
    };
    use wyrd_crypt::SecretKey;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalId, PrincipalKind, RoleRef};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerTokenPolicy;
    use wyrd_spec::auth::IssuerUrl;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use secrecy::ExposeSecret;
    use wyrd_sql::queries::auth::{trusted_issuer_by_url, upsert_trusted_issuer};

    use super::*;
    use crate::auth::pg_resolvers::{PgIssuerResolver, issuer_write_from_trusted};
    use crate::components::auth::Caller;
    use wyrd_spec::request_id::RequestId;

    const SECRET: &str = "super-secret";
    const SEEDED_ISSUER: &str = "https://idp.example.com/realms/wyrd";

    fn sealing_key() -> SecretKey {
        SecretKey::from_bytes([7_u8; 32])
    }

    async fn test_state(fixture: &PgFixture) -> AppState {
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let dir = tempfile::tempdir().expect("admin storage tempdir");
        let storage_root = dir.keep().join("admin-storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        AppState::new(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
        .with_auth(crate::components::auth::ServerAuth {
            sealing_key: Some(Arc::new(sealing_key())),
            ..crate::components::auth::ServerAuth::default()
        })
    }

    fn principal_with(tenant: DataTenantId, perms: PermissionSet) -> Caller {
        Caller {
            data_tenant_id: tenant,
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::nil()),
                PrincipalKind::User,
                tenant,
                Vec::<RoleRef>::new(),
                perms,
            ),
            request_id: RequestId::now_v7(),
        }
    }

    fn writer(tenant: DataTenantId) -> Caller {
        principal_with(
            tenant,
            PermissionSet::from_iter([Permission::service_accounts_write()]),
        )
    }

    fn reader(tenant: DataTenantId) -> Caller {
        principal_with(tenant, PermissionSet::new())
    }

    /// Start a wiremock IdP that answers OIDC discovery, returning the issuer URL.
    async fn discovery_server() -> (MockServer, IssuerUrl) {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "id_token_signing_alg_values_supported": ["RS256", "EdDSA"],
            })))
            .mount(&server)
            .await;
        let issuer_url = IssuerUrl::new(&issuer).expect("loopback http issuer is valid");
        (server, issuer_url)
    }

    fn create_issuer_request(
        issuer: IssuerUrl,
        secret: Option<&str>,
    ) -> CreateTrustedIssuerRequest {
        CreateTrustedIssuerRequest {
            issuer,
            expected_audience: "wyrd-api".to_owned(),
            client_id: "wyrd-client".to_owned(),
            client_auth: ClientAuthKind::SecretPost,
            client_secret: secret.map(ToOwned::to_owned),
            claim_mapping: ClaimMappingPayload {
                subject: "sub".to_owned(),
                email: Some("email".to_owned()),
                groups: Some("realm_access.roles".to_owned()),
            },
            group_role_map: HashMap::new(),
            default_roles: vec!["viewer".to_owned()],
            principal_kind: IssuerTokenPolicy::Human,
            jwks_ttl_secs: None,
        }
    }

    fn sample_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("my-model").expect("card name"),
            version: VersionBlock::parse("1.0.0").expect("version"),
            space: SpaceName::new("prod").expect("space"),
            uid: None,
        }
    }

    /// Seed a no-secret issuer directly so binding tests can FK-reference it
    /// without standing up discovery.
    async fn seed_issuer(fixture: &PgFixture, tenant: DataTenantId) {
        let trusted = TrustedIssuer {
            tenant_id: tenant,
            issuer: IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid"),
            jwks_uri: format!("{SEEDED_ISSUER}/jwks").parse().expect("jwks uri"),
            expected_audience: "wyrd-api".to_owned(),
            client_id: "wyrd-client".to_owned(),
            client_auth: ClientAuth::PrivateKeyJwt,
            claim_mapping: ClaimMapping {
                subject: ClaimPath::new("sub"),
                email: None,
                groups: None,
            },
            group_role_map: HashMap::new(),
            default_roles: Vec::new(),
            principal_kind: IssuerTokenPolicy::Workload,
            jwks_ttl: Duration::from_secs(3600),
        };
        let write = issuer_write_from_trusted(&trusted, None).expect("issuer encodes");
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        upsert_trusted_issuer(&mut conn, &write)
            .await
            .expect("issuer upsert");
        conn.commit().await.expect("issuer seed commits");
    }

    #[tokio::test]
    async fn create_denied_without_service_accounts_write() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        // No discovery server: the RBAC gate must short-circuit before any
        // network call or DB write.
        let issuer = IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid");

        let error = create_trusted_issuer(
            State(state),
            reader(tenant),
            Json(create_issuer_request(issuer, Some(SECRET))),
        )
        .await
        .expect_err("a principal without service_accounts:write must be denied");

        assert!(matches!(error.0, WyrdError::PermissionDeniedRbac { .. }));
    }

    #[tokio::test]
    async fn create_blocks_private_issuer_in_production() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture)
            .await
            .with_deployment_profile(DeploymentProfile::Production);
        // The SSRF guard must reject a loopback issuer under the Production
        // profile before any network call — no discovery server is stood up.
        let issuer = IssuerUrl::new("http://127.0.0.1:9/").expect("loopback issuer parses");

        let error = create_trusted_issuer(
            State(state),
            writer(tenant),
            Json(create_issuer_request(issuer, Some(SECRET))),
        )
        .await
        .expect_err("Production must block a private/loopback issuer");

        assert!(matches!(
            &error.0,
            WyrdError::MissingRequiredField { message, .. }
                if message.contains("blocked address range")
        ));
    }

    #[tokio::test]
    async fn create_blocks_metadata_endpoint_in_development() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        // Default (non-production) profile: the private/loopback block does not
        // apply, but the cloud metadata endpoint is blocked in every profile and
        // must be rejected before any network call.
        let state = test_state(&fixture).await;
        // `https` because `IssuerUrl` allows `http` only for loopback; the guard
        // rejects the metadata address before any connection is attempted.
        let issuer = IssuerUrl::new("https://169.254.169.254/").expect("metadata issuer parses");

        let error = create_trusted_issuer(
            State(state),
            writer(tenant),
            Json(create_issuer_request(issuer, Some(SECRET))),
        )
        .await
        .expect_err("the metadata endpoint must be blocked in every profile");

        assert!(matches!(
            &error.0,
            WyrdError::MissingRequiredField { message, .. }
                if message.contains("blocked address range")
        ));
    }

    #[test]
    fn ssrf_metadata_endpoint_blocked_in_every_profile() {
        let metadata: IpAddr = "169.254.169.254".parse().expect("valid ip");
        assert!(is_blocked_addr(metadata, DeploymentProfile::Development));
        assert!(is_blocked_addr(metadata, DeploymentProfile::Production));
        // An IPv4-mapped IPv6 spelling of the same address is caught too.
        let mapped: IpAddr = "::ffff:169.254.169.254".parse().expect("valid ip");
        assert!(is_blocked_addr(mapped, DeploymentProfile::Development));
    }

    #[test]
    fn ssrf_internal_ranges_blocked_only_in_production() {
        for raw in [
            "127.0.0.1",   // loopback
            "10.0.0.5",    // private
            "192.168.1.1", // private
            "172.16.0.1",  // private
            "100.64.0.1",  // CGNAT
            "::1",         // v6 loopback
            "fc00::1",     // v6 ULA
        ] {
            let ip: IpAddr = raw.parse().expect("valid ip");
            assert!(
                is_blocked_addr(ip, DeploymentProfile::Production),
                "{raw} must be blocked in Production"
            );
            assert!(
                !is_blocked_addr(ip, DeploymentProfile::Development),
                "{raw} must be allowed in Development"
            );
        }
    }

    #[test]
    fn ssrf_public_addresses_allowed_in_every_profile() {
        for raw in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            let ip: IpAddr = raw.parse().expect("valid ip");
            assert!(!is_blocked_addr(ip, DeploymentProfile::Development));
            assert!(!is_blocked_addr(ip, DeploymentProfile::Production));
        }
    }

    #[tokio::test]
    async fn create_persists_sealed_secret_and_get_redacts() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        let (_server, issuer) = discovery_server().await;

        let view = create_trusted_issuer(
            State(state.clone()),
            writer(tenant),
            Json(create_issuer_request(issuer.clone(), Some(SECRET))),
        )
        .await
        .expect("create succeeds")
        .0;
        assert_eq!(view.issuer, issuer.as_str());
        assert!(view.jwks_uri.ends_with("/jwks"));

        // The create response is structurally redacted: no secret field anywhere.
        let body = serde_json::to_value(&view).expect("view serializes");
        assert!(
            !body.to_string().contains(SECRET),
            "create response must not echo the plaintext secret"
        );

        // The stored column holds ciphertext, never the plaintext bytes.
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let row = trusted_issuer_by_url(&mut conn, issuer.as_str())
            .await
            .expect("query succeeds")
            .expect("issuer row exists");
        conn.commit().await.expect("commit");
        let enc = row.client_secret_enc.expect("secret is stored");
        assert!(!enc.is_empty(), "ciphertext must be present");
        assert_ne!(
            enc,
            SECRET.as_bytes(),
            "plaintext must never land in the column"
        );

        // The sealing key round-trips: the resolver decrypts back to the original.
        let resolver = PgIssuerResolver::new(
            Arc::new(fixture.app_pool().clone()),
            Some(Arc::new(sealing_key())),
        );
        let resolved = resolver
            .trusted_issuers(&tenant)
            .await
            .expect("resolve succeeds");
        assert_eq!(resolved.len(), 1);
        match &resolved[0].client_auth {
            ClientAuth::SecretPost(secret) => assert_eq!(secret.expose_secret(), SECRET),
            other => panic!("expected SecretPost, got {other:?}"),
        }

        // LIST returns the redacted view.
        let listed = list_trusted_issuers(State(state), writer(tenant))
            .await
            .expect("list succeeds")
            .0;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].issuer, issuer.as_str());
        let listed_body = serde_json::to_value(&listed).expect("views serialize");
        assert!(
            !listed_body.to_string().contains(SECRET),
            "list response must not echo the plaintext secret"
        );
    }

    #[tokio::test]
    async fn duplicate_create_conflicts() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        let (_server, issuer) = discovery_server().await;

        let _ = create_trusted_issuer(
            State(state.clone()),
            writer(tenant),
            Json(create_issuer_request(issuer.clone(), Some(SECRET))),
        )
        .await
        .expect("first create succeeds");

        let error = create_trusted_issuer(
            State(state),
            writer(tenant),
            Json(create_issuer_request(issuer, Some(SECRET))),
        )
        .await
        .expect_err("a duplicate issuer must conflict");

        assert!(matches!(error.0, WyrdError::AdminConflict { .. }));
    }

    #[tokio::test]
    async fn list_is_empty_and_delete_missing_issuer_is_not_found() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;

        // An empty tenant lists no issuers — absence is an empty Vec, not a 404.
        let listed = list_trusted_issuers(State(state.clone()), writer(tenant))
            .await
            .expect("list succeeds on an empty tenant")
            .0;
        assert!(listed.is_empty());

        let delete_err = delete_trusted_issuer_route(
            State(state),
            writer(tenant),
            Query(DeleteIssuerQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                cascade: false,
            }),
        )
        .await
        .expect_err("missing issuer delete is not found");
        assert!(matches!(delete_err.0, WyrdError::AdminNotFound { .. }));
    }

    #[tokio::test]
    async fn delete_issuer_with_live_binding_conflicts_then_cascades() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        seed_issuer(&fixture, tenant).await;
        let issuer = IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid");

        let _ = create_workload_binding(
            State(state.clone()),
            writer(tenant),
            Json(CreateWorkloadBindingRequest {
                issuer: issuer.clone(),
                subject: "system:serviceaccount:default/sa".to_owned(),
                audience: None,
                card_ref: sample_card_ref(),
            }),
        )
        .await
        .expect("binding create succeeds");

        // A live binding makes the issuer delete fail closed (FK RESTRICT -> 409).
        let conflict = delete_trusted_issuer_route(
            State(state.clone()),
            writer(tenant),
            Query(DeleteIssuerQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                cascade: false,
            }),
        )
        .await
        .expect_err("delete blocked by a live binding must conflict");
        assert!(matches!(conflict.0, WyrdError::AdminConflict { .. }));

        // --cascade removes the binding first, then the issuer.
        let status = delete_trusted_issuer_route(
            State(state),
            writer(tenant),
            Query(DeleteIssuerQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                cascade: true,
            }),
        )
        .await
        .expect("cascade delete succeeds");
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn workload_binding_crud_round_trip() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        seed_issuer(&fixture, tenant).await;
        let issuer = IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid");
        let subject = "system:serviceaccount:default/sa";

        let make_request = || CreateWorkloadBindingRequest {
            issuer: issuer.clone(),
            subject: subject.to_owned(),
            audience: Some("wyrd-workload".to_owned()),
            card_ref: sample_card_ref(),
        };

        let created =
            create_workload_binding(State(state.clone()), writer(tenant), Json(make_request()))
                .await
                .expect("binding create succeeds")
                .0;
        assert_eq!(created.subject, subject);

        // Duplicate (issuer, subject) conflicts.
        let conflict =
            create_workload_binding(State(state.clone()), writer(tenant), Json(make_request()))
                .await
                .expect_err("duplicate binding must conflict");
        assert!(matches!(conflict.0, WyrdError::AdminConflict { .. }));

        // LIST with the issuer filter resolves the binding.
        let listed = list_workload_bindings(
            State(state.clone()),
            writer(tenant),
            Query(BindingFilter {
                issuer: Some(SEEDED_ISSUER.to_owned()),
                subject: None,
            }),
        )
        .await
        .expect("binding list succeeds")
        .0;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].subject, subject);

        // DELETE removes it; a second delete is not found.
        let status = delete_workload_binding_route(
            State(state.clone()),
            writer(tenant),
            Query(BindingQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                subject: subject.to_owned(),
            }),
        )
        .await
        .expect("binding delete succeeds");
        assert_eq!(status, StatusCode::NO_CONTENT);

        let missing = delete_workload_binding_route(
            State(state),
            writer(tenant),
            Query(BindingQuery {
                issuer: SEEDED_ISSUER.to_owned(),
                subject: subject.to_owned(),
            }),
        )
        .await
        .expect_err("deleting an absent binding is not found");
        assert!(matches!(missing.0, WyrdError::AdminNotFound { .. }));
    }

    #[tokio::test]
    async fn create_binding_for_unknown_issuer_is_not_found() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;
        // Deliberately do NOT seed the issuer: the binding's composite FK to
        // auth_trusted_issuers has no target row.
        let issuer = IssuerUrl::new(SEEDED_ISSUER).expect("issuer is valid");

        let error = create_workload_binding(
            State(state),
            writer(tenant),
            Json(CreateWorkloadBindingRequest {
                issuer,
                subject: "system:serviceaccount:default/sa".to_owned(),
                audience: None,
                card_ref: sample_card_ref(),
            }),
        )
        .await
        .expect_err("binding create against a missing issuer must be not-found");
        // The FK violation is a missing referenced issuer (404), not a conflict.
        assert!(matches!(error.0, WyrdError::AdminNotFound { .. }));
    }

    #[tokio::test]
    async fn list_returns_every_issuer_for_the_tenant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let state = test_state(&fixture).await;

        // Seed two distinct issuers directly; the list must return both, proving
        // GET is a tenant-scoped collection read, not a single-resource get.
        for url in ["https://idp-a.example.com", "https://idp-b.example.com"] {
            let trusted = TrustedIssuer {
                tenant_id: tenant,
                issuer: IssuerUrl::new(url).expect("issuer is valid"),
                jwks_uri: format!("{url}/jwks").parse().expect("jwks uri"),
                expected_audience: "wyrd-api".to_owned(),
                client_id: "wyrd-client".to_owned(),
                client_auth: ClientAuth::PrivateKeyJwt,
                claim_mapping: ClaimMapping {
                    subject: ClaimPath::new("sub"),
                    email: None,
                    groups: None,
                },
                group_role_map: HashMap::new(),
                default_roles: Vec::new(),
                principal_kind: IssuerTokenPolicy::Workload,
                jwks_ttl: Duration::from_secs(3600),
            };
            let write = issuer_write_from_trusted(&trusted, None).expect("issuer encodes");
            let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
            upsert_trusted_issuer(&mut conn, &write)
                .await
                .expect("issuer upsert");
            conn.commit().await.expect("issuer seed commits");
        }

        let listed = list_trusted_issuers(State(state), writer(tenant))
            .await
            .expect("list succeeds")
            .0;
        assert_eq!(listed.len(), 2);
    }
}
