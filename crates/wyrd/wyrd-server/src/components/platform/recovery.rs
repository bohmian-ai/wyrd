//! Tenant administrative recovery.
//!
//! Losing every credential of a tenant administrator must not cost the tenant.
//! Because identity and credentials are separate, recovery issues a *new*
//! credential for the *existing* principal: the principal id, its role grants,
//! and every resource in the tenant are untouched.
//!
//! This is the one named capability by which the platform plane reaches into a
//! tenant, so it is authorized by its own permission and is distinguishable in
//! audit from ordinary tenant administration. It confers nothing further: a
//! platform principal that recovers a tenant still cannot read or write
//! anything inside it.

use chrono::Duration;
use secrecy::ExposeSecret;
use uuid::Uuid;
use wyrd_auth::platform_authz::PlatformAuthorization;
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, ProvisionedTenantAdmin, SecretBearer};
use wyrd_sql::queries::auth::{insert_api_key, tenant_admin_principal_id};
use wyrd_sql::queries::platform::tenants::tenant_by_id;
use wyrd_sql::{OperatorPool, TenantConn};

use crate::components::auth::PlatformCaller;
use crate::components::platform::provisioning::ProvisionError;

/// Lifetime of a recovery credential.
///
/// Matches the credential issued at provisioning: recovery restores the same
/// kind of access, not a lesser or more urgent one.
const RECOVERY_CREDENTIAL_DAYS: i64 = 365;

/// Restores administrative access to tenants that have lost it.
#[derive(Clone)]
pub struct TenantRecovery {
    /// Platform boundary the decision and its audit record commit through.
    ///
    /// The only durable capability this owner holds. The tenant transaction
    /// the replacement credential is written on is lent to it per call, so
    /// recovery cannot reach a tenant its caller did not name.
    operator: OperatorPool,
}

impl std::fmt::Debug for TenantRecovery {
    /// Prints the handle without its pools, which have no inspectable state.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantRecovery").finish_non_exhaustive()
    }
}

impl TenantRecovery {
    /// Bind recovery to the platform boundary it owns.
    #[must_use]
    pub const fn new(operator: OperatorPool) -> Self {
        Self { operator }
    }

    /// Issue a replacement credential for a tenant's existing administrator.
    ///
    /// Never creates a second administrative principal: the tenant already has
    /// one, and duplicating it would leave two roots of trust where the tenant
    /// expects one. The returned credential is the plaintext, exposed once.
    ///
    /// `conn` is the tenant transaction the caller acquired for `tenant_id`.
    /// It is therefore open before the directory refusal below, but nothing is
    /// ever written on it for a tenant that is not active: an unused
    /// transaction is discarded when this call returns.
    ///
    /// # Errors
    /// Returns [`ProvisionError::Denied`] when the caller lacks the recovery
    /// permission, [`ProvisionError::AuditUnavailable`] when the decision
    /// cannot be recorded — in which case no credential is issued — and
    /// [`ProvisionError::TenantUnavailable`] when the platform directory holds
    /// no active row for `tenant_id`, and [`ProvisionError::Store`] when the
    /// tenant has no administrative principal or a write fails.
    #[tracing::instrument(level = "info", skip(self, caller, conn), fields(tenant = %tenant_id), err)]
    pub async fn recover(
        &self,
        caller: &PlatformCaller,
        tenant_id: DataTenantId,
        mut conn: TenantConn<'_>,
    ) -> Result<ProvisionedTenantAdmin, ProvisionError> {
        let authz = PlatformAuthorization::new(self.operator.clone());
        let decision = authz
            .authorize(
                &caller.context,
                &Permission::tenant_recover_admin(),
                caller.request_id.as_str(),
                Some(tenant_id),
            )
            .await?;
        // The decision stands on its own: the replacement credential is written
        // on the tenant boundary, which this transaction cannot reach.
        decision
            .commit()
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?;

        // The tenant directory decides whether this tenant may be acted on at
        // all, and it is read on the platform boundary this call already owns.
        // A provisioning, failed, suspended, or soft-deleted tenant must not
        // acquire a new durable secret: recovery would hand out a working key
        // into state the deployment has deliberately frozen or abandoned.
        let directory = tenant_by_id(&self.operator, tenant_id)
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?;
        if !directory.is_some_and(|row| row.status == "active") {
            return Err(ProvisionError::TenantUnavailable);
        }

        let principal_id = tenant_admin_principal_id(&mut conn)
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?
            .ok_or_else(|| {
                ProvisionError::Store(
                    "tenant has no administrative principal to recover".to_owned(),
                )
            })?;

        let plaintext = wyrd_auth::issue_api_key::WyrdApiKey::generate(tenant_id);
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
            caller.principal_id().as_uuid(),
            Some(chrono::Utc::now() + Duration::days(RECOVERY_CREDENTIAL_DAYS)),
        )
        .await
        .map_err(|e| ProvisionError::Store(e.to_string()))?;

        conn.commit()
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?;

        Ok(ProvisionedTenantAdmin {
            principal_id: PrincipalId::new(principal_id),
            credential: SecretBearer::new(plaintext.secret.expose_secret().to_owned()),
        })
    }
}
