use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use arrow::array::RecordBatch;
use iceberg::table::Table;
use iceberg_catalog_sql::SqlCatalog;
use sqlx::PgPool;
use tokio::sync::mpsc;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::system_columns::is_reserved_system_column;

use crate::batch_builder::stamp_system_columns;
use crate::error::BifrostError;
use crate::registry::Registry;
use crate::types::{TableScope, TableUid};
use crate::writer::buffer::AppendBuffer;
use crate::writer::commit::run_commit;
use crate::writer::{TableWriterHandle, WriteCmd};

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
    /// onto `WriteCmd::Write` so one actor can stamp many tenants' writes.
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
                WriteCmd::Flush(reply) => {
                    let result = self.flush().await;
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
    async fn flush(&mut self) -> Result<i64, BifrostError> {
        if self.buffer.is_empty() {
            return Ok(0);
        }

        let batches = self.buffer.drain();
        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let ingested_at_us = now_micros();
        let stamp_tenant = self.scope.stamp_tenant(self.data_tenant);

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
            self.scope.control_bind(self.data_tenant),
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
