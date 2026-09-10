//! Sends one caller-sealed Arrow batch under a stable durable identity.
//!
//! The row producer exists because JSON rows have to be accumulated, projected
//! and sealed before anything durable can happen. A caller that already holds
//! an Arrow [`RecordBatch`] has none of that work left: the batch *is* the
//! unit, so buffering it would only delay it and coalescing it with a neighbour
//! would destroy the one-batch-one-identity relationship the server's duplicate
//! suppression depends on. This owner therefore seals and sends exactly one
//! batch per call, reusing the same [`BatchSink`], byte budget and stable error
//! contract the producer path uses.

use std::sync::Arc;

use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use uuid::Uuid;

use crate::config::QueueConfig;
use crate::error::WyrdQueueError;
use crate::producer::{ClientByteBudget, ClientByteGuard};
use crate::sink::{BatchSink, DurableBatchAck, OwnedIpcBytes, SealedBatch, SinkError};

/// Attempts one identical sealed batch is sent before its ambiguity is fatal.
///
/// A retryable settlement means the server may or may not have committed the
/// batch. Resending the *same* `batch_id` is therefore safe — Scribe suppresses
/// the duplicate source — and is the only resolution that keeps the caller's
/// one logical batch one durable batch.
const SEND_ATTEMPTS: u32 = 3;

/// Initial backoff before the first identical resend.
const RETRY_BACKOFF_INITIAL_MS: u64 = 10;

/// Ceiling for the doubling resend backoff.
const RETRY_BACKOFF_MAX_MS: u64 = 1_000;

/// Seals and sends one caller-built Arrow batch per call.
///
/// Constructed from the same sink, budget and tuning the row producers of a
/// client handle share, so a caller-sealed batch is charged against the same
/// handle-wide byte ceiling and settles through the same transport as every
/// buffered row.
pub struct SealedBatchSender {
    /// The shared transport every batch from this client settles through.
    sink: Arc<dyn BatchSink<ClientByteGuard>>,
    /// The handle-wide byte ceiling this batch's frame is charged against.
    budget: ClientByteBudget,
    /// The frame ceiling and other accepted queue limits.
    config: QueueConfig,
}

impl SealedBatchSender {
    /// Builds a sender over one client handle's shared sink and byte budget.
    #[must_use]
    pub fn new(
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        budget: ClientByteBudget,
        config: QueueConfig,
    ) -> Self {
        Self {
            sink,
            budget,
            config,
        }
    }

    /// Seals `batch` for `table` and settles it under one stable identity.
    ///
    /// The batch is encoded exactly once, charged against the handle budget
    /// once, and given one UUIDv7 that survives every resend, so an ambiguous
    /// settlement resolves to the same durable batch rather than a second one.
    /// The schema is the caller's: this owner never adds, drops or reorders a
    /// column, because the destination table's contract is the server's to
    /// enforce and a client that pre-judged it would be a second implementation
    /// of it.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::PayloadTooLarge`] when the encoded frame
    /// exceeds the accepted message ceiling, [`WyrdQueueError::Backpressure`]
    /// when the handle's byte or live-batch envelope cannot cover it,
    /// [`WyrdQueueError::SchemaParse`] when the batch cannot be encoded, and
    /// [`WyrdQueueError::Sink`] carrying the server's stable refusal — or the
    /// last ambiguity after [`SEND_ATTEMPTS`] identical resends.
    pub async fn send(
        &self,
        table: &str,
        batch: &RecordBatch,
    ) -> Result<DurableBatchAck, WyrdQueueError> {
        let frame = encode_ipc(batch)?;
        if frame.len() > self.config.max_message_bytes {
            return Err(WyrdQueueError::PayloadTooLarge);
        }
        let guard = self.budget.reserve_sealed(frame.len())?;
        let rows = batch.num_rows() as u64;
        let sealed = SealedBatch {
            table: table.to_owned(),
            batch_id: Uuid::now_v7().into_bytes(),
            frame: OwnedIpcBytes::new(frame, guard),
            rows,
        };
        self.settle(&sealed).await
    }

    /// Settles one already-sealed batch, resending the same identity on doubt.
    ///
    /// # Errors
    ///
    /// Returns the terminal refusal immediately, or the final ambiguity once
    /// the identical resends are exhausted.
    async fn settle(
        &self,
        sealed: &SealedBatch<ClientByteGuard>,
    ) -> Result<DurableBatchAck, WyrdQueueError> {
        let mut backoff = RETRY_BACKOFF_INITIAL_MS;
        for attempt in 1..=SEND_ATTEMPTS {
            match self.sink.send(sealed).await {
                Ok(ack) => return Ok(ack),
                Err(SinkError::Terminal(error)) => return Err(WyrdQueueError::Sink(error)),
                Err(SinkError::Retryable(error)) => {
                    if attempt == SEND_ATTEMPTS {
                        return Err(WyrdQueueError::Sink(error));
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
                    backoff = (backoff * 2).min(RETRY_BACKOFF_MAX_MS);
                }
            }
        }
        unreachable!("the loop returns on its final attempt")
    }
}

/// Encodes one Arrow batch as a single-batch IPC stream.
///
/// # Errors
///
/// Returns [`WyrdQueueError::SchemaParse`] when the writer cannot be built,
/// cannot write the batch, or cannot close the stream.
fn encode_ipc(batch: &RecordBatch) -> Result<Vec<u8>, WyrdQueueError> {
    let mut buffer = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buffer, batch.schema_ref())
            .map_err(|e| WyrdQueueError::SchemaParse(format!("IPC writer init failed: {e}")))?;
        writer
            .write(batch)
            .map_err(|e| WyrdQueueError::SchemaParse(format!("IPC write failed: {e}")))?;
        writer
            .finish()
            .map_err(|e| WyrdQueueError::SchemaParse(format!("IPC finish failed: {e}")))?;
    }
    Ok(buffer)
}
