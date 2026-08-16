//! The owned Bifrost ingest handoff from `wyrd-queue` to ordinary Rust gRPC.

use std::sync::Arc;

use async_trait::async_trait;
use wyrd_queue::{BatchSink, ClientByteGuard, DurableBatchAck, SealedBatch, SinkError};

/// The ordinary Rust transport seam for one owned batch guard type.
#[async_trait]
pub trait IngestTransport<G>: Send + Sync + 'static {
    /// Ships one owned batch, returning that exact owner on an ambiguous outcome.
    ///
    /// # Errors
    ///
    /// Returns [`SinkError::Retryable`] with the unchanged batch when a timeout
    /// or transport loss leaves durable commit ambiguous, or terminal failure
    /// after the batch owner has been consumed.
    async fn insert_batch(&self, batch: SealedBatch<G>) -> Result<DurableBatchAck, SinkError<G>>;
}

/// The Record queue sink that forwards each non-cloneable batch to gRPC.
pub struct BifrostIngestSink {
    transport: Arc<dyn IngestTransport<ClientByteGuard>>,
}

impl BifrostIngestSink {
    /// Builds a sink over the ordinary Rust owned-batch transport.
    #[must_use]
    pub fn new(transport: Arc<dyn IngestTransport<ClientByteGuard>>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl BatchSink<ClientByteGuard> for BifrostIngestSink {
    async fn send(
        &self,
        batch: SealedBatch<ClientByteGuard>,
    ) -> Result<DurableBatchAck, SinkError<ClientByteGuard>> {
        self.transport.insert_batch(batch).await
    }
}
