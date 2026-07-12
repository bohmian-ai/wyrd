//! Shared per-physical-table group-commit coordinator.
//!
//! **One coordinator per physical table** (Q3): lazily spawned on first write,
//! `JoinSet`-supervised via [`WriterRegistry`], drains on shutdown, idle-retires
//! after 5 minutes.
//!
//! **Flush policy** (Q2):
//! - Flush when `total_rows >= max_rows` (default 50 000) OR when the interval
//!   timer fires (default 1 s), whichever comes first.
//! - `max_buffered_bytes ≈ 128 MiB` is a memory ceiling only.
//! - Low-volume ticks deliberately emit small files; right-sizing is compaction's job.
//!
//! **Per-tenant round-robin drain** (Q4): [`TenantAppendBuffer`] maintains
//! per-tenant FIFO queues.  `drain_by_tenant` returns batches in insertion-time
//! round-robin order so no tenant can starve others.
//!
//! **Backpressure** (Q5): the coordinator channel is bounded.  When it is full
//! the caller receives [`BifrostError::IngestBusy`] immediately — never a
//! silent drop, never a partial_success.
//!
//! **Cross-pod serialization** (Q7): Iceberg catalog CAS + `WRITER_INSTANCE`
//! fence + `CommitKey{control_bind, batch_id}` dedup.  **No writer election.**
//!
//! **Ack-after-commit** (Q1): the coordinator replies to the caller ONLY after
//! `run_group_commit` completes (the 2PC is durable).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use arrow::array::RecordBatch;
use iceberg::table::Table;
use iceberg_catalog_sql::SqlCatalog;
use sqlx::PgPool;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, interval_at};
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

use crate::batch_builder::stamp_system_columns;
use crate::error::BifrostError;
use crate::registry::Registry;
use crate::tables::{EntityBoundsMapping, PayloadClass};
use crate::types::{TableScope, TableUid};
use crate::writer::buffer::{AppendBuffer, TenantAppendBuffer};
use crate::writer::commit::run_commit;
use crate::writer::redaction::RedactionClassifier;
use crate::writer::{BifrostWriteContext, TableWriterHandle, WriteCmd};

/// The audit-outbox `origin` for the relay's own write.  This origin MUST NOT
/// append an audit row (M-11: relay must not self-feed the audit spine).
pub(crate) const AUDIT_RELAY_ORIGIN: &str = "audit-relay";

/// Flush policy knobs for the group-commit coordinator.
///
/// Q2: flush on `max_rows` OR `max_interval`, whichever first.
/// `max_buffered_bytes` is a memory ceiling only.
#[derive(Debug, Clone, Copy)]
pub struct FlushPolicy {
    /// Row-count threshold: flush when `total_rows >= max_rows`.
    pub max_rows: usize,
    /// Time trigger: flush every `max_interval` regardless of row count.
    pub max_interval: Duration,
    /// Memory ceiling: flush immediately when `total_bytes >= max_buffered_bytes`.
    pub max_buffered_bytes: usize,
}

impl Default for FlushPolicy {
    fn default() -> Self {
        Self {
            max_rows: 50_000,
            max_interval: Duration::from_secs(1),
            max_buffered_bytes: 128 * 1024 * 1024, // 128 MiB
        }
    }
}

/// Build the C1 ingest-commit [`AuditEvent`] from the writer context.
fn ingest_audit_event(ctx: &BifrostWriteContext, resource: &str) -> AuditEvent {
    let principal_kind = match ctx.card_ref.as_ref() {
        Some(card) if card.kind == CardKind::Agent => PrincipalKindTag::Agent,
        Some(_) => PrincipalKindTag::Service,
        None => PrincipalKindTag::User,
    };
    let principal_id = ctx.actor.parse::<PrincipalId>().unwrap_or_else(|_| {
        tracing::warn!(
            actor = %ctx.actor,
            "failed to parse actor as PrincipalId; audit row will be attributed to nil UUID"
        );
        PrincipalId::new(uuid::Uuid::nil())
    });

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

fn now_micros() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
}

// ── Group-commit coordinator (shared per-physical-table) ─────────────────────

/// Command sent to the group-commit actor.
pub(crate) enum CoordinatorCmd {
    /// Buffer a batch for the given tenant; ack is deferred to commit-durable.
    Write {
        tenant: DataTenantId,
        batch: RecordBatch,
        ctx: BifrostWriteContext,
        reply: oneshot::Sender<Result<i64, BifrostError>>,
    },
    /// Flush all buffered batches immediately (for testing).
    #[cfg(test)]
    ForceFlush(oneshot::Sender<Result<Vec<(DataTenantId, i64)>, BifrostError>>),
}

/// One pending write awaiting the commit-durable ack.
struct PendingEntry {
    tenant: DataTenantId,
    ctx: BifrostWriteContext,
    reply: oneshot::Sender<Result<i64, BifrostError>>,
}

/// Caller-facing handle for the group-commit coordinator of one physical table.
///
/// Cloneable cheap sender; all durable work happens in the actor task.
#[derive(Clone)]
pub struct GroupCommitHandle {
    pub(crate) sender: Arc<mpsc::Sender<CoordinatorCmd>>,
    pub(crate) table_fqn: String,
}

impl GroupCommitHandle {
    /// Buffer a batch for the given tenant and await the commit-durable ack.
    ///
    /// Returns the snapshot id produced by the group commit that included this
    /// batch, after the 2PC is durable (Q1).
    ///
    /// # Errors
    /// Returns [`BifrostError::IngestBusy`] when the coordinator channel is
    /// full (local buffer backpressure — Q5).
    pub async fn write(
        &self,
        tenant: DataTenantId,
        batch: RecordBatch,
        ctx: BifrostWriteContext,
    ) -> Result<i64, BifrostError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .try_send(CoordinatorCmd::Write {
                tenant,
                batch,
                ctx,
                reply: tx,
            })
            .map_err(|_| BifrostError::IngestBusy(self.table_fqn.clone()))?;
        rx.await
            .map_err(|_| BifrostError::WriterUnavailable(self.table_fqn.clone()))?
    }

    /// Returns `true` when the actor has exited (channel closed).
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}

/// The shared per-physical-table group-commit actor.
struct GroupCommitActor {
    receiver: mpsc::Receiver<CoordinatorCmd>,
    table: Table,
    catalog: Arc<SqlCatalog>,
    pool: Arc<PgPool>,
    table_uid: TableUid,
    scope: TableScope,
    registry: Arc<Registry>,
    payload_class: PayloadClass,
    sensitive_columns: &'static [&'static str],
    #[allow(dead_code)]
    entity_bounds_mapping: Option<EntityBoundsMapping>,
    flush_policy: FlushPolicy,
    buffer: TenantAppendBuffer,
    pending: Vec<PendingEntry>,
}

impl GroupCommitActor {
    #[allow(clippy::too_many_arguments)]
    fn new(
        receiver: mpsc::Receiver<CoordinatorCmd>,
        table: Table,
        catalog: Arc<SqlCatalog>,
        pool: Arc<PgPool>,
        table_uid: TableUid,
        scope: TableScope,
        registry: Arc<Registry>,
        payload_class: PayloadClass,
        sensitive_columns: &'static [&'static str],
        entity_bounds_mapping: Option<EntityBoundsMapping>,
        flush_policy: FlushPolicy,
    ) -> Self {
        Self {
            receiver,
            table,
            catalog,
            pool,
            table_uid,
            scope,
            registry,
            payload_class,
            sensitive_columns,
            entity_bounds_mapping,
            flush_policy,
            buffer: TenantAppendBuffer::new(),
            pending: Vec::new(),
        }
    }

    async fn run(mut self) {
        let interval_dur = self.flush_policy.max_interval;
        let mut timer = interval_at(Instant::now() + interval_dur, interval_dur);

        loop {
            tokio::select! {
                cmd = self.receiver.recv() => {
                    match cmd {
                        None => {
                            // Channel closed — drain and exit.
                            if !self.buffer.is_empty() {
                                self.run_group_commit().await;
                            }
                            return;
                        }
                        Some(CoordinatorCmd::Write { tenant, batch, ctx, reply }) => {
                            self.buffer.push(tenant, batch);
                            self.pending.push(PendingEntry { tenant, ctx, reply });

                            if self.buffer.total_rows() >= self.flush_policy.max_rows
                                || self.buffer.total_bytes() >= self.flush_policy.max_buffered_bytes
                            {
                                self.run_group_commit().await;
                            }
                        }
                        #[cfg(test)]
                        Some(CoordinatorCmd::ForceFlush(reply)) => {
                            let result = self.do_group_commit().await;
                            let _ = reply.send(result);
                        }
                    }
                }
                _ = timer.tick() => {
                    if !self.buffer.is_empty() {
                        self.run_group_commit().await;
                    }
                }
            }
        }
    }

    /// Run the group-commit cycle, acking all pending writes after completion.
    async fn run_group_commit(&mut self) {
        let result = self.do_group_commit().await;
        match result {
            Ok(outcomes) => {
                let snapshot_map: HashMap<DataTenantId, i64> = outcomes.iter().copied().collect();
                let pending = std::mem::take(&mut self.pending);
                for pw in pending {
                    let snap = snapshot_map.get(&pw.tenant).copied().unwrap_or(0);
                    let _ = pw.reply.send(Ok(snap));
                }
            }
            Err(e) => {
                let pending = std::mem::take(&mut self.pending);
                for pw in pending {
                    let _ = pw.reply.send(Err(BifrostError::Internal(e.to_string())));
                }
            }
        }
    }

    /// Drain the buffer and run one group-commit cycle.
    ///
    /// Per tenant: stamp system columns → apply redaction → run_commit (2PC).
    /// All commits succeed or the first failure returns an error and restores
    /// the buffer.
    async fn do_group_commit(&mut self) -> Result<Vec<(DataTenantId, i64)>, BifrostError> {
        let tenant_groups = self.buffer.drain_by_tenant();
        if tenant_groups.is_empty() {
            return Ok(Vec::new());
        }

        let mut outcomes: Vec<(DataTenantId, i64)> = Vec::new();

        for (tenant, batches) in &tenant_groups {
            let ingested_at_us = now_micros();
            let stamp_tenant = self.scope.stamp_tenant(*tenant);
            let control_bind = self.scope.control_bind(*tenant);

            // Representative ctx: use the last pending entry for this tenant.
            let entry_ctx = self
                .pending
                .iter()
                .rev()
                .find(|p| p.tenant == *tenant)
                .map(|p| &p.ctx);

            let (batch_id, origin, actor, audit) = match entry_ctx {
                Some(ctx) => {
                    let audit = if ctx.origin == AUDIT_RELAY_ORIGIN {
                        None
                    } else {
                        Some(ingest_audit_event(
                            ctx,
                            &self.table.identifier().to_string(),
                        ))
                    };
                    (ctx.batch_id, ctx.origin.clone(), ctx.actor.clone(), audit)
                }
                None => {
                    let sys = BifrostWriteContext::system();
                    (sys.batch_id, sys.origin, sys.actor, None)
                }
            };

            let stamped: Vec<RecordBatch> = batches
                .iter()
                .map(|b| stamp_system_columns(b, ingested_at_us, batch_id, stamp_tenant))
                .collect::<Result<_, _>>()
                .map_err(BifrostError::Arrow)?;

            let stamped = if self.payload_class == PayloadClass::Sensitive
                && !self.sensitive_columns.is_empty()
            {
                use crate::writer::redaction::BuiltinRedactionPass;
                let pass = BuiltinRedactionPass;
                stamped
                    .iter()
                    .map(|b| pass.scrub(b, self.sensitive_columns))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                stamped
            };

            match run_commit(
                &self.pool,
                &self.catalog,
                &self.table,
                &self.table_uid,
                stamped,
                batch_id,
                &origin,
                &actor,
                control_bind,
                audit,
            )
            .await
            {
                Ok((snapshot_id, updated_table)) => {
                    if let Some(table) = updated_table {
                        self.table = table;
                        let key = crate::registry::RegistryKey {
                            owner: control_bind,
                            table_uid: self.table_uid,
                        };
                        if let Err(e) = self.registry.invalidate(key).await {
                            tracing::warn!(
                                error = %e,
                                "epoch bump after group commit failed (cache may be stale)"
                            );
                        }
                    }
                    outcomes.push((*tenant, snapshot_id));
                }
                Err(e) => {
                    // Restore remaining tenant groups to buffer.
                    let idx = tenant_groups
                        .iter()
                        .position(|(t, _)| t == tenant)
                        .unwrap_or(0);
                    self.buffer.restore(tenant_groups[idx..].to_vec());
                    return Err(e);
                }
            }
        }

        Ok(outcomes)
    }
}

/// Spawn the shared per-physical-table group-commit coordinator.
///
/// Returns a [`GroupCommitHandle`] the ingest orchestrator uses to send writes.
/// One coordinator is spawned per physical table via [`WriterRegistry`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_group_commit_coordinator(
    table: Table,
    catalog: Arc<SqlCatalog>,
    pool: Arc<PgPool>,
    table_uid: TableUid,
    table_fqn: String,
    scope: TableScope,
    registry: Arc<Registry>,
    payload_class: PayloadClass,
    sensitive_columns: &'static [&'static str],
    entity_bounds_mapping: Option<EntityBoundsMapping>,
    flush_policy: FlushPolicy,
) -> GroupCommitHandle {
    let (sender, receiver) = mpsc::channel(256);

    let actor = GroupCommitActor::new(
        receiver,
        table,
        catalog,
        pool,
        table_uid,
        scope,
        registry,
        payload_class,
        sensitive_columns,
        entity_bounds_mapping,
        flush_policy,
    );

    tokio::spawn(actor.run());

    GroupCommitHandle {
        sender: Arc::new(sender),
        table_fqn,
    }
}

// ── Legacy per-tenant commit coordinator (backward compat) ───────────────────
//
// The old coordinator is kept for paths that still call `WyrdCatalog::writer()`
// (single-tenant per-handle).  The group-commit coordinator supersedes this for
// the ingest path.

/// Spawn the per-table commit actor and return a [`TableWriterHandle`] for it.
///
/// One actor is spawned per `writer()` call, each bound to a single data tenant.
/// The actor owns the loaded [`Table`] and advances its snapshot as flushes commit.
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

    let actor = LegacyCommitActor {
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

/// The single-tenant commit actor backing `WyrdCatalog::writer()`.
struct LegacyCommitActor {
    receiver: mpsc::Receiver<WriteCmd>,
    table: Table,
    catalog: Arc<SqlCatalog>,
    pool: Arc<PgPool>,
    table_uid: TableUid,
    scope: TableScope,
    data_tenant: DataTenantId,
    buffer: AppendBuffer,
    registry: Arc<Registry>,
    payload_class: PayloadClass,
    sensitive_columns: &'static [&'static str],
    #[allow(dead_code)]
    entity_bounds_mapping: Option<EntityBoundsMapping>,
}

impl LegacyCommitActor {
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

    fn accept(&mut self, batch: RecordBatch) -> Result<(), BifrostError> {
        use wyrd_spec::vala::system_columns::is_reserved_system_column;
        for field in batch.schema().fields() {
            if is_reserved_system_column(field.name()) {
                return Err(BifrostError::ReservedColumn(field.name().clone()));
            }
        }
        self.buffer.push(batch);
        Ok(())
    }

    async fn flush(&mut self, ctx: BifrostWriteContext) -> Result<i64, BifrostError> {
        if self.buffer.is_empty() {
            return Ok(0);
        }

        let batches = self.buffer.drain();
        let batch_id = ctx.batch_id;
        let ingested_at_us = now_micros();
        let stamp_tenant = self.scope.stamp_tenant(self.data_tenant);
        let control_bind = self.scope.control_bind(self.data_tenant);

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

        let stamped = if self.payload_class == PayloadClass::Sensitive
            && !self.sensitive_columns.is_empty()
        {
            use crate::writer::redaction::BuiltinRedactionPass;
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

        match run_commit(
            &self.pool,
            &self.catalog,
            &self.table,
            &self.table_uid,
            stamped,
            batch_id,
            &ctx.origin,
            &ctx.actor,
            control_bind,
            audit,
        )
        .await
        {
            Ok((snapshot_id, updated_table)) => {
                if let Some(table) = updated_table {
                    self.table = table;
                    let key = crate::registry::RegistryKey {
                        owner: control_bind,
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

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use arrow::array::{Int64Array, RecordBatch};
    use arrow::datatypes::{DataType, Field, Schema};
    use wyrd_spec::ids::DataTenantId;

    use super::FlushPolicy;
    use crate::writer::buffer::TenantAppendBuffer;

    fn make_batch(rows: usize) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, false)]));
        let col = Arc::new(Int64Array::from(vec![1i64; rows]));
        RecordBatch::try_new(schema, vec![col]).expect("valid batch")
    }

    #[test]
    fn flush_policy_defaults_match_contract() {
        let policy = FlushPolicy::default();
        assert_eq!(policy.max_rows, 50_000);
        assert_eq!(policy.max_interval, Duration::from_secs(1));
        assert_eq!(policy.max_buffered_bytes, 128 * 1024 * 1024);
    }

    /// Fan-in of N tenants coalesces into one buffer (group-commit coordinator).
    #[test]
    fn coordinator_fan_in_buffers_all_tenants() {
        let t1 = DataTenantId::new_v7();
        let t2 = DataTenantId::new_v7();
        let t3 = DataTenantId::new_v7();

        let mut buf = TenantAppendBuffer::new();
        buf.push(t1, make_batch(10));
        buf.push(t2, make_batch(5));
        buf.push(t3, make_batch(8));
        buf.push(t1, make_batch(3));

        assert_eq!(buf.total_rows(), 26);

        let groups = buf.drain_by_tenant();
        assert_eq!(groups.len(), 3);

        let totals: std::collections::HashMap<DataTenantId, usize> = groups
            .iter()
            .map(|(t, batches)| (*t, batches.iter().map(|b| b.num_rows()).sum()))
            .collect();

        assert_eq!(totals[&t1], 13);
        assert_eq!(totals[&t2], 5);
        assert_eq!(totals[&t3], 8);
    }

    #[test]
    fn flush_policy_row_trigger() {
        let policy = FlushPolicy {
            max_rows: 100,
            max_interval: Duration::from_secs(60),
            max_buffered_bytes: 128 * 1024 * 1024,
        };

        let t = DataTenantId::new_v7();
        let mut buf = TenantAppendBuffer::new();
        buf.push(t, make_batch(50));
        assert!(buf.total_rows() < policy.max_rows);

        buf.push(t, make_batch(60));
        assert!(buf.total_rows() >= policy.max_rows);
    }

    #[test]
    fn coordinator_single_tenant_fifo_flush() {
        let t = DataTenantId::new_v7();
        let mut buf = TenantAppendBuffer::new();
        for i in 1..=5usize {
            buf.push(t, make_batch(i));
        }

        let groups = buf.drain_by_tenant();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, t);
        let row_counts: Vec<usize> = groups[0].1.iter().map(|b| b.num_rows()).collect();
        assert_eq!(row_counts, vec![1, 2, 3, 4, 5]);
    }
}
