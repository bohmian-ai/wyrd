//! The [`WriterPool`]: one [`Producer`] per destination table, keyed by
//! `(ClientScope, SinkKind, table)` and owned by [`crate::bifrost::Bifrost`].

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use arrow::record_batch::RecordBatch;
use arrow_schema::SchemaRef;
use wyrd_queue::{
    BatchSink, ClientByteBudget, ClientByteGuard, ClientByteMetrics, Producer, QueueConfig,
    SealedBatchSender, WyrdQueueError,
};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

use crate::bifrost::scope::{ClientScope, SinkKind};

/// Pool key: the full `(ClientScope, SinkKind, table)` producer identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProducerKey {
    scope: ClientScope,
    kind: SinkKind,
    table: String,
}

/// The client-tier producer pool behind [`crate::bifrost::Bifrost`].
///
/// One [`Producer`] is lazily constructed per distinct [`ProducerKey`] and
/// shared across calls; all producers drain into the one [`BatchSink`] the pool
/// owns (the sink routes by `SealedBatch.table`). Pooling by table is what lets
/// [`crate::bifrost::Bifrost::use_table`] swap the active write target without losing
/// the previous table's buffered rows: the swapped-away producer stays in the
/// pool and still drains on the next flush.
///
/// This type is crate-private. `Bifrost` is the public write door, and
/// backpressure asymmetry lives above it — `Bifrost::insert` propagates
/// queue-full while [`crate::bifrost::observe::record`] swallows and counts it.
pub(crate) struct WriterPool {
    scope: ClientScope,
    sink: Arc<dyn BatchSink<ClientByteGuard>>,
    config: QueueConfig,
    budget: ClientByteBudget,
    producers: Mutex<HashMap<ProducerKey, Arc<Producer>>>,
    /// Rejects inserts once shutdown begins its terminal drain.
    closed: AtomicBool,
    dropped: AtomicU64,
    drop_warned: AtomicBool,
}

/// Point-in-time settlement accounting for one [`crate::bifrost::Bifrost`] client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BifrostMetrics {
    /// Producers currently registered under the bounded handle pool.
    pub producers: usize,
    /// Bytes held by queued rows or a sealed batch owner.
    pub owned_bytes: usize,
    /// Lifetime charges for constructed producer queue and control slots.
    pub fixed_storage_bytes: usize,
    /// Total handle reservation including dynamic ownership and fixed storage.
    pub total_reserved_bytes: usize,
    /// Sealed batches not yet terminally acknowledged, cancelled, or poisoned.
    pub live_batches: usize,
    /// Ambiguous batches retained for a retry rather than released.
    pub retry_entries: usize,
    /// Flush or shutdown controls currently occupying producer command slots.
    pub pending_controls: usize,
}

impl WriterPool {
    /// Build a pool over a resolved [`ClientScope`], a shared sink, and the
    /// producer tuning [`QueueConfig`].
    ///
    /// Production wraps a [`crate::bifrost::BifrostIngestSink`]; tests inject a
    /// `wyrd-queue` mock or a stall sink through the same seam.
    #[must_use]
    pub(crate) fn new(
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
            closed: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
            drop_warned: AtomicBool::new(false),
        }
    }

    /// The scope every producer in this handle is keyed under.
    #[must_use]
    pub(crate) fn scope(&self) -> &ClientScope {
        &self.scope
    }

    /// Number of distinct producers currently pooled.
    #[must_use]
    pub(crate) fn producer_count(&self) -> usize {
        self.producers.lock().expect("producer pool poisoned").len()
    }

    /// Rows dropped by the fire-and-forget observe path under backpressure.
    #[must_use]
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::SeqCst)
    }

    /// Returns the bounded ownership and control state for this client handle.
    ///
    /// This is a point-in-time telemetry view: concurrent inserts or background
    /// drains can change individual counters immediately after it is returned.
    /// A durable batch ACK reports zero dynamic bytes, batches, retry entries,
    /// and pending controls. Constructed producers retain their fixed storage
    /// charge until [`Self::shutdown`] removes them; then total reservation is
    /// also zero.
    ///
    /// # Panics
    ///
    /// Panics if the producer pool mutex is poisoned, which indicates an
    /// invariant-breaking panic in another handle operation.
    #[must_use]
    pub(crate) fn metrics(&self) -> BifrostMetrics {
        let ClientByteMetrics {
            owned_bytes,
            fixed_storage_bytes,
            total_reserved_bytes,
            live_batches,
            retry_entries,
            ..
        } = self.budget.metrics();
        let producers = self.producers.lock().expect("producer pool poisoned");
        BifrostMetrics {
            producers: producers.len(),
            owned_bytes,
            fixed_storage_bytes,
            total_reserved_bytes,
            live_batches,
            retry_entries,
            pending_controls: producers
                .values()
                .filter(|producer| producer.has_pending_control())
                .count(),
        }
    }

    /// Enqueue one row, **propagating** queue-full.
    ///
    /// On the first call for a `table` the producer is built from `schema`;
    /// later calls reuse the pooled producer, so the schema a table was first
    /// registered under is the one its batches are sealed with.
    ///
    /// Both correlation fields are optional. An omitted `card_ref` becomes a
    /// null in the sealed batch, which the server stores against the
    /// authenticated principal with a null `card_uid`.
    ///
    /// The queue-domain [`WyrdQueueError`] is propagated verbatim (rather than
    /// projected onto the shared catalog) so the client-tier code —
    /// `WYRD_CLIENT_429_QUEUE_FULL`, which has no dedicated catalog variant —
    /// survives to the caller. [`crate::bifrost::Bifrost`] wraps it in
    /// [`crate::bifrost::BifrostClientError::Queue`] at the public boundary.
    ///
    /// # Errors
    /// Returns [`WyrdQueueError::QueueFull`] (code `WYRD_CLIENT_429_QUEUE_FULL`)
    /// when the producer's bounded channel is saturated or the pool has closed.
    pub(crate) fn insert(
        &self,
        table: &str,
        schema: &SchemaRef,
        json: Vec<u8>,
        card_ref: Option<CardRef>,
        run_id: Option<RunId>,
    ) -> Result<(), WyrdQueueError> {
        self.producer_for(table, schema)?
            .enqueue(json, card_ref, run_id)
    }

    /// Send one caller-built Arrow batch as its own durable batch.
    ///
    /// This deliberately bypasses the producer pool. A producer exists to
    /// accumulate and seal JSON rows; an Arrow batch arrives already sealed, so
    /// routing it through a producer would only delay it and risk coalescing it
    /// with buffered rows under a different identity. It is still charged
    /// against this handle's one byte budget and settles through this handle's
    /// one sink, so the accounting and transport contract are unchanged.
    ///
    /// Durability is complete when this resolves; unlike [`Self::insert`] there
    /// is nothing left for a later flush to do.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::QueueFull`] once the pool has closed, and
    /// otherwise the encode, byte-envelope, or stable server refusal reported
    /// by [`SealedBatchSender::send`].
    pub(crate) async fn write_batch(
        &self,
        table: &str,
        batch: &RecordBatch,
    ) -> Result<(), WyrdQueueError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(WyrdQueueError::QueueFull);
        }
        SealedBatchSender::new(Arc::clone(&self.sink), self.budget.clone(), self.config)
            .send(table, batch)
            .await
            .map(|_| ())
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
    pub(crate) fn flush(&self) -> Result<(), WyrdQueueError> {
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
    /// The handle closes and snapshots the producer registry under the one
    /// producer-map lock that admission also holds, so no producer can be
    /// created or returned after the snapshot. Once each producer enters its
    /// draining state, new rows are rejected and buffered rows are sent before
    /// this method returns. Terminally drained
    /// producers are removed and release their fixed-storage guards; a timed
    /// out ambiguous producer stays retained for a later shutdown retry.
    ///
    /// # Errors
    /// Returns the first [`WyrdQueueError`] reported by a producer shutdown
    /// after every producer has been stopped, including a failed terminal sink
    /// acknowledgement.
    ///
    /// # Panics
    /// Panics if the producer pool mutex is poisoned, which indicates an
    /// invariant-breaking panic in another handle operation.
    pub(crate) fn shutdown(&self) -> Result<(), WyrdQueueError> {
        let mut producers = {
            let pool = self.producers.lock().expect("producer pool poisoned");
            self.closed.store(true, Ordering::Release);
            pool.iter()
                .map(|(key, producer)| (key.table.clone(), Arc::clone(producer)))
                .collect::<Vec<_>>()
        };
        producers.sort_by(|left, right| left.0.cmp(&right.0));
        let mut first_error = None;
        for (_, producer) in producers {
            if let Err(error) = producer.shutdown()
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        self.producers
            .lock()
            .expect("producer pool poisoned")
            .retain(|_, producer| !producer.is_drained());
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

    /// Get-or-create the pooled producer for `(scope, Record, table)`.
    ///
    /// Closure is observed while holding the producer-map lock, the same lock
    /// [`Self::shutdown`] closes and snapshots under, so an admitted producer is
    /// always in the shutdown snapshot and a lookup after it is refused.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdQueueError::QueueFull`] once the pool has closed,
    /// [`WyrdQueueError::Backpressure`] when the producer ceiling is reached, and
    /// the producer construction error otherwise.
    ///
    /// # Panics
    ///
    /// Panics if the producer pool mutex is poisoned.
    fn producer_for(
        &self,
        table: &str,
        schema: &SchemaRef,
    ) -> Result<Arc<Producer>, WyrdQueueError> {
        let kind = SinkKind::Record;
        let mut pool = self.producers.lock().expect("producer pool poisoned");
        if self.closed.load(Ordering::Acquire) {
            return Err(WyrdQueueError::QueueFull);
        }
        if let Some(producer) = pool.iter().find_map(|(key, producer)| {
            (key.scope == self.scope && key.kind == kind && key.table == table)
                .then(|| Arc::clone(producer))
        }) {
            return Ok(producer);
        }
        if pool.len() >= self.config.max_producers() {
            return Err(WyrdQueueError::Backpressure);
        }
        let producer = Arc::new(Producer::with_budget(
            table,
            schema.clone(),
            Arc::clone(&self.sink),
            self.config,
            self.budget.clone(),
        )?);
        let key = ProducerKey {
            scope: self.scope.clone(),
            kind,
            table: table.to_owned(),
        };
        pool.insert(key, Arc::clone(&producer));
        Ok(producer)
    }
}

/// Admission and shutdown ordering over the shared producer-map lock.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClientConfig;
    use crate::transport::HttpConfig;
    use arrow_schema::{DataType, Field, Schema};
    use wyrd_queue::MockSink;

    /// A producer lookup that crosses shutdown is either in the drained
    /// snapshot or refused, never created behind it.
    ///
    /// The test holds the producer-map lock while shutdown starts, proving
    /// closure is not observable until the lock is released; then a lookup
    /// arriving after shutdown's snapshot — the interleaving where an insert
    /// passed admission before shutdown — is refused without creating a
    /// producer. The row admitted before shutdown is durably acknowledged and
    /// no producer or dynamic ownership remains.
    ///
    /// # Panics
    ///
    /// Panics if shutdown misses the admitted row, a late lookup creates a
    /// producer, or ownership outlives shutdown.
    #[test]
    fn shutdown_and_producer_admission_share_the_pool_lock() {
        let sink = Arc::new(MockSink::new());
        let config = ClientConfig {
            http: HttpConfig {
                base_url: "http://x".to_owned(),
                ..HttpConfig::default()
            },
            credential: Some("secret".to_owned().into()),
            ..ClientConfig::default()
        };
        let pool = WriterPool::new(
            ClientScope::from_config(&config).expect("scope"),
            Arc::clone(&sink) as Arc<dyn BatchSink<ClientByteGuard>>,
            QueueConfig::default(),
        );
        let schema: SchemaRef =
            Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        pool.insert("admitted", &schema, br#"{"id": 1}"#.to_vec(), None, None)
            .expect("row admitted before shutdown");

        std::thread::scope(|scope| {
            let guard = pool.producers.lock().expect("producer pool lock");
            let shutdown = scope.spawn(|| pool.shutdown());
            assert!(
                !pool.closed.load(Ordering::Acquire),
                "closure is only published under the producer-map lock"
            );
            drop(guard);
            shutdown
                .join()
                .expect("shutdown thread")
                .expect("shutdown drains the admitted row");
        });

        assert_eq!(
            sink.received().len(),
            1,
            "admitted row durably acknowledged"
        );
        assert!(matches!(
            pool.producer_for("late", &schema),
            Err(WyrdQueueError::QueueFull)
        ));
        assert!(matches!(
            pool.insert("late", &schema, br#"{"id": 2}"#.to_vec(), None, None),
            Err(WyrdQueueError::QueueFull)
        ));
        let metrics = pool.metrics();
        assert_eq!(metrics.producers, 0);
        assert_eq!(metrics.owned_bytes, 0);
        assert_eq!(metrics.live_batches, 0);
        assert_eq!(metrics.total_reserved_bytes, 0);
    }
}
