//! Cross-tenant relay operations for the Vala audit outbox.
//!
//! Functions here enumerate or operate across ALL tenant partitions using the
//! platform admin pool (BYPASSRLS). This is intentional — the audit relay
//! discovers pending work across every tenant before binding to a single tenant
//! for shipping. Isolation is enforced at the database-role boundary (the admin
//! pool uses `wyrd_platform_admin`, which has BYPASSRLS and is never exposed to
//! tenant request paths).
// raw-query grep allowlist: relay queries are cross-tenant BYPASSRLS; run `mise run sqlx:prepare` to promote to macros.

use crate::OperatorPool;
use crate::SqlError;

/// List all distinct `data_tenant_id` values that have at least one shipped
/// outbox row.
///
/// Uses the operator pool (BYPASSRLS `wyrd_platform_admin`) to enumerate across
/// all tenants — called by the reconciler worker to discover which tenants need
/// checking. Never called on the tenant request path.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn list_audit_tenant_ids(op: &OperatorPool) -> Result<Vec<uuid::Uuid>, SqlError> {
    sqlx::query_as::<_, (uuid::Uuid,)>(
        r#"
        SELECT DISTINCT data_tenant_id
          FROM vala.audit_outbox
         WHERE shipped = true
         ORDER BY data_tenant_id
        "#,
    )
    .fetch_all(op.pool())
    .await
    .map_err(SqlError::from)
    .map(|rows| rows.into_iter().map(|(id,)| id).collect())
}
