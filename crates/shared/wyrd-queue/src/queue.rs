//! The bounded staging, sealing, and retry owner for one producer.

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use arrow_schema::SchemaRef;
use crossbeam_queue::ArrayQueue;
use uuid::Uuid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

use crate::batch_builder::BatchBuilder;
use crate::config::QueueConfig;
use crate::error::WyrdQueueError;
use crate::producer::{ClientByteBudget, ClientByteGuard, Counters, RetryPermit};
use crate::sink::{BatchSink, DurableBatchAck, OwnedIpcBytes, SealedBatch, SinkError};

/// One buffered JSON row and the client reservation that pays for its bytes.
#[derive(Debug)]
pub struct Row {
    /// The caller-owned JSON representation of one row.
    pub json: Vec<u8>,
    /// The per-row card correlation field.
    pub card_ref: CardRef,
    /// The optional per-row run correlation field.
    pub run_id: Option<RunId>,
    /// The one handle-wide byte reservation held while the row is buffered.
    pub(crate) _guard: ClientByteGuard,
}

/// The retry state couples a retained sealed batch with its bounded retry slot.
#[derive(Debug)]
struct RetryEntry {
    /// The exact batch returned by the ambiguous transport outcome.
    batch: SealedBatch<ClientByteGuard>,
    /// Releases the global retry slot when the entry is settled or discarded.
    _permit: RetryPermit,
}

/// Result of one sealing pass.
#[derive(Debug, Default, Clone)]
pub struct FlushOutcome {
    /// Identities durably acknowledged during the pass.
    pub batch_ids: Vec<[u8; 16]>,
    /// Rows durably acknowledged during the pass.
    pub rows_flushed: usize,
}

/// Minimal flush surface retained for queue consumers that do not need control commands.
#[async_trait::async_trait]
pub trait Flushable: Send + Sync {
    /// Flushes pending work through the durable sink.
    ///
    /// # Errors
    ///
    /// Returns serialization, backpressure, or transport settlement errors.
    async fn flush(&self) -> Result<usize, WyrdQueueError>;
}

/// Concrete bounded staging and retry owner for one destination table.
pub struct RecordQueue {
    table: String,
    schema: SchemaRef,
    staging: Arc<ArrayQueue<Row>>,
    retry: Mutex<VecDeque<RetryEntry>>,
    sink: Arc<dyn BatchSink<ClientByteGuard>>,
    config: QueueConfig,
    budget: ClientByteBudget,
    counters: Arc<Counters>,
}

impl RecordQueue {
    /// Constructs one queue over pre-bounded staging and one shared handle budget.
    pub(crate) fn new(
        table: String,
        schema: SchemaRef,
        staging: Arc<ArrayQueue<Row>>,
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        config: QueueConfig,
        budget: ClientByteBudget,
        counters: Arc<Counters>,
    ) -> Self {
        Self {
            table,
            schema,
            staging,
            retry: Mutex::new(VecDeque::new()),
            sink,
            config,
            budget,
            counters,
        }
    }

    /// Pushes a pre-reserved row into fixed staging, returning it on saturation.
    pub(crate) fn push(&self, row: Row) -> Option<Row> {
        self.staging.push(row).err()
    }

    /// Returns the current fixed staging depth for producer-triggered sealing.
    #[must_use]
    pub(crate) fn staging_len(&self) -> usize {
        self.staging.len()
    }

    /// Returns whether staging or retained ambiguous batches remain.
    #[must_use]
    pub(crate) fn has_pending(&self) -> bool {
        !self.staging.is_empty()
            || !self
                .retry
                .lock()
                .expect("retry lock is not poisoned")
                .is_empty()
    }

    /// Transfers one row to staging or records visible backpressure after one seal.
    pub(crate) async fn ingest(&self, row: Row) {
        if let Some(row) = self.push(row) {
            let _ = self.seal_and_send().await;
            if self.push(row).is_some() {
                self.counters.dropped.fetch_add(1, Ordering::SeqCst);
                tracing::warn!("row rejected: fixed staging remains full after seal");
            }
        }
    }

    /// Drains the fixed staging ring without allocating a second payload owner.
    fn drain(&self) -> Vec<Row> {
        let mut rows = Vec::with_capacity(self.staging.len());
        while let Some(row) = self.staging.pop() {
            rows.push(row);
        }
        rows
    }

    /// Restores unsealed rows after an earlier chunk enters retry.
    ///
    /// Staging was drained from this same fixed ring immediately before this
    /// method, so every row must fit back without allocation or loss.
    ///
    /// # Panics
    ///
    /// Panics only if the fixed ring violates its own drain-then-restore
    /// capacity invariant, indicating an internal queue ownership defect.
    fn restore_unsealed(&self, rows: Vec<Row>) {
        for row in rows {
            self.staging
                .push(row)
                .expect("drained fixed staging capacity must restore unsealed rows");
        }
    }

    /// Builds one IPC frame while a conservative frame reservation is live.
    fn build_frame(&self, rows: &[Row]) -> Result<Vec<u8>, WyrdQueueError> {
        let mut builder = BatchBuilder::new(self.schema.clone());
        for row in rows {
            let json = std::str::from_utf8(&row.json).map_err(|error| {
                WyrdQueueError::SchemaParse(format!("row is not UTF-8: {error}"))
            })?;
            builder.append_json_row(json, &row.card_ref, row.run_id.as_ref())?;
        }
        builder.finish_ipc()
    }

    /// Adds one ambiguous batch to the bounded retry set before retaining it.
    fn retain_retry(&self, batch: SealedBatch<ClientByteGuard>) -> Result<(), WyrdQueueError> {
        let permit = self.budget.reserve_retry()?;
        self.retry
            .lock()
            .expect("retry lock is not poisoned")
            .push_back(RetryEntry {
                batch,
                _permit: permit,
            });
        Ok(())
    }

    /// Sends a batch once and records only an explicit durable acknowledgement.
    async fn send_one(
        &self,
        batch: SealedBatch<ClientByteGuard>,
        outcome: &mut FlushOutcome,
    ) -> Result<(), WyrdQueueError> {
        match self.sink.send(batch).await {
            Ok(DurableBatchAck { batch_id, rows }) => {
                outcome.batch_ids.push(batch_id);
                outcome.rows_flushed += rows as usize;
                Ok(())
            }
            Err(SinkError::Retryable { error, batch }) => {
                self.retain_retry(batch)?;
                Err(WyrdQueueError::Sink(error))
            }
            Err(SinkError::Terminal(error)) => Err(WyrdQueueError::Sink(error)),
        }
    }

    /// Seals all currently available work, moving rather than cloning each payload.
    ///
    /// # Errors
    ///
    /// Returns queue backpressure, serialization, terminal sink, or ambiguous
    /// transport errors. Retryable sink errors retain the original sealed owner.
    pub async fn seal_and_send(&self) -> Result<FlushOutcome, WyrdQueueError> {
        let mut outcome = FlushOutcome::default();
        loop {
            let retry = self
                .retry
                .lock()
                .expect("retry lock is not poisoned")
                .pop_front();
            if let Some(retry) = retry {
                self.send_one(retry.batch, &mut outcome).await?;
                continue;
            }
            break;
        }

        let mut rows = self.drain();
        let chunk_size = self.config.flush_max_rows();
        while !rows.is_empty() {
            let rest = if rows.len() > chunk_size {
                rows.split_off(chunk_size)
            } else {
                Vec::new()
            };
            let chunk = std::mem::replace(&mut rows, rest);
            let frame_guard = self.budget.reserve(self.config.max_message_bytes)?;
            let frame = self.build_frame(&chunk)?;
            if frame.len() > self.config.max_message_bytes {
                return Err(WyrdQueueError::PayloadTooLarge);
            }
            let frame_guard = frame_guard.resize(frame.len());
            let frame_guard = frame_guard.attach_batch()?;
            let batch = SealedBatch {
                table: self.table.clone(),
                batch_id: Uuid::now_v7().into_bytes(),
                frame: OwnedIpcBytes::new(frame, frame_guard),
                rows: chunk.len() as u64,
            };
            drop(chunk);
            if let Err(error) = self.send_one(batch, &mut outcome).await {
                self.restore_unsealed(rows);
                return Err(error);
            }
        }
        Ok(outcome)
    }
}

#[async_trait::async_trait]
impl Flushable for RecordQueue {
    async fn flush(&self) -> Result<usize, WyrdQueueError> {
        self.seal_and_send()
            .await
            .map(|outcome| outcome.rows_flushed)
    }
}
