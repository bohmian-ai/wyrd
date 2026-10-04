//! Forge's reads of one table's durable maintenance authority.
//!
//! Oracle records an active table read for every query that may still open a
//! table's objects. Forge never interprets which snapshots a reader needs: any
//! active read blocks destructive work for the whole table, and the check
//! serializes with cut acquisition on the table's maintenance-authority row.

use vala_sql::TenantConn;
use vala_sql::queries::oracle_reader_authority::BifrostTableMaintenanceAuthority;
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
    /// closed here.
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
        let stored: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT table_uid FROM vala.bifrost_tables \
             WHERE data_tenant_id = wyrd.current_tenant() AND fqn = $1",
        )
        .bind(&fqn)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(|error| ForgeError::Sql(vala_sql::SqlError::from(error)))?;
        let stored = stored.ok_or_else(|| ForgeError::Invariant {
            detail: format!("table {fqn} has no Bifrost registration to resolve authority from"),
        })?;
        let table_uid =
            <[u8; 16]>::try_from(stored.as_slice()).map_err(|_| ForgeError::Invariant {
                detail: format!("table {fqn} has a registered UID that is not 16 bytes"),
            })?;
        Ok(TableAuthorityIdentity {
            tenant,
            table_uid,
            catalog_name: BIFROST_CATALOG_NAME.to_owned(),
            namespace_name: table_ref.namespace.as_str().to_owned(),
            table_name: table_ref.name.clone(),
        })
    }

    /// Reports whether an Oracle query is still reading one table.
    ///
    /// Takes the table's maintenance authority for the rest of this
    /// transaction, so the answer cannot be overtaken by a cut acquisition
    /// before the caller commits.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the table is unregistered and
    /// [`ForgeError::Sql`] when the lock or active-read statements fail.
    pub(super) async fn has_active_reads(
        &mut self,
        tenant: DataTenantId,
        table_ref: &TableRef,
    ) -> Result<bool, ForgeError> {
        let identity = self.identity(tenant, table_ref).await?;
        BifrostTableMaintenanceAuthority::new(self.conn)
            .has_active_reads(&identity)
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
}
