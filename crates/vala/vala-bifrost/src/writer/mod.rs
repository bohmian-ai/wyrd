use arrow::array::RecordBatch;
use tokio::sync::{mpsc, oneshot};

use crate::error::BifrostError;

pub mod buffer;
pub mod commit;
pub mod coordinator;
pub mod file_writer;

pub use coordinator::spawn_commit_coordinator;

pub enum WriteCmd {
    Write(RecordBatch, oneshot::Sender<Result<(), BifrostError>>),
    Flush(oneshot::Sender<Result<i64, BifrostError>>),
}

pub struct TableWriterHandle {
    pub(crate) sender: mpsc::Sender<WriteCmd>,
}

impl TableWriterHandle {
    pub async fn write(&self, batch: RecordBatch) -> Result<(), BifrostError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(WriteCmd::Write(batch, tx))
            .await
            .map_err(|_| BifrostError::WriterUnavailable("channel closed".into()))?;
        rx.await
            .map_err(|_| BifrostError::WriterUnavailable("reply dropped".into()))?
    }

    pub async fn flush(&self) -> Result<i64, BifrostError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(WriteCmd::Flush(tx))
            .await
            .map_err(|_| BifrostError::WriterUnavailable("channel closed".into()))?;
        rx.await
            .map_err(|_| BifrostError::WriterUnavailable("reply dropped".into()))?
    }
}
