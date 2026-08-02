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

use crate::{OperatorPool, SqlError, TenantConn};

/// Assert the exact lease fence inside an already-open tenant transaction.
///
/// The check locks the lease row until the surrounding transaction commits or
/// rolls back. This makes the final ownership check part of the same atomic
/// operation as the tenant mutation and its audit event.
///
/// # Errors
/// Returns [`SqlError::InvariantViolation`] when the lease is no longer owned
/// by the supplied owner and fencing token, or [`SqlError`] for query errors.
pub async fn assert_fence(
    conn: &mut TenantConn<'_>,
    lease_key: &str,
    owner: Uuid,
    fencing_token: i64,
) -> Result<(), SqlError> {
    let owned: bool = sqlx::query_scalar("SELECT vala.assert_maintenance_lease_fence($1, $2, $3)")
        .bind(lease_key)
        .bind(owner)
        .bind(fencing_token)
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;

    if owned {
        Ok(())
    } else {
        Err(SqlError::InvariantViolation {
            detail: format!("maintenance lease fence lost for `{lease_key}`"),
        })
    }
}

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

/// Release a maintenance lease only when the caller still owns its fence.
///
/// A stale worker cannot delete a successor's lease because all three identity
/// columns are part of the conditional predicate. The boolean result is false
/// when the row was already reclaimed or the supplied fence is stale.
///
/// # Errors
/// Returns [`SqlError`] when the conditional delete cannot be executed.
pub async fn release_lease_fenced(
    op: &OperatorPool,
    lease_key: &str,
    owner: Uuid,
    fencing_token: i64,
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        r#"
        DELETE FROM vala.maintenance_leases
         WHERE lease_key = $1
           AND owner = $2
           AND fencing_token = $3
        "#,
    )
    .bind(lease_key)
    .bind(owner)
    .bind(fencing_token)
    .execute(op.pool())
    .await
    .map_err(SqlError::from)?;

    Ok(result.rows_affected() == 1)
}

/// Authoritative evidence returned by one successful lease acquisition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseAcquisition {
    /// Monotonic token that fences every durable operation by this owner.
    pub fencing_token: i64,
    /// Whether this acquisition replaced an expired row owned by another owner.
    pub takeover: bool,
}

/// Try to acquire a maintenance lease, returning authoritative acquisition evidence.
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
) -> Result<Option<LeaseAcquisition>, SqlError> {
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
    let acquisition = sqlx::query_as::<_, (i64, bool)>(
        r#"
        WITH clock AS MATERIALIZED (
          SELECT statement_timestamp() AS now
        ),
        prior AS MATERIALIZED (
          SELECT owner, expires_at
          FROM vala.maintenance_leases
          WHERE lease_key = $1
          FOR UPDATE
        ),
        decision AS MATERIALIZED (
          SELECT clock.now AS now,
                 prior.owner AS prior_owner,
                 prior.expires_at AS prior_expires_at
            FROM clock
            LEFT JOIN prior ON true
        ),
        acquired AS (
          INSERT INTO vala.maintenance_leases
              (lease_key, owner, fencing_token, expires_at, heartbeat_at)
          SELECT $1, $2, nextval('vala.maintenance_fencing_seq'),
                 decision.now + ($3 * interval '1 second'),
                 decision.now
            FROM decision
          ON CONFLICT (lease_key) DO UPDATE
            SET owner = EXCLUDED.owner,
                fencing_token = nextval('vala.maintenance_fencing_seq'),
                expires_at = EXCLUDED.expires_at,
                heartbeat_at = EXCLUDED.heartbeat_at
          WHERE vala.maintenance_leases.expires_at < EXCLUDED.heartbeat_at
             OR vala.maintenance_leases.owner = EXCLUDED.owner
          RETURNING fencing_token
        )
        SELECT acquired.fencing_token,
               COALESCE(
                 decision.prior_owner IS NOT NULL
                 AND decision.prior_owner <> $2
                 AND decision.prior_expires_at < decision.now,
                 false
               ) AS takeover
        FROM acquired
        CROSS JOIN decision
        "#,
    )
    .bind(lease_key)
    .bind(owner)
    .bind(lease_secs)
    .fetch_optional(op.pool())
    .await
    .map_err(SqlError::from)?;

    Ok(
        acquisition.map(|(fencing_token, takeover)| LeaseAcquisition {
            fencing_token,
            takeover,
        }),
    )
}
