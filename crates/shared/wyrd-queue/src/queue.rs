//! The staging, sealing, outbox, and retry owner for one producer.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use arrow::record_batch::RecordBatch;
use arrow_schema::SchemaRef;
use tokio::time::Instant;
use uuid::Uuid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::ids::RunId;

use crate::batch_builder::BatchBuilder;
use crate::config::QueueConfig;
use crate::error::WyrdQueueError;
use crate::producer::{ClientByteBudget, ClientByteGuard, Counters, RetryPermit};
use crate::sealed_sender::encode_ipc;
use crate::sink::{BatchSink, DurableBatchAck, OwnedIpcBytes, SealedBatch, SinkError};

/// Initial ceiling for one retained ambiguity backoff.
const RETRY_BACKOFF_INITIAL_MS: u64 = 10;
/// Maximum ceiling for one retained ambiguity backoff.
const RETRY_BACKOFF_MAX_MS: u64 = 1_000;

/// One buffered JSON row and the client reservation that pays for its bytes.
#[derive(Debug)]
pub struct Row {
    /// The caller-owned JSON representation of one row.
    pub json: Vec<u8>,
    /// The optional per-row card correlation field.
    ///
    /// Card correlation is optional on the wire: an absent value stores the
    /// row against the authenticated principal with a null `card_uid`, so the
    /// buffered row carries the caller's choice rather than forcing one.
    pub card_ref: Option<CardRef>,
    /// The optional per-row run correlation field.
    pub run_id: Option<RunId>,
    /// The one handle-wide byte reservation held while the row is buffered.
    pub(crate) _guard: ClientByteGuard,
}

/// One caller-built Arrow batch and the reservation that pays for its arrays.
///
/// The batch is already the durable unit, so it never enters JSON staging: the
/// background owner encodes it as its own sealed batch.
#[derive(Debug)]
pub(crate) struct OwnedBatch {
    /// The caller's batch, sent with its schema unchanged.
    pub(crate) batch: RecordBatch,
    /// The handle-wide reservation charged for the batch's array memory.
    pub(crate) guard: ClientByteGuard,
    /// Originating request the batch's sealed frame is published under.
    pub(crate) request_id: Option<RequestId>,
}

/// One admitted unit of producer work carried by the data channel.
#[derive(Debug)]
pub(crate) enum Entry {
    /// Every JSON row of one logical record, admitted together and staged
    /// with its neighbours.
    Rows(Vec<Row>),
    /// An Arrow batch sealed alone under its own stable identity.
    Batch(OwnedBatch),
}

impl Entry {
    /// Returns the number of rows this entry carries.
    pub(crate) fn rows(&self) -> usize {
        match self {
            Self::Rows(rows) => rows.len(),
            Self::Batch(owned) => owned.batch.num_rows(),
        }
    }
}

/// A retained sealed batch, its retry slot, and when it may next be sent.
#[derive(Debug)]
struct RetryEntry {
    /// The exact batch retained by the ambiguous transport outcome.
    batch: SealedBatch<ClientByteGuard>,
    /// Ambiguous attempts so far, which sizes the next backoff.
    attempts: u32,
    /// Earliest instant the batch may be resent.
    due: Instant,
    /// Counts the entry in the handle's retry metric until it settles.
    _permit: RetryPermit,
}

/// One send attempt whose ownership state determines retry-slot reuse.
#[derive(Debug)]
enum SendEntry {
    /// A newly sealed batch that has not been retained yet.
    Fresh(SealedBatch<ClientByteGuard>),
    /// An ambiguous batch moving with its already-held retry slot.
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
}

/// One finished sink attempt, carrying its batch owner back for settlement.
pub(crate) struct Attempt {
    /// The sealed owner the attempt borrowed, returned untouched.
    entry: SendEntry,
    /// The sink settlement, or the elapsed send deadline.
    result: Result<Result<DurableBatchAck, SinkError>, tokio::time::error::Elapsed>,
}

/// One sink attempt in flight that owns its sealed batch until it settles.
///
/// The future moves the batch owner in, lends it to the sink under the send
/// deadline, and hands it back in an [`Attempt`]. Its owner can therefore keep
/// staging and sealing other rows while the network round trip runs, and can
/// still retain the exact batch identity and bytes when the outcome is
/// ambiguous.
pub(crate) struct InFlight(Pin<Box<dyn Future<Output = Attempt> + Send>>);

impl Future for InFlight {
    type Output = Attempt;

    /// Polls the boxed attempt.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Attempt> {
        self.get_mut().0.as_mut().poll(cx)
    }
}

/// How one finished attempt settled.
#[derive(Debug)]
pub(crate) enum Settled {
    /// The server durably acknowledged the batch, releasing its owner.
    Acked,
    /// The outcome was ambiguous; the batch is retained under its identity.
    Retained(WyrdQueueError),
    /// The server definitely refused the batch; its rows are a counted loss.
    Lost(WyrdQueueError),
}

impl Settled {
    /// Projects the settlement onto the caller-facing result.
    ///
    /// # Errors
    ///
    /// Returns the retained or lost batch's error.
    pub(crate) fn into_result(self) -> Result<(), WyrdQueueError> {
        match self {
            Self::Acked => Ok(()),
            Self::Retained(error) | Self::Lost(error) => Err(error),
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

/// Staged rows and their admitted JSON bytes, the seal trigger.
#[derive(Debug, Default)]
struct Staging {
    /// Rows awaiting a seal, in admission order.
    rows: VecDeque<Row>,
    /// Sum of the staged rows' JSON lengths.
    bytes: usize,
}

/// Staging, sealing, and retry owner for one destination table.
///
/// Staged rows are sealed synchronously into the outbox, a FIFO of fresh
/// sealed batches awaiting the sink. Retained ambiguous batches wait apart,
/// each until its own backoff elapses, so they never block fresh batches.
/// Every buffer grows only with admitted bytes; nothing is preallocated.
pub struct RecordQueue {
    table: String,
    schema: SchemaRef,
    staging: Mutex<Staging>,
    outbox: Mutex<VecDeque<SealedBatch<ClientByteGuard>>>,
    retained: Mutex<Vec<RetryEntry>>,
    sink: Arc<dyn BatchSink<ClientByteGuard>>,
    config: QueueConfig,
    budget: ClientByteBudget,
    counters: Arc<Counters>,
}

impl RecordQueue {
    /// Constructs one queue over one shared handle budget.
    pub(crate) fn new(
        table: String,
        schema: SchemaRef,
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        config: QueueConfig,
        budget: ClientByteBudget,
        counters: Arc<Counters>,
    ) -> Self {
        Self {
            table,
            schema,
            staging: Mutex::new(Staging::default()),
            outbox: Mutex::new(VecDeque::new()),
            retained: Mutex::new(Vec::new()),
            sink,
            config,
            budget,
            counters,
        }
    }

    /// Locks staging.
    ///
    /// # Panics
    ///
    /// Panics if the staging lock is poisoned.
    fn staging(&self) -> std::sync::MutexGuard<'_, Staging> {
        self.staging.lock().expect("staging lock is not poisoned")
    }

    /// Locks the outbox.
    ///
    /// # Panics
    ///
    /// Panics if the outbox lock is poisoned.
    fn outbox(&self) -> std::sync::MutexGuard<'_, VecDeque<SealedBatch<ClientByteGuard>>> {
        self.outbox.lock().expect("outbox lock is not poisoned")
    }

    /// Locks the retained set.
    ///
    /// # Panics
    ///
    /// Panics if the retained lock is poisoned.
    fn retained(&self) -> std::sync::MutexGuard<'_, Vec<RetryEntry>> {
        self.retained.lock().expect("retained lock is not poisoned")
    }

    /// Appends one admitted row to staging.
    pub(crate) fn push(&self, row: Row) {
        let mut staging = self.staging();
        staging.bytes += row.json.len();
        staging.rows.push_back(row);
        self.counters.staged.fetch_add(1, Ordering::AcqRel);
    }

    /// Returns the staged row count.
    #[must_use]
    pub(crate) fn staging_len(&self) -> usize {
        self.staging().rows.len()
    }

    /// Returns whether staged rows reached the message-size seal target.
    #[must_use]
    pub(crate) fn staging_full(&self) -> bool {
        self.staging().bytes >= self.config.max_message_bytes
    }

    /// Returns whether staging, the outbox, or the retained set still holds
    /// unsettled work.
    #[must_use]
    pub(crate) fn has_pending(&self) -> bool {
        self.staging_len() > 0 || !self.outbox().is_empty() || !self.retained().is_empty()
    }

    /// Returns whether any retained ambiguity still awaits reconciliation.
    #[must_use]
    pub(crate) fn has_retained(&self) -> bool {
        !self.retained().is_empty()
    }

    /// Returns the earliest instant a retained batch may be resent.
    #[must_use]
    pub(crate) fn next_retry_due(&self) -> Option<Instant> {
        self.retained().iter().map(|entry| entry.due).min()
    }

    /// Places one admitted entry without awaiting the sink.
    ///
    /// JSON rows enter staging. An Arrow batch is sealed alone onto the back
    /// of the outbox.
    ///
    /// # Errors
    ///
    /// Returns an Arrow batch's encode, size, or byte-budget error after
    /// settling its rows as lost. JSON rows never fail here.
    pub(crate) fn place(&self, entry: Entry) -> Result<(), WyrdQueueError> {
        match entry {
            Entry::Rows(rows) => {
                for row in rows {
                    self.push(row);
                }
                Ok(())
            }
            Entry::Batch(batch) => self.seal_batch(batch),
        }
    }

    /// Seals one Arrow batch as its own frame onto the back of the outbox.
    ///
    /// Before encoding, the array reservation grows to cover the arrays plus a
    /// frame of up to the message ceiling, since both are live while the
    /// encoder runs and the encoder cannot grow past that ceiling. The arrays
    /// are released as soon as the frame exists, and the reservation shrinks
    /// to the frame. The UUIDv7 minted here survives every retained retry.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] before encoding when the
    /// overlap does not fit, [`WyrdQueueError::PayloadTooLarge`] once the
    /// frame would exceed the message ceiling, and
    /// [`WyrdQueueError::SchemaParse`] when encoding fails; those rows are
    /// settled as lost.
    fn seal_batch(&self, owned: OwnedBatch) -> Result<(), WyrdQueueError> {
        let OwnedBatch {
            batch,
            guard,
            request_id,
        } = owned;
        let rows = batch.num_rows() as u64;
        let limit = self.config.max_message_bytes;
        let sealed = guard
            .fit(batch.get_array_memory_size().saturating_add(limit))
            .and_then(|guard| {
                let frame = encode_ipc(&batch, limit);
                drop(batch);
                let frame = frame?;
                let guard = guard.fit(frame.len())?.attach_batch();
                Ok(SealedBatch {
                    table: self.table.clone(),
                    batch_id: Uuid::now_v7().into_bytes(),
                    frame: OwnedIpcBytes::new(frame, guard),
                    rows,
                    request_id,
                })
            });
        match sealed {
            Ok(sealed) => {
                self.outbox().push_back(sealed);
                Ok(())
            }
            Err(error) => {
                self.settle_loss(rows, None, error.code());
                Err(error)
            }
        }
    }

    /// Takes every staged row, leaving staging empty.
    fn drain(&self) -> Vec<Row> {
        let mut staging = self.staging();
        staging.bytes = 0;
        self.counters
            .staged
            .fetch_sub(staging.rows.len(), Ordering::AcqRel);
        staging.rows.drain(..).collect()
    }

    /// Returns unsealed rows to the front of staging, ahead of any row
    /// admitted since they were taken.
    fn restore_unsealed(&self, rows: Vec<Row>) {
        let mut staging = self.staging();
        self.counters.staged.fetch_add(rows.len(), Ordering::AcqRel);
        for row in rows.into_iter().rev() {
            staging.bytes += row.json.len();
            staging.rows.push_front(row);
        }
    }

    /// Builds one IPC frame while a frame reservation of the message ceiling
    /// is live.
    ///
    /// The encoder is capped at that ceiling, so it never allocates past the
    /// reservation that pays for it.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::SchemaParse`] for a row that is not UTF-8 or
    /// does not fit the schema, and [`WyrdQueueError::PayloadTooLarge`] once
    /// the frame would exceed `max_message_bytes`.
    fn build_frame(&self, rows: &[Row]) -> Result<Vec<u8>, WyrdQueueError> {
        let mut builder = BatchBuilder::new(self.schema.clone());
        for row in rows {
            let json = std::str::from_utf8(&row.json).map_err(|error| {
                WyrdQueueError::SchemaParse(format!("row is not UTF-8: {error}"))
            })?;
            builder.append_json_row(json, row.card_ref.as_ref(), row.run_id.as_ref())?;
        }
        encode_ipc(&builder.finish()?, self.config.max_message_bytes)
    }

    /// Retains one ambiguous attempt until its backoff elapses.
    ///
    /// A fresh ambiguity takes a retry slot; a retained attempt keeps its
    /// own. Each ambiguous attempt lengthens the backoff, and the batch keeps
    /// its UUIDv7 and bytes.
    fn retain_retry(&self, entry: SendEntry) {
        let (batch, attempts, permit) = match entry {
            SendEntry::Fresh(batch) => (batch, 0, self.budget.reserve_retry()),
            SendEntry::Retained(entry) => (entry.batch, entry.attempts, entry._permit),
        };
        let due = Instant::now() + retry_backoff(batch.batch_id, attempts);
        self.retained().push(RetryEntry {
            batch,
            attempts: attempts.saturating_add(1),
            due,
            _permit: permit,
        });
    }

    /// Settles `rows` this queue accepted but will never publish, exactly once.
    ///
    /// Every post-admission discard routes here: an Arrow batch that cannot
    /// be sealed, a row too large to frame, and a terminal sink refusal. The
    /// rows are counted as dropped, and one warning names only the table,
    /// row count, stable `code`, and the batch and request identities when a
    /// sealed `owner` exists. The owner's bytes and any retry slot are then
    /// released before the handle's loss observer is told, so an observer
    /// reading the handle's ownership sees the settled values without a later
    /// enqueue.
    fn settle_loss(&self, rows: u64, owner: Option<SendEntry>, code: &str) {
        self.counters.dropped.fetch_add(rows, Ordering::AcqRel);
        let batch = owner.as_ref().map(SendEntry::batch);
        tracing::warn!(
            table = %self.table,
            rows,
            code,
            batch_id = ?batch.map(|batch| Uuid::from_bytes(batch.batch_id)),
            request_id = ?batch.and_then(|batch| batch.request_id.as_ref()).map(RequestId::as_str),
            "bifrost rows lost before durable acknowledgement"
        );
        drop(owner);
        self.budget.report_loss(rows);
    }

    /// Takes the next sendable batch into one owned sink attempt.
    ///
    /// A retained batch due by `cutoff` goes first, then the head of the
    /// outbox. A retained attempt is re-retained with a due time after its
    /// own settlement, so a caller that holds `cutoff` at the start of a pass
    /// attempts each retained batch at most once in that pass. The attempt
    /// lends the batch to the sink under the send deadline; a deadline
    /// cancels only the borrowing sink future, and the untouched owner
    /// returns in the [`Attempt`] with the same UUIDv7.
    pub(crate) fn start(&self, cutoff: Instant) -> Option<InFlight> {
        let entry = {
            let mut retained = self.retained();
            retained
                .iter()
                .position(|entry| entry.due <= cutoff)
                .map(|index| SendEntry::Retained(retained.swap_remove(index)))
        }
        .or_else(|| self.outbox().pop_front().map(SendEntry::Fresh))?;
        let sink = Arc::clone(&self.sink);
        let deadline = self.config.flush_timeout();
        Some(InFlight(Box::pin(async move {
            let result = tokio::time::timeout(deadline, sink.send(entry.batch())).await;
            Attempt { entry, result }
        })))
    }

    /// Settles one finished attempt and records an explicit ACK in `outcome`.
    ///
    /// An acknowledgement releases the owner. An ambiguous result or an
    /// elapsed deadline retains the exact owner for a later retry, and a
    /// terminal refusal settles its rows as lost.
    pub(crate) fn settle(&self, attempt: Attempt, outcome: &mut FlushOutcome) -> Settled {
        let Attempt { entry, result } = attempt;
        match result {
            Ok(Ok(DurableBatchAck { batch_id, rows })) => {
                outcome.batch_ids.push(batch_id);
                outcome.rows_flushed += rows as usize;
                Settled::Acked
            }
            Ok(Err(SinkError::Retryable(error))) => {
                self.retain_retry(entry);
                Settled::Retained(WyrdQueueError::Sink(error))
            }
            Ok(Err(SinkError::Terminal(error))) => {
                self.settle_loss(entry.batch().rows, Some(entry), error.code());
                Settled::Lost(WyrdQueueError::Sink(error))
            }
            Err(_) => {
                self.retain_retry(entry);
                Settled::Retained(WyrdQueueError::FlushTimeout)
            }
        }
    }

    /// Sends every sendable batch one at a time, awaiting each.
    ///
    /// # Errors
    ///
    /// Returns the first non-acknowledged settlement; the batches behind it
    /// stay in the outbox.
    async fn send_outbox(&self, outcome: &mut FlushOutcome) -> Result<(), WyrdQueueError> {
        while let Some(attempt) = self.start(Instant::now()) {
            self.settle(attempt.await, outcome).into_result()?;
        }
        Ok(())
    }

    /// Seals all staged rows onto the back of the outbox without awaiting the sink.
    ///
    /// Rows are chunked so each chunk's JSON stays within the message
    /// ceiling. Each chunk reserves a frame of the message ceiling from the
    /// sealing headroom before encoding, then shrinks it to the encoded size.
    /// A chunk that cannot be framed is bisected so every encodable row still
    /// seals; a single row that cannot be framed is settled as lost. Rows
    /// already sealed stay in the outbox even when a later chunk fails.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::Backpressure`] when the frame reservation is
    /// refused, restoring every unsealed row to staging, and the framing
    /// error of a lost row after restoring the rows behind it.
    pub(crate) fn seal(&self) -> Result<(), WyrdQueueError> {
        let mut chunks = VecDeque::new();
        let mut chunk = Vec::new();
        let mut chunk_bytes = 0;
        for row in self.drain() {
            if !chunk.is_empty() && chunk_bytes + row.json.len() > self.config.max_message_bytes {
                chunks.push_back(std::mem::take(&mut chunk));
                chunk_bytes = 0;
            }
            chunk_bytes += row.json.len();
            chunk.push(row);
        }
        if !chunk.is_empty() {
            chunks.push_back(chunk);
        }
        while let Some(mut chunk) = chunks.pop_front() {
            let frame_guard = match self.budget.reserve_sealing(self.config.max_message_bytes) {
                Ok(guard) => guard,
                Err(error) => {
                    chunk.extend(chunks.into_iter().flatten());
                    self.restore_unsealed(chunk);
                    return Err(error);
                }
            };
            let frame = match self.build_frame(&chunk) {
                Ok(frame) => frame,
                Err(_) if chunk.len() > 1 => {
                    drop(frame_guard);
                    let right = chunk.split_off(chunk.len() / 2);
                    chunks.push_front(right);
                    chunks.push_front(chunk);
                    continue;
                }
                Err(error) => {
                    drop(frame_guard);
                    drop(chunk);
                    self.settle_loss(1, None, error.code());
                    self.restore_unsealed(chunks.into_iter().flatten().collect());
                    return Err(error);
                }
            };
            let frame_guard = frame_guard.resize(frame.len()).attach_batch();
            let batch = SealedBatch {
                table: self.table.clone(),
                batch_id: Uuid::now_v7().into_bytes(),
                frame: OwnedIpcBytes::new(frame, frame_guard),
                rows: chunk.len() as u64,
                request_id: None,
            };
            drop(chunk);
            self.outbox().push_back(batch);
        }
        Ok(())
    }

    /// Sends the outbox, seals staging, and sends what that sealed, in order.
    ///
    /// Batches sealed before a framing failure are still sent before the
    /// failure is reported, so one unframeable row never holds back its
    /// neighbours.
    ///
    /// # Errors
    ///
    /// Returns the first send settlement error, or [`Self::seal`]'s error once
    /// every batch it did seal has been sent.
    pub(crate) async fn flush_into(
        &self,
        outcome: &mut FlushOutcome,
    ) -> Result<(), WyrdQueueError> {
        self.send_outbox(outcome).await?;
        let sealed = self.seal();
        self.send_outbox(outcome).await?;
        sealed
    }

    /// Seals and sends all currently available work, moving rather than
    /// cloning each payload.
    ///
    /// # Errors
    ///
    /// Returns queue backpressure, serialization, terminal sink, or ambiguous
    /// transport errors. Retryable sink errors retain the original sealed owner.
    pub async fn seal_and_send(&self) -> Result<FlushOutcome, WyrdQueueError> {
        let mut outcome = FlushOutcome::default();
        self.flush_into(&mut outcome).await?;
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

/// Derives deterministic equal jitter from a stable batch and attempt number.
///
/// The exponential ceiling starts at 10 ms and caps at one second. The stable
/// mixer chooses within the inclusive upper half of that ceiling, avoiding
/// global randomness while preventing every retained batch from retrying in
/// lockstep.
#[must_use]
pub(crate) fn retry_backoff(batch_id: [u8; 16], attempt: u32) -> Duration {
    let shift = attempt.min(63);
    let ceiling = RETRY_BACKOFF_INITIAL_MS
        .checked_shl(shift)
        .unwrap_or(u64::MAX)
        .min(RETRY_BACKOFF_MAX_MS);
    let floor = ceiling / 2;
    let mut mixed = u64::from(attempt).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for byte in batch_id {
        mixed ^= u64::from(byte);
        mixed = mixed.wrapping_mul(0x100_0000_01b3);
    }
    Duration::from_millis(floor + mixed % (ceiling - floor + 1))
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
            card_ref: Some("prod/Service/queue@1.0.0".parse().expect("valid test card")),
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

    /// Seals `batch`, admitted with its array reservation, through a fresh
    /// queue over a `budget_bytes` handle and `max_message_bytes` ceiling,
    /// returning the result, dropped rows, published batches, and the bytes
    /// still reserved once the queue is gone.
    ///
    /// # Panics
    ///
    /// Panics when the budget cannot admit the batch's arrays.
    async fn seal(
        batch: &RecordBatch,
        budget_bytes: usize,
        max_message_bytes: usize,
    ) -> (Result<(), WyrdQueueError>, u64, usize, usize) {
        let budget = ClientByteBudget::new(budget_bytes);
        let sink = Arc::new(MockSink::new());
        let counters = Arc::new(Counters::default());
        let queue = RecordQueue::new(
            "events".to_owned(),
            batch.schema(),
            sink.clone(),
            QueueConfig {
                max_message_bytes,
                ..QueueConfig::default()
            },
            budget.clone(),
            counters.clone(),
        );
        let owned = OwnedBatch {
            guard: budget
                .reserve(batch.get_array_memory_size())
                .expect("the arrays are admitted"),
            batch: batch.clone(),
            request_id: None,
        };
        let result = match queue.place(Entry::Batch(owned)) {
            Ok(()) => queue.send_outbox(&mut FlushOutcome::default()).await,
            Err(error) => Err(error),
        };
        drop(queue);
        (
            result,
            counters.dropped.load(Ordering::Acquire),
            sink.received().len(),
            budget.used_bytes(),
        )
    }

    /// For a long and a schema-heavy wide batch, sealing reserves the arrays
    /// plus the message ceiling before encoding: a near-ceiling budget seals,
    /// one byte less refuses before the encoder allocates, and an encoding
    /// past the ceiling stops at it. Each refusal settles the rows once and
    /// every outcome releases all bytes.
    #[tokio::test]
    async fn arrow_sealing_reserves_the_encoding_overlap_first() {
        let long = RecordBatch::try_from_iter([(
            "id",
            Arc::new(Int64Array::from_iter_values(0..1024)) as arrow::array::ArrayRef,
        )])
        .expect("long batch builds");
        let wide = RecordBatch::try_from_iter((0..256).map(|column| {
            (
                format!("column_with_a_long_descriptive_name_{column}"),
                Arc::new(Int64Array::from(vec![column])) as arrow::array::ArrayRef,
            )
        }))
        .expect("wide batch builds");
        for batch in [long, wide] {
            let rows = batch.num_rows() as u64;
            let arrays = batch.get_array_memory_size();
            let frame = encode_ipc(&batch, usize::MAX)
                .expect("unbounded encoding")
                .len();
            let near = seal(&batch, arrays + frame, frame).await;
            assert!(near.0.is_ok(), "{:?}", near.0);
            assert_eq!((near.1, near.2, near.3), (0, 1, 0));
            let short = seal(&batch, arrays + frame - 1, frame).await;
            assert!(matches!(short.0, Err(WyrdQueueError::Backpressure)));
            assert_eq!((short.1, short.2, short.3), (rows, 0, 0));
            let oversized = seal(&batch, QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT, frame - 1).await;
            assert!(matches!(oversized.0, Err(WyrdQueueError::PayloadTooLarge)));
            assert_eq!((oversized.1, oversized.2, oversized.3), (rows, 0, 0));
        }
    }

    /// Aggregate oversize bisects left-first and preserves every row exactly once.
    #[tokio::test]
    async fn oversize_split_preserves_rows() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let sink = Arc::new(MockSink::new());
        let counters = Arc::new(Counters::default());
        let probe = RecordQueue::new(
            "events".to_owned(),
            schema.clone(),
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
            sink.clone(),
            QueueConfig {
                max_message_bytes: singleton_bytes,
                ..QueueConfig::default()
            },
            budget,
            counters,
        );
        for id in 1..=4 {
            queue.push(row(&queue.budget, id));
        }
        let outcome = queue.seal_and_send().await.expect("split leaves settle");
        assert_eq!(outcome.rows_flushed, 4);
        assert_eq!(receipt_ids(&sink.received()), vec![1, 2, 3, 4]);
        assert_eq!(queue.counters.dropped.load(Ordering::Acquire), 0);

        let poison_sink = Arc::new(MockSink::new());
        let poison_budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let poison_counters = Arc::new(Counters::default());
        let poison_queue = RecordQueue::new(
            "events".to_owned(),
            queue.schema.clone(),
            poison_sink.clone(),
            QueueConfig::default(),
            poison_budget.clone(),
            poison_counters.clone(),
        );
        poison_queue.push(row(&poison_budget, 1));
        let mut poison = row(&poison_budget, 2);
        poison.json = vec![0xff];
        poison_queue.push(poison);
        poison_queue.push(row(&poison_budget, 3));
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

    /// A fixed identity produces the exact deterministic sequence through the
    /// one-second cap, and an ambiguous send is retained with its first
    /// backoff as its due time.
    #[tokio::test]
    async fn retained_retry_backoff_is_deterministic_and_capped() {
        let delays = (0..12)
            .map(|attempt| retry_backoff([0; 16], attempt).as_millis() as u64)
            .collect::<Vec<_>>();
        assert_eq!(
            delays,
            vec![5, 15, 27, 47, 157, 293, 358, 975, 692, 910, 627, 796]
        );
        for (attempt, delay) in delays.iter().enumerate() {
            let ceiling = 10_u64
                .checked_shl((attempt as u32).min(63))
                .unwrap_or(u64::MAX)
                .min(1_000);
            assert!(*delay >= ceiling / 2);
            assert!(*delay <= ceiling);
        }

        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let sink = Arc::new(MockSink::new());
        sink.fail_next(1);
        let queue = RecordQueue::new(
            "events".to_owned(),
            Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)])),
            sink.clone(),
            QueueConfig::default(),
            budget.clone(),
            Arc::new(Counters::default()),
        );
        queue.push(row(&budget, 1));
        let before = Instant::now();
        assert!(queue.seal_and_send().await.is_err());
        let after = Instant::now();
        let backoff = retry_backoff(sink.attempted()[0], 0);
        let due = queue
            .next_retry_due()
            .expect("the ambiguous batch is retained");
        assert!(due >= before + backoff && due <= after + backoff);
        assert_eq!(budget.metrics().retry_entries, 1);
    }

    /// A retained batch is started only when it was due by the caller's
    /// cutoff: once its retry settles ambiguously again, the same cutoff no
    /// longer starts it even after its new backoff elapses, so a drain pass
    /// that holds its start instant attempts each retained batch once and
    /// cannot be kept open by batches re-arming one another.
    ///
    /// # Panics
    ///
    /// Panics when the batch is not retained after an ambiguous attempt, the
    /// held cutoff restarts it, or a later cutoff does not.
    #[tokio::test(start_paused = true)]
    async fn a_held_cutoff_starts_each_retained_batch_once() {
        let budget = ClientByteBudget::new(QueueConfig::DEFAULT_CLIENT_BYTE_LIMIT);
        let sink = Arc::new(MockSink::new());
        let queue = RecordQueue::new(
            "events".to_owned(),
            Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)])),
            sink.clone(),
            QueueConfig::default(),
            budget.clone(),
            Arc::new(Counters::default()),
        );
        queue
            .place(Entry::Rows(vec![row(&budget, 1)]))
            .expect("row staged");
        queue.seal().expect("row sealed");
        sink.fail_next(2);

        let first = queue.start(Instant::now()).expect("the fresh batch starts");
        queue.settle(first.await, &mut FlushOutcome::default());
        let due = queue.next_retry_due().expect("the ambiguity is retained");
        tokio::time::advance(due - Instant::now()).await;

        let cutoff = Instant::now();
        let retry = queue.start(cutoff).expect("the due batch starts once");
        queue.settle(retry.await, &mut FlushOutcome::default());
        let due = queue.next_retry_due().expect("the retry is retained again");
        tokio::time::advance(due - Instant::now()).await;

        assert!(
            queue.start(cutoff).is_none(),
            "the held cutoff does not restart a batch re-retained after it"
        );
        let resend = queue
            .start(Instant::now())
            .expect("a later pass retries the batch");
        queue.settle(resend.await, &mut FlushOutcome::default());
        assert_eq!(sink.attempted().len(), 3);
    }
}
