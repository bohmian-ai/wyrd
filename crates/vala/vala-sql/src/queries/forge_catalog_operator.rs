//! Operator-only active-table roster for cross-tenant Forge maintenance.
//!
//! The roster intentionally reads every active Bifrost registration through
//! [`OperatorPool`], which is backed by the `wyrd_platform_admin` BYPASSRLS
//! role. It remains separate from tenant-scoped OLAP catalog queries so the
//! query-module boundary makes that elevated access explicit.
// raw-query grep allowlist: the cross-tenant Forge roster is intentionally operator-owned.

use crate::row_types::olap_catalog::BifrostTableRow;
use crate::{OperatorPool, SqlError};

/// Lists active Bifrost registrations for the cross-tenant Forge scheduler.
///
/// This operator-owned roster is deliberately independent of staging-file
/// history, so an active Iceberg table remains eligible for maintenance after
/// its `file_list` rows become terminal.
///
/// # Errors
///
/// Returns [`SqlError`] when the operator-scoped catalog read fails.
pub async fn list_active_tables_for_operator(
    op: &OperatorPool,
) -> Result<Vec<BifrostTableRow>, SqlError> {
    sqlx::query_as::<_, BifrostTableRow>(
        r#"
        SELECT data_tenant_id, table_uid, fqn, fingerprint, status,
               partition_columns, registered_at, updated_at, origin, actor
          FROM vala.bifrost_tables
         WHERE status = 'active'
         ORDER BY data_tenant_id, fqn
        "#,
    )
    .fetch_all(op.pool())
    .await
    .map_err(SqlError::from)
}
