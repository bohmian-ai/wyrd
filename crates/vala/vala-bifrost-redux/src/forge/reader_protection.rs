//! Forge's read of the durable Oracle reader protection for one table.
//!
//! Reader protection is published by a different process than the one deciding
//! destructive maintenance, so Forge only ever reads it, and it reads it the
//! same way for every protocol: resolve the registered table UID, list every
//! epoch's validated frontier for that table, and project the frontier into the
//! shape the deciding owner already understands.
//!
//! A header that exists protects, whatever its epoch's heartbeat or state says.
//! Only the audited invalidation-and-release sequence removes one, so this
//! module never filters by liveness and never substitutes an empty set for
//! evidence it could not read.

use vala_sql::TenantConn;
use vala_sql::queries::oracle_reader_authority::{
    BifrostTableMaintenanceAuthority, OracleTableProtections,
};
use vala_sql::row_types::forge_tasks::SnapshotWatermark;
use vala_sql::row_types::oracle_reader_authority::{ProtectionRecord, TableAuthorityIdentity};
use wyrd_spec::DataTenantId;

use super::error::ForgeError;
use crate::catalog::{BIFROST_CATALOG_NAME, TableRef};

/// One tenant transaction's view of durable reader protection.
///
/// Holds the connection rather than taking one per call because every consumer
/// reads protection alongside other durable roots inside one transaction: a
/// decision assembled from roots read in two transactions is not a smaller
/// decision, it is an unsafe one.
pub(super) struct ReaderProtection<'conn, 'tx> {
    /// Tenant-bound connection every statement runs inside.
    conn: &'conn mut TenantConn<'tx>,
}

impl<'conn, 'tx> ReaderProtection<'conn, 'tx> {
    /// Binds the protection read to one tenant transaction.
    pub(super) fn new(conn: &'conn mut TenantConn<'tx>) -> Self {
        Self { conn }
    }

    /// Resolves the durable protection identity of one registered table.
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
    async fn identity(
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
            detail: format!("table {fqn} has no Bifrost registration to resolve protection from"),
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

    /// Lists every epoch's validated protection frontier for one table.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the table is unregistered and
    /// [`ForgeError::Sql`] when a stored header, member, digest, or encoding
    /// version fails validation, which is contradictory evidence rather than an
    /// absence of protection.
    pub(super) async fn records(
        &mut self,
        tenant: DataTenantId,
        table_ref: &TableRef,
    ) -> Result<Vec<ProtectionRecord>, ForgeError> {
        let identity = self.identity(tenant, table_ref).await?;
        OracleTableProtections::new(self.conn)
            .list_table_protection(&identity)
            .await
            .map_err(ForgeError::Sql)
    }

    /// Projects every protected chain into the watermark the policy consumes.
    ///
    /// One watermark per frontier member, naming that chain's oldest active cut.
    /// Everything from that snapshot forward on the chain must survive, which is
    /// exactly what a watermark already means to the expiry policy, so no new
    /// protection vocabulary is introduced for reader cuts.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`ReaderProtection::records`].
    pub(super) async fn watermarks(
        &mut self,
        tenant: DataTenantId,
        table_ref: &TableRef,
    ) -> Result<Vec<SnapshotWatermark>, ForgeError> {
        let records = self.records(tenant, table_ref).await?;
        let mut watermarks = Vec::new();
        for record in records {
            for member in record.frontier.members {
                watermarks.push(SnapshotWatermark {
                    snapshot_id: member.protected_snapshot_id,
                    timestamp_ms: member.protected_snapshot_timestamp_ms,
                });
            }
        }
        watermarks.sort_by_key(|watermark| (watermark.timestamp_ms, watermark.snapshot_id));
        watermarks.dedup();
        Ok(watermarks)
    }

    /// Lists every snapshot an unresolved Forge expiration already claims.
    ///
    /// This reads the same admission index Oracle widening consults, through
    /// the same owner, so a claim has exactly one reader on each side of the
    /// boundary.
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

    /// Lists every snapshot id any epoch's proven ancestry still requires.
    ///
    /// Orphan collection needs the complete set rather than the chain endpoints
    /// because it protects objects reachable from each of those snapshots, not
    /// a cutoff.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`ReaderProtection::records`].
    pub(super) async fn protected_snapshot_ids(
        &mut self,
        tenant: DataTenantId,
        table_ref: &TableRef,
    ) -> Result<Vec<i64>, ForgeError> {
        let records = self.records(tenant, table_ref).await?;
        let mut ids = std::collections::BTreeSet::new();
        for record in records {
            for member in record.frontier.members {
                ids.extend(member.ancestry_path);
            }
        }
        Ok(ids.into_iter().collect())
    }
}
