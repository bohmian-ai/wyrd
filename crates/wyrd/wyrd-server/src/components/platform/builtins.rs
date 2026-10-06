//! Eager provisioning of the canonical Bifrost built-in tables.
//!
//! Every tenant owns the whole canonical inventory from the moment it is
//! usable: tenant provisioning ensures it before promotion, and server startup
//! ensures it again for every active tenant so a table added to the inventory
//! reaches tenants that predate it. Both paths call the catalog's own
//! idempotent [`BifrostCatalog::ensure_builtin`] over the canonical
//! [`builtin_tables`] list; this module owns only *when* that happens.

use std::sync::Arc;

use vala_bifrost_redux::catalog::{BifrostCatalog, BifrostCatalogError};
use vala_bifrost_redux::tables::builtin_tables;
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::platform::tenants::list_active_tenant_ids;
use wyrd_sql::{OperatorPool, SqlError};

/// Failure to ensure the built-in inventory.
#[derive(Debug, thiserror::Error)]
pub enum BuiltinTablesError {
    /// The tenant directory could not be listed.
    #[error("active tenants could not be listed: {0}")]
    Directory(#[from] SqlError),
    /// The catalog could not ensure one built-in for one tenant.
    #[error("built-in tables could not be ensured for tenant {tenant}: {source}")]
    Catalog {
        /// Tenant whose inventory was being ensured.
        tenant: DataTenantId,
        /// Catalog failure for the first built-in that could not be ensured.
        #[source]
        source: BifrostCatalogError,
    },
}

/// Ensures the canonical built-in inventory through the shared catalog.
///
/// Held by tenant provisioning and constructed once at startup; both share the
/// one ensure-all loop so the two paths cannot drift on which tables a tenant
/// receives.
#[derive(Clone)]
pub struct BuiltinTables {
    /// Catalog owning table registration and the idempotent ensure operation.
    catalog: Arc<BifrostCatalog>,
}

impl std::fmt::Debug for BuiltinTables {
    /// Prints the handle without the catalog, which has no inspectable state.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuiltinTables").finish_non_exhaustive()
    }
}

impl BuiltinTables {
    /// Bind built-in provisioning to the process's shared catalog.
    #[must_use]
    pub const fn new(catalog: Arc<BifrostCatalog>) -> Self {
        Self { catalog }
    }

    /// Ensure every canonical built-in exists for one tenant.
    ///
    /// Each ensure is idempotent, so a repeat — a resumed provisioning or a
    /// restart — registers nothing new and returns success. Tables are ensured
    /// in inventory order and the first failure stops the loop; tables ensured
    /// before it remain registered, which a retry absorbs.
    ///
    /// # Errors
    /// Returns [`BuiltinTablesError::Catalog`] when the catalog cannot ensure a
    /// built-in for `tenant`.
    pub async fn ensure_tenant(&self, tenant: DataTenantId) -> Result<(), BuiltinTablesError> {
        for definition in builtin_tables() {
            self.catalog
                .ensure_builtin(tenant, definition)
                .await
                .map_err(|source| BuiltinTablesError::Catalog { tenant, source })?;
        }
        Ok(())
    }

    /// Ensure every canonical built-in for every active tenant.
    ///
    /// Startup reconciliation: lists the active tenants on the operator
    /// boundary, which alone may read the directory, then ensures each
    /// tenant's inventory under that tenant's own catalog binding. Suspended
    /// and deleted tenants are skipped, matching every other startup sweep, as
    /// is [`DataTenantId::SYSTEM_OWNER`]: it attributes platform records and
    /// owns no data-tenant table binding.
    ///
    /// # Errors
    /// Returns [`BuiltinTablesError::Directory`] when the directory read fails
    /// and [`BuiltinTablesError::Catalog`] for the first tenant whose inventory
    /// cannot be ensured; tenants reconciled before it stay reconciled.
    #[tracing::instrument(level = "info", skip_all, err)]
    pub async fn reconcile(&self, directory: &OperatorPool) -> Result<(), BuiltinTablesError> {
        // ponytail: sequential per tenant; bound-concurrent if startup with many tenants is slow.
        for tenant in list_active_tenant_ids(directory).await? {
            if tenant == DataTenantId::SYSTEM_OWNER {
                continue;
            }
            self.ensure_tenant(tenant).await?;
        }
        Ok(())
    }
}
