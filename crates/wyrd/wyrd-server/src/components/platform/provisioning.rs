//! Tenant provisioning.
//!
//! Creating a tenant is one authorized operation that yields a *usable* tenant:
//! the directory row, its administrative principal, that principal's role, its
//! first credential, and the tenant's builtin roles. Nothing partial is ever
//! presented as live — the tenant is promoted only once every piece exists.
//!
//! Provisioning necessarily crosses two boundaries. The directory row sits at
//! platform scope behind the operator role; the tenant's principal, roles, and
//! credential sit under row-level security inside the tenant. One transaction
//! cannot span both, so the order is chosen so that every failure leaves a
//! tenant that is visibly incomplete rather than one that looks live and has no
//! way in.

use chrono::Duration;
use secrecy::ExposeSecret;
use uuid::Uuid;
use wyrd_auth::platform_authz::{PlatformAuthorization, PlatformAuthzError};
use wyrd_auth::seed::seed_builtin_roles_for_tenant;
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    CreateTenantRequest, CreateTenantResponse, ProvisionedTenant, ProvisionedTenantAdmin,
    SecretBearer,
};
use wyrd_sql::queries::auth::{
    grant_role_to_service_account, insert_api_key, insert_service_account, role_by_name,
    tenant_admin_principal_id,
};
use wyrd_sql::queries::platform::provisioning::{
    insert_provisioning_tenant, mark_tenant_active, mark_tenant_failed,
};
use wyrd_sql::{OperatorPool, SqlError, TenantConn};

use crate::components::auth::PlatformCaller;

/// Role a tenant administrative principal is granted at provisioning.
///
/// The tenant's own `admin` builtin, not a platform grant: the tenant
/// administrator is the root of trust *inside* one tenant and holds no
/// authority over the directory that created it.
const TENANT_ADMIN_ROLE: &str = "admin";

/// Lifetime of the credential handed back at provisioning.
///
/// Long, because it is the only way into a brand-new tenant and an operator may
/// not configure it immediately. Rotating it to something shorter-lived is the
/// tenant administrator's first available action.
const INITIAL_CREDENTIAL_DAYS: i64 = 365;

/// Tenant provisioning failure.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionError {
    /// The caller is not authorized to create tenants. Already audited.
    #[error("not authorized to provision tenants")]
    Denied,
    /// The slug is already taken by another tenant.
    #[error("tenant slug is already in use")]
    SlugTaken,
    /// Authorization could not be recorded, so provisioning did not proceed.
    #[error("provisioning could not be audited: {0}")]
    AuditUnavailable(String),
    /// A store write failed; the tenant is left visibly incomplete.
    #[error("tenant provisioning failed: {0}")]
    Store(String),
}

impl From<PlatformAuthzError> for ProvisionError {
    fn from(error: PlatformAuthzError) -> Self {
        match error {
            PlatformAuthzError::Denied { .. } => Self::Denied,
            PlatformAuthzError::AuditUnavailable(error) => {
                Self::AuditUnavailable(error.to_string())
            }
            PlatformAuthzError::Transaction(error) => Self::Store(error.to_string()),
        }
    }
}

/// Provisions tenants on behalf of an authorized platform caller.
///
/// Owns both boundaries it must write across, because provisioning is only
/// correct as the ordered composition of the two.
#[derive(Clone)]
pub struct TenantProvisioning {
    /// Platform boundary owning the tenant directory.
    operator: OperatorPool,
    /// Row-level-secured pool the new tenant's own rows are written through.
    app: sqlx::PgPool,
}

impl std::fmt::Debug for TenantProvisioning {
    /// Prints the handle without its pools, which have no inspectable state.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantProvisioning").finish_non_exhaustive()
    }
}

impl TenantProvisioning {
    /// Bind provisioning to the two boundaries it writes across.
    #[must_use]
    pub const fn new(operator: OperatorPool, app: sqlx::PgPool) -> Self {
        Self { operator, app }
    }

    /// Provision a tenant and return it with its one-time admin credential.
    ///
    /// The authorization decision and the directory row commit together, so a
    /// tenant never exists without a recorded decision permitting it. The
    /// tenant-scoped work then commits on its own boundary, and only then is the
    /// tenant promoted to active. A failure after the directory row exists marks
    /// the tenant failed rather than leaving it to look live.
    ///
    /// # Errors
    /// Returns [`ProvisionError::Denied`] when the caller lacks
    /// `tenants:write`, [`ProvisionError::SlugTaken`] when the slug is in use,
    /// [`ProvisionError::AuditUnavailable`] when the decision cannot be
    /// recorded — in which case nothing is created — and
    /// [`ProvisionError::Store`] when a write fails.
    #[tracing::instrument(level = "info", skip(self, caller), fields(slug = %request.slug), err)]
    pub async fn provision(
        &self,
        caller: &PlatformCaller,
        request: CreateTenantRequest,
    ) -> Result<CreateTenantResponse, ProvisionError> {
        let data_tenant_id = DataTenantId::new_v7();

        // The handle must outlive the transaction it lends out.
        let authz = PlatformAuthorization::new(self.operator.clone());
        let mut tx = authz
            .authorize(
                &caller.context,
                &Permission::tenant_create(),
                caller.request_id.as_str(),
                caller.credential_id,
                Some(data_tenant_id),
            )
            .await?;

        // A previously failed attempt at this slug is resumed under its own
        // tenant id rather than refused. Keeping the original id matters: the
        // failed attempt may already have written tenant-scoped rows, and a
        // second id would orphan them.
        let claim = insert_provisioning_tenant(
            &mut tx,
            data_tenant_id,
            request.slug.as_str(),
            &request.display_name,
        )
        .await
        .map_err(slug_or_store)?;
        let data_tenant_id = claim.tenant_id(data_tenant_id);
        tx.commit()
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?;

        // From here the tenant exists but is not usable. Any failure marks it
        // failed rather than leaving a tenant that looks live with no way in.
        match self
            .establish_tenant_administration(data_tenant_id, caller.principal_id().as_uuid())
            .await
        {
            Ok(admin) => {
                mark_tenant_active(&self.operator, data_tenant_id)
                    .await
                    .map_err(|e| ProvisionError::Store(e.to_string()))?;
                Ok(CreateTenantResponse {
                    tenant: ProvisionedTenant {
                        id: data_tenant_id,
                        slug: request.slug,
                        display_name: request.display_name,
                        status: "active".to_owned(),
                    },
                    admin,
                })
            }
            Err(error) => {
                let _ =
                    mark_tenant_failed(&self.operator, data_tenant_id, &error.to_string()).await;
                Err(error)
            }
        }
    }

    /// Create the tenant's administrative principal, roles, and credential.
    ///
    /// One tenant-scoped transaction: the principal, its builtin roles, its
    /// administrative grant, and its credential all commit together, so the
    /// tenant either has a complete way in or none at all.
    ///
    /// # Errors
    /// Returns [`ProvisionError::Store`] when any write fails.
    async fn establish_tenant_administration(
        &self,
        data_tenant_id: DataTenantId,
        created_by: Uuid,
    ) -> Result<ProvisionedTenantAdmin, ProvisionError> {
        let mut conn = TenantConn::acquire(&self.app, data_tenant_id)
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?;

        seed_builtin_roles_for_tenant(&mut conn, data_tenant_id)
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?;

        // A resumed attempt may find the administrative principal already
        // created by the failed one. Reusing it is what keeps the tenant's
        // identity stable across the retry; creating a second would leave the
        // first behind holding the same role.
        let principal_id = match tenant_admin_principal_id(&mut conn)
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?
        {
            Some(existing) => existing,
            None => {
                let principal_id = Uuid::now_v7();
                insert_service_account(
                    &mut conn,
                    principal_id,
                    "tenant_admin",
                    None,
                    "tenant-admin",
                    Some("Tenant administrative principal"),
                    created_by,
                )
                .await
                .map_err(|e| ProvisionError::Store(e.to_string()))?;
                principal_id
            }
        };

        let role = role_by_name(&mut conn, TENANT_ADMIN_ROLE)
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?
            .ok_or_else(|| ProvisionError::Store("tenant admin role was not seeded".to_owned()))?;
        grant_role_to_service_account(&mut conn, principal_id, role.id)
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?;

        let plaintext = wyrd_auth::issue_api_key::WyrdApiKey::generate(data_tenant_id);
        let raw = plaintext.secret.clone();
        let key_hash = tokio::task::spawn_blocking(move || wyrd_auth_issue::hash_api_key(&raw))
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?
            .map_err(|e| ProvisionError::Store(e.to_string()))?;
        insert_api_key(
            &mut conn,
            Uuid::now_v7(),
            principal_id,
            &plaintext.prefix,
            &key_hash,
            created_by,
            Some(chrono::Utc::now() + Duration::days(INITIAL_CREDENTIAL_DAYS)),
        )
        .await
        .map_err(|e| ProvisionError::Store(e.to_string()))?;

        conn.commit()
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?;

        Ok(ProvisionedTenantAdmin {
            principal_id: wyrd_spec::auth::PrincipalId::new(principal_id),
            credential: SecretBearer::new(plaintext.secret.expose_secret().to_owned()),
        })
    }
}

/// Distinguish a taken slug from a genuine store failure.
///
/// A slug collision is how two concurrent creations for the same tenant
/// converge on one row, so it is an expected outcome rather than a fault. The
/// SQL tier already classifies the violation; this only names what it means
/// here.
fn slug_or_store(error: SqlError) -> ProvisionError {
    match error {
        SqlError::UniqueViolation { .. } => ProvisionError::SlugTaken,
        other => ProvisionError::Store(other.to_string()),
    }
}
