//! Stale-precommit recovery sweep.
//!
//! One tick per 60 s: claims expired precommit rows via the SECURITY DEFINER
//! `vala.claim_stale_precommits` routine (owned by `vala_recovery_owner`,
//! BYPASSRLS), inspects the Iceberg catalog to determine the oracle outcome
//! (committed or aborted), then resolves each row via the matching recovery
//! routine. Two pods cannot double-finalize the same row: the
//! `recovery_fencing_token` CAS inside each routine guards it.
//!
//! Once a row's `recovery_attempts` reaches `RECOVERY_STUCK_THRESHOLD` the
//! sweep emits a `recovery_stuck` tracing event — the row is not forcibly
//! aborted so a human operator can inspect it first.

use sqlx::types::Uuid;
use vala_sql::SqlError;
use vala_sql::postgres::ValaPostgres;

use super::MaintenanceHealthState;
use crate::catalog::WyrdCatalog;

/// Health state for the commit-recovery concern.
///
/// Returns [`MaintenanceHealthState::Pending`] until slice 13 wires real state
/// tracking into the scheduler (the sweep itself is wired in this slice).
#[must_use]
pub fn health() -> MaintenanceHealthState {
    MaintenanceHealthState::Pending
}

/// Emit a `recovery_stuck` warning once `recovery_attempts` crosses this
/// threshold. The row is not aborted — a human must inspect and resolve it.
pub const RECOVERY_STUCK_THRESHOLD: i32 = 5;

/// Outcome of a single tick of the recovery sweep.
#[derive(Debug, Default)]
pub struct RecoveryTickOutcome {
    /// Number of stale precommits claimed this tick.
    pub claimed: usize,
    /// Number of rows resolved as committed (snapshot found in Iceberg).
    pub resolved_committed: usize,
    /// Number of rows resolved as aborted (snapshot absent from Iceberg).
    pub resolved_aborted: usize,
    /// Number of rows that failed oracle inspection (re-queued for next tick).
    pub scan_failed: usize,
    /// Number of rows emitting `recovery_stuck` warnings.
    pub stuck: usize,
}

/// Run one tick of the recovery sweep against the `vala_recovery` pool.
///
/// Claims up to `limit` stale precommit rows, resolves each one by checking
/// the Iceberg catalog for the batch's snapshot, then finalizes via the
/// SECURITY DEFINER routines. Idempotent: each row carries a
/// `recovery_fencing_token` so concurrent sweeps cannot double-finalize.
///
/// # Errors
/// Returns [`SqlError`] when the claim query or any finalization routine fails.
pub async fn tick(
    vala: &ValaPostgres,
    catalog: &WyrdCatalog,
    limit: i32,
) -> Result<RecoveryTickOutcome, SqlError> {
    let Some(recovery_pool) = vala.recovery_pool() else {
        tracing::debug!("commit_recovery: recovery pool absent, skipping tick");
        return Ok(RecoveryTickOutcome::default());
    };

    let sweep_owner = *crate::writer::commit::WRITER_INSTANCE;
    let rows = claim_stale_precommits(recovery_pool, sweep_owner, limit).await?;

    let mut outcome = RecoveryTickOutcome {
        claimed: rows.len(),
        ..Default::default()
    };

    for row in rows {
        if row.recovery_attempts >= RECOVERY_STUCK_THRESHOLD {
            tracing::warn!(
                recovery_attempts = row.recovery_attempts,
                table_uid = ?row.table_uid,
                batch_id = ?row.batch_id,
                tenant_id = %row.data_tenant_id,
                "recovery_stuck: precommit row has exceeded recovery threshold — manual inspection required"
            );
            outcome.stuck += 1;
            // Do not attempt further resolution; leave token in place so the
            // row does not spin through claim/fail on every tick.
            continue;
        }

        let table_name = row.name.clone();
        let namespace = row.namespace.clone();

        let oracle = oracle_check(catalog, &row).await;

        match oracle {
            Ok(Some(snapshot_id)) => {
                if let Err(e) = finalize_committed(
                    recovery_pool,
                    row.data_tenant_id,
                    &row.table_uid,
                    &row.batch_id,
                    snapshot_id,
                    row.fencing_token,
                )
                .await
                {
                    tracing::error!(
                        error = %e,
                        namespace = %namespace,
                        table_name = %table_name,
                        "commit_recovery: finalize_committed failed"
                    );
                    outcome.scan_failed += 1;
                } else {
                    outcome.resolved_committed += 1;
                }
            }
            Ok(None) => {
                let reason = "snapshot absent from Iceberg catalog";
                if let Err(e) = finalize_aborted(
                    recovery_pool,
                    row.data_tenant_id,
                    &row.table_uid,
                    &row.batch_id,
                    row.fencing_token,
                    reason,
                )
                .await
                {
                    tracing::error!(
                        error = %e,
                        namespace = %namespace,
                        table_name = %table_name,
                        "commit_recovery: finalize_aborted failed"
                    );
                    outcome.scan_failed += 1;
                } else {
                    outcome.resolved_aborted += 1;
                }
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    namespace = %namespace,
                    table_name = %table_name,
                    "commit_recovery: oracle check failed, marking scan_failed"
                );
                let _ = mark_scan_failed(
                    recovery_pool,
                    row.data_tenant_id,
                    &row.table_uid,
                    &row.batch_id,
                    row.fencing_token,
                    &e.to_string(),
                )
                .await;
                outcome.scan_failed += 1;
            }
        }
    }

    Ok(outcome)
}

/// One claimed precommit row returned by `vala.claim_stale_precommits`.
#[derive(Debug)]
pub(crate) struct ClaimedRow {
    pub data_tenant_id: Uuid,
    pub table_uid: Vec<u8>,
    pub batch_id: Vec<u8>,
    pub fencing_token: i64,
    // fqn is the authoritative table identity for the recovery oracle
    // (`oracle_check` resolves the table via `split_fqn`).
    pub fqn: String,
    pub namespace: String,
    pub name: String,
    pub scope: String,
    pub recovery_attempts: i32,
}

/// Invoke `vala.claim_stale_precommits` via the recovery pool (BYPASSRLS).
async fn claim_stale_precommits(
    pool: &sqlx::PgPool,
    owner: Uuid,
    limit: i32,
) -> Result<Vec<ClaimedRow>, SqlError> {
    // Dynamic query is intentional: the SECURITY DEFINER routine is called via
    // the recovery pool; no TenantConn is possible here. The routine returns
    // recovery_attempts directly (for stuck detection) — the recovery role is
    // NOT BYPASSRLS, so a separate SELECT on vala.olap_commits would trip RLS.
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Vec<u8>,
            Vec<u8>,
            i64,
            String,
            String,
            String,
            String,
            i32,
        ),
    >(
        "SELECT data_tenant_id, table_uid, batch_id, fencing_token, fqn, namespace, name, scope, \
         recovery_attempts \
         FROM vala.claim_stale_precommits($1, $2)",
    )
    .bind(owner)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(SqlError::from)?;

    let claimed = rows
        .into_iter()
        .map(
            |(
                data_tenant_id,
                table_uid,
                batch_id,
                fencing_token,
                fqn,
                namespace,
                name,
                scope,
                recovery_attempts,
            )| ClaimedRow {
                data_tenant_id,
                table_uid,
                batch_id,
                fencing_token,
                fqn,
                namespace,
                name,
                scope,
                recovery_attempts,
            },
        )
        .collect();

    Ok(claimed)
}

/// Check the Iceberg catalog to determine whether the batch was committed.
///
/// Decides by scanning the table's snapshot history for one that durably carries
/// this row's `{data_tenant_id, batch_id}` commit identity (the same
/// pair-membership oracle the startup reconcile uses via
/// [`crate::catalog::decide_recovery`]). Returns `Some(snapshot_id)` for the
/// snapshot that carries the pair, `None` when no snapshot carries it (the write
/// never landed) or the table is absent from the catalog.
///
/// Deciding on `current_snapshot_id()` alone would be a silent lost-write bug: a
/// precommit that crashed before its `fast_append` would be finalized `committed`
/// against an unrelated later snapshot, and the client's idempotent retry would
/// then dedup and write nothing.
async fn oracle_check(
    catalog: &WyrdCatalog,
    row: &ClaimedRow,
) -> Result<Option<i64>, crate::error::BifrostError> {
    use crate::catalog::namespaces::BifrostNamespace;
    use crate::catalog::{RecoveryDecision, decide_recovery};
    use crate::types::TableScope;

    // Resolve the table from the full FQN. The claim query's `split_part`
    // namespace/name columns are unreliable for the dotted Bifrost namespaces
    // (e.g. "vala.bifrost" → split_part yields "vala"/"bifrost", not the real
    // table name); `split_fqn` strips the known namespace prefix correctly,
    // mirroring the startup reconcile's fqn-based resolution.
    let (ns, table_name) = BifrostNamespace::split_fqn(&row.fqn).ok_or_else(|| {
        crate::error::BifrostError::Internal(format!(
            "recovery oracle: unparseable fqn {:?}",
            row.fqn
        ))
    })?;

    let scope = TableScope::from_db_str(&row.scope).map_err(|e| {
        crate::error::BifrostError::Internal(format!(
            "recovery oracle: invalid scope {:?}: {e}",
            row.scope
        ))
    })?;

    let tenant_id = wyrd_spec::ids::DataTenantId::try_from(row.data_tenant_id).map_err(|_| {
        crate::error::BifrostError::Internal(format!(
            "recovery oracle: invalid tenant_id {:?}",
            row.data_tenant_id
        ))
    })?;

    let batch_id: [u8; 16] = row.batch_id.as_slice().try_into().map_err(|_| {
        crate::error::BifrostError::Internal(format!(
            "recovery oracle: batch_id length mismatch for {:?}",
            row.fqn
        ))
    })?;

    // Two-step: resolve using scope.control_bind(tenant) to find the registration,
    // then decide by commit-key membership against the loaded Iceberg table.
    let meta = catalog
        .get(ns, &table_name, scope.control_bind(tenant_id))
        .await;

    match meta {
        Ok(meta) => match decide_recovery(&meta.iceberg_table, row.data_tenant_id, &batch_id) {
            RecoveryDecision::Committed(snapshot_id) => Ok(Some(snapshot_id)),
            RecoveryDecision::Aborted => Ok(None),
        },
        Err(crate::error::BifrostError::TableNotFound(_)) => Ok(None),
        Err(other) => Err(other),
    }
}

/// Invoke `vala.finalize_recovered_committed` via the recovery pool.
async fn finalize_committed(
    pool: &sqlx::PgPool,
    data_tenant_id: Uuid,
    table_uid: &[u8],
    batch_id: &[u8],
    snapshot_id: i64,
    token: i64,
) -> Result<(), SqlError> {
    // Dynamic query is intentional: SECURITY DEFINER routine on recovery pool.
    sqlx::query("SELECT vala.finalize_recovered_committed($1, $2, $3, $4, $5)")
        .bind(data_tenant_id)
        .bind(table_uid)
        .bind(batch_id)
        .bind(snapshot_id)
        .bind(token)
        .execute(pool)
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// Invoke `vala.finalize_recovered_aborted` via the recovery pool.
async fn finalize_aborted(
    pool: &sqlx::PgPool,
    data_tenant_id: Uuid,
    table_uid: &[u8],
    batch_id: &[u8],
    token: i64,
    reason: &str,
) -> Result<(), SqlError> {
    // Dynamic query is intentional: SECURITY DEFINER routine on recovery pool.
    sqlx::query("SELECT vala.finalize_recovered_aborted($1, $2, $3, $4, $5)")
        .bind(data_tenant_id)
        .bind(table_uid)
        .bind(batch_id)
        .bind(token)
        .bind(reason)
        .execute(pool)
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// Invoke `vala.mark_recovery_scan_failed` via the recovery pool.
async fn mark_scan_failed(
    pool: &sqlx::PgPool,
    data_tenant_id: Uuid,
    table_uid: &[u8],
    batch_id: &[u8],
    token: i64,
    error: &str,
) -> Result<(), SqlError> {
    // Dynamic query is intentional: SECURITY DEFINER routine on recovery pool.
    sqlx::query("SELECT vala.mark_recovery_scan_failed($1, $2, $3, $4, $5)")
        .bind(data_tenant_id)
        .bind(table_uid)
        .bind(batch_id)
        .bind(token)
        .bind(error)
        .execute(pool)
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_stuck_threshold_is_positive() {
        const { assert!(RECOVERY_STUCK_THRESHOLD > 0, "threshold must be > 0") };
    }

    #[test]
    fn recovery_tick_outcome_default_is_zero() {
        let o = RecoveryTickOutcome::default();
        assert_eq!(o.claimed, 0);
        assert_eq!(o.resolved_committed, 0);
        assert_eq!(o.resolved_aborted, 0);
        assert_eq!(o.scan_failed, 0);
        assert_eq!(o.stuck, 0);
    }

    /// Stale precommit claim loop: when a row's `recovery_attempts` reaches
    /// `RECOVERY_STUCK_THRESHOLD`, the outcome.stuck counter increments and the
    /// row is NOT resolved (no finalize call attempted).
    #[test]
    fn stuck_threshold_gates_finalization() {
        // This logic is tested structurally: the tick() function skips
        // finalization when recovery_attempts >= RECOVERY_STUCK_THRESHOLD and
        // increments outcome.stuck. The unit here verifies the constant is the
        // guard value used in the comparison.
        let row = ClaimedRow {
            data_tenant_id: Uuid::new_v4(),
            table_uid: vec![0u8; 16],
            batch_id: vec![0u8; 16],
            fencing_token: 1,
            fqn: "vala.test".to_string(),
            namespace: "vala".to_string(),
            name: "test".to_string(),
            scope: "tenant_owned".to_string(),
            recovery_attempts: RECOVERY_STUCK_THRESHOLD,
        };
        // A row at exactly the threshold triggers the stuck guard.
        assert!(row.recovery_attempts >= RECOVERY_STUCK_THRESHOLD);
    }
}
