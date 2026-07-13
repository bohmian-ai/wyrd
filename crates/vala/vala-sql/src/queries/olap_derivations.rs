//! Tenant-scoped reads + writes for the cross-table derivation registry:
//! `vala.olap_derivations`.
//!
//! A derivation reads a source table and writes a target table. Progress is
//! tracked by a durable WATERMARK: the last source commit position (`batch_id`)
//! fully consumed. This mirrors how `vala.olap_commits` tracks commit progress —
//! by opaque 16-byte `batch_id`, never by a numeric snapshot-id compare. When a
//! derivation has never advanced, `select_derivation_pin` resolves to the
//! `registered_watermark` (the earliest position captured at registration) so
//! the worker replays from the registered/earliest position rather than an
//! undefined start.
//!
//! All callers pass a [`TenantConn`]; the wyrd-sql RLS bind enforces tenant
//! scope for the wyrd_app role alongside the explicit predicates below.
// raw-query grep allowlist: olap_derivations post-dates the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::Uuid;
use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::olap_derivations::{DerivationFreshnessRow, DerivationRow};

/// Register a derivation idempotently.
///
/// Inserts one `(source, target, control_bind)` derivation for the current
/// tenant. `registered_watermark` is the earliest source position the
/// derivation is pinned to (`None` = from the beginning). `ON CONFLICT DO
/// NOTHING` keys on the `(data_tenant_id, source_table_uid, target_table_uid,
/// control_bind)` unique constraint so re-registration is a no-op.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or an RLS policy rejects the row.
pub async fn insert_derivation(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
    source_table_uid: &[u8; 16],
    target_table_uid: &[u8; 16],
    control_bind: Uuid,
    fqn: &str,
    registered_watermark: Option<&[u8; 16]>,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.olap_derivations
            (data_tenant_id, derivation_uid, source_table_uid, target_table_uid,
             control_bind, fqn, registered_watermark)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6)
        ON CONFLICT (data_tenant_id, source_table_uid, target_table_uid, control_bind)
        DO NOTHING
        "#,
    )
    .bind(derivation_uid.as_slice())
    .bind(source_table_uid.as_slice())
    .bind(target_table_uid.as_slice())
    .bind(control_bind)
    .bind(fqn)
    .bind(registered_watermark.map(|w| w.as_slice()))
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Flip a derivation into the `deriving` (in-progress) health state.
///
/// Called when the worker claims a derivation and starts consuming a delta.
/// Returns `true` when a row moved into `deriving`, `false` when no matching
/// derivation exists for the current tenant.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn mark_deriving(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE vala.olap_derivations
           SET derivation_state = 'deriving',
               updated_at       = now()
         WHERE derivation_uid   = $1
        "#,
    )
    .bind(derivation_uid.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(result.rows_affected() > 0)
}

/// Advance a derivation's watermark to `watermark` after a delta is fully
/// derived into the target.
///
/// Records `watermark` as the last consumed source commit position, stamps
/// `derived_at`, and returns the derivation to `idle`. `watermark` is an opaque
/// 16-byte source `batch_id`; it is stored, not numerically compared. Returns
/// `true` when a row advanced, `false` when no matching derivation exists.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn advance_watermark(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
    watermark: &[u8; 16],
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE vala.olap_derivations
           SET watermark        = $2,
               derivation_state = 'idle',
               derived_at       = now(),
               updated_at       = now()
         WHERE derivation_uid   = $1
        "#,
    )
    .bind(derivation_uid.as_slice())
    .bind(watermark.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(result.rows_affected() > 0)
}

/// Read the freshness / lag of one derivation for the current tenant.
///
/// `lag_seconds` is `now() - derived_at` in seconds, `NULL` when the derivation
/// has never advanced. Used by tests and the Task F read-back polling loop.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn derivation_freshness(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
) -> Result<Option<DerivationFreshnessRow>, SqlError> {
    sqlx::query_as::<_, DerivationFreshnessRow>(
        r#"
        SELECT derivation_uid,
               watermark,
               derivation_state,
               derived_at,
               EXTRACT(EPOCH FROM (now() - derived_at))::float8 AS lag_seconds
          FROM vala.olap_derivations
         WHERE derivation_uid = $1
        "#,
    )
    .bind(derivation_uid.as_slice())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// List derivations that need work: registered against `source_table_uid` and
/// not in a terminal-failure/degraded state.
///
/// The worker uses this to find derivations to advance for a source table that
/// just committed. Filters out `degraded` rows (structurally broken) so only
/// runnable candidates are returned.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn select_derivation_candidates(
    conn: &mut TenantConn<'_>,
    source_table_uid: &[u8; 16],
) -> Result<Vec<DerivationRow>, SqlError> {
    sqlx::query_as::<_, DerivationRow>(
        r#"
        SELECT data_tenant_id,
               derivation_uid,
               source_table_uid,
               target_table_uid,
               control_bind,
               watermark,
               registered_watermark,
               derivation_state,
               fqn,
               registered_at,
               derived_at,
               updated_at
          FROM vala.olap_derivations
         WHERE source_table_uid = $1
           AND derivation_state <> 'degraded'
        "#,
    )
    .bind(source_table_uid.as_slice())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Return the pinned watermark for a derivation: the position from which the
/// worker should read next.
///
/// Resolves to `COALESCE(watermark, registered_watermark)` — a NULL watermark
/// (nothing consumed yet) pins to the registered/earliest position rather than
/// an undefined start. Returns `Ok(None)` when the derivation does not exist,
/// and `Ok(Some(None))` when both the watermark and the registered pin are NULL
/// (pin from the absolute beginning of the source).
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn select_derivation_pin(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
) -> Result<Option<Option<Vec<u8>>>, SqlError> {
    let pin: Option<(Option<Vec<u8>>,)> = sqlx::query_as(
        r#"
        SELECT COALESCE(watermark, registered_watermark) AS pin
          FROM vala.olap_derivations
         WHERE derivation_uid = $1
        "#,
    )
    .bind(derivation_uid.as_slice())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(pin.map(|(p,)| p))
}
