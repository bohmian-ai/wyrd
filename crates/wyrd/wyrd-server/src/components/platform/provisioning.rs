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
    SecretBearer, TenantListResponse,
};
use wyrd_sql::queries::auth::{
    grant_role_to_service_account, insert_api_key, insert_service_account, list_api_key_metadata,
    revoke_api_key, role_by_name, tenant_admin_principal_id,
};
use wyrd_sql::queries::platform::provisioning::{
    insert_provisioning_tenant, mark_tenant_active, mark_tenant_failed, set_tenant_suspended,
};
use wyrd_sql::queries::platform::tenants::{list_tenants, tenant_by_id};
use wyrd_sql::row_types::platform::TenantRow;
use wyrd_sql::{OperatorPool, SqlError, WyrdPostgres};

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
    /// The named tenant is not one that may be acted on: it does not exist,
    /// is soft-deleted, or is in a lifecycle state other than active. The
    /// causes deliberately share one variant so a refusal cannot be used to
    /// discover which tenants exist.
    #[error("no active tenant to act on")]
    TenantUnavailable,
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
    /// Wyrd control-plane handle the new tenant's own rows are written through.
    ///
    /// Held rather than a bare pool so tenant work is acquired through
    /// [`WyrdPostgres::tenant_conn`], which carries the acquisition telemetry
    /// and the row-level-security tenant bind. A privileged transaction is not
    /// a portable capability this service hands out.
    postgres: WyrdPostgres,
}

impl std::fmt::Debug for TenantProvisioning {
    /// Prints the handle without its boundaries, which have no inspectable state.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantProvisioning").finish_non_exhaustive()
    }
}

impl TenantProvisioning {
    /// Bind provisioning to the two boundaries it writes across.
    #[must_use]
    pub const fn new(operator: OperatorPool, postgres: WyrdPostgres) -> Self {
        Self { operator, postgres }
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
        let mut conn = authz
            .authorize(
                &caller.context,
                &Permission::tenant_create(),
                caller.request_id.as_str(),
                Some(data_tenant_id),
            )
            .await?;

        // A previously failed attempt at this slug is resumed under its own
        // tenant id rather than refused. Keeping the original id matters: the
        // failed attempt may already have written tenant-scoped rows, and a
        // second id would orphan them.
        let claim = insert_provisioning_tenant(
            &mut conn,
            data_tenant_id,
            request.slug.as_str(),
            &request.display_name,
        )
        .await
        .map_err(slug_or_store)?;
        let data_tenant_id = claim.tenant_id(data_tenant_id);
        conn.commit()
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?;

        // From here the tenant exists but is not usable. Any failure marks it
        // failed rather than leaving a tenant that looks live with no way in.
        // Promotion is the last stage, so it is part of what can fail: a tenant
        // whose rows exist but that never became active is as unusable as one
        // that never got its administrator, and must be marked the same way.
        let established = match self
            .establish_tenant_administration(data_tenant_id, caller.principal_id().as_uuid())
            .await
        {
            Ok(admin) => match mark_tenant_active(&self.operator, data_tenant_id).await {
                Ok(true) => Ok(admin),
                Ok(false) => Err(ProvisionError::Store(
                    "tenant left provisioning before it could be promoted".to_owned(),
                )),
                Err(error) => Err(ProvisionError::Store(error.to_string())),
            },
            Err(error) => Err(error),
        };

        let error = match established {
            Ok(admin) => {
                return Ok(CreateTenantResponse {
                    tenant: ProvisionedTenant {
                        id: data_tenant_id,
                        slug: request.slug,
                        display_name: request.display_name,
                        status: "active".to_owned(),
                    },
                    admin,
                });
            }
            Err(error) => error,
        };

        // Marking the tenant failed is what makes the slug retryable, so
        // failing to record it is surfaced rather than swallowed: the caller
        // otherwise retries against a row that will not be adopted until it
        // goes stale.
        if let Err(mark) =
            mark_tenant_failed(&self.operator, data_tenant_id, &error.to_string()).await
        {
            return Err(ProvisionError::Store(format!(
                "{error}; the tenant could not be marked failed: {mark}"
            )));
        }
        Err(error)
    }

    /// List the tenant directory an operator administers.
    ///
    /// Every lifecycle state is included, because the states an operator needs
    /// to see are precisely the non-active ones. Reading the directory is an
    /// authorized decision like any other and is audited as one.
    ///
    /// # Errors
    /// Returns [`ProvisionError::Denied`] when the caller lacks `tenants:read`,
    /// [`ProvisionError::AuditUnavailable`] when the decision cannot be
    /// recorded, and [`ProvisionError::Store`] when the read fails.
    #[tracing::instrument(level = "info", skip(self, caller), err)]
    pub async fn list(
        &self,
        caller: &PlatformCaller,
    ) -> Result<TenantListResponse, ProvisionError> {
        self.authorize_read(caller, None).await?;
        let rows = list_tenants(&self.operator)
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?;
        Ok(TenantListResponse {
            tenants: rows
                .into_iter()
                .map(summarize)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    /// Read one tenant's directory row.
    ///
    /// Refuses an unknown or soft-deleted tenant with the same
    /// [`ProvisionError::TenantUnavailable`] every other miss uses, so an
    /// operator learns nothing from a refusal that a listing would not tell
    /// them anyway.
    ///
    /// # Errors
    /// Returns [`ProvisionError::Denied`] when the caller lacks `tenants:read`,
    /// [`ProvisionError::TenantUnavailable`] when no such row exists,
    /// [`ProvisionError::AuditUnavailable`] when the decision cannot be
    /// recorded, and [`ProvisionError::Store`] when the read fails.
    #[tracing::instrument(level = "info", skip(self, caller), fields(tenant = %tenant_id), err)]
    pub async fn inspect(
        &self,
        caller: &PlatformCaller,
        tenant_id: DataTenantId,
    ) -> Result<ProvisionedTenant, ProvisionError> {
        self.authorize_read(caller, Some(tenant_id)).await?;
        tenant_by_id(&self.operator, tenant_id)
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?
            .ok_or(ProvisionError::TenantUnavailable)
            .and_then(summarize)
    }

    /// Suspend a tenant or restore it.
    ///
    /// Suspension freezes admission without destroying anything: the tenant's
    /// rows, grants, and credentials survive, so resuming restores exactly what
    /// was there. The transition is conditional on the tenant currently holding
    /// the opposite state, so suspending a `provisioning` or `failed` tenant —
    /// or replaying either call — changes nothing and is refused rather than
    /// silently reported as done.
    ///
    /// # Errors
    /// Returns [`ProvisionError::Denied`] when the caller lacks
    /// `tenants:suspend`, [`ProvisionError::TenantUnavailable`] when the tenant
    /// is not in the state the transition requires,
    /// [`ProvisionError::AuditUnavailable`] when the decision cannot be
    /// recorded — in which case nothing changes — and
    /// [`ProvisionError::Store`] when the write fails.
    #[tracing::instrument(level = "info", skip(self, caller), fields(tenant = %tenant_id), err)]
    pub async fn set_suspended(
        &self,
        caller: &PlatformCaller,
        tenant_id: DataTenantId,
        suspended: bool,
    ) -> Result<(), ProvisionError> {
        // The handle must outlive the transaction it lends out.
        let authz = PlatformAuthorization::new(self.operator.clone());
        let mut decision = authz
            .authorize(
                &caller.context,
                &Permission::tenant_suspend(),
                caller.request_id.as_str(),
                Some(tenant_id),
            )
            .await?;

        // Both halves of the same plane, so both commit together. A transition
        // recorded as allowed that never applied — or an applied one with no
        // record — is exactly the mismatch the canonical audit rule forbids.
        if set_tenant_suspended(&mut decision, tenant_id, suspended)
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))?
        {
            decision
                .commit()
                .await
                .map_err(|e| ProvisionError::Store(e.to_string()))
        } else {
            Err(ProvisionError::TenantUnavailable)
        }
    }

    /// Authorize and audit a directory read, then release the decision.
    ///
    /// The read itself runs outside this transaction: it touches only the
    /// directory the decision was recorded against, so holding the transaction
    /// open across it would buy nothing.
    ///
    /// # Errors
    /// Returns [`ProvisionError::Denied`] when the caller lacks `tenants:read`,
    /// [`ProvisionError::AuditUnavailable`] when the decision cannot be
    /// recorded, and [`ProvisionError::Store`] when the commit fails.
    async fn authorize_read(
        &self,
        caller: &PlatformCaller,
        tenant_id: Option<DataTenantId>,
    ) -> Result<(), ProvisionError> {
        let authz = PlatformAuthorization::new(self.operator.clone());
        let decision = authz
            .authorize(
                &caller.context,
                &Permission::tenant_read(),
                caller.request_id.as_str(),
                tenant_id,
            )
            .await?;
        decision
            .commit()
            .await
            .map_err(|e| ProvisionError::Store(e.to_string()))
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
        let mut conn = self
            .postgres
            .tenant_conn(data_tenant_id)
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
            Some(existing) => {
                // The failed attempt may have committed a credential before it
                // stopped, and its plaintext was never disclosed to anyone. A
                // retry returns exactly one usable way in, so every credential
                // this principal already holds is retired first rather than
                // left live and unaccounted for.
                for credential in list_api_key_metadata(&mut conn, existing)
                    .await
                    .map_err(|e| ProvisionError::Store(e.to_string()))?
                {
                    revoke_api_key(&mut conn, credential.id)
                        .await
                        .map_err(|e| ProvisionError::Store(e.to_string()))?;
                }
                existing
            }
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

/// Project a directory row onto the wire tenant shape.
///
/// A stored id or slug that fails validation means the directory holds a row
/// no current write path could have produced, so it is reported as a store
/// failure rather than papered over with a substitute value an operator would
/// then act on.
///
/// # Errors
/// Returns [`ProvisionError::Store`] when the stored id is not a Wyrd tenant id
/// or the stored slug is not a valid slug.
fn summarize(row: TenantRow) -> Result<ProvisionedTenant, ProvisionError> {
    Ok(ProvisionedTenant {
        id: DataTenantId::new(row.data_tenant_id)
            .map_err(|e| ProvisionError::Store(e.to_string()))?,
        slug: row
            .slug
            .parse()
            .map_err(|e: wyrd_spec::ids::IdError| ProvisionError::Store(e.to_string()))?,
        display_name: row.display_name,
        status: row.status,
    })
}
