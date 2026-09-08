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
    /// The exact batch retained by the ambiguous transport outcome.
    batch: SealedBatch<ClientByteGuard>,
    /// Releases the global retry slot when the entry is settled or discarded.
    _permit: RetryPermit,
}

/// One send attempt whose ownership state determines retry-slot admission.
#[derive(Debug)]
enum SendEntry {
    /// A newly sealed batch that has not consumed retry cardinality.
    Fresh(SealedBatch<ClientByteGuard>),
    /// An ambiguous batch moving with its already-reserved retry permit.
    Retained(RetryEntry),
}

impl SendEntry {
    /// Borrows the stable batch identity and allocation for one sink attempt.
    fn batch(&self) -> &SealedBatch<ClientByteGuard> {
        match self {
            Self::Fresh(batch) => batch,
            Self::Retained(entry) => &entry.batch,
        }
    }

    /// Retains ambiguity, reserving cardinality only for a fresh batch.
    ///
    /// # Errors
    ///
    /// Returns backpressure only when a fresh ambiguity cannot reserve its
    /// first retry slot. A retained attempt reuses its existing permit.
    fn into_retry(self, budget: &ClientByteBudget) -> Result<RetryEntry, WyrdQueueError> {
        match self {
            Self::Fresh(batch) => Ok(RetryEntry {
                batch,
                _permit: budget.reserve_retry()?,
            }),
            Self::Retained(entry) => Ok(entry),
        }
    }
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

    /// Returns the oldest retained batch identity for deterministic retry scheduling.
    #[must_use]
    pub(crate) fn retry_batch_id(&self) -> Option<[u8; 16]> {
        self.retry
            .lock()
            .expect("retry lock is not poisoned")
            .front()
            .map(|entry| entry.batch.batch_id)
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
            builder.append_json_row(json, Some(&row.card_ref), row.run_id.as_ref())?;
        }
        builder.finish_ipc()
    }

    /// Adds one ambiguous attempt to the bounded retry set without replacing its permit.
    ///
    /// # Errors
    ///
    /// Returns backpressure only when a fresh ambiguity cannot reserve its
    /// first retry slot.
    fn retain_retry(&self, entry: SendEntry) -> Result<(), WyrdQueueError> {
        let entry = entry.into_retry(&self.budget)?;
        self.retry
            .lock()
            .expect("retry lock is not poisoned")
            .push_back(entry);
        Ok(())
    }

    /// Sends a borrowed batch within the configured deadline and records explicit ACKs.
    ///
    /// The queue owns `batch` across the entire await. A deadline cancels only
    /// the borrow-based sink future, then moves that untouched owner into the
    /// bounded retry state with the same UUIDv7.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::FlushTimeout`] after retaining the exact batch
    /// on deadline expiry, a sink error after retry or terminal settlement, or
    /// backpressure if the bounded retry state cannot retain the batch.
    async fn send_one(
        &self,
        entry: SendEntry,
        outcome: &mut FlushOutcome,
    ) -> Result<(), WyrdQueueError> {
        match tokio::time::timeout(self.config.flush_timeout(), self.sink.send(entry.batch())).await
        {
            Ok(Ok(DurableBatchAck { batch_id, rows })) => {
                outcome.batch_ids.push(batch_id);
                outcome.rows_flushed += rows as usize;
                Ok(())
            }
            Ok(Err(SinkError::Retryable(error))) => {
                self.retain_retry(entry)?;
                Err(WyrdQueueError::Sink(error))
            }
            Ok(Err(SinkError::Terminal(error))) => Err(WyrdQueueError::Sink(error)),
            Err(_) => {
                self.retain_retry(entry)?;
                Err(WyrdQueueError::FlushTimeout)
            }
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
                self.send_one(SendEntry::Retained(retry), &mut outcome)
                    .await?;
                continue;
            }
            break;
        }

        let mut rows = self.drain();
        let chunk_size = self.config.flush_max_rows();
        let mut chunks = VecDeque::new();
        while !rows.is_empty() {
            let rest = if rows.len() > chunk_size {
                rows.split_off(chunk_size)
            } else {
                Vec::new()
            };
            chunks.push_back(std::mem::replace(&mut rows, rest));
        }
        while let Some(mut chunk) = chunks.pop_front() {
            let frame_guard = match self.budget.reserve(self.config.max_message_bytes) {
                Ok(guard) => guard,
                Err(error) => {
                    chunk.extend(chunks.into_iter().flatten());
                    self.restore_unsealed(chunk);
                    return Err(error);
                }
            };
            let frame = self.build_frame(&chunk);
            let oversized = frame
                .as_ref()
                .is_ok_and(|frame| frame.len() > self.config.max_message_bytes);
            if frame.is_err() || oversized {
                drop(frame_guard);
                if chunk.len() > 1 {
                    let right = chunk.split_off(chunk.len() / 2);
                    chunks.push_front(right);
                    chunks.push_front(chunk);
                    continue;
                }
                self.counters.dropped.fetch_add(1, Ordering::AcqRel);
                let later = chunks.into_iter().flatten().collect();
                self.restore_unsealed(later);
                return match frame {
                    Err(error) => Err(error),
                    Ok(_) => Err(WyrdQueueError::PayloadTooLarge),
                };
            }
            let frame = frame.expect("successful frame result checked above");
            let frame_guard = frame_guard.resize(frame.len());
            let frame_guard = match frame_guard.attach_batch() {
                Ok(guard) => guard,
                Err(error) => {
                    chunk.extend(chunks.into_iter().flatten());
                    self.restore_unsealed(chunk);
                    return Err(error);
                }
            };
            let batch = SealedBatch {
                table: self.table.clone(),
                batch_id: Uuid::now_v7().into_bytes(),
                frame: OwnedIpcBytes::new(frame, frame_guard),
                rows: chunk.len() as u64,
            };
            drop(chunk);
            if let Err(error) = self.send_one(SendEntry::Fresh(batch), &mut outcome).await {
                self.restore_unsealed(chunks.into_iter().flatten().collect());
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

#[cfg(test)]
mod tests {
    //! Settlement tests for sealing, splitting, and restoration.

    use std::io::Cursor;

    use arrow::array::Int64Array;
    use arrow::ipc::reader::StreamReader;
    use arrow_schema::{DataType, Field, Schema};

    use super::*;
    use crate::sink::MockSink;

    /// Builds one charged queue row for a split-settlement test.
    fn row(budget: &ClientByteBudget, id: i64) -> Row {
        let json = format!(r#"{{"id":{id}}}"#).into_bytes();
        Row {
            _guard: budget.reserve(json.capacity()).expect("row bytes fit"),
            json,
            card_ref: "prod/Service/queue@1.0.0".parse().expect("valid test card"),
            run_id: None,
        }
    }

    /// Reads user IDs from an ordered sequence of IPC receipts.
    fn receipt_ids(receipts: &[crate::sink::BatchReceipt]) -> Vec<i64> {
        receipts
            .iter()
            .flat_map(|receipt| {
                StreamReader::try_new(Cursor::new(&receipt.bytes), None)
                    .expect("receipt is valid IPC")
                    .flat_map(|batch| {
                        let batch = batch.expect("receipt batch decodes");
                        let ids = batch
                            .column(0)
                            .as_any()
                            .downcast_ref::<Int64Array>()
                            .expect("id column is int64");
                        (0..ids.len())
                            .map(|index| ids.value(index))
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Aggregate oversize bisects left-first and preserves every row exactly once.
    #[tokio::test]
    async fn oversize_split_preserves_rows() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let budget = ClientByteBudget::new(QueueConfig::MAX_CLIENT_BYTE_LIMIT);
        let staging = Arc::new(ArrayQueue::new(8));
        let sink = Arc::new(MockSink::new());
        let counters = Arc::new(Counters::default());
        let probe = RecordQueue::new(
            "events".to_owned(),
            schema.clone(),
            staging.clone(),
            sink.clone(),
            QueueConfig::default(),
            budget.clone(),
            counters.clone(),
        );
        let singleton_bytes = probe
            .build_frame(&[row(&budget, 1)])
            .expect("singleton builds")
            .len();
        let aggregate_rows = (1..=4).map(|id| row(&budget, id)).collect::<Vec<_>>();
        let aggregate_bytes = probe
            .build_frame(&aggregate_rows)
            .expect("aggregate builds")
            .len();
        drop(aggregate_rows);
        assert!(aggregate_bytes > singleton_bytes);
        drop(probe);

        let queue = RecordQueue::new(
            "events".to_owned(),
            schema,
            staging,
            sink.clone(),
            QueueConfig {
                max_message_bytes: singleton_bytes,
                flush_max_rows: 4,
                ..QueueConfig::default()
            },
            budget,
            counters,
        );
        for id in 1..=4 {
            assert!(queue.push(row(&queue.budget, id)).is_none());
        }
        let outcome = queue.seal_and_send().await.expect("split leaves settle");
        assert_eq!(outcome.rows_flushed, 4);
        assert_eq!(receipt_ids(&sink.received()), vec![1, 2, 3, 4]);
        assert_eq!(queue.counters.dropped.load(Ordering::Acquire), 0);

        let poison_sink = Arc::new(MockSink::new());
        let poison_budget = ClientByteBudget::new(QueueConfig::MAX_CLIENT_BYTE_LIMIT);
        let poison_counters = Arc::new(Counters::default());
        let poison_queue = RecordQueue::new(
            "events".to_owned(),
            queue.schema.clone(),
            Arc::new(ArrayQueue::new(3)),
            poison_sink.clone(),
            QueueConfig {
                flush_max_rows: 3,
                ..QueueConfig::default()
            },
            poison_budget.clone(),
            poison_counters.clone(),
        );
        assert!(poison_queue.push(row(&poison_budget, 1)).is_none());
        let mut poison = row(&poison_budget, 2);
        poison.json = vec![0xff];
        assert!(poison_queue.push(poison).is_none());
        assert!(poison_queue.push(row(&poison_budget, 3)).is_none());
        assert!(matches!(
            poison_queue.seal_and_send().await,
            Err(WyrdQueueError::SchemaParse(_))
        ));
        assert_eq!(receipt_ids(&poison_sink.received()), vec![1]);
        assert_eq!(poison_queue.staging_len(), 1, "later row restored");
        assert_eq!(poison_counters.dropped.load(Ordering::Acquire), 1);
        poison_queue
            .seal_and_send()
            .await
            .expect("restored later row settles");
        assert_eq!(receipt_ids(&poison_sink.received()), vec![1, 3]);
    }
}
