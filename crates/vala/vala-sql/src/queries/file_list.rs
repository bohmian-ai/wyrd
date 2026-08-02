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

// Additional Forge lifecycle reads share the same tenant-bound connection.

// raw-query grep allowlist: this tenant-scoped file-list query post-dates the sqlx offline cache; run `mise run sqlx:prepare` to promote it to a macro. It remains bound to `TenantConn` and `wyrd.current_tenant()` and introduces no tenant-boundary exception.

/// Lists file paths whose staging lifecycle is not terminal for one physical table.
///
/// The optional `path` narrows the same nonterminal predicate for focused
/// reference proofs; it never broadens tenant or table scope.
///
/// # Errors
///
/// Returns [`SqlError::Query`] when PostgreSQL cannot execute the tenant-scoped
/// read.
pub async fn list_nonterminal_file_paths(
    conn: &mut TenantConn<'_>,
    namespace: &str,
    table_name: &str,
    path: Option<&str>,
) -> Result<Vec<String>, SqlError> {
    sqlx::query_scalar(
        r#"
        SELECT file_path
          FROM vala.file_list
         WHERE data_tenant_id = wyrd.current_tenant()
           AND namespace = $1
           AND table_name = $2
           AND (NOT compacted OR committed_snapshot_id IS NULL)
           AND ($3::text IS NULL OR file_path = $3)
         ORDER BY file_path
        "#,
    )
    .bind(namespace)
    .bind(table_name)
    .bind(path)
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Lists exact path/size inputs awaiting Forge planning for one demanded table.
///
/// The table-scoped read is not roster discovery: callers already hold one
/// durable planning demand and use this result only to construct its exact plan.
///
/// # Errors
/// Returns SQL errors or an invariant violation for a negative persisted size.
pub async fn list_nonterminal_files(
    conn: &mut TenantConn<'_>,
    namespace: &str,
    table_name: &str,
) -> Result<Vec<(String, u64)>, SqlError> {
    let rows: Vec<(String, i64)> = sqlx::query_as("SELECT file_path,file_size FROM vala.file_list WHERE data_tenant_id=wyrd.current_tenant() AND namespace=$1 AND table_name=$2 AND (NOT compacted OR committed_snapshot_id IS NULL) ORDER BY file_path")
        .bind(namespace).bind(table_name).fetch_all(&mut **conn.transaction()).await.map_err(SqlError::from)?;
    rows.into_iter()
        .map(|(path, size)| {
            u64::try_from(size).map(|value| (path, value)).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "Forge staging file size is negative".to_owned(),
                }
            })
        })
        .collect()
}
