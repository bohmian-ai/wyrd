//! The staging buffer and its seal-and-send workhorse.
//!
//! [`RecordQueue`] is the concrete [`Flushable`] over the stage-2
//! `crossbeam_queue::ArrayQueue`. It owns the sink, the resolved schema, and the
//! config; `seal_and_send` drains the buffer, builds one user-only Arrow IPC
//! batch per `flush_max_rows` chunk, mints a stable `batch_id` per sealed batch,
//! and ships it — re-buffering the rows on any sink failure (no silent drop).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use arrow_schema::SchemaRef;
use crossbeam_queue::ArrayQueue;
use uuid::Uuid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

use crate::batch_builder::BatchBuilder;
use crate::config::QueueConfig;
use crate::error::WyrdQueueError;
use crate::producer::Counters;
use crate::sink::{BatchSink, SealedBatch};

/// One buffered record: a serialized JSON row plus its per-row correlation.
///
/// `card_ref` and `run_id` are per-row because the queue batches records from
/// different cards and runs before it flushes — correlation is row-grain, never
/// batch-grain. The queue treats both as opaque correlation values.
#[derive(Debug, Clone)]
pub struct Row {
    /// The `model_dump_json()` / `json.dumps` output for one record.
    pub json: Vec<u8>,
    /// Client-supplied card reference for this row.
    pub card_ref: CardRef,
    /// Optional client-supplied run identifier for this row.
    pub run_id: Option<RunId>,
}

/// The buffer-on-failure contract for the stage-2 staging buffer.
#[async_trait::async_trait]
pub trait Flushable: Send + Sync {
    /// Number of rows currently buffered.
    fn len(&self) -> usize;
    /// Whether the buffer is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Drain up to a batch, hand it to the sink, and return the rows flushed. On
    /// sink failure the sealed rows are re-buffered (no silent drop).
    ///
    /// # Errors
    /// Surfaces the queue-domain and sink errors from the seal/send path.
    async fn flush(&self) -> Result<usize, WyrdQueueError>;
}

/// Outcome of one seal-and-send pass.
#[derive(Debug, Default, Clone)]
pub struct FlushOutcome {
    /// The stable `batch_id`s sealed and sent in this pass (one per batch).
    pub batch_ids: Vec<[u8; 16]>,
    /// Total rows flushed across those batches.
    pub rows_flushed: usize,
}

/// Concrete staging buffer + seal-and-send workhorse.
pub struct RecordQueue {
    table: String,
    schema: SchemaRef,
    staging: Arc<ArrayQueue<Row>>,
    sink: Arc<dyn BatchSink>,
    config: QueueConfig,
    counters: Arc<Counters>,
}

impl RecordQueue {
    /// Construct the staging buffer over a shared `ArrayQueue`.
    pub(crate) fn new(
        table: String,
        schema: SchemaRef,
        staging: Arc<ArrayQueue<Row>>,
        sink: Arc<dyn BatchSink>,
        config: QueueConfig,
        counters: Arc<Counters>,
    ) -> Self {
        Self {
            table,
            schema,
            staging,
            sink,
            config,
            counters,
        }
    }

    /// Push a row into staging; returns the row back if the buffer is full.
    ///
    /// Returns the `Row` by value on rejection, mirroring `ArrayQueue::push` so
    /// the caller can re-buffer without a heap allocation on the hot ingest path;
    /// boxing the (large, `CardRef`-carrying) `Row` here would defeat that.
    #[allow(clippy::result_large_err)]
    pub(crate) fn push(&self, row: Row) -> Result<(), Row> {
        self.staging.push(row)
    }

    /// Current staging depth.
    pub(crate) fn staging_len(&self) -> usize {
        self.staging.len()
    }

    /// Buffer one row into staging, sealing once to make room if the buffer is
    /// full. A row that still cannot land after a seal is dropped with a counter
    /// bump (never silently) — the flush path is the only backpressure valve the
    /// stage-2 buffer has.
    pub(crate) async fn ingest(&self, row: Row) {
        if let Err(row) = self.push(row) {
            let _ = self.seal_and_send().await;
            if self.push(row).is_err() {
                self.counters.dropped.fetch_add(1, Ordering::SeqCst);
                tracing::warn!("row dropped: staging full after a seal");
            }
        }
    }

    fn drain(&self) -> Vec<Row> {
        let mut rows = Vec::with_capacity(self.staging.len());
        while let Some(row) = self.staging.pop() {
            rows.push(row);
        }
        rows
    }

    fn build_frames(&self, rows: &[Row]) -> Result<Vec<u8>, WyrdQueueError> {
        let mut builder = BatchBuilder::new(self.schema.clone());
        for row in rows {
            let json = std::str::from_utf8(&row.json)
                .map_err(|e| WyrdQueueError::SchemaParse(format!("row is not UTF-8: {e}")))?;
            builder.append_json_row(json, &row.card_ref, row.run_id.as_ref())?;
        }
        builder.finish_ipc()
    }

    fn rebuffer<I: IntoIterator<Item = Row>>(&self, rows: I) {
        let mut overflow = 0_u64;
        for row in rows {
            if self.staging.push(row).is_err() {
                overflow += 1;
            }
        }
        if overflow > 0 {
            self.counters.dropped.fetch_add(overflow, Ordering::SeqCst);
            tracing::warn!(overflow, "staging overflow on re-buffer; rows dropped");
        }
    }

    /// Drain staging, seal each `flush_max_rows` chunk into one IPC batch under a
    /// stable `batch_id`, and ship it. On sink failure or flush-deadline timeout,
    /// the affected rows are re-buffered.
    ///
    /// # Errors
    /// - [`WyrdQueueError::Sink`] if the sink rejects a batch.
    /// - [`WyrdQueueError::FlushTimeout`] if a `send` exceeds `flush_timeout_ms`.
    /// - [`WyrdQueueError::PayloadTooLarge`] if a single row exceeds `max_message_bytes`.
    /// - [`WyrdQueueError::SchemaParse`] / [`WyrdQueueError::ReservedColumn`] if a
    ///   row cannot be built.
    pub async fn seal_and_send(&self) -> Result<FlushOutcome, WyrdQueueError> {
        let mut rows = self.drain();
        if rows.is_empty() {
            return Ok(FlushOutcome::default());
        }

        let chunk_size = self.config.flush_max_rows.max(1);
        let mut pending: VecDeque<Vec<Row>> = VecDeque::new();
        while !rows.is_empty() {
            let rest = if rows.len() > chunk_size {
                rows.split_off(chunk_size)
            } else {
                Vec::new()
            };
            pending.push_back(std::mem::replace(&mut rows, rest));
        }

        let mut outcome = FlushOutcome::default();
        let timeout = Duration::from_millis(self.config.flush_timeout_ms.max(1));
        while let Some(mut chunk) = pending.pop_front() {
            let frames = match self.build_frames(&chunk) {
                Ok(frames) => frames,
                Err(err) => {
                    // Poison chunk: cannot build. Drop it (counted) and re-buffer the
                    // rest so no downstream row is lost.
                    self.counters
                        .dropped
                        .fetch_add(chunk.len() as u64, Ordering::SeqCst);
                    self.rebuffer(pending.into_iter().flatten());
                    return Err(err);
                }
            };

            if frames.len() > self.config.max_message_bytes {
                if chunk.len() <= 1 {
                    self.counters
                        .dropped
                        .fetch_add(chunk.len() as u64, Ordering::SeqCst);
                    self.rebuffer(pending.into_iter().flatten());
                    return Err(WyrdQueueError::PayloadTooLarge);
                }
                let mid = chunk.len() / 2;
                let right = chunk.split_off(mid);
                pending.push_front(right);
                pending.push_front(chunk);
                continue;
            }

            let batch_id = Uuid::now_v7().into_bytes();
            let rows_in = chunk.len();
            let sealed = SealedBatch {
                table: self.table.clone(),
                batch_id,
                frames,
                rows: rows_in as u64,
            };
            match tokio::time::timeout(timeout, self.sink.send(sealed)).await {
                Ok(Ok(_accepted)) => {
                    outcome.batch_ids.push(batch_id);
                    outcome.rows_flushed += rows_in;
                }
                Ok(Err(err)) => {
                    self.rebuffer(chunk.into_iter().chain(pending.into_iter().flatten()));
                    return Err(WyrdQueueError::Sink(err));
                }
                Err(_elapsed) => {
                    self.rebuffer(chunk.into_iter().chain(pending.into_iter().flatten()));
                    return Err(WyrdQueueError::FlushTimeout);
                }
            }
        }
        Ok(outcome)
    }
}

#[async_trait::async_trait]
impl Flushable for RecordQueue {
    fn len(&self) -> usize {
        self.staging.len()
    }

    async fn flush(&self) -> Result<usize, WyrdQueueError> {
        self.seal_and_send().await.map(|o| o.rows_flushed)
    }
}
