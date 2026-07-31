//! Tenant-scoped reads over Bifrost staging-file state.

use crate::{SqlError, TenantConn};

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
