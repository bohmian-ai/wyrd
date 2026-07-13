//! Core derivation-runtime logic: source-delta scan → transform → target write.
//!
//! [`DerivationRuntime`] owns the pre-fetched table UIDs and orchestrates one
//! tick: it enumerates tenants with committed source data, then calls
//! [`process_tenant`](DerivationRuntime::process_tenant) per tenant to consume
//! the unprocessed delta and advance the watermark.

use std::sync::Arc;

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

        fn uid(row: &vala_sql::row_types::olap_catalog::BifrostTableRow) -> Result<[u8; 16], BifrostError> {
            row.table_uid
                .as_slice()
                .try_into()
                .map_err(|_| BifrostError::Internal("table_uid length mismatch".to_string()))
        }

        Ok(Self {
            source: uid(&src.row)?,
            messages: uid(&msg.row)?,
            embeddings: uid(&emb.row)?,
            tool_calls: uid(&tc.row)?,
            memory: uid(&mem.row)?,
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
        let tenant_uuids =
            vala_sql::queries::olap_derivations::list_source_commit_tenants(
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
        let unprocessed = filter_after_pin(all_batches, &pin);
        if unprocessed.is_empty() {
            return Ok(());
        }

        // ── 4. Mark derivation as in-progress ────────────────────────────
        {
            let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            vala_sql::queries::olap_derivations::mark_deriving(
                &mut conn,
                &derivation_uid_bytes,
            )
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

        let mut last_batch_id: Option<[u8; 16]> = None;

        // ── 6. Process each unprocessed source batch ──────────────────────
        for batch in &unprocessed {
            let batch_id_arr: [u8; 16] = batch
                .batch_id
                .as_slice()
                .try_into()
                .map_err(|_| BifrostError::Internal("source batch_id length mismatch".to_string()))?;

            let source_records = self.scan_source_batch(tenant, &batch_id_arr).await?;
            for source_record in &source_records {
                let derived_batches = self
                    .derivation
                    .derive(source_record, batch_id_arr)
                    .map_err(|e| {
                        BifrostError::Internal(format!("genai derivation failed: {e}"))
                    })?;

                for db in derived_batches {
                    let target_uid = target_uid_for(
                        db.target_name,
                        &self.uids,
                    );
                    let derived_batch_id = GenAiFromSpans::derived_batch_id(
                        &target_uid,
                        &self.uids.source,
                        &batch_id_arr,
                    );
                    let ctx = BifrostWriteContext {
                        batch_id: derived_batch_id,
                        origin: "derivation:genai".to_owned(),
                        actor: "system".to_owned(),
                        request_id: wyrd_spec::request_id::RequestId::now_v7(),
                        card_ref: None,
                    };

                    let handle = match db.target_name {
                        MessagesTable::NAME => &messages_handle,
                        EmbeddingsTable::NAME => &embeddings_handle,
                        ToolCallsTable::NAME => &tool_calls_handle,
                        MemoryTable::NAME => &memory_handle,
                        other => {
                            tracing::warn!(target = other, "derivation: unknown target table — skipped");
                            continue;
                        }
                    };

                    if let Err(e) = handle.write(tenant, vec![db.batch], ctx).await {
                        tracing::error!(
                            target = db.target_name,
                            tenant = %tenant,
                            error = %e,
                            "derivation: target write failed"
                        );
                    }
                }
            }

            last_batch_id = Some(batch_id_arr);
        }

        // ── 7. Advance watermark to last processed batch ──────────────────
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

        Ok(())
    }

    /// Scan `traces.spans` for one `wyrd_batch_id` and one tenant.
    ///
    /// Registers the table in a per-call `SessionContext` (tenant-filtered by
    /// `TenantPredicateRule`) then issues a DataFusion `filter` on the binary
    /// `wyrd_batch_id` column. Returns the collected `RecordBatch`es.
    async fn scan_source_batch(
        &self,
        tenant: DataTenantId,
        batch_id: &[u8; 16],
    ) -> Result<Vec<arrow::array::RecordBatch>, BifrostError> {
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
        ctx.register_table(
            TableReference::bare(SpansTable::NAME),
            Arc::new(provider),
        )
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
    pin: &Option<Option<Vec<u8>>>,
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
