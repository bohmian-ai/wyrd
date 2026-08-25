//! Tenant-scoped reads of the sealed `vala.file_list` manifest.

use std::collections::BTreeSet;
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

/// Cut-aware scan membership and complete sealed lineage for one table.
pub struct HotFileCut {
    /// Rows that remain unresolved and must be scanned as hot Parquet.
    pub hot_files: Vec<HotFileRow>,
    /// Every sealed row retained for live-tail watermark derivation.
    pub sealed_manifest: Vec<HotFileRow>,
    /// Whether a legacy prepared row lacked safe publication evidence.
    pub ambiguous_publication: bool,
}

/// Classifies one row against the pinned publication cut.
fn is_unresolved_hot(
    row: &HotFileRow,
    pinned_paths: &BTreeSet<String>,
    pinned_operation: Option<uuid::Uuid>,
) -> (bool, bool) {
    let represented_by_path = pinned_paths.contains(&row.file_path);
    let represented_by_operation = row.compacted
        && pinned_operation.is_some()
        && row.publication_operation_id == pinned_operation;
    let ambiguous = row.compacted
        && row.committed_snapshot_id.is_none()
        && row.publication_operation_id.is_none()
        && !represented_by_path;
    (
        row.committed_snapshot_id.is_none() && !represented_by_path && !represented_by_operation,
        ambiguous,
    )
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
    pub async fn unresolved_for_cut(
        &self,
        conn: &mut TenantConn<'_>,
        pinned_paths: &BTreeSet<String>,
        pinned_operation: Option<uuid::Uuid>,
    ) -> Result<HotFileCut, SqlError> {
        let rows = sqlx::query_as::<_, HotFileRow>(
            "SELECT id, data_tenant_id, namespace, table_name, file_path, file_ordinal, file_checksum, file_size, row_count, partition_granularity, partition_start, compacted, committed_snapshot_id, publication_operation_id, node_id, writer_epoch, wal_lsn_min, wal_lsn_max, created_at FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 ORDER BY partition_granularity, partition_start, created_at, file_ordinal, id",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(&self.namespace)
        .bind(&self.table_name)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let mut hot_files = Vec::new();
        let mut ambiguous_publication = false;
        for row in &rows {
            let (unresolved, ambiguous) = is_unresolved_hot(row, pinned_paths, pinned_operation);
            ambiguous_publication |= ambiguous;
            if unresolved {
                hot_files.push(row.clone());
            }
        }
        Ok(HotFileCut {
            hot_files,
            sealed_manifest: rows,
            ambiguous_publication,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one valid manifest row for cut-membership policy tests.
    fn row(compacted: bool, committed: Option<i64>, operation: Option<uuid::Uuid>) -> HotFileRow {
        HotFileRow {
            id: uuid::Uuid::now_v7(),
            data_tenant_id: uuid::Uuid::now_v7(),
            namespace: "vala.bifrost".to_owned(),
            table_name: "events".to_owned(),
            file_path: "events/a.parquet".to_owned(),
            file_ordinal: 0,
            file_checksum: None,
            file_size: 1,
            row_count: 1,
            partition_granularity: "hour".to_owned(),
            partition_start: chrono::DateTime::from_timestamp_micros(1_787_493_600_000_000)
                .expect("fixture partition start"),
            compacted,
            committed_snapshot_id: committed,
            publication_operation_id: operation,
            node_id: uuid::Uuid::now_v7(),
            writer_epoch: 1,
            wal_lsn_min: 1,
            wal_lsn_max: 2,
            created_at: chrono::Utc::now(),
        }
    }

    /// Covers hot, path, operation, committed, reset, unrelated, and ambiguous states.
    #[test]
    fn hot_cut_excludes_only_snapshot_represented_rows() {
        let pinned = uuid::Uuid::from_u128(7);
        let unrelated = uuid::Uuid::from_u128(8);
        let empty = BTreeSet::new();
        assert_eq!(
            is_unresolved_hot(&row(false, None, None), &empty, Some(pinned)),
            (true, false)
        );
        assert_eq!(
            is_unresolved_hot(&row(true, None, Some(unrelated)), &empty, Some(pinned)),
            (true, false)
        );
        assert_eq!(
            is_unresolved_hot(&row(true, None, Some(pinned)), &empty, Some(pinned)),
            (false, false)
        );
        assert_eq!(
            is_unresolved_hot(&row(true, Some(9), Some(pinned)), &empty, Some(pinned)),
            (false, false)
        );
        assert_eq!(
            is_unresolved_hot(&row(true, None, None), &empty, Some(pinned)),
            (true, true)
        );
        let mut path = BTreeSet::new();
        path.insert("events/a.parquet".to_owned());
        assert_eq!(
            is_unresolved_hot(&row(true, None, None), &path, Some(pinned)),
            (false, false)
        );
    }
}
