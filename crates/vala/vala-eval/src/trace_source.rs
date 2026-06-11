//! Read-only trace source abstraction for trace assertion tasks.
//!
//! The eval engine never reaches into a span store directly. The orchestrator
//! injects a [`TraceSource`], and that source decides whether spans live in an
//! in-memory capture buffer, an archive reader, or a future server-backed store.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use wyrd_spec::vala::ids::TraceId;
use wyrd_spec::vala::trace::SpanRecord;

/// Failure returned by a trace source when spans cannot be supplied yet or at
/// all for the requested trace id.
#[derive(Debug, Error)]
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
}

/// Read-only handle from the eval engine into a trace store.
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

/// Scripted trace source for executor tests.
pub struct MockTraceSource {
    scripted: Mutex<VecDeque<Result<Arc<Vec<SpanRecord>>, TraceUnavailable>>>,
    seen: Mutex<Vec<(TraceId, Duration)>>,
}

impl MockTraceSource {
    /// Build a mock source from scripted fetch results.
    #[must_use]
    pub fn new<I>(scripted: I) -> Self
    where
        I: IntoIterator<Item = Result<Arc<Vec<SpanRecord>>, TraceUnavailable>>,
    {
        Self {
            scripted: Mutex::new(scripted.into_iter().collect()),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Return fetch calls observed by the mock.
    pub async fn calls(&self) -> Vec<(TraceId, Duration)> {
        self.seen.lock().await.clone()
    }
}

#[async_trait]
impl TraceSource for MockTraceSource {
    async fn fetch(
        &self,
        trace_id: TraceId,
        deadline: Duration,
    ) -> Result<Arc<Vec<SpanRecord>>, TraceUnavailable> {
        self.seen.lock().await.push((trace_id, deadline));
        match self.scripted.lock().await.pop_front() {
            Some(result) => result,
            None => Err(TraceUnavailable::NotFound { trace_id }),
        }
    }
}
