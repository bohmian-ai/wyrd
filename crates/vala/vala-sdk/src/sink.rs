//! The swap seam: [`BifrostIngestSink`], a `wyrd-queue` [`BatchSink`] that maps
//! a sealed Record batch onto the ingest RPC.

use std::sync::Arc;

use async_trait::async_trait;
use wyrd_queue::{BatchSink, SealedBatch};
use wyrd_spec::error::WyrdError;

/// The ingest RPC seam mirroring `wyrd.v1.BifrostIngestService::InsertBatch`.
///
/// One call ships one sealed frame: the destination `table`, the 16-byte
/// idempotency `batch_id` (proto `wyrd_batch_id`), frame sequence zero, and the
/// Arrow IPC payload (proto `arrow_ipc`, carrying the per-row `card_ref`/`run_id`
/// correlation columns). The concrete bidi transport must consume the matching
/// per-frame ACK before returning. This seam keeps
/// [`BifrostIngestSink`] fully unit-testable server-free.
#[async_trait]
pub trait IngestTransport: Send + Sync + 'static {
    /// Ship one sealed frame to the ingest service; return its admitted row count.
    ///
    /// Must be **idempotent on `(batch_id, frame_sequence)`** — a retry
    /// re-sends the same identity so the server does not duplicate durable rows.
    ///
    /// # Errors
    /// Returns a [`WyrdError`] mapped from the transport/server failure.
    async fn insert_batch(
        &self,
        table: &str,
        batch_id: [u8; 16],
        frame_sequence: u64,
        arrow_ipc: Vec<u8>,
    ) -> Result<u64, WyrdError>;
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
            .insert_batch(&batch.table, batch.batch_id, 0, batch.frames)
            .await
    }
}
