//! Per-physical-table group-commit coordinator.
//!
//! A coordinator owns one loaded [`Table`] and buffers writes from N tenants,
//! then commits them as **one Iceberg `fast_append` per chunk** via
//! [`run_group_commit`] — N requests amortize a single catalog commit.
//!
//! **Flush policy** (Q2):
//! - Flush when `total_rows >= max_rows` (default 50 000) OR the interval timer
//!   fires (default 1 s), whichever comes first; `max_buffered_bytes` (~128 MiB)
//!   is a memory ceiling.
//! - A flush chunks its buffered write units at `max_commit_keys`, so one
//!   `fast_append` never carries an unbounded number of commit keys.
//!
//! **Per-tenant fairness** (Q4): [`TenantAppendBuffer`] drains in interleaved
//! round-robin order, so a high-volume tenant cannot starve a quiet one or push
//! its writes ahead into every chunk.
//!
//! **Backpressure** (Q5): the coordinator channel is bounded. When it is full the
//! caller receives [`BifrostError::IngestBusy`] immediately — never a silent drop.
//!
//! **Ack-after-commit** (Q1): each write's reply resolves ONLY after the group
//! commit that covers it is durable (the 2PC completed).
//!
//! **Cross-pod serialization** (Q7): Iceberg catalog CAS + the `WRITER_INSTANCE`
//! fence + `CommitKey{tenant, batch_id}` dedup. No writer election.

use std::collections::{HashMap, HashSet};
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
use wyrd_spec::vala::managed_columns::is_reserved_managed_column;

use crate::batch_builder::stamp_managed_columns;
use crate::error::BifrostError;
use crate::registry::{Registry, RegistryKey};
use crate::tables::PayloadClass;
use crate::types::{TableScope, TableUid};
use crate::writer::buffer::{TenantAppendBuffer, batch_byte_estimate};
use crate::writer::commit::{CommitGroup, run_group_commit};
use crate::writer::redaction::RedactionClassifier;
use crate::writer::{BifrostWriteContext, CommitEvent, CommitKey, CommitNotifier};

/// The audit-outbox `origin` for the relay's own write. This origin MUST NOT
/// append an audit row (M-11: the relay must not self-feed the audit spine).
pub(crate) const AUDIT_RELAY_ORIGIN: &str = "audit-relay";

/// The reply channel one write awaits: the covering group commit's snapshot id.
type CommitReply = oneshot::Sender<Result<i64, BifrostError>>;

async fn emit_commit_events(
    notifier: &dyn CommitNotifier,
    table_uid: [u8; 16],
    committed: &[CommitKey],
    _replayed: &[(CommitKey, i64)],
) {
    for key in committed {
        let event = CommitEvent::SpanCommitted {
            table_uid,
            tenant: key.tenant,
            batch_id: key.batch_id,
        };
        notifier.notify(event).await;
    }
}

/// Flush policy knobs for the group-commit coordinator.
///
/// Q2: flush on `max_rows` OR `max_interval`, whichever first. `max_buffered_bytes`
/// is a memory ceiling; `max_commit_keys` caps how many commit keys ride one
/// `fast_append`.
#[derive(Debug, Clone, Copy)]
pub struct FlushPolicy {
    /// Row-count threshold: flush when `total_rows >= max_rows`.
    pub max_rows: usize,
    /// Time trigger: flush every `max_interval` regardless of row count.
    pub max_interval: Duration,
    /// Memory ceiling: flush when `total_bytes >= max_buffered_bytes`.
    pub max_buffered_bytes: usize,
    /// Chunk size: at most this many commit keys per `fast_append`.
    pub max_commit_keys: usize,
}

impl Default for FlushPolicy {
    fn default() -> Self {
        Self {
            max_rows: 50_000,
            max_interval: Duration::from_secs(1),
            max_buffered_bytes: 128 * 1024 * 1024, // 128 MiB
            max_commit_keys: 128,
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
        detail: None,
    }
}

fn now_micros() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
}

/// The first reserved system column present in any of `batches`, if any. A caller
/// batch must carry user fields only; the coordinator stamps system columns.
fn first_reserved_column(batches: &[RecordBatch]) -> Option<String> {
    batches.iter().find_map(|batch| {
        batch
            .schema()
            .fields()
            .iter()
            .find(|f| is_reserved_managed_column(f.name()))
            .map(|f| f.name().clone())
    })
}

/// Command sent to the group-commit actor.
pub(crate) enum CoordinatorCmd {
    /// Buffer one commit unit (all `batches` share `ctx.batch_id`) for `tenant`.
    /// The reply resolves only when the group commit covering it is durable (Q1).
    Write {
        tenant: DataTenantId,
        batches: Vec<RecordBatch>,
        ctx: Box<BifrostWriteContext>,
        reply: CommitReply,
    },
}

/// One buffered commit unit awaiting its covering commit: the caller's batches
/// (all under `ctx.batch_id`), the attribution context, and the reply channel.
struct PendingWrite {
    batches: Vec<RecordBatch>,
    ctx: BifrostWriteContext,
    reply: CommitReply,
}

/// Caller-facing handle for one physical table's group-commit coordinator.
///
/// Cloneable cheap sender; all durable work happens in the actor task.
#[derive(Clone)]
pub struct GroupCommitHandle {
    pub(crate) sender: Arc<mpsc::Sender<CoordinatorCmd>>,
    pub(crate) table_fqn: String,
    /// Physical table UID — stable for the handle's lifetime. Used by the
    /// derivation runtime to compute deterministic exactly-once keys.
    pub table_uid: [u8; 16],
}

impl GroupCommitHandle {
    /// Enqueue a commit unit and return the receiver for its commit-durable ack
    /// WITHOUT awaiting it. Splitting send from await lets a caller buffer many
    /// units before any flush, and lets shutdown drain them (the reply channel is
    /// independent of this handle's sender).
    ///
    /// # Errors
    /// [`BifrostError::IngestBusy`] when the coordinator channel is full (Q5).
    pub(crate) fn send_write(
        &self,
        tenant: DataTenantId,
        batches: Vec<RecordBatch>,
        ctx: BifrostWriteContext,
    ) -> Result<oneshot::Receiver<Result<i64, BifrostError>>, BifrostError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .try_send(CoordinatorCmd::Write {
                tenant,
                batches,
                ctx: Box::new(ctx),
                reply: tx,
            })
            .map_err(|_| BifrostError::IngestBusy(self.table_fqn.clone()))?;
        Ok(rx)
    }

    /// Buffer a commit unit for `tenant` and await its commit-durable ack (Q1).
    ///
    /// # Errors
    /// [`BifrostError::IngestBusy`] on channel-full backpressure (Q5);
    /// [`BifrostError::WriterUnavailable`] if the coordinator exited before acking.
    pub async fn write(
        &self,
        tenant: DataTenantId,
        batches: Vec<RecordBatch>,
        ctx: BifrostWriteContext,
    ) -> Result<i64, BifrostError> {
        let rx = self.send_write(tenant, batches, ctx)?;
        rx.await
            .map_err(|_| BifrostError::WriterUnavailable(self.table_fqn.clone()))?
    }

    /// One-shot: submit a single commit unit, close this (sole) handle so the
    /// coordinator drains immediately, and await the commit-durable snapshot id.
    /// For writers that own their coordinator for exactly one commit (the audit
    /// relay, a single ingest stream).
    ///
    /// # Errors
    /// [`BifrostError::IngestBusy`] on backpressure;
    /// [`BifrostError::WriterUnavailable`] if the coordinator exited before acking.
    pub async fn commit_one(
        self,
        tenant: DataTenantId,
        batches: Vec<RecordBatch>,
        ctx: BifrostWriteContext,
    ) -> Result<i64, BifrostError> {
        let fqn = self.table_fqn.clone();
        let rx = self.send_write(tenant, batches, ctx)?;
        // Drop the last sender so the actor's recv() yields None → drain → commit.
        drop(self);
        rx.await.map_err(|_| BifrostError::WriterUnavailable(fqn))?
    }

    /// Returns `true` when the actor has exited (channel closed).
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}

/// The per-physical-table group-commit actor.
struct GroupCommitActor {
    receiver: mpsc::Receiver<CoordinatorCmd>,
    table: Table,
    catalog: Arc<SqlCatalog>,
    pool: Arc<PgPool>,
    table_uid: TableUid,
    scope: TableScope,
    registry: Arc<Registry>,
    notifier: Arc<dyn CommitNotifier>,
    payload_class: PayloadClass,
    sensitive_columns: &'static [&'static str],
    flush_policy: FlushPolicy,
    buffer: TenantAppendBuffer<PendingWrite>,
}

impl GroupCommitActor {
    async fn run(mut self) {
        let interval_dur = self.flush_policy.max_interval;
        let mut timer = interval_at(Instant::now() + interval_dur, interval_dur);
        // A slow flush must not make the timer fire back-to-back to "catch up"
        // (the default `Burst` behavior); delay the next tick a full interval
        // past completion instead. Matches the repo's Delay/Skip convention.
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                cmd = self.receiver.recv() => {
                    match cmd {
                        None => {
                            // All senders dropped — drain the buffer and exit (Q3).
                            if !self.buffer.is_empty() {
                                self.flush().await;
                            }
                            return;
                        }
                        Some(CoordinatorCmd::Write { tenant, batches, ctx, reply }) => {
                            self.accept(tenant, batches, *ctx, reply).await;
                        }
                    }
                }
                _ = timer.tick() => {
                    if !self.buffer.is_empty() {
                        self.flush().await;
                    }
                }
            }
        }
    }

    /// Validate one write unit and buffer it; flush if a size threshold is crossed.
    async fn accept(
        &mut self,
        tenant: DataTenantId,
        batches: Vec<RecordBatch>,
        ctx: BifrostWriteContext,
        reply: CommitReply,
    ) {
        if let Some(col) = first_reserved_column(&batches) {
            let _ = reply.send(Err(BifrostError::ReservedColumn(col)));
            return;
        }
        let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
        let bytes: usize = batches.iter().map(batch_byte_estimate).sum();
        self.buffer.push(
            tenant,
            PendingWrite {
                batches,
                ctx,
                reply,
            },
            rows,
            bytes,
        );

        if self.buffer.total_rows() >= self.flush_policy.max_rows
            || self.buffer.total_bytes() >= self.flush_policy.max_buffered_bytes
        {
            self.flush().await;
        }
    }

    /// Drain the buffer and commit it as one or more group commits, chunked at
    /// `max_commit_keys`, acking each write after its covering commit is durable.
    async fn flush(&mut self) {
        let writes = self.buffer.drain_round_robin();
        if writes.is_empty() {
            return;
        }

        let ingested_at_us = now_micros();
        let table_fqn = self.table.identifier().to_string();

        // Build one commit group per buffered write unit; a per-unit stamping or
        // redaction failure fails just that unit's reply.
        let mut groups: Vec<(CommitGroup, CommitReply)> = Vec::with_capacity(writes.len());
        for (tenant, pending) in writes {
            let PendingWrite {
                batches,
                ctx,
                reply,
            } = pending;
            match self.prepare_group(tenant, &batches, ctx, ingested_at_us, &table_fqn) {
                Ok(group) => groups.push((group, reply)),
                Err(e) => {
                    let _ = reply.send(Err(e));
                }
            }
        }

        // Commit each chunk as ONE fast_append; ack its writes; advance the table.
        let max = self.flush_policy.max_commit_keys.max(1);
        while !groups.is_empty() {
            let take = groups.len().min(max);
            let chunk: Vec<(CommitGroup, CommitReply)> = groups.drain(..take).collect();
            self.commit_chunk(chunk, &table_fqn).await;
        }
    }

    /// Stamp system columns, apply redaction, and assemble the [`CommitGroup`] for
    /// one buffered write unit.
    fn prepare_group(
        &self,
        tenant: DataTenantId,
        batches: &[RecordBatch],
        ctx: BifrostWriteContext,
        ingested_at_us: i64,
        table_fqn: &str,
    ) -> Result<CommitGroup, BifrostError> {
        let stamp_tenant = self.scope.stamp_tenant(tenant);
        let redact =
            self.payload_class == PayloadClass::Sensitive && !self.sensitive_columns.is_empty();

        let mut stamped: Vec<RecordBatch> = Vec::with_capacity(batches.len());
        for batch in batches {
            let s = stamp_managed_columns(batch, ingested_at_us, ctx.batch_id, stamp_tenant)
                .map_err(BifrostError::Arrow)?;
            let s = if redact {
                use crate::writer::redaction::BuiltinRedactionPass;
                BuiltinRedactionPass.scrub(&s, self.sensitive_columns)?
            } else {
                s
            };
            stamped.push(s);
        }

        let audit = if ctx.origin == AUDIT_RELAY_ORIGIN {
            None
        } else {
            Some(ingest_audit_event(&ctx, table_fqn))
        };
        let key = CommitKey::new(tenant, ctx.batch_id);
        Ok(CommitGroup {
            key,
            ctx,
            batches: stamped,
            audit,
        })
    }

    /// Run one chunk as a single group commit, advance the table on a fresh append,
    /// invalidate the registry for every affected `control_bind`, and ack each
    /// write with the snapshot that covers its key.
    async fn commit_chunk(&mut self, chunk: Vec<(CommitGroup, CommitReply)>, table_fqn: &str) {
        let mut cgs: Vec<CommitGroup> = Vec::with_capacity(chunk.len());
        let mut replies: Vec<(CommitKey, CommitReply)> = Vec::with_capacity(chunk.len());
        for (cg, reply) in chunk {
            replies.push((cg.key, reply));
            cgs.push(cg);
        }

        match run_group_commit(
            &self.pool,
            &self.catalog,
            &self.table,
            &self.table_uid,
            self.scope,
            cgs,
        )
        .await
        {
            Ok(outcome) => {
                if let Some(table) = outcome.updated_table {
                    self.table = table;
                    self.invalidate_registry(&outcome.committed).await;
                }
                emit_commit_events(
                    self.notifier.as_ref(),
                    self.table_uid.0,
                    &outcome.committed,
                    &outcome.replayed,
                )
                .await;
                // key → covering snapshot: fresh keys share outcome.snapshot_id;
                // replayed keys keep the snapshot they originally committed to.
                let mut snaps: HashMap<CommitKey, i64> = HashMap::new();
                if let Some(sid) = outcome.snapshot_id {
                    for key in &outcome.committed {
                        snaps.insert(*key, sid);
                    }
                }
                for (key, sid) in &outcome.replayed {
                    snaps.insert(*key, *sid);
                }
                for (key, reply) in replies {
                    let result = match snaps.get(&key) {
                        Some(sid) => Ok(*sid),
                        None => Err(BifrostError::WriterUnavailable(table_fqn.to_string())),
                    };
                    let _ = reply.send(result);
                }
            }
            Err(e) => {
                for (_key, reply) in replies {
                    let _ = reply.send(Err(BifrostError::Internal(e.to_string())));
                }
            }
        }
    }

    /// Epoch-bump the reader cache for each distinct `control_bind` whose snapshot
    /// this flush advanced.
    async fn invalidate_registry(&self, committed: &[CommitKey]) {
        let binds: HashSet<DataTenantId> = committed
            .iter()
            .map(|k| self.scope.control_bind(k.tenant))
            .collect();
        for owner in binds {
            let key = RegistryKey {
                owner,
                table_uid: self.table_uid,
            };
            if let Err(e) = self.registry.invalidate(key).await {
                tracing::warn!(
                    error = %e,
                    "epoch bump after group commit failed (cache may be stale)"
                );
            }
        }
    }
}

/// Spawn the per-physical-table group-commit coordinator with a no-op notifier.
///
/// This retains the crate-internal constructor used by existing focused
/// commit tests. Production catalog writers use
/// [`spawn_group_commit_coordinator_with_notifier`].
#[cfg(test)]
pub(crate) fn spawn_group_commit_coordinator(inputs: GroupCoordinatorInputs) -> GroupCommitHandle {
    spawn_group_commit_coordinator_with_notifier(
        inputs,
        Arc::new(crate::writer::NoOpCommitNotifier),
    )
}

/// Inputs shared by both variants of the group-commit coordinator spawner.
pub(crate) struct GroupCoordinatorInputs {
    pub table: Table,
    pub catalog: Arc<SqlCatalog>,
    pub pool: Arc<PgPool>,
    pub table_uid: TableUid,
    pub table_fqn: String,
    pub scope: TableScope,
    pub registry: Arc<Registry>,
    pub payload_class: PayloadClass,
    pub sensitive_columns: &'static [&'static str],
    pub flush_policy: FlushPolicy,
}

/// Spawn the per-physical-table group-commit coordinator with a commit notifier.
///
/// Returns a [`GroupCommitHandle`] the caller uses to submit writes. The actor
/// owns the loaded [`Table`] and advances its snapshot as flushes commit.
pub(crate) fn spawn_group_commit_coordinator_with_notifier(
    inputs: GroupCoordinatorInputs,
    commit_notifier: Arc<dyn CommitNotifier>,
) -> GroupCommitHandle {
    let GroupCoordinatorInputs {
        table,
        catalog,
        pool,
        table_uid,
        table_fqn,
        scope,
        registry,
        payload_class,
        sensitive_columns,
        flush_policy,
    } = inputs;
    let (sender, receiver) = mpsc::channel(256);

    let actor = GroupCommitActor {
        receiver,
        table,
        catalog,
        pool,
        table_uid,
        scope,
        registry,
        notifier: commit_notifier,
        payload_class,
        sensitive_columns,
        flush_policy,
        buffer: TenantAppendBuffer::new(),
    };

    tokio::spawn(actor.run());

    GroupCommitHandle {
        sender: Arc::new(sender),
        table_fqn,
        table_uid: table_uid.0,
    }
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;

    use super::{CommitEvent, CommitKey, CommitNotifier, FlushPolicy, emit_commit_events};

    #[derive(Clone, Default)]
    struct SpyNotifier {
        events: Arc<Mutex<Vec<CommitEvent>>>,
    }

    #[async_trait]
    impl CommitNotifier for SpyNotifier {
        async fn notify(&self, event: CommitEvent) {
            self.events
                .lock()
                .expect("spy mutex is not poisoned")
                .push(event);
        }
    }

    #[test]
    fn flush_policy_defaults_match_contract() {
        let policy = FlushPolicy::default();
        assert_eq!(policy.max_rows, 50_000);
        assert_eq!(policy.max_interval, Duration::from_secs(1));
        assert_eq!(policy.max_buffered_bytes, 128 * 1024 * 1024);
        assert_eq!(policy.max_commit_keys, 128);
    }

    #[tokio::test]
    async fn notify_fires_once_per_committed_key() {
        let notifier = SpyNotifier::default();
        let table_uid = [7; 16];
        let key = CommitKey::new(wyrd_spec::ids::DataTenantId::new_v7(), [9; 16]);

        emit_commit_events(&notifier, table_uid, &[key], &[]).await;
        emit_commit_events(&notifier, table_uid, &[], &[(key, 42)]).await;

        let events = notifier.events.lock().expect("spy mutex is not poisoned");
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            CommitEvent::SpanCommitted {
                table_uid,
                tenant: key.tenant,
                batch_id: key.batch_id,
            }
        );
    }
}

#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use sqlx::postgres::PgListener;
    use tempfile::tempdir;
    use tokio::time::timeout;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_storage::settings::BackendConfig;

    use super::{BifrostWriteContext, CommitEvent, TableScope};
    use crate::catalog::WyrdCatalog;
    use crate::catalog::namespaces::BifrostNamespace;

    fn batch() -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "value",
                DataType::Int64,
                false,
            )])),
            vec![Arc::new(Int64Array::from(vec![1_i64]))],
        )
        .expect("coordinator test batch schema is valid")
    }

    fn context(tenant: DataTenantId, batch_id: [u8; 16]) -> BifrostWriteContext {
        BifrostWriteContext {
            batch_id,
            origin: "coordinator-test".to_owned(),
            actor: tenant.to_string(),
            request_id: RequestId::now_v7(),
            card_ref: None,
        }
    }

    #[tokio::test]
    async fn coordinator_notifies_fresh_finalize_and_not_replay() {
        let fixture = PgFixture::start().await.expect("fixture");
        let pool = Arc::new(fixture.app_pool().clone());
        let storage = tempdir().expect("storage tempdir");
        let backend = BackendConfig::Local {
            root: storage.path().to_path_buf(),
        };
        let catalog = Arc::new(
            WyrdCatalog::new(&fixture.catalog_uri(), &backend, pool.clone(), None)
                .await
                .expect("catalog"),
        );
        let tenant = fixture.data_tenant_id();
        let table_name = "coordinator_notify";
        let table_uid = catalog
            .create_table(crate::catalog::CreateTableRequest {
                ns: BifrostNamespace::Bifrost,

                name: table_name,

                user_fields: vec![Field::new("value", DataType::Int64, false)],

                scope: TableScope::TenantOwned,

                tenant,

                partition_columns: &[],

                audit: None,
            })
            .await
            .expect("test table");

        let mut listener = PgListener::connect_with(pool.as_ref())
            .await
            .expect("listener connection");
        listener
            .listen("vala_commits")
            .await
            .expect("listener setup");

        let batch_id = [0x42; 16];
        let writer = catalog
            .writer(
                BifrostNamespace::Bifrost,
                table_name,
                TableScope::TenantOwned,
                tenant,
            )
            .await
            .expect("writer");
        writer
            .commit_one(tenant, vec![batch()], context(tenant, batch_id))
            .await
            .expect("fresh commit");

        let notification = timeout(Duration::from_secs(5), listener.recv())
            .await
            .expect("fresh notification arrives")
            .expect("listener receives fresh notification");
        let event: CommitEvent =
            serde_json::from_str(notification.payload()).expect("notification decodes");
        assert_eq!(
            event,
            CommitEvent::SpanCommitted {
                table_uid: *table_uid.as_bytes(),
                tenant,
                batch_id,
            }
        );

        let replay_writer = catalog
            .writer(
                BifrostNamespace::Bifrost,
                table_name,
                TableScope::TenantOwned,
                tenant,
            )
            .await
            .expect("replay writer");
        replay_writer
            .commit_one(tenant, vec![batch()], context(tenant, batch_id))
            .await
            .expect("replay commit");

        assert!(
            timeout(Duration::from_millis(250), listener.recv())
                .await
                .is_err(),
            "replayed key must not emit a notification"
        );
    }
}
