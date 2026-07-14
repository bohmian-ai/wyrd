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
use sha2::{Digest, Sha256};
use uuid::Uuid;
use vala_sql::row_types::maintenance::MaintenanceLeaseKey;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::WyrdCatalog;
use crate::catalog::namespaces::BifrostNamespace;
use crate::error::BifrostError;
use crate::serving::repair::heartbeat::LeaseHeartbeat;
use crate::session::wyrd_session_context;
use crate::tables::DomainTable;
use crate::tables::genai::{
    DomainDerivation, EmbeddingsTable, GenAiFromSpans, MemoryTable, MessagesTable, ToolCallsTable,
};
use crate::tables::traces::SpansTable;
use crate::types::TableScope;
use crate::writer::BifrostWriteContext;

/// Lease duration for the cross-table derivation maintenance lease.
///
/// Mirrors [`FALLBACK_CADENCE_SECS`](super::FALLBACK_CADENCE_SECS): one full
/// fallback interval fits within one lease period, so a single tick always
/// completes before its own lease expires under normal load.
const LEASE_SECS: i64 = 60;

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
///
/// `worker_owner` is a stable per-process UUID used as the maintenance-lease
/// owner identity. It must be generated once per spawned worker (not per tick)
/// so the owner field stays stable across renewals.
pub(super) struct DerivationRuntime {
    pub catalog: Arc<WyrdCatalog>,
    pub pool: sqlx::PgPool,
    pub op: vala_sql::OperatorPool,
    pub uids: TableUids,
    pub derivation: GenAiFromSpans,
    pub worker_owner: Uuid,
}

/// Open group-commit handles for every genai target table. Held together so
/// [`process_tenant`](DerivationRuntime::process_tenant) can pass them into
/// [`process_batches`](DerivationRuntime::process_batches) as one bundle.
struct TargetHandles {
    messages: crate::writer::coordinator::GroupCommitHandle,
    embeddings: crate::writer::coordinator::GroupCommitHandle,
    tool_calls: crate::writer::coordinator::GroupCommitHandle,
    memory: crate::writer::coordinator::GroupCommitHandle,
}

impl DerivationRuntime {
    /// Enumerate tenants with committed source data and run a derivation pass
    /// for each. Per-tenant errors are logged but do not abort other tenants.
    ///
    /// Acquires the shared cross-table-derivation maintenance lease before
    /// enumerating tenants. If another pod already holds the lease this tick
    /// returns `Ok(())` immediately (fail-closed singleton). The lease is
    /// renewed between tenants and again inside
    /// [`fenced_advance_watermark`](Self::fenced_advance_watermark) before every
    /// watermark advance.
    pub(super) async fn run_tick(&self) -> Result<(), BifrostError> {
        let lease_key = MaintenanceLeaseKey::cross_table_derivation(
            &GenAiFromSpans::DERIVATION_UID,
            &Uuid::nil(),
        );
        let token = vala_sql::queries::maintenance_leases::try_acquire_lease(
            &self.op,
            lease_key.as_str(),
            self.worker_owner,
            LEASE_SECS,
        )
        .await
        .map_err(BifrostError::Sql)?;
        let Some(fencing_token) = token else {
            tracing::debug!(
                owner = %self.worker_owner,
                "derivation: lease held by another pod — skipping tick"
            );
            return Ok(());
        };
        let heartbeat = LeaseHeartbeat::new(self.worker_owner, fencing_token, lease_key.as_str());

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
            match heartbeat.renew(&self.op, LEASE_SECS).await {
                Ok(true) => {}
                Ok(false) => {
                    tracing::warn!(
                        owner = %self.worker_owner,
                        tenant = %tenant,
                        "derivation: lease lost between tenants — aborting tick"
                    );
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(
                        owner = %self.worker_owner,
                        error = %e,
                        "derivation: lease renewal error between tenants — aborting tick"
                    );
                    return Err(BifrostError::Sql(e));
                }
            }
            if let Err(e) = self.process_tenant(tenant, &heartbeat).await {
                tracing::error!(tenant = %tenant, error = %e, "derivation: tenant tick failed");
            }
        }
        Ok(())
    }

    async fn process_tenant(
        &self,
        tenant: DataTenantId,
        heartbeat: &LeaseHeartbeat,
    ) -> Result<(), BifrostError> {
        let derivation_uid_bytes: [u8; 16] = *GenAiFromSpans::DERIVATION_UID.as_bytes();

        if !self.check_contract(tenant, &derivation_uid_bytes).await? {
            return Ok(());
        }
        let pin = self.load_pin(tenant, &derivation_uid_bytes).await?;
        let all_batches = self.load_committed_batches(tenant).await?;

        // Filter to batches strictly after the watermark pin (or all batches
        // when pin is None = derive from the beginning).
        let unprocessed = filter_after_pin(all_batches, pin.as_ref());
        if unprocessed.is_empty() {
            return Ok(());
        }

        self.mark_deriving_tx(tenant, &derivation_uid_bytes).await?;

        let handles = self.open_target_writers(tenant).await?;
        let batch_result = self
            .process_batches(
                tenant,
                &unprocessed,
                &handles.messages,
                &handles.embeddings,
                &handles.tool_calls,
                &handles.memory,
            )
            .await;

        // `process_batches` returns the last batch_id that was FULLY written
        // (all targets) plus any error that stopped the loop. Advance the
        // watermark to the last successful batch even when an error occurred —
        // those batches completed. Then, if an error occurred, mark the
        // derivation failed and propagate.
        let (last_batch_id, batch_err) = match batch_result {
            Ok(last) => (last, None),
            Err((last, err)) => (last, Some(err)),
        };

        if let Some(wm) = last_batch_id {
            self.fenced_advance_watermark(tenant, heartbeat, &derivation_uid_bytes, &wm)
                .await?;
        }

        if let Some(err) = batch_err {
            self.mark_failed_best_effort(tenant, &derivation_uid_bytes)
                .await;
            tracing::error!(
                tenant = %tenant,
                error = %err,
                "derivation: target write failed; derivation marked failed, watermark not advanced past failed batch"
            );
            return Err(err);
        }

        Ok(())
    }

    /// Register the derivation with immutable contract fingerprints and check
    /// for drift.
    ///
    /// Returns `Ok(true)` when processing should proceed
    /// ([`ContractOutcome::Registered`] or [`ContractOutcome::IdenticalReplay`]).
    /// Returns `Ok(false)` when a contract drift was detected
    /// ([`ContractOutcome::DriftRejected`]); the caller must skip this tenant to
    /// prevent stale-watermark reuse under changed semantics. Drift is a
    /// control-plane rejection, not a runtime failure, so we do not mark the
    /// derivation `failed`.
    ///
    /// Fingerprint algorithms:
    /// - `source_schema_fingerprint`: [`SpansTable::schema_fingerprint`] —
    ///   SHA-256 of declared user Arrow fields (name || NUL || `data_type` ||
    ///   NUL per field).
    /// - `transform_fingerprint`: SHA-256 of
    ///   [`GenAiFromSpans::CONTRACT_VERSION`].
    /// - `target_set_fingerprint`: SHA-256 of target UIDs concatenated in
    ///   lexicographic sort order.
    ///
    /// [`ContractOutcome::Registered`]: vala_sql::queries::olap_derivations::ContractOutcome::Registered
    /// [`ContractOutcome::IdenticalReplay`]: vala_sql::queries::olap_derivations::ContractOutcome::IdenticalReplay
    /// [`ContractOutcome::DriftRejected`]: vala_sql::queries::olap_derivations::ContractOutcome::DriftRejected
    async fn check_contract(
        &self,
        tenant: DataTenantId,
        derivation_uid_bytes: &[u8; 16],
    ) -> Result<bool, BifrostError> {
        let source_schema_fingerprint = SpansTable::schema_fingerprint();

        let transform_fingerprint: [u8; 32] = {
            let mut h = Sha256::new();
            h.update(GenAiFromSpans::CONTRACT_VERSION.as_bytes());
            h.finalize().into()
        };

        let target_set_fingerprint: [u8; 32] = {
            let mut target_uids = [
                self.uids.messages,
                self.uids.embeddings,
                self.uids.tool_calls,
                self.uids.memory,
            ];
            target_uids.sort_unstable();
            let mut h = Sha256::new();
            for uid in &target_uids {
                h.update(uid.as_slice());
            }
            h.finalize().into()
        };

        let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
            .await
            .map_err(BifrostError::Sql)?;
        let outcome = vala_sql::queries::olap_derivations::register_derivation_with_contract(
            &mut conn,
            derivation_uid_bytes,
            &self.uids.source,
            // target_table_uid records the "primary" target for registration;
            // full target-set semantics live in target_set_fingerprint.
            &self.uids.messages,
            Uuid::nil(),
            "genai_from_spans",
            None,
            vala_sql::queries::olap_derivations::DerivationContract {
                source_schema_fingerprint: &source_schema_fingerprint,
                transform_fingerprint: &transform_fingerprint,
                target_set_fingerprint: &target_set_fingerprint,
            },
        )
        .await
        .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;

        if let vala_sql::queries::olap_derivations::ContractOutcome::DriftRejected { differing } =
            outcome
        {
            tracing::error!(
                tenant = %tenant,
                differing = ?differing,
                "derivation contract drift; skipping tenant to prevent stale watermark reuse"
            );
            return Ok(false);
        }

        Ok(true)
    }

    /// Renew the maintenance lease then advance the watermark — fail-closed.
    ///
    /// Skips the watermark advance and returns `Ok(())` if the lease was lost:
    /// no watermark ever moves on behalf of a stale holder.
    async fn fenced_advance_watermark(
        &self,
        tenant: DataTenantId,
        heartbeat: &LeaseHeartbeat,
        derivation_uid_bytes: &[u8; 16],
        wm: &[u8; 16],
    ) -> Result<(), BifrostError> {
        match heartbeat.renew(&self.op, LEASE_SECS).await {
            Ok(true) => {}
            Ok(false) => {
                tracing::error!(
                    owner = %self.worker_owner,
                    tenant = %tenant,
                    "derivation: lease lost before watermark advance — skipping (fail-closed)"
                );
                return Ok(());
            }
            Err(e) => return Err(BifrostError::Sql(e)),
        }
        self.advance_watermark_tx(tenant, derivation_uid_bytes, wm)
            .await
    }

    async fn load_pin(
        &self,
        tenant: DataTenantId,
        derivation_uid_bytes: &[u8; 16],
    ) -> Result<Option<Option<Vec<u8>>>, BifrostError> {
        let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
            .await
            .map_err(BifrostError::Sql)?;
        let pin = vala_sql::queries::olap_derivations::select_derivation_pin(
            &mut conn,
            derivation_uid_bytes,
        )
        .await
        .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;
        Ok(pin)
    }

    async fn load_committed_batches(
        &self,
        tenant: DataTenantId,
    ) -> Result<Vec<vala_sql::queries::olap_derivations::CommittedSourceBatch>, BifrostError> {
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
        Ok(batches)
    }

    async fn mark_deriving_tx(
        &self,
        tenant: DataTenantId,
        derivation_uid_bytes: &[u8; 16],
    ) -> Result<(), BifrostError> {
        let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
            .await
            .map_err(BifrostError::Sql)?;
        vala_sql::queries::olap_derivations::mark_deriving(&mut conn, derivation_uid_bytes)
            .await
            .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)
    }

    async fn advance_watermark_tx(
        &self,
        tenant: DataTenantId,
        derivation_uid_bytes: &[u8; 16],
        wm: &[u8; 16],
    ) -> Result<(), BifrostError> {
        let mut conn = vala_sql::TenantConn::acquire(&self.pool, tenant)
            .await
            .map_err(BifrostError::Sql)?;
        vala_sql::queries::olap_derivations::advance_watermark(&mut conn, derivation_uid_bytes, wm)
            .await
            .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)
    }

    /// Best-effort `mark_failed`: if marking failed itself errors we still let the
    /// caller propagate the original batch error, so the caller sees a real
    /// failure. The watermark stays where it was.
    async fn mark_failed_best_effort(&self, tenant: DataTenantId, derivation_uid_bytes: &[u8; 16]) {
        if let Ok(mut conn) = vala_sql::TenantConn::acquire(&self.pool, tenant).await {
            let _ =
                vala_sql::queries::olap_derivations::mark_failed(&mut conn, derivation_uid_bytes)
                    .await;
            let _ = conn.commit().await;
        }
    }

    async fn open_target_writers(
        &self,
        tenant: DataTenantId,
    ) -> Result<TargetHandles, BifrostError> {
        Ok(TargetHandles {
            messages: self
                .catalog
                .typed_writer::<MessagesTable>(TableScope::SystemShared, tenant)
                .await?,
            embeddings: self
                .catalog
                .typed_writer::<EmbeddingsTable>(TableScope::SystemShared, tenant)
                .await?,
            tool_calls: self
                .catalog
                .typed_writer::<ToolCallsTable>(TableScope::SystemShared, tenant)
                .await?,
            memory: self
                .catalog
                .typed_writer::<MemoryTable>(TableScope::SystemShared, tenant)
                .await?,
        })
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
    /// Fail-closed invariant: a source position advances ONLY after every
    /// required target write for that batch returns `Ok`. On failure the last
    /// fully-completed batch id is returned so partial progress is preserved
    /// and the failed batch retries on the next tick.
    ///
    /// Chunk-safety invariant: `DataFusion` may split one logical source batch
    /// across multiple physical `RecordBatch` chunks. We concatenate them into
    /// a single batch before calling `derive`, so one deterministic
    /// `derived_batch_id` per (source batch, target) covers all rows and no
    /// chunk gets its own idempotency key.
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
            let batch_id: [u8; 16] = batch.batch_id.as_slice().try_into().map_err(|_| {
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

            // ── Concatenate physical chunks into one logical batch ────────
            // An empty scan (0 chunks, or chunks with 0 rows total) means this
            // source batch produced no derived rows. Advance the watermark across
            // it — it was fully "processed" — but skip writing any target commit.
            let total_rows: usize = chunks.iter().map(RecordBatch::num_rows).sum();
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

            // ── Derive: exactly one call per source batch ─────────────────
            let derived = self
                .derivation
                .derive(&source_batch, batch_id)
                .map_err(|e| {
                    (
                        last_completed,
                        BifrostError::Internal(format!("genai derivation failed: {e}")),
                    )
                })?;

            // ── Write each target; fail-closed on any error ───────────────
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
