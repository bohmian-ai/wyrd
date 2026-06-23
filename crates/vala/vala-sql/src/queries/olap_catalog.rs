//! Tenant-scoped reads + writes for the Vala OLAP catalog control tables:
//! `vala.bifrost_tables`, `vala.olap_commits`, `vala.refresh_epochs`.
//! All callers must pass a [`TenantConn`] — the wyrd-sql RLS bind enforces
//! tenant scope.
// raw-query grep allowlist: olap control tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::olap_catalog::{BifrostTableRow, OlapCommitRow};

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
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.olap_commits
            (data_tenant_id, table_uid, batch_id, state, origin, actor)
        VALUES (wyrd.current_tenant(), $1, $2, 'precommit', 'system', 'system')
        ON CONFLICT (data_tenant_id, table_uid, batch_id) DO NOTHING
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Transition the anchor to `committed` and record the discovered snapshot id.
///
/// Only rows in `precommit` state are updated; already-committed rows are
/// left unchanged (idempotent replay).
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn finalize_committed(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    snapshot_id: i64,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        UPDATE vala.olap_commits
           SET state = 'committed', committed_at = now(), snapshot_id = $3
         WHERE table_uid = $1 AND batch_id = $2 AND state = 'precommit'
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(snapshot_id)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Transition the anchor to `failed` and record the error.
///
/// Sets `finalized_at`, not `committed_at` — these are distinct FSM
/// transitions.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn finalize_failed(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    batch_id: &[u8; 16],
    error_code: &str,
    error_detail: &str,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        UPDATE vala.olap_commits
           SET state = 'failed', finalized_at = now(),
               error_code = $3, error_detail = $4
         WHERE table_uid = $1 AND batch_id = $2 AND state = 'precommit'
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(batch_id.as_slice())
    .bind(error_code)
    .bind(error_detail)
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
