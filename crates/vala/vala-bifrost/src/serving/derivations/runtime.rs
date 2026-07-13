//! Core derivation-runtime logic: source-delta scan → transform → target write.
//!
//! [`DerivationRuntime`] owns the pre-fetched table UIDs and orchestrates one
//! tick: it enumerates tenants with committed source data, then calls
//! [`process_tenant`](DerivationRuntime::process_tenant) per tenant to consume
//! the unprocessed delta and advance the watermark.

use std::sync::Arc;

use arrow::array::RecordBatch;
use arrow::compute::concat_batches;
use datafusion::common::TableReference;
use datafusion::prelude::{col, lit};
use datafusion::scalar::ScalarValue;
use uuid::Uuid;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::WyrdCatalog;
use crate::catalog::namespaces::BifrostNamespace;
use crate::error::BifrostError;
use crate::session::wyrd_session_context;
use crate::tables::DomainTable;
use crate::tables::genai::{
    DomainDerivation, EmbeddingsTable, GenAiFromSpans, MemoryTable, MessagesTable, ToolCallsTable,
};
use crate::tables::traces::SpansTable;
use crate::types::TableScope;
use crate::writer::BifrostWriteContext;

fn table_uid_from_row(
    row: &vala_sql::row_types::olap_catalog::BifrostTableRow,
) -> Result<[u8; 16], BifrostError> {
    row.table_uid
        .as_slice()
        .try_into()
        .map_err(|_| BifrostError::Internal("table_uid length mismatch".to_string()))
}

/// Pre-fetched table UIDs for the genai derivation.
///
/// Initialised once at worker startup from the catalog SQL registry. UIDs are
/// stable for the lifetime of the registration (they're set at
/// `register_all` time and never change), so caching here is safe.
pub(super) struct TableUids {
    pub source: [u8; 16],
    pub messages: [u8; 16],
    pub embeddings: [u8; 16],
    pub tool_calls: [u8; 16],
    pub memory: [u8; 16],
}

impl TableUids {
    /// Look up all five UIDs from the SQL catalog. Requires at least one
    /// committed tenant to exist for the source table (i.e. `register_all`
    /// has already run at startup).
    pub(super) async fn load(catalog: &WyrdCatalog) -> Result<Self, BifrostError> {
        let owner = DataTenantId::SYSTEM_OWNER;

        let src = catalog
            .get(BifrostNamespace::Traces, SpansTable::NAME, owner)
            .await?;
        let msg = catalog
            .get(BifrostNamespace::GenAi, MessagesTable::NAME, owner)
            .await?;
        let emb = catalog
            .get(BifrostNamespace::GenAi, EmbeddingsTable::NAME, owner)
            .await?;
        let tc = catalog
            .get(BifrostNamespace::GenAi, ToolCallsTable::NAME, owner)
            .await?;
        let mem = catalog
            .get(BifrostNamespace::GenAi, MemoryTable::NAME, owner)
            .await?;

        Ok(Self {
            source: table_uid_from_row(&src.row)?,
            messages: table_uid_from_row(&msg.row)?,
            embeddings: table_uid_from_row(&emb.row)?,
            tool_calls: table_uid_from_row(&tc.row)?,
            memory: table_uid_from_row(&mem.row)?,
        })
    }
}

/// Stateless derivation runtime for one tick.
///
/// Accepts a snapshot of pre-fetched UIDs and a live catalog + pools. Calling
/// [`run_tick`](Self::run_tick) drives one full derivation pass.
pub(super) struct DerivationRuntime {
    pub catalog: Arc<WyrdCatalog>,
    pub pool: sqlx::PgPool,
    pub op: vala_sql::OperatorPool,
    pub uids: TableUids,
    pub derivation: GenAiFromSpans,
}

impl DerivationRuntime {
    /// Enumerate tenants with committed source data and run a derivation pass
    /// for each. Per-tenant errors are logged but do not abort other tenants.
    pub(super) async fn run_tick(&self) -> Result<(), BifrostError> {
        let tenant_uuids = vala_sql::queries::olap_derivations::list_source_commit_tenants(
            &self.op,
            &self.uids.source,
        )
        .await
        .map_err(BifrostError::Sql)?;

        for raw_uuid in tenant_uuids {
            let Ok(tenant) = DataTenantId::try_from(raw_uuid) else {
                tracing::warn!(uuid = %raw_uuid, "derivation: skipping non-UUIDv7 tenant");
                continue;
            };
            if let Err(e) = self.process_tenant(tenant).await {
                tracing::error!(tenant = %tenant, error = %e, "derivation: tenant tick failed");
            }
        }
        Ok(())
    }

    async fn process_tenant(&self, tenant: DataTenantId) -> Result<(), BifrostError> {
        let derivation_uid = GenAiFromSpans::DERIVATION_UID;
        let derivation_uid_bytes: [u8; 16] = *derivation_uid.as_bytes();

        // ── 1. Upsert derivation registration (idempotent) ────────────────
        {
            let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            vala_sql::queries::olap_derivations::insert_derivation(
                &mut conn,
                &derivation_uid_bytes,
                &self.uids.source,
                // target_table_uid: use messages as the "primary" target for
                // registration; the derivation emits to multiple targets but
                // `olap_derivations` tracks watermark per (source, derivation_uid).
                &self.uids.messages,
                Uuid::nil(), // control_bind: nil for system derivations
                "genai_from_spans",
                None,
            )
            .await
            .map_err(BifrostError::Sql)?;
            conn.commit().await.map_err(BifrostError::Sql)?;
        }

        // ── 2. Resolve pin (COALESCE(watermark, registered_watermark)) ───
        let pin = {
            let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            let pin = vala_sql::queries::olap_derivations::select_derivation_pin(
                &mut conn,
                &derivation_uid_bytes,
            )
            .await
            .map_err(BifrostError::Sql)?;
            conn.commit().await.map_err(BifrostError::Sql)?;
            pin
        };

        // ── 3. Enumerate committed source batches for this tenant ─────────
        let all_batches = {
            let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            let batches = vala_sql::queries::olap_derivations::list_tenant_committed_batches(
                &mut conn,
                &self.uids.source,
            )
            .await
            .map_err(BifrostError::Sql)?;
            conn.commit().await.map_err(BifrostError::Sql)?;
            batches
        };

        // Filter to batches strictly after the watermark pin (or all batches
        // when pin is None = derive from the beginning).
        let unprocessed = filter_after_pin(all_batches, pin.as_ref());
        if unprocessed.is_empty() {
            return Ok(());
        }

        // ── 4. Mark derivation as in-progress ────────────────────────────
        {
            let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            vala_sql::queries::olap_derivations::mark_deriving(&mut conn, &derivation_uid_bytes)
                .await
                .map_err(BifrostError::Sql)?;
            conn.commit().await.map_err(BifrostError::Sql)?;
        }

        // ── 5. Open target writers (one coordinator per target table) ─────
        let messages_handle = self
            .catalog
            .typed_writer::<MessagesTable>(TableScope::SystemShared, tenant)
            .await?;
        let embeddings_handle = self
            .catalog
            .typed_writer::<EmbeddingsTable>(TableScope::SystemShared, tenant)
            .await?;
        let tool_calls_handle = self
            .catalog
            .typed_writer::<ToolCallsTable>(TableScope::SystemShared, tenant)
            .await?;
        let memory_handle = self
            .catalog
            .typed_writer::<MemoryTable>(TableScope::SystemShared, tenant)
            .await?;

        let batch_result = self
            .process_batches(
                tenant,
                &unprocessed,
                &messages_handle,
                &embeddings_handle,
                &tool_calls_handle,
                &memory_handle,
            )
            .await;

        // ── 7. Extract the last fully-completed batch and any failure ─────
        //
        // `process_batches` returns the last batch_id that was FULLY written
        // (all targets) plus any error that stopped the loop. We advance the
        // watermark to the last successful batch even when an error occurred —
        // that is safe because those batches completed. Then, if an error
        // occurred, mark the derivation failed and propagate.
        let (last_batch_id, batch_err) = match batch_result {
            Ok(last) => (last, None),
            Err((last, err)) => (last, Some(err)),
        };

        // ── 8. Advance watermark to last FULLY-completed batch ────────────
        if let Some(wm) = last_batch_id {
            let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            vala_sql::queries::olap_derivations::advance_watermark(
                &mut conn,
                &derivation_uid_bytes,
                &wm,
            )
            .await
            .map_err(BifrostError::Sql)?;
            conn.commit().await.map_err(BifrostError::Sql)?;
        }

        // ── 9. On failure: persist failed state, then surface the error ───
        if let Some(err) = batch_err {
            // Best-effort: if marking failed itself errors we still propagate
            // the original batch error so the caller sees a real failure.
            if let Ok(mut conn) = vala_sql::TenantConn::acquire(&self.pool, tenant).await {
                let _ = vala_sql::queries::olap_derivations::mark_failed(
                    &mut conn,
                    &derivation_uid_bytes,
                )
                .await;
                let _ = conn.commit().await;
            }
            tracing::error!(
                tenant = %tenant,
                error = %err,
                "derivation: target write failed; derivation marked failed, watermark not advanced past failed batch"
            );
            return Err(err);
        }

        Ok(())
    }

    /// Process all unprocessed source batches for `tenant`.
    ///
    /// Returns `Ok(last_completed_batch_id)` when every batch was fully written.
    /// Returns `Err((last_completed_batch_id, error))` when a target write fails:
    /// `last_completed_batch_id` is the last batch that completed successfully
    /// (all targets written, or an empty scan — both count as fully processed),
    /// and the loop stops at the first failure so no batches after the failed one
    /// are marked as complete.
    ///
    /// Fix 1 (fail-closed): a source position advances ONLY after every required
    /// target write for that batch returns `Ok`. On failure the last fully-
    /// completed batch id is returned so partial progress is preserved.
    ///
    /// Fix 2 (no chunk loss): DataFusion may split one logical source batch across
    /// multiple physical `RecordBatch` chunks. We concatenate them into a single
    /// batch before calling `derive`, so one deterministic `derived_batch_id` per
    /// (source batch, target) covers all rows.
    async fn process_batches(
        &self,
        tenant: DataTenantId,
        batches: &[vala_sql::queries::olap_derivations::CommittedSourceBatch],
        messages_handle: &crate::writer::coordinator::GroupCommitHandle,
        embeddings_handle: &crate::writer::coordinator::GroupCommitHandle,
        tool_calls_handle: &crate::writer::coordinator::GroupCommitHandle,
        memory_handle: &crate::writer::coordinator::GroupCommitHandle,
    ) -> Result<Option<[u8; 16]>, (Option<[u8; 16]>, BifrostError)> {
        let mut last_completed: Option<[u8; 16]> = None;

        for batch in batches {
            let batch_id: [u8; 16] = batch
                .batch_id
                .as_slice()
                .try_into()
                .map_err(|_| {
                    (
                        last_completed,
                        BifrostError::Internal("source batch_id length mismatch".to_string()),
                    )
                })?;

            // ── Scan: collect all physical chunks for this source batch ───
            let chunks = self
                .scan_source_batch(tenant, &batch_id)
                .await
                .map_err(|e| (last_completed, e))?;

            // ── Concatenate chunks into one logical batch (Fix 2) ─────────
            // An empty scan (0 chunks, or chunks with 0 rows total) means this
            // source batch produced no derived rows. Advance the watermark across
            // it — it was fully "processed" — but skip writing any target commit.
            let total_rows: usize = chunks.iter().map(|c| c.num_rows()).sum();
            if total_rows == 0 {
                last_completed = Some(batch_id);
                continue;
            }

            // `concat_batches` requires a schema; all chunks share the source schema.
            let schema = chunks[0].schema();
            let source_batch = concat_batches(&schema, &chunks).map_err(|e| {
                (
                    last_completed,
                    BifrostError::Internal(format!("concat source chunks failed: {e}")),
                )
            })?;

            // ── Derive: exactly one call per source batch (Fix 2) ─────────
            let derived = self
                .derivation
                .derive(&source_batch, batch_id)
                .map_err(|e| {
                    (
                        last_completed,
                        BifrostError::Internal(format!("genai derivation failed: {e}")),
                    )
                })?;

            // ── Write each target; fail-closed on any error (Fix 1) ───────
            for db in derived {
                let target_uid = target_uid_for(db.target_name, &self.uids);
                let ctx = BifrostWriteContext {
                    batch_id: GenAiFromSpans::derived_batch_id(
                        &target_uid,
                        &self.uids.source,
                        &batch_id,
                    ),
                    origin: "derivation:genai".to_owned(),
                    actor: "system".to_owned(),
                    request_id: wyrd_spec::request_id::RequestId::now_v7(),
                    card_ref: None,
                };
                let handle = match db.target_name {
                    MessagesTable::NAME => messages_handle,
                    EmbeddingsTable::NAME => embeddings_handle,
                    ToolCallsTable::NAME => tool_calls_handle,
                    MemoryTable::NAME => memory_handle,
                    other => {
                        tracing::warn!(
                            target = other,
                            "derivation: unknown target table — skipped"
                        );
                        continue;
                    }
                };
                // Fail-closed: propagate the error rather than logging+continuing.
                handle
                    .write(tenant, vec![db.batch], ctx)
                    .await
                    .map_err(|e| (last_completed, e))?;
            }

            // All targets for this batch written — advance the completed cursor.
            last_completed = Some(batch_id);
        }

        Ok(last_completed)
    }

    /// Scan `traces.spans` for one `wyrd_batch_id` and one tenant.
    ///
    /// Registers the table in a per-call `SessionContext` (tenant-filtered by
    /// `TenantPredicateRule`) then issues a `DataFusion` `filter` on the binary
    /// `wyrd_batch_id` column. Returns the collected `RecordBatch`es.
    async fn scan_source_batch(
        &self,
        tenant: DataTenantId,
        batch_id: &[u8; 16],
    ) -> Result<Vec<RecordBatch>, BifrostError> {
        let provider = match self
            .catalog
            .provider(BifrostNamespace::Traces, SpansTable::NAME, tenant)
            .await
        {
            Ok(p) => p,
            Err(BifrostError::TableNotFound(_)) => return Ok(Vec::new()),
            Err(other) => return Err(other),
        };

        let ctx = wyrd_session_context(tenant);
        ctx.register_table(TableReference::bare(SpansTable::NAME), Arc::new(provider))
            .map_err(|e| BifrostError::Internal(e.to_string()))?;

        let batch_id_scalar = ScalarValue::FixedSizeBinary(16, Some(batch_id.to_vec()));
        let filter_expr = col("wyrd_batch_id").eq(lit(batch_id_scalar));

        let batches = ctx
            .table(SpansTable::NAME)
            .await
            .map_err(|e| BifrostError::Internal(e.to_string()))?
            .filter(filter_expr)
            .map_err(|e| BifrostError::Internal(e.to_string()))?
            .collect()
            .await
            .map_err(|e| BifrostError::Internal(e.to_string()))?;

        Ok(batches)
    }
}

/// Return the physical table UID for a derived-batch target name.
fn target_uid_for(target_name: &str, uids: &TableUids) -> [u8; 16] {
    match target_name {
        MessagesTable::NAME => uids.messages,
        EmbeddingsTable::NAME => uids.embeddings,
        ToolCallsTable::NAME => uids.tool_calls,
        MemoryTable::NAME => uids.memory,
        _ => [0u8; 16],
    }
}

/// Return the subset of `batches` that are strictly after the pin.
///
/// When the pin is `None` (never advanced, no registered watermark), ALL
/// batches are returned — derive from the beginning. When the pin is
/// `Some(None)`, same: the registered watermark is also absent, derive from
/// the start. When pin is `Some(Some(wm))`, skip batches up to and including
/// `wm`; return only those after it.
fn filter_after_pin(
    batches: Vec<vala_sql::queries::olap_derivations::CommittedSourceBatch>,
    pin: Option<&Option<Vec<u8>>>,
) -> Vec<vala_sql::queries::olap_derivations::CommittedSourceBatch> {
    match pin {
        // No pin yet — derive everything from the beginning.
        None | Some(None) => batches,
        Some(Some(wm)) => {
            let mut found = false;
            batches
                .into_iter()
                .filter(|b| {
                    if found {
                        return true;
                    }
                    if &b.batch_id == wm {
                        found = true;
                    }
                    false
                })
                .collect()
        }
    }
}
