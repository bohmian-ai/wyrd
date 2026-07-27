//! The swap seam: [`BifrostIngestSink`], a `wyrd-queue` [`BatchSink`] that maps
//! a sealed Record batch onto the ingest RPC.

use std::sync::Arc;

use async_trait::async_trait;
use wyrd_queue::{BatchSink, SealedBatch};
use wyrd_spec::error::WyrdError;

/// The ingest RPC seam mirroring `wyrd.v1.BifrostIngestService::InsertBatch`.
/// One call ships one sealed batch and returns after the exact batch identity is
/// acknowledged. This seam keeps
/// [`BifrostIngestSink`] fully unit-testable server-free.
#[async_trait]
pub trait IngestTransport: Send + Sync + 'static {
    /// Ship one sealed batch to the ingest service.
    ///
    /// Must be idempotent on `batch_id` during the server's retention window.
    ///
    /// # Errors
    /// Returns a [`WyrdError`] mapped from the transport/server failure.
    async fn insert_batch(
        &self,
        table: &str,
        batch_id: [u8; 16],
        arrow_ipc: Vec<u8>,
    ) -> Result<(), WyrdError>;
}

/// The Record-kind [`BatchSink`]: hands each sealed batch to the ingest RPC.
///
/// The sink adds no state of its own beyond the transport — `batch_id` is
/// forwarded verbatim into the RPC, so idempotency is preserved across the
/// producer's re-send-on-failure path.
pub struct BifrostIngestSink {
    transport: Arc<dyn IngestTransport>,
}

impl BifrostIngestSink {
    /// Build the sink over an [`IngestTransport`].
    #[must_use]
    pub fn new(transport: Arc<dyn IngestTransport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl BatchSink for BifrostIngestSink {
    async fn send(&self, batch: SealedBatch) -> Result<u64, WyrdError> {
        self.transport
            .insert_batch(&batch.table, batch.batch_id, batch.frames)
            .await?;
        Ok(batch.rows)
    }
}
