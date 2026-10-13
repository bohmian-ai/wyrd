//! Forge's reads of one table's durable maintenance authority.
//!
//! Oracle records nothing about its reads: snapshot retention protects them.
//! Destruction runs only under [`TableAuthority::exclusive`], a capability
//! that holds the table's maintenance-authority row through the effect's known
//! outcome, so Forge's destructive protocols on one table never overlap.

use vala_sql::TenantConn;
use vala_sql::queries::oracle_reader_authority::{
    BifrostTableMaintenanceAuthority, ExclusiveTableAuthority,
};
use vala_sql::row_types::oracle_reader_authority::TableAuthorityIdentity;
use wyrd_spec::DataTenantId;

use super::error::ForgeError;
use crate::catalog::{BIFROST_CATALOG_NAME, TableRef};

/// One tenant transaction's view of a table's maintenance authority.
///
/// Holds the connection rather than taking one per call because every consumer
/// reads this alongside other durable roots inside one transaction: a decision
/// assembled from roots read in two transactions is not a smaller decision, it
/// is an unsafe one.
pub(super) struct TableAuthority<'conn, 'tx> {
    /// Tenant-bound connection every statement runs inside.
    conn: &'conn mut TenantConn<'tx>,
}

impl<'conn, 'tx> TableAuthority<'conn, 'tx> {
    /// Binds the authority reads to one tenant transaction.
    pub(super) fn new(conn: &'conn mut TenantConn<'tx>) -> Self {
        Self { conn }
    }

    /// Resolves the durable authority identity of one registered table.
    ///
    /// `table_uid`, not a reconstructed namespace string, is durable table
    /// identity, so this is a registry lookup rather than a formatting step. A
    /// table with no registration row cannot be proven unprotected and fails
    /// closed here. The registry read itself belongs to `vala-sql`'s
    /// `olap_catalog` owner; this method only projects its typed row.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] when the lookup fails and
    /// [`ForgeError::Invariant`] when the table has no registration row or its
    /// stored UID is not the 16 bytes the schema requires.
    pub(super) async fn identity(
        &mut self,
        tenant: DataTenantId,
        table_ref: &TableRef,
    ) -> Result<TableAuthorityIdentity, ForgeError> {
        let fqn = table_ref.fqn();
        let registered = vala_sql::queries::olap_catalog::get_by_fqn(self.conn, &fqn)
            .await
            .map_err(ForgeError::Sql)?
            .ok_or_else(|| ForgeError::Invariant {
                detail: format!(
                    "table {fqn} has no Bifrost registration to resolve authority from"
                ),
            })?;
        let table_uid = <[u8; 16]>::try_from(registered.table_uid.as_slice()).map_err(|_| {
            ForgeError::Invariant {
                detail: format!("table {fqn} has a registered UID that is not 16 bytes"),
            }
        })?;
        Ok(TableAuthorityIdentity {
            tenant,
            table_uid,
            catalog_name: BIFROST_CATALOG_NAME.to_owned(),
            namespace_name: table_ref.namespace.as_str().to_owned(),
            table_name: table_ref.name.clone(),
        })
    }

    /// Takes the table's exclusive maintenance authority for destructive work.
    ///
    /// This is the only way Forge obtains the capability its snapshot-expiry
    /// commit, expired-cleanup delete, and orphan delete require. The returned
    /// value keeps this transaction mutably borrowed, so the caller drops it
    /// and then commits the transaction once the external effect's outcome is
    /// known — and before settlement, which takes the same row exclusively on
    /// another connection.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the table is unregistered and
    /// [`ForgeError::Sql`] when the lock or identity check fails.
    pub(super) async fn exclusive(
        mut self,
        tenant: DataTenantId,
        table_ref: &TableRef,
    ) -> Result<ExclusiveTableAuthority<'conn, 'tx>, ForgeError> {
        let identity = self.identity(tenant, table_ref).await?;
        BifrostTableMaintenanceAuthority::new(self.conn)
            .exclusive(identity)
            .await
            .map_err(ForgeError::Sql)
    }

    /// Lists every snapshot an unresolved Forge expiration already claims.
    ///
    /// A claimed snapshot is already owned by another prepared operation, so a
    /// new selection drops it rather than preparing the same deletion twice.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the table is unregistered and
    /// [`ForgeError::Sql`] when the index read fails.
    pub(super) async fn claimed_snapshot_ids(
        &mut self,
        tenant: DataTenantId,
        table_ref: &TableRef,
    ) -> Result<Vec<i64>, ForgeError> {
        let identity = self.identity(tenant, table_ref).await?;
        BifrostTableMaintenanceAuthority::new(self.conn)
            .claimed_snapshots(&identity)
            .await
            .map_err(ForgeError::Sql)
    }

    /// Records the table's replaced snapshots and returns those still inside
    /// the retention window.
    ///
    /// `replaced` is every retained snapshot that is not current. A returned
    /// snapshot was first observed replaced less than `retention` ago, so an
    /// Oracle query may still be reading it and expiry must keep it.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the table is unregistered and
    /// [`ForgeError::Sql`] when the statement fails.
    pub(super) async fn retained_supersessions(
        &mut self,
        tenant: DataTenantId,
        table_ref: &TableRef,
        replaced: &[i64],
        retention: std::time::Duration,
    ) -> Result<Vec<i64>, ForgeError> {
        let identity = self.identity(tenant, table_ref).await?;
        BifrostTableMaintenanceAuthority::new(self.conn)
            .retained_supersessions(&identity, replaced, retention)
            .await
            .map_err(ForgeError::Sql)
    }
}

/// Fails closed unless a live exclusive authority covers `tenant`'s exact
/// table `table_ref`.
///
/// Every destructive Forge effect takes the capability by reference and calls
/// this before acting, so the borrow keeps the authority held through the
/// effect and a capability for one table can never authorize another.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the capability covers a different
/// tenant, catalog, namespace, or table.
pub(super) fn require_covers(
    exclusive: &ExclusiveTableAuthority<'_, '_>,
    tenant: DataTenantId,
    table_ref: &TableRef,
) -> Result<(), ForgeError> {
    let identity = exclusive.identity();
    if exclusive.tenant() == tenant
        && identity.tenant == tenant
        && identity.catalog_name == BIFROST_CATALOG_NAME
        && identity.namespace_name == table_ref.namespace.as_str()
        && identity.table_name == table_ref.name
    {
        return Ok(());
    }
    Err(ForgeError::Invariant {
        detail: format!(
            "destructive maintenance on {} is not covered by its exclusive table authority",
            table_ref.fqn()
        ),
    })
}
