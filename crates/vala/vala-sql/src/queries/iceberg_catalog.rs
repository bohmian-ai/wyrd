//! Tenant-free diagnostic helpers over `iceberg_catalog.iceberg_tables`.
//!
//! This table holds every tenant's `metadata_location` and is a cross-tenant
//! oracle. The whole module is gated behind the `diagnostics` feature (ops +
//! tests only) and must not be reachable from any request path, HTTP handler,
//! MCP tool, or Python export.
#![cfg(feature = "diagnostics")]

use sqlx::PgPool;

use crate::SqlError;

/// Count rows in `iceberg_catalog.iceberg_tables`.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn iceberg_table_count(pool: &PgPool) -> Result<i64, SqlError> {
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM iceberg_catalog.iceberg_tables")
        .fetch_one(pool)
        .await
        .map_err(SqlError::from)?;
    Ok(n)
}

/// Look up `metadata_location` for a specific iceberg table entry.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn iceberg_metadata_location(
    pool: &PgPool,
    catalog: &str,
    namespace: &str,
    name: &str,
) -> Result<Option<String>, SqlError> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT metadata_location
           FROM iceberg_catalog.iceberg_tables
          WHERE catalog_name = $1 AND table_namespace = $2 AND table_name = $3",
    )
    .bind(catalog)
    .bind(namespace)
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(SqlError::from)?;
    Ok(row.and_then(|(loc,)| loc))
}
