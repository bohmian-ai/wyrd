//! The [`Bifrost`] write handle: a pool of one [`Producer`] per destination,
//! keyed by `(ClientScope, SinkKind, table)`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use arrow_schema::SchemaRef;
use wyrd_queue::{
    BatchSink, ClientByteBudget, ClientByteGuard, ClientByteMetrics, Producer, QueueConfig,
    WyrdQueueError,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

use crate::scope::{ClientScope, SinkKind};

/// Pool key: the full `(ClientScope, SinkKind, table)` producer identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProducerKey {
    scope: ClientScope,
    kind: SinkKind,
    table: String,
}

/// The client-tier write handle over the pooled producers.
///
/// One [`Producer`] is lazily constructed per distinct [`ProducerKey`] and
/// shared across calls; all producers drain into the one [`BatchSink`] the
/// handle owns (the sink routes by `SealedBatch.table`). Backpressure is
/// asymmetric — [`Bifrost::insert`] propagates queue-full, while the
/// [`crate::observe`] path swallows and counts it.
pub struct Bifrost {
    scope: ClientScope,
    sink: Arc<dyn BatchSink<ClientByteGuard>>,
    config: QueueConfig,
    budget: ClientByteBudget,
    producers: Mutex<HashMap<ProducerKey, Arc<Producer>>>,
    dropped: AtomicU64,
    drop_warned: AtomicBool,
}

/// Point-in-time settlement accounting for one [`Bifrost`] client handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BifrostMetrics {
    /// Producers currently registered under the bounded handle pool.
    pub producers: usize,
    /// Bytes held by queued rows or a sealed batch owner.
    pub owned_bytes: usize,
    /// Sealed batches not yet terminally acknowledged, cancelled, or poisoned.
    pub live_batches: usize,
    /// Ambiguous batches retained for a retry rather than released.
    pub retry_entries: usize,
    /// Flush or shutdown controls currently occupying producer command slots.
    pub pending_controls: usize,
}

impl Bifrost {
    /// Build a handle over a resolved [`ClientScope`], a shared sink, and the
    /// producer tuning [`QueueConfig`].
    ///
    /// Production wraps a [`crate::BifrostIngestSink`]; tests inject a
    /// `wyrd-queue` mock or a stall sink through the same seam.
    #[must_use]
    pub fn new(
        scope: ClientScope,
        sink: Arc<dyn BatchSink<ClientByteGuard>>,
        config: QueueConfig,
    ) -> Self {
        Self {
            scope,
            sink,
            config,
            budget: ClientByteBudget::new(config.client_byte_limit()),
            producers: Mutex::new(HashMap::new()),
            dropped: AtomicU64::new(0),
            drop_warned: AtomicBool::new(false),
        }
    }

    /// The scope every producer in this handle is keyed under.
    #[must_use]
    pub fn scope(&self) -> &ClientScope {
        &self.scope
    }

    /// Number of distinct producers currently pooled.
    #[must_use]
    pub fn producer_count(&self) -> usize {
        self.producers.lock().expect("producer pool poisoned").len()
    }

    /// Rows dropped by the fire-and-forget observe path under backpressure.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::SeqCst)
    }

    /// Returns the bounded ownership and control state for this client handle.
    ///
    /// This is a point-in-time telemetry view: concurrent inserts or background
    /// drains can change individual counters immediately after it is returned.
    /// A fully settled handle reports zero bytes, batches, retry entries, and
    /// pending controls.
    ///
    /// # Panics
    ///
    /// Panics if the producer pool mutex is poisoned, which indicates an
    /// invariant-breaking panic in another handle operation.
    #[must_use]
    pub fn metrics(&self) -> BifrostMetrics {
        let ClientByteMetrics {
            owned_bytes,
            live_batches,
            retry_entries,
        } = self.budget.metrics();
        let producers = self.producers.lock().expect("producer pool poisoned");
        BifrostMetrics {
            producers: producers.len(),
            owned_bytes,
            live_batches,
            retry_entries,
            pending_controls: producers
                .values()
                .filter(|producer| producer.has_pending_control())
                .count(),
        }
    }

    /// Explicit write path: enqueue one row, **propagating** queue-full.
    ///
    /// On the first call for a `(kind, table)` the producer is built from
    /// `schema`; later calls reuse the pooled producer and ignore `schema`.
    ///
    /// The queue-domain [`WyrdQueueError`] is propagated verbatim (rather than
    /// projected onto [`WyrdError`]) so the client-tier code —
    /// `WYRD_CLIENT_429_QUEUE_FULL`, which has no dedicated `WyrdError` variant —
    /// survives to the caller. The PyO3 surface (C4c) maps it at its boundary.
    ///
    /// # Errors
    /// Returns [`WyrdQueueError::QueueFull`] (code `WYRD_CLIENT_429_QUEUE_FULL`)
    /// when the producer's bounded channel is saturated.
    pub fn insert(
        &self,
        kind: SinkKind,
        table: &str,
        schema: &SchemaRef,
        json: Vec<u8>,
        card_ref: CardRef,
        run_id: Option<RunId>,
    ) -> Result<(), WyrdQueueError> {
        self.producer_for(kind, table, schema)?
            .enqueue(json, card_ref, run_id)
    }

    /// Flush every pooled producer and wait for each sink acknowledgement.
    ///
    /// The producer set is snapshotted before blocking so concurrent inserts
    /// can continue to resolve their existing handles without holding the pool
    /// mutex across network IO. Every producer is attempted in deterministic
    /// table order even after an earlier producer reports an error.
    ///
    /// # Errors
    /// Returns the first [`WyrdQueueError`] reported by a producer flush after
    /// all producers have been attempted, including transport failures returned
    /// by the configured sink.
    ///
    /// # Panics
    /// Panics if the producer pool mutex is poisoned, which indicates an
    /// invariant-breaking panic in another handle operation.
    pub fn flush(&self) -> Result<(), WyrdQueueError> {
        let mut producers = self
            .producers
            .lock()
            .expect("producer pool poisoned")
            .iter()
            .map(|(key, producer)| (key.table.clone(), Arc::clone(producer)))
            .collect::<Vec<_>>();
        producers.sort_by(|left, right| left.0.cmp(&right.0));
        let mut first_error = None;
        for (_, producer) in producers {
            if let Err(error) = producer.flush()
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Drain every pooled producer and stop its background task.
    ///
    /// The producer set is snapshotted before blocking. Once each producer
    /// enters its draining state, new rows are rejected and buffered rows are
    /// sent before this method returns; later producers are still drained after
    /// an earlier producer fails.
    ///
    /// # Errors
    /// Returns the first [`WyrdQueueError`] reported by a producer shutdown
    /// after every producer has been stopped, including a failed terminal sink
    /// acknowledgement.
    ///
    /// # Panics
    /// Panics if the producer pool mutex is poisoned, which indicates an
    /// invariant-breaking panic in another handle operation.
    pub fn shutdown(&self) -> Result<(), WyrdQueueError> {
        let mut producers = self
            .producers
            .lock()
            .expect("producer pool poisoned")
            .iter()
            .map(|(key, producer)| (key.table.clone(), Arc::clone(producer)))
            .collect::<Vec<_>>();
        producers.sort_by(|left, right| left.0.cmp(&right.0));
        let mut first_error = None;
        for (_, producer) in producers {
            if let Err(error) = producer.shutdown()
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Record one fire-and-forget drop: bump the counter and warn once.
    ///
    /// The observe path calls this after swallowing a queue-full so telemetry
    /// backpressure is visible in metrics without ever surfacing to the caller.
    pub(crate) fn note_drop(&self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
        if !self.drop_warned.swap(true, Ordering::SeqCst) {
            tracing::warn!(
                "vala observe: telemetry record dropped under queue backpressure \
                 (fire-and-forget; further drops counted silently)"
            );
        }
    }

    /// Get-or-create the pooled producer for `(scope, kind, table)`.
    fn producer_for(
        &self,
        kind: SinkKind,
        table: &str,
        schema: &SchemaRef,
    ) -> Result<Arc<Producer>, WyrdQueueError> {
        let key = ProducerKey {
            scope: self.scope.clone(),
            kind,
            table: table.to_owned(),
        };
        let mut pool = self.producers.lock().expect("producer pool poisoned");
        if let Some(producer) = pool.get(&key) {
            return Ok(Arc::clone(producer));
        }
        if pool.len() >= self.config.max_producers() {
            return Err(WyrdQueueError::Backpressure);
        }
        let producer = Arc::new(Producer::with_budget(
            table.to_owned(),
            schema.clone(),
            Arc::clone(&self.sink),
            self.config,
            self.budget.clone(),
        ));
        pool.insert(key, Arc::clone(&producer));
        Ok(producer)
    }
}

/// Build a user Arrow [`SchemaRef`] from a JSON-Schema value via C4a's
/// [`json_schema_to_arrow`](wyrd_queue::json_schema_to_arrow) forward mapping.
///
/// # Errors
/// Returns [`WyrdError`] (code `WYRD_VALA_400_SCHEMA_PARSE`) when the
/// JSON-Schema node is unsupported or malformed.
pub fn schema_from_json_schema(schema: &serde_json::Value) -> Result<SchemaRef, WyrdError> {
    let arrow = wyrd_queue::json_schema_to_arrow(schema)?;
    Ok(Arc::new(arrow))
}
