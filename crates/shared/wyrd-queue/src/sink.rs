//! Owned sealed batches and the bounded queue-to-transport handoff.
//!
//! A [`SealedBatch`] moves through staging, transport, and retry without cloning
//! its IPC payload or budget guard. A retryable sink failure returns that exact
//! owner; an acknowledgement or terminal error consumes it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use wyrd_spec::error::WyrdError;

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
    bytes: Vec<u8>,
    guard: G,
}

impl<G> OwnedIpcBytes<G> {
    /// Couples already-owned IPC bytes with their one exclusive reservation.
    #[must_use]
    pub fn new(bytes: Vec<u8>, guard: G) -> Self {
        Self { bytes, guard }
    }

    /// Borrows the owned IPC bytes for a copy-free transport request build.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the owned reservation after terminal settlement.
    #[must_use]
    pub fn into_guard(self) -> G {
        self.guard
    }

    /// Splits this owner so the transport can zero-copy convert the IPC vector.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, G) {
        (self.bytes, self.guard)
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
pub enum SinkError<G> {
    /// The server may not have acknowledged the operation; retry the returned
    /// exact owner with the same batch identity.
    Retryable {
        /// The stable transport or server error.
        error: WyrdError,
        /// The unchanged owned batch retained for retry or status resolution.
        batch: SealedBatch<G>,
    },
    /// The batch reached a terminal outcome and its owner has been consumed.
    Terminal(WyrdError),
}

impl<G> SinkError<G> {
    /// Borrows the stable error without exposing a retry batch by accident.
    #[must_use]
    pub fn error(&self) -> &WyrdError {
        match self {
            Self::Retryable { error, .. } | Self::Terminal(error) => error,
        }
    }
}

/// The queue's transport seam for one owned batch type.
#[async_trait::async_trait]
pub trait BatchSink<G>: Send + Sync + 'static {
    /// Sends one owned batch and settles it or returns the exact owner for retry.
    ///
    /// # Errors
    ///
    /// Returns [`SinkError::Retryable`] when commit acknowledgement is ambiguous
    /// and the caller must retain the batch, or [`SinkError::Terminal`] when the
    /// owner is consumed by a terminal rejection.
    async fn send(&self, batch: SealedBatch<G>) -> Result<DurableBatchAck, SinkError<G>>;
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
}

/// In-memory loopback sink for queue tests.
#[derive(Debug, Default)]
pub struct MockSink {
    received: Mutex<Vec<BatchReceipt>>,
    attempted: Mutex<Vec<[u8; 16]>>,
    fail_next: AtomicUsize,
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
impl<G: Send + 'static> BatchSink<G> for MockSink {
    async fn send(&self, batch: SealedBatch<G>) -> Result<DurableBatchAck, SinkError<G>> {
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
                return Err(SinkError::Retryable {
                    error: WyrdError::ServiceUnavailable {
                        message: "mock sink forced ambiguous failure".to_owned(),
                        details: serde_json::json!({ "mock": true }),
                    },
                    batch,
                });
            }
        }
        let receipt = BatchReceipt {
            table: batch.table,
            batch_id: batch.batch_id,
            bytes: batch.frame.bytes().to_vec(),
            rows: batch.rows,
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
