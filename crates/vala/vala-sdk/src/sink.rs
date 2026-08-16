//! The owned Bifrost ingest handoff from `wyrd-queue` to ordinary Rust gRPC.

use std::sync::Arc;

use async_trait::async_trait;
use wyrd_queue::{BatchSink, ClientByteGuard, DurableBatchAck, SealedBatch, SinkError};

/// The ordinary Rust transport seam for one owned batch guard type.
#[async_trait]
pub trait IngestTransport<G>: Send + Sync + 'static {
    /// Borrows one owned batch while the queue retains its recovery shell.
    ///
    /// # Errors
    ///
    /// Returns [`SinkError::Retryable`] when a timeout or transport loss leaves
    /// durable commit ambiguous, allowing the queue to retain the unchanged
    /// batch; terminal failure lets the queue consume the owner.
    async fn insert_batch(&self, batch: &SealedBatch<G>) -> Result<DurableBatchAck, SinkError>;
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
    /// Forwards a borrowed sealed owner to the ordinary-Rust transport.
    ///
    /// # Errors
    ///
    /// Propagates the transport's retryable or terminal settlement without
    /// consuming the queue-owned batch shell.
    async fn send(
        &self,
        batch: &SealedBatch<ClientByteGuard>,
    ) -> Result<DurableBatchAck, SinkError> {
        self.transport.insert_batch(batch).await
    }
}
