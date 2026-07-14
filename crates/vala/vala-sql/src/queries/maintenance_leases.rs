//! Maintenance lease heartbeat queries for the Vala OLAP warehouse.
//!
//! Maintenance workers (compaction, snapshot expiry, orphan GC) hold a named
//! lease identified by `lease_key`. Each lease carries a monotonic
//! `fencing_token` stamped when the worker claimed it. [`renew_lease_fenced`]
//! extends the lease only when the caller still owns it — if another pod claimed
//! the lease the UPDATE matches zero rows and returns `false` (ownership lost).
//!
//! Lease rows live in `vala.maintenance_leases` (created by migration
//! `20260906000001_maintenance_leases`). The table is a cross-tenant
//! control-plane surface with no tenant column and no RLS, so these functions
//! take `&OperatorPool` (the `wyrd_platform_admin` BYPASSRLS pool) rather than a
//! tenant-scoped `TenantConn`.

use sqlx::types::Uuid;

use crate::{OperatorPool, SqlError};

/// Extend a named maintenance lease by `lease_secs` seconds, fenced by
/// `(owner, fencing_token)`.
///
/// The UPDATE is conditioned on:
/// - `lease_key = $1` — must be the exact lease this worker claimed.
/// - `owner = $2` — stable per-process identity.
/// - `fencing_token = $3` — monotonic token stamped at claim time.
/// - `expires_at > now()` — only a live lease can be renewed (an expired lease
///   may have been reclaimed by another worker).
///
/// Returns `true` when the renewal applied (ownership confirmed).
/// Returns `false` when the row was not found or the conditions did not match
/// (ownership lost — caller must stop its in-flight operation).
///
/// # Errors
/// Returns [`SqlError`] when the database query fails. A query error is
/// **distinct** from ownership loss: the caller should treat it as a transient
/// fault and retry.
pub async fn renew_lease_fenced(
    op: &OperatorPool,
    lease_key: &str,
    owner: Uuid,
    fencing_token: i64,
    lease_secs: i64,
) -> Result<bool, SqlError> {
    // Dynamic query is intentional: maintenance leases are a cross-tenant
    // control-plane table accessed via the operator pool (BYPASSRLS).
    let result = sqlx::query(
        r#"
        UPDATE vala.maintenance_leases
           SET expires_at     = now() + ($4 * interval '1 second'),
               heartbeat_at   = now()
         WHERE lease_key      = $1
           AND owner          = $2
           AND fencing_token  = $3
           AND expires_at     > now()
        "#,
    )
    .bind(lease_key)
    .bind(owner)
    .bind(fencing_token)
    .bind(lease_secs)
    .execute(op.pool())
    .await
    .map_err(SqlError::from)?;

    Ok(result.rows_affected() > 0)
}

/// Try to acquire a maintenance lease, returning the fencing token on success.
///
/// Uses `INSERT … ON CONFLICT` so only one caller wins the race: a brand-new
/// lease key inserts, and an existing but expired lease is taken over. Returns
/// `Some(fencing_token)` when this caller now holds the lease, `None` when
/// another caller still holds a live lease.
///
/// # Errors
/// Returns [`SqlError`] when the database query fails.
pub async fn try_acquire_lease(
    op: &OperatorPool,
    lease_key: &str,
    owner: Uuid,
    lease_secs: i64,
) -> Result<Option<i64>, SqlError> {
    // Dynamic query is intentional: maintenance leases are a cross-tenant
    // control-plane table accessed via the operator pool (BYPASSRLS).
    //
    // The WHERE clause also allows re-acquisition by the SAME owner (equal
    // `owner`), which is how back-to-back ticks from one process (NOTIFY →
    // fallback → NOTIFY, …) keep re-taking their own lease without the second
    // acquire silently no-op'ing. Same-owner re-acquire bumps the fencing token,
    // so any lingering copy of the old token can no longer renew (fencing check
    // in `renew_lease` requires (owner, fencing_token) match). A different
    // owner is still blocked until the current owner's lease expires.
    let token: Option<i64> = sqlx::query_scalar(
        r#"
        INSERT INTO vala.maintenance_leases
            (lease_key, owner, fencing_token, expires_at, heartbeat_at)
        VALUES ($1, $2, nextval('vala.maintenance_fencing_seq'),
                now() + ($3 * interval '1 second'), now())
        ON CONFLICT (lease_key) DO
            UPDATE SET
                owner         = EXCLUDED.owner,
                fencing_token = nextval('vala.maintenance_fencing_seq'),
                expires_at    = now() + ($3 * interval '1 second'),
                heartbeat_at  = now()
            WHERE vala.maintenance_leases.expires_at < now()
               OR vala.maintenance_leases.owner    = EXCLUDED.owner
        RETURNING fencing_token
        "#,
    )
    .bind(lease_key)
    .bind(owner)
    .bind(lease_secs)
    .fetch_optional(op.pool())
    .await
    .map_err(SqlError::from)?;

    Ok(token)
}
