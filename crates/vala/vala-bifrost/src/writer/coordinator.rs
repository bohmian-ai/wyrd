use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use arrow::array::RecordBatch;
use arrow::array::{Array, FixedSizeBinaryArray, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::DataType;
use chrono::{DateTime, Utc};
use iceberg::table::Table;
use iceberg_catalog_sql::SqlCatalog;
use sqlx::PgPool;
use tokio::sync::mpsc;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
use wyrd_spec::vala::system_columns::{WYRD_EVENT_TIME, is_reserved_system_column};

use crate::batch_builder::stamp_system_columns;
use crate::error::BifrostError;
use crate::registry::Registry;
use crate::tables::{EntityBoundsMapping, PayloadClass};
use crate::types::{TableScope, TableUid};
use crate::writer::buffer::AppendBuffer;
use crate::writer::commit::run_commit;
use crate::writer::redaction::{BuiltinRedactionPass, RedactionClassifier};
use crate::writer::{BifrostWriteContext, TableWriterHandle, WriteCmd};

/// The audit-outbox `origin` that marks the relay's own write into
/// `vala.system.audit_log`. A commit under this origin MUST NOT append an audit
/// row, or the relay would self-feed the spine (review M-11).
pub(crate) const AUDIT_RELAY_ORIGIN: &str = "audit-relay";

/// Build the C1 ingest-commit [`AuditEvent`] from the writer context.
///
/// The Principal is not reachable this deep in the write path (S3.C5 seam), so
/// the attribution is reconstructed from the [`BifrostWriteContext`] the C1
/// orchestrator set: `principal_id` parses `actor`, the writer-identity
/// `card_ref` carries the principal kind (Service/Agent), and a card-less write
/// is a `User`. An internal record write authenticates as `Internal`. `result`
/// is `Success` because this event is only appended on the finalized commit.
fn ingest_audit_event(ctx: &BifrostWriteContext, resource: &str) -> AuditEvent {
    let principal_kind = match ctx.card_ref.as_ref() {
        Some(card) if card.kind == CardKind::Agent => PrincipalKindTag::Agent,
        Some(_) => PrincipalKindTag::Service,
        None => PrincipalKindTag::User,
    };
    let principal_id = ctx
        .actor
        .parse::<PrincipalId>()
        .unwrap_or_else(|_| PrincipalId::new(uuid::Uuid::nil()));

    AuditEvent {
        request_id: ctx.request_id.clone(),
        trace_id: None,
        operation: format!("bifrost.{}.commit", ctx.origin),
        resource: resource.to_string(),
        card_ref: ctx.card_ref.clone(),
        principal_id,
        principal_kind,
        auth_method: AuthMethod::Internal,
        permission: "bifrost.record_write".to_string(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: "bifrost record commit".to_string(),
    }
}

struct CommitActor {
    receiver: mpsc::Receiver<WriteCmd>,
    table: Table,
    catalog: Arc<SqlCatalog>,
    pool: Arc<PgPool>,
    table_uid: TableUid,
    scope: TableScope,
    /// Authenticated **data tenant** for this writer handle. Server-stamped into
    /// `data_tenant_id` on `SystemShared` rows; the control-plane RLS bind for the
    /// `vala.olap_commits` precommit/finalize rows is *derived* from it via
    /// `scope.control_bind`, never conflated with it (C2/N-M12).
    ///
    /// This is per-handle today: one coordinator is spawned per `writer()` call,
    /// so a handle (and its actor) serves a single data tenant. When the shared
    /// per-physical-table coordinator (M5/D3/M16) lands, the data tenant must move
    /// onto `WriteCmd::Write` so one actor can stamp many tenants' writes. Design
    /// (queue, backpressure, flush timing, compaction, single/multi-tenant) is not
    /// yet locked — see
    /// `.dev/plan/foundations/12-olap-warehouse/stage5/07-write-path-group-commit.md`.
    data_tenant: DataTenantId,
    buffer: AppendBuffer,
    registry: Arc<Registry>,
    payload_class: PayloadClass,
    sensitive_columns: &'static [&'static str],
    /// Optional entity → time bounds mapping for best-effort acceleration (M-06).
    /// When `Some`, each successful flush upserts per-entity min/max event time
    /// into `vala.entity_time_bounds` in a separate best-effort transaction.
    entity_bounds_mapping: Option<EntityBoundsMapping>,
}

/// Microseconds since the Unix epoch, used for the server-stamped
/// `wyrd_event_time` / `wyrd_ingested_at` system columns.
fn now_micros() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
}

impl CommitActor {
    /// Serially process [`WriteCmd`]s until the channel closes. Serial execution
    /// is the concurrency boundary: one actor per table means buffered batches
    /// and the flush commit never race.
    async fn run(mut self) {
        while let Some(cmd) = self.receiver.recv().await {
            match cmd {
                WriteCmd::Write(batch, reply) => {
                    let _ = reply.send(self.accept(batch));
                }
                WriteCmd::Flush(ctx, reply) => {
                    let result = self.flush(*ctx).await;
                    let _ = reply.send(result);
                }
            }
        }
    }

    /// Validate and buffer a caller batch. Callers supply user fields only — the
    /// server stamps every system column (`wyrd_event_time`, `wyrd_ingested_at`,
    /// `wyrd_batch_id`, and `data_tenant_id` on `SystemShared`). A caller batch that
    /// already carries any reserved system column is rejected (MAJOR-9 step 3):
    /// the server-stamped value must be the sole source, never coexisting with a
    /// caller-supplied duplicate.
    fn accept(&mut self, batch: RecordBatch) -> Result<(), BifrostError> {
        for field in batch.schema().fields() {
            if is_reserved_system_column(field.name()) {
                return Err(BifrostError::ReservedColumn(field.name().clone()));
            }
        }
        self.buffer.push(batch);
        Ok(())
    }

    /// Stamp every buffered batch with the system columns and commit them as one
    /// 2PC transaction. On any failure the drained batches are restored to the
    /// buffer so the caller can retry. An empty buffer is a no-op returning `0`.
    async fn flush(&mut self, ctx: BifrostWriteContext) -> Result<i64, BifrostError> {
        if self.buffer.is_empty() {
            return Ok(0);
        }

        let batches = self.buffer.drain();
        let batch_id = ctx.batch_id;
        let ingested_at_us = now_micros();
        let stamp_tenant = self.scope.stamp_tenant(self.data_tenant);

        // Audit attribution for the transactional outbox append (S3.C5). The
        // relay's own write (`origin == "audit-relay"`) is skipped so it never
        // self-feeds the spine (M-11); every other commit appends one row in the
        // same tx as the finalize.
        let audit = if ctx.origin == AUDIT_RELAY_ORIGIN {
            None
        } else {
            Some(ingest_audit_event(
                &ctx,
                &self.table.identifier().to_string(),
            ))
        };

        let stamped: Vec<RecordBatch> = match batches
            .iter()
            .map(|b| stamp_system_columns(b, ingested_at_us, batch_id, stamp_tenant))
            .collect::<Result<_, _>>()
            .map_err(BifrostError::Arrow)
        {
            Ok(stamped) => stamped,
            Err(e) => {
                self.buffer = AppendBuffer::from_vec(batches);
                return Err(e);
            }
        };

        // M-03: for Sensitive tables run the built-in redaction pass over
        // SENSITIVE_PAYLOAD_COLUMNS before committing. A classifier error
        // refuses the commit (WYRD_VALA_500_REDACTION_FAILED).
        let stamped = if self.payload_class == PayloadClass::Sensitive
            && !self.sensitive_columns.is_empty()
        {
            let pass = BuiltinRedactionPass;
            match stamped
                .iter()
                .map(|b| pass.scrub(b, self.sensitive_columns))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(redacted) => redacted,
                Err(e) => {
                    self.buffer = AppendBuffer::from_vec(batches);
                    return Err(e);
                }
            }
        } else {
            stamped
        };

        // Extract entity bounds before run_commit consumes `stamped` (best-effort M-06).
        let entity_bounds: Vec<(String, DateTime<Utc>, DateTime<Utc>)> =
            if let Some(mapping) = &self.entity_bounds_mapping {
                extract_entity_bounds(&stamped, &mapping.entity_id_column)
            } else {
                vec![]
            };

        match run_commit(
            &self.pool,
            &self.catalog,
            &self.table,
            &self.table_uid,
            stamped,
            batch_id,
            &ctx.origin,
            &ctx.actor,
            self.scope.control_bind(self.data_tenant),
            audit,
        )
        .await
        {
            Ok((snapshot_id, updated_table)) => {
                // Adopt the post-commit table snapshot so the next flush bases its
                // transaction on the current ref (MAJOR-2). The idempotent-replay
                // path returns `None` and leaves the snapshot untouched.
                if let Some(table) = updated_table {
                    self.table = table;
                    // Bump refresh_epochs + evict cache so other pods see the new
                    // snapshot on their next get(). Best-effort: a failure here
                    // means the next reader pays a full reload, not correctness.
                    let key = crate::registry::RegistryKey {
                        owner: self.scope.control_bind(self.data_tenant),
                        table_uid: self.table_uid,
                    };
                    if let Err(e) = self.registry.invalidate(key).await {
                        tracing::warn!(
                            error = %e,
                            "epoch bump after commit failed (cache may be stale)"
                        );
                    }
                }

                // Best-effort entity_time_bounds upsert (M-06).
                if !entity_bounds.is_empty()
                    && let Some(mapping) = &self.entity_bounds_mapping
                {
                    spawn_entity_bounds_upsert(
                        Arc::clone(&self.pool),
                        self.data_tenant,
                        self.table_uid.0,
                        mapping.entity_kind.clone(),
                        entity_bounds,
                    );
                }

                Ok(snapshot_id)
            }
            Err(e) => {
                self.buffer = AppendBuffer::from_vec(batches);
                Err(e)
            }
        }
    }
}

fn spawn_entity_bounds_upsert(
    pool: Arc<PgPool>,
    data_tenant: DataTenantId,
    table_uid_arr: [u8; 16],
    entity_kind: String,
    entity_bounds: Vec<(String, DateTime<Utc>, DateTime<Utc>)>,
) {
    tokio::spawn(async move {
        match vala_sql::TenantConn::acquire(&pool, data_tenant).await {
            Ok(mut conn) => {
                let refs: Vec<(&str, DateTime<Utc>, DateTime<Utc>)> = entity_bounds
                    .iter()
                    .map(|(id, min_t, max_t)| (id.as_str(), *min_t, *max_t))
                    .collect();
                if let Err(e) = vala_sql::queries::olap_catalog::record_entity_bounds(
                    &mut conn,
                    &table_uid_arr,
                    &entity_kind,
                    &refs,
                )
                .await
                {
                    tracing::warn!(error = %e, "entity_time_bounds upsert failed (best-effort)");
                    return;
                }
                if let Err(e) = conn.commit().await {
                    tracing::warn!(error = %e, "entity_time_bounds commit failed (best-effort)");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "entity_time_bounds connection failed (best-effort)");
            }
        }
    });
}

/// Extract per-entity (min, max) `wyrd_event_time` bounds from a slice of committed
/// batches. Entities are identified by `entity_id_column` (Utf8 or FixedSizeBinary(16)).
/// Binary values are hex-encoded. Rows where either the entity column or the timestamp
/// is null are skipped. Returns an empty vec when the column is missing or the type is
/// unsupported (caller treats a missing result as a bounds miss, not an error).
fn extract_entity_bounds(
    batches: &[RecordBatch],
    entity_id_column: &str,
) -> Vec<(String, DateTime<Utc>, DateTime<Utc>)> {
    let mut bounds: HashMap<String, (DateTime<Utc>, DateTime<Utc>)> = HashMap::new();

    for batch in batches {
        let Some(id_idx) = batch.schema().index_of(entity_id_column).ok() else {
            continue;
        };
        let Some(ts_idx) = batch.schema().index_of(WYRD_EVENT_TIME).ok() else {
            continue;
        };

        let id_col = batch.column(id_idx);
        let ts_col = batch.column(ts_idx);

        let Some(ts_arr) = ts_col.as_any().downcast_ref::<TimestampMicrosecondArray>() else {
            continue;
        };

        for row in 0..batch.num_rows() {
            if ts_arr.is_null(row) || id_col.is_null(row) {
                continue;
            }
            let Some(entity_id) = entity_id_str(id_col.as_ref(), row) else {
                continue;
            };
            let ts_us = ts_arr.value(row);
            let Some(ts) = DateTime::<Utc>::from_timestamp_micros(ts_us) else {
                continue;
            };
            let entry = bounds.entry(entity_id).or_insert((ts, ts));
            if ts < entry.0 {
                entry.0 = ts;
            }
            if ts > entry.1 {
                entry.1 = ts;
            }
        }
    }

    bounds
        .into_iter()
        .map(|(id, (min, max))| (id, min, max))
        .collect()
}

fn entity_id_str(col: &dyn Array, row: usize) -> Option<String> {
    match col.data_type() {
        DataType::Utf8 => col
            .as_any()
            .downcast_ref::<StringArray>()
            .map(|a| a.value(row).to_string()),
        DataType::FixedSizeBinary(_) => col
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .map(|a| hex::encode(a.value(row))),
        _ => None,
    }
}

/// Spawn the per-table commit actor and return a [`TableWriterHandle`] for it.
///
/// One actor is spawned per `writer()` call, each bound to a single data tenant
/// (see [`CommitActor::data_tenant`]). The actor owns the loaded [`Table`] and
/// advances its snapshot as flushes commit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_commit_coordinator(
    table: Table,
    catalog: Arc<SqlCatalog>,
    pool: Arc<PgPool>,
    table_uid: TableUid,
    table_fqn: String,
    scope: TableScope,
    data_tenant: DataTenantId,
    registry: Arc<Registry>,
    payload_class: PayloadClass,
    sensitive_columns: &'static [&'static str],
    entity_bounds_mapping: Option<EntityBoundsMapping>,
) -> TableWriterHandle {
    let (sender, receiver) = mpsc::channel(64);

    let actor = CommitActor {
        receiver,
        table,
        catalog,
        pool,
        table_uid,
        scope,
        data_tenant,
        buffer: AppendBuffer::new(),
        registry,
        payload_class,
        sensitive_columns,
        entity_bounds_mapping,
    };

    tokio::spawn(actor.run());

    TableWriterHandle { sender, table_fqn }
}
