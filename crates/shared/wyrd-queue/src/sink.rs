//! Owned sealed batches and the bounded queue-to-transport handoff.
//!
//! A [`SealedBatch`] moves through staging, transport, and retry without cloning
//! its IPC payload or budget guard. A retryable sink failure leaves that exact
//! owner with the queue; an acknowledgement or terminal error consumes it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

/// The server acknowledgement that settles one durable batch identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableBatchAck {
    /// The identity the server durably accepted.
    pub batch_id: [u8; 16],
    /// The accepted row count used by queue metrics.
    pub rows: u64,
}

/// IPC bytes coupled to the exclusive client-budget guard that pays for them.
///
/// The guard is generic so `wyrd-queue` remains transport-free while the
/// ordinary Rust client uses [`crate::ClientByteGuard`].
#[derive(Debug)]
pub struct OwnedIpcBytes<G> {
    bytes: SharedIpcBytes,
    guard: G,
}

/// Clone-cheap immutable IPC storage shared only with a cancellable transport borrow.
///
/// A [`SealedBatch`] remains non-cloneable and keeps the exclusive client-byte
/// guard. Cloning this shell only extends the immutable frame allocation long
/// enough for an in-flight `Bytes` request after the queue has retained the
/// authoritative batch owner.
#[derive(Debug, Clone)]
pub struct SharedIpcBytes {
    /// Immutable frame allocation retained by the sealed-batch owner.
    bytes: Arc<Vec<u8>>,
}

impl SharedIpcBytes {
    /// Wraps an owned IPC allocation before it enters a sealed batch.
    #[must_use]
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: Arc::new(bytes),
        }
    }
}

impl AsRef<[u8]> for SharedIpcBytes {
    /// Borrows the immutable IPC allocation for `Bytes::from_owner`.
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

impl<G> OwnedIpcBytes<G> {
    /// Couples already-owned IPC bytes with their one exclusive reservation.
    #[must_use]
    pub fn new(bytes: Vec<u8>, guard: G) -> Self {
        Self {
            bytes: SharedIpcBytes::new(bytes),
            guard,
        }
    }

    /// Borrows the owned IPC bytes for a copy-free transport request build.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    /// Clones the dependency-free shared byte owner for a cancellable transport borrow.
    ///
    /// The clone increments only the `Arc` control block. It never copies the
    /// IPC allocation or its exclusive client budget guard, which remains held
    /// by this [`OwnedIpcBytes`] until terminal batch settlement.
    #[must_use]
    pub fn shared_bytes(&self) -> SharedIpcBytes {
        self.bytes.clone()
    }

    /// Returns the owned reservation after terminal settlement.
    #[must_use]
    pub fn into_guard(self) -> G {
        self.guard
    }
}

/// One non-cloneable IPC batch that is ready for transport.
#[derive(Debug)]
pub struct SealedBatch<G> {
    /// Destination table identifier, opaque to the queue.
    pub table: String,
    /// UUIDv7 idempotency identity minted before the first transport attempt.
    pub batch_id: [u8; 16],
    /// One owned Arrow IPC frame and its client-byte reservation.
    pub frame: OwnedIpcBytes<G>,
    /// The number of logical rows represented by this frame.
    pub rows: u64,
    /// Originating request every transport attempt of this frame carries, or
    /// `None` for a frame coalesced from unrelated rows, which mints one per
    /// attempt.
    pub request_id: Option<RequestId>,
}

impl<G> SealedBatch<G> {
    /// Borrows the owned IPC frame without exposing a mutable payload path.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.frame.bytes()
    }
}

/// Retryable or terminal failure from a sink that consumed a sealed batch.
#[derive(Debug)]
pub enum SinkError {
    /// The server either may not have acknowledged the operation or returned
    /// stable no-write `WYRD_VALA_429_INGEST_BUSY`; the queue retains the
    /// unchanged UUID, bytes, allocation guard, and retry permit until the
    /// exact owner reaches ACK or terminal settlement.
    Retryable(WyrdError),
    /// The batch reached a terminal outcome and its owner has been consumed.
    Terminal(WyrdError),
}

impl SinkError {
    /// Borrows the stable error without exposing a retry batch by accident.
    #[must_use]
    pub fn error(&self) -> &WyrdError {
        match self {
            Self::Retryable(error) | Self::Terminal(error) => error,
        }
    }
}

/// The queue's transport seam for one owned batch type.
#[async_trait::async_trait]
pub trait BatchSink<G>: Send + Sync + 'static {
    /// Borrows one owned batch and reports its transport settlement.
    ///
    /// The queue keeps the non-cloneable batch shell and guard while this
    /// future is pending. It can therefore cancel a deadline-expired future
    /// without losing the stable identity or owned bytes needed for retry.
    ///
    /// # Errors
    ///
    /// Returns [`SinkError::Retryable`] when commit acknowledgement is
    /// ambiguous or stable no-write `WYRD_VALA_429_INGEST_BUSY` requires the
    /// exact borrowed UUID, bytes, allocation guard, and retry permit to remain
    /// retained. Returns [`SinkError::Terminal`] only when the queue may consume
    /// the owner after a permanent rejection.
    async fn send(&self, batch: &SealedBatch<G>) -> Result<DurableBatchAck, SinkError>;
}

/// Snapshot-only test receipt that never recreates a queue ownership token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchReceipt {
    /// Destination table recorded by the test sink.
    pub table: String,
    /// Stable batch identity recorded by the test sink.
    pub batch_id: [u8; 16],
    /// A shared immutable view of the sent bytes for assertions.
    pub bytes: Vec<u8>,
    /// Logical row count.
    pub rows: u64,
    /// Originating request the settled frame carried.
    pub request_id: Option<RequestId>,
}

/// In-memory loopback sink for queue tests.
#[derive(Debug, Default)]
pub struct MockSink {
    received: Mutex<Vec<BatchReceipt>>,
    attempted: Mutex<Vec<[u8; 16]>>,
    fail_next: AtomicUsize,
    terminal_next: AtomicUsize,
}

impl MockSink {
    /// Constructs an empty test sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Arranges for the next `n` sends to return retryable ambiguity.
    pub fn fail_next(&self, n: usize) {
        self.fail_next.store(n, Ordering::SeqCst);
    }

    /// Arranges for the next `n` sends not already failing retryably to be
    /// terminally refused.
    pub fn terminal_next(&self, n: usize) {
        self.terminal_next.store(n, Ordering::SeqCst);
    }

    /// Returns assertion receipts rather than cloneable owned batches.
    #[must_use]
    pub fn received(&self) -> Vec<BatchReceipt> {
        self.received
            .lock()
            .expect("mock sink is not poisoned")
            .clone()
    }

    /// Returns every attempted identity, including retained ambiguous attempts.
    #[must_use]
    pub fn attempted(&self) -> Vec<[u8; 16]> {
        self.attempted
            .lock()
            .expect("mock sink is not poisoned")
            .clone()
    }
}

#[async_trait::async_trait]
impl<G: Send + Sync + 'static> BatchSink<G> for MockSink {
    /// Records a borrowed batch and returns a deterministic mock settlement.
    ///
    /// # Errors
    ///
    /// Returns a retryable service-unavailable error while `fail_next` still
    /// has budget; the queue retains the batch because this sink borrows it.
    /// Then returns a terminal validation refusal while `terminal_next` does.
    async fn send(&self, batch: &SealedBatch<G>) -> Result<DurableBatchAck, SinkError> {
        self.attempted
            .lock()
            .expect("mock sink is not poisoned")
            .push(batch.batch_id);
        loop {
            let remaining = self.fail_next.load(Ordering::SeqCst);
            if remaining == 0 {
                break;
            }
            if self
                .fail_next
                .compare_exchange(remaining, remaining - 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Err(SinkError::Retryable(WyrdError::ServiceUnavailable {
                    message: "mock sink forced ambiguous failure".to_owned(),
                    details: serde_json::json!({ "mock": true }),
                }));
            }
        }
        if self
            .terminal_next
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(SinkError::Terminal(WyrdError::Validation {
                message: "mock sink forced terminal refusal".to_owned(),
                details: serde_json::json!({ "mock": true }),
            }));
        }
        let receipt = BatchReceipt {
            table: batch.table.clone(),
            batch_id: batch.batch_id,
            bytes: batch.bytes().to_vec(),
            rows: batch.rows,
            request_id: batch.request_id.clone(),
        };
        let ack = DurableBatchAck {
            batch_id: receipt.batch_id,
            rows: receipt.rows,
        };
        self.received
            .lock()
            .expect("mock sink is not poisoned")
            .push(receipt);
        Ok(ack)
    }
}
