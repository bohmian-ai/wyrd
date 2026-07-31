//! Tenant-scoped reads of the sealed `vala.file_list` manifest.

use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::file_list::HotFileRow;

/// Owner for ordered hot-file manifest reads.
pub struct HotFileCatalog {
    /// Logical namespace whose sealed files this reader projects.
    namespace: String,
    /// Logical table name whose sealed files this reader projects.
    table_name: String,
}

impl HotFileCatalog {
    /// Creates a manifest reader for one logical table identity.
    #[must_use]
    pub fn new(namespace: &str, table_name: &str) -> Self {
        Self {
            namespace: namespace.to_owned(),
            table_name: table_name.to_owned(),
        }
    }

    /// Reads tenant-scoped sealed files for exact pinned-snapshot subtraction.
    ///
    /// Compacted transition rows remain visible because their committed
    /// snapshot may be newer than Oracle's independently pinned snapshot.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the RLS-bound transaction or manifest query fails.
    pub async fn active_files(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<Vec<HotFileRow>, SqlError> {
        let rows = sqlx::query_as::<_, HotFileRow>(
            "SELECT id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, partition_day, compacted, committed_snapshot_id, node_id, writer_epoch, wal_lsn_min, wal_lsn_max, created_at FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 ORDER BY partition_day, created_at, id",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(&self.namespace)
        .bind(&self.table_name)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        Ok(rows)
    }
}
