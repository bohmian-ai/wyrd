//! Operator bootstrap: mint the first admin API key for a fresh tenant.
//!
//! This is the local one-shot path behind the `wyrd-server bootstrap-key`
//! subcommand. It resolves a tenant slug, seeds builtin roles, ensures an admin
//! Service service-account, grants `runtime_admin`, then issues and audits an
//! API key through the unchanged production issuance seams — all inside one
//! tenant transaction so a mid-run failure rolls back. The plaintext key is
//! returned to the caller for a single stdout print and is never logged.

use sqlx::PgPool;
use uuid::Uuid;
use wyrd_runtime::builtin_roles::builtin_role_uuid;
use wyrd_runtime::{PermissionSet, Principal, PrincipalId, PrincipalKind};
use wyrd_semver::VersionBlock;
use wyrd_spec::auth::{IssueKeyRequest, SecretBearer};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::{DataTenantId, TenantSlug};
use wyrd_sql::queries::auth::{
    grant_role_to_service_account, insert_service_account, role_by_name,
    service_account_by_card_ref,
};
use wyrd_sql::queries::platform::tenant_resolver::resolve_by_slug_for_app;
use wyrd_sql::{SqlError, TenantConn};

use crate::auth::issue_api_key::{IssueApiKey, IssueKeyError};
use crate::auth::seed::{SeedError, seed_builtin_roles_for_tenant};

/// Stable system-operator principal id used as `created_by` and the audit
/// `issuer_principal_id`. A fixed nil-style UUID that intentionally references
/// no `auth_users` row.
pub const SYSTEM_OPERATOR_ID: Uuid = Uuid::from_u128(0x018f_0000_0000_7000_8000_0000_0000_00a0);

const BOOTSTRAP_REQUEST_ID: &str = "bootstrap-key";
const BOOTSTRAP_PRINCIPAL_KIND: &str = "service";
const RUNTIME_ADMIN_ROLE: &str = "runtime_admin";

/// Failure while bootstrapping the first admin API key.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    /// The tenant slug did not resolve to an active tenant.
    #[error("unknown tenant slug: {0}")]
    UnknownTenant(String),
    /// Tenant resolution or transaction control failed.
    #[error("tenant sql operation failed: {0}")]
    Sql(#[from] SqlError),
    /// Seeding builtin roles failed.
    #[error("seeding builtin roles failed: {0}")]
    Seed(#[from] SeedError),
    /// A tenant-scoped query failed.
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    /// API-key issuance failed.
    #[error("api key issuance failed: {0}")]
    Issue(#[from] IssueKeyError),
    /// The credential-issuance audit event could not be staged.
    #[error("api key issuance audit failed: {0}")]
    Audit(#[from] wyrd_spec::error::WyrdError),
}

/// Mint the first admin API key for `slug` and return the plaintext key once.
///
/// Seeds builtin roles, ensures the admin Service service-account, grants
/// `runtime_admin`, then issues and audits a key — all in one `TenantConn`
/// transaction that commits only on success. Safe to rerun for the same tenant:
/// the service-account and role grant are create-if-absent while each run mints
/// a fresh key.
///
/// # Errors
/// Returns [`BootstrapError::UnknownTenant`] when the slug does not resolve, and
/// the corresponding variant when any resolve, seed, query, or issuance step
/// fails.
pub async fn bootstrap_admin_key(
    pool: &PgPool,
    slug: &TenantSlug,
) -> Result<SecretBearer, BootstrapError> {
    let data_tenant_id = resolve_by_slug_for_app(pool, slug)
        .await?
        .ok_or_else(|| BootstrapError::UnknownTenant(slug.as_str().to_owned()))?;

    let card_ref = bootstrap_admin_card_ref();
    let actor = bootstrap_operator_principal(data_tenant_id);

    let mut conn = TenantConn::acquire(pool, data_tenant_id).await?;

    seed_builtin_roles_for_tenant(&mut conn, data_tenant_id).await?;

    let service_account_id = ensure_service_account(&mut conn, &card_ref).await?;
    let role_id = resolve_runtime_admin_role(&mut conn, data_tenant_id).await?;
    grant_role_to_service_account(&mut conn, service_account_id, role_id).await?;

    let issuer = IssueApiKey::default();
    let request = IssueKeyRequest {
        card_ref: card_ref.clone(),
        label: Some("bootstrap-admin".to_owned()),
        expires_in_seconds: None,
    };
    let issued = issuer.execute(&mut conn, request, &actor).await?;
    issuer
        .audit(&mut conn, &issued, &actor, BOOTSTRAP_REQUEST_ID)
        .await?;

    conn.commit().await?;

    Ok(issued.response.key)
}

/// Find the admin Service service-account, creating it on first run.
async fn ensure_service_account(
    conn: &mut TenantConn<'_>,
    card_ref: &CardRef,
) -> Result<Uuid, BootstrapError> {
    if let Some(row) = service_account_by_card_ref(conn, BOOTSTRAP_PRINCIPAL_KIND, card_ref).await?
    {
        return Ok(row.id);
    }

    let id = Uuid::new_v4();
    insert_service_account(
        conn,
        id,
        BOOTSTRAP_PRINCIPAL_KIND,
        card_ref,
        card_ref.name.as_str(),
        Some("Wyrd bootstrap admin"),
        SYSTEM_OPERATOR_ID,
    )
    .await?;
    Ok(id)
}

/// Resolve the `runtime_admin` role id, falling back to its deterministic
/// per-tenant id when the seed row is not yet visible by name.
async fn resolve_runtime_admin_role(
    conn: &mut TenantConn<'_>,
    data_tenant_id: DataTenantId,
) -> Result<Uuid, BootstrapError> {
    let role_id = match role_by_name(conn, RUNTIME_ADMIN_ROLE).await? {
        Some(row) => row.id,
        None => builtin_role_uuid(data_tenant_id, RUNTIME_ADMIN_ROLE),
    };
    Ok(role_id)
}

/// The fixed Service card the bootstrap admin key binds to.
fn bootstrap_admin_card_ref() -> CardRef {
    CardRef {
        kind: CardKind::Service,
        name: CardName::new("bootstrap-admin").expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: Some(SpaceName::new("system").expect("static space name is valid")),
        uid: None,
    }
}

/// The system-operator principal carried into issuance and audit so that
/// `created_by` and `issuer_principal_id` are the fixed operator id.
fn bootstrap_operator_principal(data_tenant_id: DataTenantId) -> Principal {
    Principal::new(
        PrincipalId::new(SYSTEM_OPERATOR_ID),
        PrincipalKind::User,
        data_tenant_id,
        Vec::new(),
        PermissionSet::new(),
    )
}
