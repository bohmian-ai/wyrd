use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use arrow::array::RecordBatch;
use iceberg::table::Table;
use iceberg_catalog_sql::SqlCatalog;
use sqlx::PgPool;
use tokio::sync::mpsc;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
use wyrd_spec::vala::system_columns::is_reserved_system_column;

use crate::batch_builder::stamp_system_columns;
use crate::error::BifrostError;
use crate::registry::Registry;
use crate::types::{TableScope, TableUid};
use crate::writer::buffer::AppendBuffer;
use crate::writer::commit::run_commit;
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
                Ok(snapshot_id)
            }
            Err(e) => {
                self.buffer = AppendBuffer::from_vec(batches);
                Err(e)
            }
        }
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
    };

    tokio::spawn(actor.run());

    TableWriterHandle { sender, table_fqn }
}
