use arrow::array::RecordBatch;
use tokio::sync::{mpsc, oneshot};

use crate::error::BifrostError;

pub mod buffer;
pub mod commit;
pub mod coordinator;
pub mod file_writer;

pub use coordinator::spawn_commit_coordinator;

/// A command sent to the per-table [`CommitActor`](coordinator). Each variant
/// carries a oneshot channel the actor replies on, so callers await the result.
pub enum WriteCmd {
    /// Buffer a caller batch (user fields only — the actor stamps system columns).
    Write(RecordBatch, oneshot::Sender<Result<(), BifrostError>>),
    /// Flush the buffered batches as one 2PC commit; replies with the snapshot id.
    Flush(oneshot::Sender<Result<i64, BifrostError>>),
}

/// Caller-facing handle to a single table's commit actor.
///
/// Cloneable channel sender plus the table FQN used to tag errors when the actor
/// is gone. All durable work happens in the actor task; the handle only sends
/// commands and awaits replies.
pub struct TableWriterHandle {
    pub(crate) sender: mpsc::Sender<WriteCmd>,
    pub(crate) table_fqn: String,
}

impl TableWriterHandle {
    /// Buffer `batch` for the next flush. The batch must carry user fields only;
    /// any reserved system column is rejected by the actor.
    ///
    /// # Errors
    /// Returns [`BifrostError::WriterUnavailable`] if the actor has stopped, or
    /// the actor's validation error (e.g. [`BifrostError::ReservedColumn`]).
    pub async fn write(&self, batch: RecordBatch) -> Result<(), BifrostError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(WriteCmd::Write(batch, tx))
            .await
            .map_err(|_| BifrostError::WriterUnavailable(self.table_fqn.clone()))?;
        rx.await
            .map_err(|_| BifrostError::WriterUnavailable(self.table_fqn.clone()))?
    }

    /// Commit all buffered batches as one 2PC transaction and return the new
    /// Iceberg snapshot id (or `0` when the buffer is empty).
    ///
    /// # Errors
    /// Returns [`BifrostError::WriterUnavailable`] if the actor has stopped, or
    /// the commit error surfaced by the 2PC flow.
    pub async fn flush(&self) -> Result<i64, BifrostError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(WriteCmd::Flush(tx))
            .await
            .map_err(|_| BifrostError::WriterUnavailable(self.table_fqn.clone()))?;
        rx.await
            .map_err(|_| BifrostError::WriterUnavailable(self.table_fqn.clone()))?
    }
}
