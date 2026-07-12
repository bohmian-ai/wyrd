//! Audit-seal checkpoint persistence: `vala.audit_seal_checkpoints`.
//!
//! Records the Ed25519 checkpoint signature that covers a sealed `[seq_lo,
//! seq_hi]` range for one tenant. The verifier reads these rows and
//! recomputes the range hash from the Iceberg `audit_log` table to confirm
//! the signature still holds.
// raw-query grep allowlist: audit_seal_checkpoints table post-dates the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use wyrd_spec::DataTenantId;

use crate::SqlError;
use crate::TenantConn;

/// One sealed range checkpoint row.
#[derive(Debug, Clone)]
pub struct AuditSealCheckpoint {
    /// Tenant this checkpoint covers.
    pub data_tenant_id: DataTenantId,
    /// First (inclusive) `seq` in the sealed range.
    pub seq_lo: i64,
    /// Last (inclusive) `seq` in the sealed range.
    pub seq_hi: i64,
    /// SHA256 hash of the ordered audit log column bytes for `[seq_lo, seq_hi]`.
    pub range_hash: Vec<u8>,
    /// Ed25519 signature over `AUDIT_SEAL_DOMAIN || range_hash`.
    pub signature: Vec<u8>,
}

/// Insert (or idempotently upsert) one audit-seal checkpoint.
///
/// The upsert is idempotent: if a checkpoint for the same `(tenant, seq_lo,
/// seq_hi)` already exists (e.g. a worker restarts mid-seal), the existing
/// row is left unchanged (`ON CONFLICT DO NOTHING`).
///
/// # Errors
/// Returns [`SqlError`] when the insert fails.
pub async fn upsert_checkpoint(
    conn: &mut TenantConn<'_>,
    seq_lo: i64,
    seq_hi: i64,
    range_hash: &[u8],
    signature: &[u8],
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.audit_seal_checkpoints
            (data_tenant_id, seq_lo, seq_hi, range_hash, signature)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4)
        ON CONFLICT (data_tenant_id, seq_lo, seq_hi) DO NOTHING
        "#,
    )
    .bind(seq_lo)
    .bind(seq_hi)
    .bind(range_hash)
    .bind(signature)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Load all seal checkpoints for the current tenant, ordered by `seq_lo`.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn list_checkpoints(
    conn: &mut TenantConn<'_>,
) -> Result<Vec<AuditSealCheckpoint>, SqlError> {
    let rows: Vec<(i64, i64, Vec<u8>, Vec<u8>)> = sqlx::query_as(
        r#"
        SELECT seq_lo, seq_hi, range_hash, signature
          FROM vala.audit_seal_checkpoints
         WHERE data_tenant_id = wyrd.current_tenant()
         ORDER BY seq_lo
        "#,
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    let tenant = conn.data_tenant_id();
    Ok(rows
        .into_iter()
        .map(
            |(seq_lo, seq_hi, range_hash, signature)| AuditSealCheckpoint {
                data_tenant_id: tenant,
                seq_lo,
                seq_hi,
                range_hash,
                signature,
            },
        )
        .collect())
}
