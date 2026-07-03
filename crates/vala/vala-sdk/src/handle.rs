//! The [`Bifrost`] write handle: a pool of one [`Producer`] per destination,
//! keyed by `(ClientScope, SinkKind, table)`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use arrow_schema::SchemaRef;
use wyrd_queue::{BatchSink, Producer, QueueConfig, WyrdQueueError};
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
    sink: Arc<dyn BatchSink>,
    config: QueueConfig,
    producers: Mutex<HashMap<ProducerKey, Arc<Producer>>>,
    dropped: AtomicU64,
    drop_warned: AtomicBool,
}

impl Bifrost {
    /// Build a handle over a resolved [`ClientScope`], a shared sink, and the
    /// producer tuning [`QueueConfig`].
    ///
    /// Production wraps a [`crate::BifrostIngestSink`]; tests inject a
    /// `wyrd-queue` mock or a stall sink through the same seam.
    #[must_use]
    pub fn new(scope: ClientScope, sink: Arc<dyn BatchSink>, config: QueueConfig) -> Self {
        Self {
            scope,
            sink,
            config,
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
        self.producers
            .lock()
            .expect("producer pool poisoned")
            .len()
    }

    /// Rows dropped by the fire-and-forget observe path under backpressure.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::SeqCst)
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
        self.producer_for(kind, table, schema)
            .enqueue(json, card_ref, run_id)
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
    fn producer_for(&self, kind: SinkKind, table: &str, schema: &SchemaRef) -> Arc<Producer> {
        let key = ProducerKey {
            scope: self.scope.clone(),
            kind,
            table: table.to_owned(),
        };
        let mut pool = self.producers.lock().expect("producer pool poisoned");
        Arc::clone(pool.entry(key).or_insert_with(|| {
            Arc::new(Producer::new(
                table.to_owned(),
                schema.clone(),
                Arc::clone(&self.sink),
                self.config,
            ))
        }))
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
