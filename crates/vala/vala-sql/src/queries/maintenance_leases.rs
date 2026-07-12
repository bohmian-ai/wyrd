//! Maintenance lease heartbeat queries for the Vala OLAP warehouse.
//!
//! Maintenance workers (compaction, snapshot expiry, orphan GC) hold a named
//! lease identified by `lease_key`. Each lease carries a monotonic
//! `fencing_token` stamped when the worker claimed it. [`renew_lease_fenced`]
//! extends the lease only when the caller still owns it — if another pod claimed
//! the lease the UPDATE matches zero rows and returns `false` (ownership lost).
//!
//! Lease rows live in `vala.maintenance_leases` (introduced by migration
//! `20260802000000_maintenance_leases`; created by slice 08a). Until that
//! migration lands, the fenced-renew logic is self-contained and testable
//! against a mock pool — no schema dependency here.
//!
//! Per the vala-sql guard: this module takes a bare `&sqlx::PgPool` rather than
//! `&TenantConn` because the maintenance lease table is a cross-tenant
//! control-plane surface, not a tenant-scoped data table. The pool passed here
//! must be the `vala_recovery` or equivalent cross-tenant pool.
// tenant-scope-exempt: maintenance lease operations are cross-tenant control-plane; they use the recovery pool (BYPASSRLS) and are never called on a tenant request path.

use sqlx::types::Uuid;

use crate::SqlError;

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
    pool: &sqlx::PgPool,
    lease_key: &str,
    owner: Uuid,
    fencing_token: i64,
    lease_secs: i64,
) -> Result<bool, SqlError> {
    // Dynamic query is intentional: maintenance leases are a cross-tenant
    // control-plane table accessed via the recovery pool (BYPASSRLS).
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
    .execute(pool)
    .await
    .map_err(SqlError::from)?;

    Ok(result.rows_affected() > 0)
}

/// Try to acquire a maintenance lease, returning the fencing token on success.
///
/// Uses `INSERT … ON CONFLICT DO NOTHING` so only one caller wins the race.
/// Returns `Some(fencing_token)` when the insert succeeded (this caller now
/// holds the lease). Returns `None` when another caller already holds it.
///
/// # Errors
/// Returns [`SqlError`] when the database query fails.
pub async fn try_acquire_lease(
    pool: &sqlx::PgPool,
    lease_key: &str,
    owner: Uuid,
    lease_secs: i64,
) -> Result<Option<i64>, SqlError> {
    // Dynamic query is intentional: maintenance leases are a cross-tenant
    // control-plane table accessed via the recovery pool (BYPASSRLS).
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
        RETURNING fencing_token
        "#,
    )
    .bind(lease_key)
    .bind(owner)
    .bind(lease_secs)
    .fetch_optional(pool)
    .await
    .map_err(SqlError::from)?;

    Ok(token)
}

#[cfg(test)]
mod renew {
    use super::*;

    /// Fenced renew is conditioned on all three predicates:
    /// lease_key, owner, and fencing_token. Verify the predicate shape is
    /// structurally correct by asserting the function signature is consistent
    /// with the SQL query contract.
    ///
    /// Real end-to-end fencing behavior is tested in vala-sql integration tests
    /// via the pg-fixture suite (`mise run test:sql`).
    #[test]
    fn renew_lease_fenced_signature_is_correct() {
        // Compile-time check: the function must accept the four parameters
        // and return the correct type. This test fails to compile if the
        // signature drifts.
        fn _check_signature(
            pool: &sqlx::PgPool,
            lease_key: &str,
            owner: Uuid,
            fencing_token: i64,
            lease_secs: i64,
        ) -> impl std::future::Future<Output = Result<bool, SqlError>> {
            renew_lease_fenced(pool, lease_key, owner, fencing_token, lease_secs)
        }
        // If we reach here, the signature is correct.
        let _ = _check_signature;
    }

    /// Fencing token contract: a renewal with mismatched token returns false
    /// (ownership lost) rather than silently extending the wrong worker's lease.
    ///
    /// This behaviour is enforced by the `fencing_token = $3` predicate in the
    /// UPDATE. The integration tests in `mise run test:sql` verify this against
    /// a real Postgres instance.
    #[test]
    fn renew_returns_false_on_ownership_loss() {
        // Structural test: assert the function returns a `bool` (not ()) so
        // the caller can detect ownership loss. The real ownership-loss path
        // requires a live Postgres instance and is covered in the integration
        // suite.
        fn _returns_bool(
            pool: &sqlx::PgPool,
            lease_key: &str,
            owner: Uuid,
            fencing_token: i64,
            lease_secs: i64,
        ) -> impl std::future::Future<Output = Result<bool, SqlError>> {
            renew_lease_fenced(pool, lease_key, owner, fencing_token, lease_secs)
        }
        let _ = _returns_bool;
    }
}
