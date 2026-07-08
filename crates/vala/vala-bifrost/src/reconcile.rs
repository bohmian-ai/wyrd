//! Cross-store audit reconciliation (M-13).
//!
//! Three checks per tenant:
//! 1. **Seq-gap** (SQL): `audit_outbox.seq` is contiguous up to the head.
//! 2. **Hash-chain** (SQL): `prev_hash` → `entry_hash` chain is unbroken.
//! 3. **Outbox↔warehouse parity** (cross-store): every `shipped=true` outbox
//!    row has exactly one matching row in `vala.system.audit_log` by `seq`.
//!
//! Checks 1 and 2 read Postgres only (delegated to `vala_sql`). Check 3 is the
//! M-13 fix: parity is proven by the Bifrost layer because it can read **both**
//! the SQL outbox and the Iceberg `audit_log` via `DataFusion` — a SQL-only
//! verifier can't observe Iceberg.

use std::collections::HashSet;
use std::sync::Arc;

use arrow::array::Array;
use datafusion::prelude::SessionContext;
use sqlx::PgPool;
use wyrd_spec::ids::DataTenantId;

use vala_sql::TenantConn;
use vala_sql::queries::audit_outbox::{
    ChainBreak, SeqGap, check_hash_chain, check_seq_gaps, shipped_outbox_refs,
};

use crate::catalog::WyrdCatalog;
use crate::catalog::namespaces::BifrostNamespace;
use crate::error::BifrostError;
use crate::session::wyrd_session_context;
use crate::tables::DomainTable;
use crate::tables::system::AuditLogTable;

/// Results of one reconciliation pass for a single tenant.
#[derive(Debug)]
pub struct AuditReconcileResult {
    /// Tenant whose audit was reconciled.
    pub tenant_id: DataTenantId,
    /// Gaps detected in the outbox `seq` sequence (check 1).
    pub seq_gaps: Vec<SeqGap>,
    /// Hash-chain breaks (check 2).
    pub chain_breaks: Vec<ChainBreak>,
    /// `seq` values present in the shipped outbox but absent in the Iceberg warehouse (check 3).
    pub parity_misses: Vec<i64>,
    /// `seq` values present in the warehouse but not in the shipped outbox (check 3).
    pub orphan_warehouse_seqs: Vec<i64>,
}

impl AuditReconcileResult {
    /// True when all three checks pass with zero violations.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.seq_gaps.is_empty()
            && self.chain_breaks.is_empty()
            && self.parity_misses.is_empty()
            && self.orphan_warehouse_seqs.is_empty()
    }
}

/// Run all three audit reconciliation checks for `tenant_id`.
///
/// `app_pool` is the application pool (used for per-tenant `TenantConn`).
/// `catalog` is the Bifrost catalog used to open the `audit_log` Iceberg provider.
///
/// # Errors
/// Returns [`BifrostError`] when any SQL query or `DataFusion` scan fails.
pub async fn reconcile_audit(
    app_pool: &PgPool,
    catalog: &WyrdCatalog,
    tenant_id: DataTenantId,
) -> Result<AuditReconcileResult, BifrostError> {
    let mut conn = TenantConn::acquire(app_pool, tenant_id)
        .await
        .map_err(BifrostError::Sql)?;

    let seq_gaps = check_seq_gaps(&mut conn).await.map_err(BifrostError::Sql)?;
    let chain_breaks = check_hash_chain(&mut conn)
        .await
        .map_err(BifrostError::Sql)?;
    let shipped_refs = shipped_outbox_refs(&mut conn)
        .await
        .map_err(BifrostError::Sql)?;
    conn.commit().await.map_err(BifrostError::Sql)?;

    let (parity_misses, orphan_warehouse_seqs) =
        parity_check(catalog, tenant_id, &shipped_refs).await?;

    Ok(AuditReconcileResult {
        tenant_id,
        seq_gaps,
        chain_breaks,
        parity_misses,
        orphan_warehouse_seqs,
    })
}

/// Parity check (M-13): compare shipped outbox `seq` values against those
/// present in the Iceberg `audit_log` for `tenant_id` via `DataFusion`.
///
/// Returns `(parity_misses, orphan_warehouse_seqs)`:
/// - `parity_misses`: seqs in shipped outbox but absent in warehouse.
/// - `orphan_warehouse_seqs`: seqs in warehouse but absent in shipped outbox.
async fn parity_check(
    catalog: &WyrdCatalog,
    tenant_id: DataTenantId,
    shipped: &[vala_sql::queries::audit_outbox::ShippedOutboxRef],
) -> Result<(Vec<i64>, Vec<i64>), BifrostError> {
    let outbox_seqs: HashSet<i64> = shipped.iter().map(|r| r.seq).collect();

    let warehouse_seqs = audit_log_seqs(catalog, tenant_id).await?;

    let parity_misses: Vec<i64> = outbox_seqs
        .iter()
        .filter(|s| !warehouse_seqs.contains(s))
        .copied()
        .collect();
    let orphan_warehouse_seqs: Vec<i64> = warehouse_seqs
        .iter()
        .filter(|s| !outbox_seqs.contains(s))
        .copied()
        .collect();

    Ok((parity_misses, orphan_warehouse_seqs))
}

/// Scan `vala.system.audit_log` via `DataFusion` and return all `seq` values for
/// the given tenant. Returns an empty vec when no rows exist (table may be empty
/// for a tenant with no shipped rows yet).
async fn audit_log_seqs(
    catalog: &WyrdCatalog,
    tenant_id: DataTenantId,
) -> Result<HashSet<i64>, BifrostError> {
    let provider = match catalog
        .provider(BifrostNamespace::System, AuditLogTable::NAME, tenant_id)
        .await
    {
        Ok(p) => p,
        Err(BifrostError::TableNotFound(_)) => return Ok(HashSet::new()),
        Err(other) => return Err(other),
    };

    let ctx: SessionContext = wyrd_session_context(tenant_id);
    ctx.register_table(
        datafusion::common::TableReference::bare(AuditLogTable::NAME),
        Arc::new(provider),
    )
    .map_err(|e| BifrostError::Internal(e.to_string()))?;

    let df = ctx
        .sql("SELECT seq FROM audit_log")
        .await
        .map_err(|e| BifrostError::Internal(e.to_string()))?;

    let batches = df
        .collect()
        .await
        .map_err(|e| BifrostError::Internal(e.to_string()))?;

    let mut seqs = HashSet::new();
    for batch in &batches {
        let Some(col) = batch.column_by_name("seq") else {
            continue;
        };
        let arr = col
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .ok_or_else(|| BifrostError::Internal("audit_log seq column is not Int64".into()))?;
        for i in 0..arr.len() {
            if !arr.is_null(i) {
                seqs.insert(arr.value(i));
            }
        }
    }

    Ok(seqs)
}
