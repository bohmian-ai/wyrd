//! Tenant-scoped reads + writes for the Vala OLAP catalog control tables:
//! `vala.bifrost_tables`, `vala.refresh_epochs`, `vala.olap_indexes`, and `vala.entity_time_bounds`.
//! All callers must pass a [`TenantConn`] — the wyrd-sql RLS bind enforces
//! tenant scope for wyrd_app-role paths.
// raw-query grep allowlist: olap control tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::olap_catalog::{
    BifrostTableRow, DeclaredIndexRow, EntityTimeBoundsRow, ProjectionCandidateRow,
};

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
    physical_layout: &serde_json::Value,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.bifrost_tables
            (data_tenant_id, table_uid, fqn, fingerprint, physical_layout, origin, actor)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4, 'system', 'system')
        ON CONFLICT (data_tenant_id, table_uid)
        DO UPDATE SET
            fqn             = EXCLUDED.fqn,
            fingerprint     = EXCLUDED.fingerprint,
            physical_layout = EXCLUDED.physical_layout,
            updated_at      = now()
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(fqn)
    .bind(fingerprint.as_slice())
    .bind(physical_layout)
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
        SELECT data_tenant_id, table_uid, fqn, fingerprint, status,
               physical_layout, registered_at, updated_at, origin, actor
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
        SELECT data_tenant_id, table_uid, fqn, fingerprint, status,
               physical_layout, registered_at, updated_at, origin, actor
          FROM vala.bifrost_tables
        "#,
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Delete a Bifrost table registration and all dependent rows for the current
/// tenant, in FK order. `vala.refresh_epochs` reference
/// `vala.bifrost_tables` with no `ON DELETE CASCADE`. Children are removed first. Intended for test/bench teardown.
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
    sqlx::query("DELETE FROM vala.bifrost_tables WHERE fqn = $1")
        .bind(fqn)
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

// ── vala.olap_indexes ────────────────────────────────────────────────────────

/// Declare (upsert) a skip index for a domain table column. Sets initial
/// `index_state = 'building'`; the Stage-5 maintenance worker transitions to
/// `ready` after filling the index. Idempotent on `(data_tenant_id, table_uid,
/// column_name, index_kind)`.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or an RLS policy rejects the row.
pub async fn declare_index(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    column_name: &str,
    index_kind: &str,
    params: Option<&serde_json::Value>,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.olap_indexes
            (data_tenant_id, table_uid, column_name, index_kind, index_state, params)
        VALUES (wyrd.current_tenant(), $1, $2, $3, 'building', $4)
        ON CONFLICT (data_tenant_id, table_uid, column_name, index_kind)
        DO NOTHING
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(column_name)
    .bind(index_kind)
    .bind(params)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Advance an index to a new state. Enforces the state-machine transition
/// via the database trigger (building→ready|failed, ready→deprecated).
///
/// # Errors
/// Returns [`SqlError`] when the transition is invalid or the query fails.
pub async fn update_index_state(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    column_name: &str,
    index_kind: &str,
    new_state: &str,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        UPDATE vala.olap_indexes
           SET index_state = $4, updated_at = now()
         WHERE data_tenant_id = wyrd.current_tenant()
           AND table_uid    = $1
           AND column_name  = $2
           AND index_kind   = $3
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(column_name)
    .bind(index_kind)
    .bind(new_state)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// List all declared indexes for the given table visible to the current tenant.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn list_indexes_for_table(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
) -> Result<Vec<DeclaredIndexRow>, SqlError> {
    sqlx::query_as::<_, DeclaredIndexRow>(
        r#"
        SELECT data_tenant_id, table_uid, column_name, index_kind, index_state,
               params, created_at, updated_at
          FROM vala.olap_indexes
         WHERE table_uid = $1
        "#,
    )
    .bind(table_uid.as_slice())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

// ── vala.entity_time_bounds ──────────────────────────────────────────────────

/// Upsert per-entity min/max `wyrd_event_time` bounds derived from a committed
/// batch (M-06). Takes a slice of `(entity_id, min_event_time, max_event_time)`
/// tuples; uses `LEAST`/`GREATEST` to widen existing bounds. Idempotent and
/// safe under concurrent writers (no window-shrinking). Best-effort: a failure
/// here is a bounds miss, not a commit failure.
///
/// `entity_kind` must be one of `trace`, `agent_run`, `session`.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn record_entity_bounds(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    entity_kind: &str,
    bounds: &[(
        &str,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
    )],
) -> Result<(), SqlError> {
    for (entity_id, min_t, max_t) in bounds {
        sqlx::query(
            r#"
            INSERT INTO vala.entity_time_bounds
                (data_tenant_id, table_uid, entity_kind, entity_id, min_event_time, max_event_time)
            VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5)
            ON CONFLICT (data_tenant_id, table_uid, entity_kind, entity_id)
            DO UPDATE SET
                min_event_time = LEAST   (entity_time_bounds.min_event_time, EXCLUDED.min_event_time),
                max_event_time = GREATEST(entity_time_bounds.max_event_time, EXCLUDED.max_event_time)
            "#,
        )
        .bind(table_uid.as_slice())
        .bind(entity_kind)
        .bind(entity_id)
        .bind(min_t)
        .bind(max_t)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
    }
    Ok(())
}

/// Fetch the known min/max event-time window for an entity, or `None` when no
/// bounds have been recorded yet (a bounds miss — caller falls back to its
/// default scan window).
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn entity_bounds_for(
    conn: &mut TenantConn<'_>,
    table_uid: &[u8; 16],
    entity_kind: &str,
    entity_id: &str,
) -> Result<Option<EntityTimeBoundsRow>, SqlError> {
    sqlx::query_as::<_, EntityTimeBoundsRow>(
        r#"
        SELECT data_tenant_id, table_uid, entity_kind, entity_id,
               min_event_time, max_event_time
          FROM vala.entity_time_bounds
         WHERE table_uid   = $1
           AND entity_kind = $2
           AND entity_id   = $3
        "#,
    )
    .bind(table_uid.as_slice())
    .bind(entity_kind)
    .bind(entity_id)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

// ── vala.olap_projections ────────────────────────────────────────────────────

/// List all projection candidate rows for a given source table (tenant-scoped).
///
/// Returns all registered projections whose `source_table_uid` matches the
/// given table. The serving layer's matcher uses this to find substitutable
/// projections for a source scan. Filters to non-degraded states so the
/// matcher only sees candidates that are at least structurally valid.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn list_by_source(
    conn: &mut TenantConn<'_>,
    source_table_uid: &[u8; 16],
) -> Result<Vec<ProjectionCandidateRow>, SqlError> {
    sqlx::query_as::<_, ProjectionCandidateRow>(
        r#"
        SELECT data_tenant_id,
               projection_uid,
               source_table_uid,
               fqn,
               projection_kind,
               projection_state,
               refresh_epoch,
               source_refresh_epoch,
               built_for_snapshot_id,
               commit_lag,
               source_schema_fingerprint
          FROM vala.olap_projections
         WHERE source_table_uid = $1
           AND projection_state <> 'degraded'
        "#,
    )
    .bind(source_table_uid.as_slice())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}
