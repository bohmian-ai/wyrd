use std::sync::Arc;

use arrow::array::RecordBatch;
use iceberg::table::Table;
use iceberg_catalog_sql::SqlCatalog;
use sqlx::PgPool;
use tokio::sync::mpsc;
use wyrd_spec::ids::DataTenantId;

use crate::error::BifrostError;
use crate::types::{TableScope, TableUid};
use crate::writer::commit::run_commit;
use crate::writer::{TableWriterHandle, WriteCmd};

struct CommitActor {
    receiver: mpsc::Receiver<WriteCmd>,
    table: Table,
    catalog: Arc<SqlCatalog>,
    pool: Arc<PgPool>,
    table_uid: TableUid,
    #[allow(dead_code)]
    scope: TableScope,
    tenant: DataTenantId,
    buffer: Vec<RecordBatch>,
}

impl CommitActor {
    async fn run(mut self) {
        while let Some(cmd) = self.receiver.recv().await {
            match cmd {
                WriteCmd::Write(batch, reply) => {
                    self.buffer.push(batch);
                    let _ = reply.send(Ok(()));
                }
                WriteCmd::Flush(reply) => {
                    let result = self.flush().await;
                    let _ = reply.send(result);
                }
            }
        }
    }

    async fn flush(&mut self) -> Result<i64, BifrostError> {
        if self.buffer.is_empty() {
            return Ok(0);
        }

        let batches = std::mem::take(&mut self.buffer);
        let batch_id = *uuid::Uuid::now_v7().as_bytes();

        let result = run_commit(
            &self.pool,
            &self.catalog,
            &self.table,
            &self.table_uid,
            batches.clone(),
            batch_id,
            self.tenant,
        )
        .await;

        if result.is_err() {
            self.buffer = batches;
        }

        result
    }
}

pub fn spawn_commit_coordinator(
    table: Table,
    catalog: Arc<SqlCatalog>,
    pool: Arc<PgPool>,
    table_uid: TableUid,
    scope: TableScope,
    tenant: DataTenantId,
) -> TableWriterHandle {
    let (sender, receiver) = mpsc::channel(64);

    let actor = CommitActor {
        receiver,
        table,
        catalog,
        pool,
        table_uid,
        scope,
        tenant,
        buffer: Vec::new(),
    };

    tokio::spawn(actor.run());

    TableWriterHandle { sender }
}
