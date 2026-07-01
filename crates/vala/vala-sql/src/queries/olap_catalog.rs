//! Tenant-scoped reads + writes for the Vala OLAP catalog control tables:
//! `vala.bifrost_tables`, `vala.olap_commits`, `vala.refresh_epochs`,
//! and `vala.olap_recovery_events`.
//! All callers must pass a [`TenantConn`] — the wyrd-sql RLS bind enforces
//! tenant scope for wyrd_app-role paths; recovery paths use SECURITY DEFINER
//! routines owned by vala_recovery_owner (BYPASSRLS).
// raw-query grep allowlist: olap control tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::Uuid;
use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::olap_catalog::{BifrostTableRow, ClaimedPrecommitRow, OlapCommitRow};

// ── vala.bifrost_tables ──────────────────────────────────────────────────────

/// Insert or update a Bifrost table registration for the current tenant.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or an RLS policy rejects the row.
pub async fn upsert_table(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    fqn: &str,
    fingerprint: &[u8; 32],
    scope: &str,
    partition_columns: &[String],
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.bifrost_tables
            (data_tenant_id, table_uid, fqn, fingerprint, scope, partition_columns, origin, actor)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, 'system', 'system')
        ON CONFLICT (data_tenant_id, table_uid)
        DO UPDATE SET
            fqn               = EXCLUDED.fqn,
            fingerprint       = EXCLUDED.fingerprint,
            scope             = EXCLUDED.scope,
            partition_columns = EXCLUDED.partition_columns,
            updated_at        = now()
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(fqn)
    .bind(fingerprint.as_slice())
    .bind(scope)
    .bind(partition_columns)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Look up a Bifrost table registration by FQN for the current tenant.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn get_by_fqn(
    conn: &mut TenantConn<'_>,
    fqn: &str,
) -> Result<Option<BifrostTableRow>, SqlError> {
    sqlx::query_as::<_, BifrostTableRow>(
        r#"
        SELECT data_tenant_id, table_uid, fqn, fingerprint, scope, status,
               partition_columns, registered_at, updated_at, origin, actor
          FROM vala.bifrost_tables
         WHERE fqn = $1
        "#,
    )
    .bind(fqn)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// List all `vala.bifrost_tables` rows visible to the current tenant bind.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn list_tables_for_tenant(
    conn: &mut TenantConn<'_>,
) -> Result<Vec<BifrostTableRow>, SqlError> {
    sqlx::query_as::<_, BifrostTableRow>(
        r#"
        SELECT data_tenant_id, table_uid, fqn, fingerprint, scope, status,
               partition_columns, registered_at, updated_at, origin, actor
          FROM vala.bifrost_tables
        "#,
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Delete a Bifrost table registration and all dependent rows for the current
/// tenant, in FK order. `vala.refresh_epochs` and `vala.olap_commits` reference
/// `vala.bifrost_tables` with no `ON DELETE CASCADE`, so children are removed
/// first. Intended for test/bench teardown.
///
/// # Errors
/// Returns [`SqlError`] when any delete fails.
pub async fn delete_table(conn: &mut TenantConn<'_>, fqn: &str) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        DELETE FROM vala.refresh_epochs
         WHERE table_uid IN (SELECT table_uid FROM vala.bifrost_tables WHERE fqn = $1)
        "#,
    )
    .bind(fqn)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    sqlx::query(
        r#"
        DELETE FROM vala.olap_commits
         WHERE table_uid IN (SELECT table_uid FROM vala.bifrost_tables WHERE fqn = $1)
        "#,
    )
    .bind(fqn)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    sqlx::query("DELETE FROM vala.bifrost_tables WHERE fqn = $1")
        .bind(fqn)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;

    Ok(())
}

// ── vala.olap_commits ────────────────────────────────────────────────────────

/// Record a precommit anchor for the given (table, batch) pair.
///
/// Idempotent: a duplicate `batch_id` is a no-op, not a unique-violation.
/// The engine calls [`lookup_idempotent`] BEFORE writing any Parquet to
/// dispatch on prior state.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or an RLS policy rejects the row.
pub async fn precommit(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    origin: &str,
    actor: &str,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.olap_commits
            (data_tenant_id, table_uid, batch_id, state, origin, actor)
        VALUES (wyrd.current_tenant(), $1, $2, 'precommit', $3, $4)
        ON CONFLICT (data_tenant_id, table_uid, batch_id) DO NOTHING
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(origin)
    .bind(actor)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Transition the anchor to `committed` and record the discovered snapshot id.
///
/// Guards on `writer_owner` and `writer_fencing_token` ensure only the owning
/// writer (not a recovery worker that has claimed the row) can commit. Returns
/// `true` when the update applied; `false` when the fence was lost or the row
/// was already finalized.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn finalize_committed(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    snapshot_id: i64,
    writer_owner: Uuid,
    writer_fencing_token: i64,
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE vala.olap_commits
           SET state = 'committed', committed_at = now(), snapshot_id = $3,
               writer_lease_expires_at = NULL, writer_heartbeat_at = NULL
         WHERE table_uid = $1 AND batch_id = $2 AND state = 'precommit'
           AND writer_owner = $4 AND writer_fencing_token = $5
           AND recovery_fencing_token IS NULL
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(snapshot_id)
    .bind(writer_owner)
    .bind(writer_fencing_token)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected() > 0)
}

/// Transition the anchor to `failed` and record the error.
///
/// Guards on `writer_owner` and `writer_fencing_token` ensure only the owning
/// writer (not a recovery worker) can finalize to failed. Returns `true` when
/// the update applied.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn finalize_failed(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    error_code: &str,
    error_detail: &str,
    writer_owner: Uuid,
    writer_fencing_token: i64,
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE vala.olap_commits
           SET state = 'failed', finalized_at = now(),
               error_code = $3, error_detail = $4,
               writer_lease_expires_at = NULL
         WHERE table_uid = $1 AND batch_id = $2 AND state = 'precommit'
           AND writer_owner = $5 AND writer_fencing_token = $6
           AND recovery_fencing_token IS NULL
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(error_code)
    .bind(error_detail)
    .bind(writer_owner)
    .bind(writer_fencing_token)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected() > 0)
}

/// Transition the anchor to `aborted` for the current tenant.
///
/// Used by the writer to voluntarily abort a precommit before any data has
/// been written to Iceberg.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn finalize_aborted(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        UPDATE vala.olap_commits
           SET state = 'aborted'
         WHERE table_uid = $1 AND batch_id = $2 AND state = 'precommit'
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Return the existing anchor row for (table, batch), if any.
///
/// The engine calls this before writing Parquet to dispatch on prior state:
/// `committed` → idempotent replay; `failed` → 409 DuplicateFailedBatch;
/// `precommit` → in-flight duplicate; `None` → fresh write.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn lookup_idempotent(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
) -> Result<Option<OlapCommitRow>, SqlError> {
    sqlx::query_as::<_, OlapCommitRow>(
        r#"
        SELECT data_tenant_id, table_uid, batch_id, snapshot_id,
               state, precommit_at, committed_at, finalized_at,
               error_code, error_detail, origin, actor
          FROM vala.olap_commits
         WHERE table_uid = $1 AND batch_id = $2
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Allocate a monotonically increasing writer fencing token from the shared sequence.
///
/// The sequence `vala.writer_fencing_seq` is `GRANT USAGE` to `wyrd_app`, so this
/// is callable from a normal `TenantConn` (no `SECURITY DEFINER` required).
///
/// # Errors
/// Returns [`SqlError`] when the sequence read fails.
pub async fn mint_writer_fencing_token(conn: &mut TenantConn<'_>) -> Result<i64, SqlError> {
    let token: i64 = sqlx::query_scalar("SELECT nextval('vala.writer_fencing_seq')")
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
    Ok(token)
}

// ── writer lease ─────────────────────────────────────────────────────────────

/// Stamp writer-lease columns on an existing precommit row.
///
/// Called immediately after [`precommit`] within the same transaction to record
/// the writer's identity and initial lease expiry.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn record_writer_lease(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    owner: Uuid,
    token: i64,
    lease_secs: i64,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        UPDATE vala.olap_commits
           SET writer_owner            = $3,
               writer_fencing_token    = $4,
               writer_lease_expires_at = now() + ($5 * interval '1 second'),
               writer_heartbeat_at     = now()
         WHERE table_uid = $1 AND batch_id = $2 AND state = 'precommit'
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(owner)
    .bind(token)
    .bind(lease_secs)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Refresh the heartbeat and extend the writer lease.
///
/// Returns `true` when the update applied (owner and token matched and the row
/// is still in `precommit`).
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn heartbeat_writer_lease(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    owner: Uuid,
    token: i64,
    lease_secs: i64,
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE vala.olap_commits
           SET writer_heartbeat_at     = now(),
               writer_lease_expires_at = now() + ($5 * interval '1 second')
         WHERE table_uid = $1 AND batch_id = $2
           AND state = 'precommit'
           AND writer_owner = $3
           AND writer_fencing_token = $4
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(owner)
    .bind(token)
    .bind(lease_secs)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected() > 0)
}

/// Extend the writer lease only when recovery has not yet claimed the row.
///
/// Returns `true` when the fence is still held (update applied); `false` when
/// `recovery_fencing_token` is set (fence lost to recovery) or the row is gone.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn renew_writer_fence(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    owner: Uuid,
    token: i64,
    lease_secs: i64,
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE vala.olap_commits
           SET writer_heartbeat_at     = now(),
               writer_lease_expires_at = now() + ($5 * interval '1 second')
         WHERE table_uid = $1 AND batch_id = $2
           AND state = 'precommit'
           AND writer_owner = $3
           AND writer_fencing_token = $4
           AND recovery_fencing_token IS NULL
           AND writer_lease_expires_at > now()
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(owner)
    .bind(token)
    .bind(lease_secs)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected() > 0)
}

// ── vala.olap_recovery_events ────────────────────────────────────────────────

/// Insert a `fence_lost_after_append` audit event via the wyrd_app role.
///
/// Called by the writer after it discovers it lost its fence post-append.
/// The INSERT reads `data_tenant_id` and `state` from `olap_commits` (both
/// RLS-filtered to the current tenant) so the audit row inherits the correct
/// tenant key.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
#[allow(clippy::too_many_arguments)]
pub async fn record_fence_loss_after_append(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    owner: Uuid,
    token: i64,
    snapshot_id: i64,
    rolled_back: bool,
    error: Option<&str>,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.olap_recovery_events
            (data_tenant_id, table_uid, batch_id, event_kind, old_state, new_state,
             snapshot_id, recovery_owner, fencing_token, rolled_back, error, recorded_at)
        SELECT data_tenant_id, $1, $2,
               'fence_lost_after_append', state, state,
               $5, $3, $4, $6, $7, now()
          FROM vala.olap_commits
         WHERE table_uid = $1 AND batch_id = $2
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(owner)
    .bind(token)
    .bind(snapshot_id)
    .bind(rolled_back)
    .bind(error)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

// ── SECURITY DEFINER recovery routine wrappers ───────────────────────────────
// These call vala_recovery_owner-owned functions that bypass RLS. The conn
// provides the underlying connection; the SECURITY DEFINER functions execute
// under the definer's privileges regardless of the session role.

/// Claim up to `limit` stale precommit rows for recovery.
///
/// Rows are eligible when their writer lease has expired (or was never set) and
/// no other recovery worker has claimed them. Each claimed row receives a unique
/// `recovery_fencing_token` stamped atomically.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn claim_stale_precommits(
    conn: &mut TenantConn<'_>,
    owner: Uuid,
    limit: i32,
) -> Result<Vec<ClaimedPrecommitRow>, SqlError> {
    sqlx::query_as::<_, ClaimedPrecommitRow>("SELECT * FROM vala.claim_stale_precommits($1, $2)")
        .bind(owner)
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Finalize a claimed precommit as `committed` via the recovery path.
///
/// Only applies when `recovery_fencing_token` matches `token`, preventing
/// double-finalization from concurrent recovery workers.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn finalize_recovered_committed(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    snapshot_id: i64,
    token: i64,
) -> Result<(), SqlError> {
    sqlx::query("SELECT vala.finalize_recovered_committed($1, $2, $3, $4)")
        .bind(table_uid.as_slice())
        .bind(batch_id.as_slice())
        .bind(snapshot_id)
        .bind(token)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// Finalize a claimed precommit as `aborted` via the recovery path.
///
/// Only applies when `recovery_fencing_token` matches `token`.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn finalize_recovered_aborted(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    token: i64,
    reason: &str,
) -> Result<(), SqlError> {
    sqlx::query("SELECT vala.finalize_recovered_aborted($1, $2, $3, $4)")
        .bind(table_uid.as_slice())
        .bind(batch_id.as_slice())
        .bind(token)
        .bind(reason)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// Record a failed recovery scan attempt without changing the FSM state.
///
/// Increments `recovery_attempts` and persists the error for observability.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn mark_recovery_scan_failed(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    token: i64,
    error: &str,
) -> Result<(), SqlError> {
    sqlx::query("SELECT vala.mark_recovery_scan_failed($1, $2, $3, $4)")
        .bind(table_uid.as_slice())
        .bind(batch_id.as_slice())
        .bind(token)
        .bind(error)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

// ── vala.refresh_epochs ──────────────────────────────────────────────────────

/// Increment the refresh epoch for a table and return the new value.
///
/// Inserts with epoch=1 on first call; subsequent calls increment by 1.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn bump_epoch(conn: &mut TenantConn<'_>, table_uid: &[u8; 16]) -> Result<i64, SqlError> {
    let epoch: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO vala.refresh_epochs (data_tenant_id, table_uid, epoch)
        VALUES (wyrd.current_tenant(), $1, 1)
        ON CONFLICT (data_tenant_id, table_uid)
        DO UPDATE SET epoch = vala.refresh_epochs.epoch + 1,
                      bumped_at = now()
        RETURNING epoch
        "#,
    )
    .bind(table_uid.as_slice())
    .fetch_one(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(epoch)
}

/// Return the current refresh epoch for a table, or 0 if not yet written.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn current_epoch(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
) -> Result<i64, SqlError> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT epoch FROM vala.refresh_epochs WHERE table_uid = $1")
            .bind(table_uid.as_slice())
            .fetch_optional(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
    Ok(row.map(|(e,)| e).unwrap_or(0))
}
