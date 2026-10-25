//! Read-only trace source abstraction for trace checks.
//!
//! The verification engine never reaches into a span store directly. Its
//! caller injects a [`TraceSource`], and that source decides whether spans live
//! in an in-memory capture buffer, an archive reader, or a server-backed store.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;
use tokio::sync::RwLock;
use wyrd_spec::vala::ids::TraceId;
use wyrd_spec::vala::trace::SpanRecord;

/// Failure returned by a trace source when spans cannot be supplied yet or at
/// all for the requested trace id.
#[derive(Debug, Clone, Error)]
pub enum TraceUnavailable {
    /// The trace has not landed in the backing store yet.
    #[error("trace not yet landed: {trace_id}")]
    NotYetLanded {
        /// Requested trace id.
        trace_id: TraceId,
    },
    /// The trace fetch exceeded the supplied deadline.
    #[error("trace fetch exceeded deadline {deadline:?}: {trace_id}")]
    TimedOut {
        /// Requested trace id.
        trace_id: TraceId,
        /// Deadline supplied by the executor.
        deadline: Duration,
    },
    /// The source knows the trace is absent.
    #[error("trace not found: {trace_id}")]
    NotFound {
        /// Requested trace id.
        trace_id: TraceId,
    },
    /// The backing store failed; the same read may succeed on a later attempt.
    #[error("trace source failed: {reason}")]
    Failed {
        /// Store failure detail.
        reason: String,
    },
}

/// Read-only handle from the verification engine into a trace store.
#[async_trait]
pub trait TraceSource: Send + Sync {
    /// Fetch all spans visible for `trace_id`.
    ///
    /// # Errors
    /// Returns [`TraceUnavailable`] when the source cannot supply spans within
    /// the requested deadline.
    async fn fetch(
        &self,
        trace_id: TraceId,
        deadline: Duration,
    ) -> Result<Arc<Vec<SpanRecord>>, TraceUnavailable>;
}

/// In-memory trace source for local orchestration and tests.
///
/// This type is infrastructure around the engine, not engine hot-path state:
/// repeated fetches return the stored `Arc<Vec<SpanRecord>>` directly.
#[derive(Default)]
pub struct InMemoryTraceSource {
    by_trace: RwLock<HashMap<TraceId, Arc<Vec<SpanRecord>>>>,
}

impl InMemoryTraceSource {
    /// Build an empty in-memory source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert spans for one trace id and return the shared span set.
    pub async fn insert(&self, trace_id: TraceId, spans: Vec<SpanRecord>) -> Arc<Vec<SpanRecord>> {
        let spans = Arc::new(spans);
        self.by_trace
            .write()
            .await
            .insert(trace_id, Arc::clone(&spans));
        spans
    }
}

#[async_trait]
impl TraceSource for InMemoryTraceSource {
    async fn fetch(
        &self,
        trace_id: TraceId,
        _deadline: Duration,
    ) -> Result<Arc<Vec<SpanRecord>>, TraceUnavailable> {
        self.by_trace
            .read()
            .await
            .get(&trace_id)
            .cloned()
            .ok_or(TraceUnavailable::NotFound { trace_id })
    }
}

/// Project a trace's spans into the JSON document span selectors read.
///
/// Read by Verifier `trace` checks.
pub(crate) fn project_spans(trace_id: TraceId, spans: &[SpanRecord]) -> Value {
    let projected: Vec<Value> = spans.iter().map(project_one).collect();
    serde_json::json!({
        "trace_id": trace_id.to_hex(),
        "spans": projected,
    })
}

/// Project one span into its selector document.
fn project_one(span: &SpanRecord) -> Value {
    serde_json::json!({
        "trace_id": span.trace_id.to_hex(),
        "span_id": span.span_id.to_hex(),
        "parent_span_id": span.parent_span_id.map(|id| id.to_hex()),
        "name": &span.name,
        "kind": &span.kind,
        "start_time": span.start_time,
        "end_time": span.end_time,
        "duration_ms": span.duration_ms,
        "status": &span.status,
        "attributes": &span.attributes,
        "events": &span.events,
        "links": &span.links,
    })
}
