//! Tenant-scoped reads of the sealed `vala.file_list` manifest.

use wyrd_spec::DataTenantId;
use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::file_list::HotFileRow;

/// Owner for ordered hot-file manifest reads.
#[derive(Clone)]
pub struct HotFileCatalog {
    /// Vala SQL root used to open an authenticated tenant connection per read.
    postgres: crate::ValaPostgres,
}

impl HotFileCatalog {
    /// Creates a manifest reader over one Vala Postgres handle.
    #[must_use]
    pub fn new(postgres: crate::ValaPostgres) -> Self {
        Self { postgres }
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
        tenant: DataTenantId,
        namespace: &str,
        table_name: &str,
    ) -> Result<Vec<HotFileRow>, SqlError> {
        let mut conn = TenantConn::acquire(self.postgres.pool(), tenant).await?;
        let rows = sqlx::query_as::<_, HotFileRow>(
            "SELECT id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, partition_day, compacted, committed_snapshot_id, node_id, writer_epoch, wal_lsn_min, wal_lsn_max, created_at FROM vala.file_list WHERE namespace = $1 AND table_name = $2 ORDER BY partition_day, created_at, id",
        )
        .bind(namespace)
        .bind(table_name)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        conn.commit().await?;
        Ok(rows)
    }
}
