//! Export the sealed audit history for a tenant.
//!
//! Lists all seal checkpoints together with their ranges for archival and
//! operator inspection. The export does not include the raw audit row data —
//! the checkpoint `(seq_lo, seq_hi, range_hash, signature)` is the sealed
//! evidence; the raw rows live in the Iceberg `audit_log`.

use sqlx::PgPool;
use vala_sql::TenantConn;
use vala_sql::queries::audit_seal::{AuditSealCheckpoint, list_checkpoints};
use wyrd_spec::ids::DataTenantId;

use crate::error::BifrostError;

/// Fetch all seal checkpoints for `tenant_id`, ordered by `seq_lo`.
///
/// # Errors
/// Returns [`BifrostError`] when the SQL query fails.
pub async fn export_tenant_sealed_history(
    app_pool: &PgPool,
    tenant_id: DataTenantId,
) -> Result<Vec<AuditSealCheckpoint>, BifrostError> {
    let mut conn = TenantConn::acquire(app_pool, tenant_id)
        .await
        .map_err(BifrostError::Sql)?;
    let checkpoints = list_checkpoints(&mut conn)
        .await
        .map_err(BifrostError::Sql)?;
    conn.commit().await.map_err(BifrostError::Sql)?;
    Ok(checkpoints)
}
