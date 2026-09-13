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
    /// Returns [`SinkError::Retryable`] when timeout or transport loss leaves
    /// durable commit ambiguous, or when stable no-write
    /// `WYRD_VALA_429_INGEST_BUSY` requires retry. In both cases the queue
    /// retains the exact UUID, bytes, allocation guard, and retry permit.
    /// Terminal failure lets the queue consume that owner.
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
