//! Trace evidence read through the execution caller's ordinary Bifrost query.
//!
//! [`CallerTraces`] is the engine's [`TraceSource`] for `trace` inputs. Each
//! fetch is one public query of `bifrost.traces.spans` under the accepted caller,
//! so Oracle's table authorization, tenancy, and audit decide the read exactly
//! as for any other query that caller makes. The read selects every retained
//! span of the exact trace ID, whatever workload, Card, or time it carries:
//! a binding owns the result, never a narrower trace input.

use std::sync::Arc;
use std::time::Duration;

use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use vala_verify::{TraceSource, TraceUnavailable};
use wyrd_client::bifrost::rows::{SPAN_COLUMNS, span_record};
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::ids::TraceId;
use wyrd_spec::vala::trace::SpanRecord;

use crate::components::auth::Caller;
use crate::query::scheduled::ScheduledQueryCaller;
use crate::state::AppState;

/// Most spans one trace read decodes; a larger trace is refused before any
/// check reads it.
// ponytail: fixed server-owned ceiling; make it a Deployment limit when an
// operator needs to tune it.
pub const TRACE_SPAN_LIMIT: usize = 10_000;

/// Stable query error of a tenant span table no export has created yet.
const SPANS_NOT_CREATED: &str = "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND";

/// Reads one execution's trace inputs as its accepted caller.
pub struct CallerTraces {
    /// Server state each span query runs against.
    state: AppState,
    /// The caller every span query is authorized and audited as; each
    /// statement runs under its own request ID ([`super::statement_caller`]),
    /// since executions read traces concurrently.
    caller: Caller,
}

impl CallerTraces {
    /// A trace reader for `caller`.
    ///
    /// # Errors
    /// Returns the [`ScheduledQueryCaller::authenticated`] refusal when the
    /// caller cannot form an Oracle query context.
    pub fn new(state: AppState, caller: Caller) -> Result<Self, wyrd_spec::error::WyrdError> {
        crate::query::service::oracle_context(&caller)?;
        Ok(Self { state, caller })
    }

    /// The span statement of `trace_id`: every retained span of that exact
    /// trace, with no time or attribution predicate, start-time then span-ID
    /// ordered so positional selectors are reproducible, asking for one span
    /// past [`TRACE_SPAN_LIMIT`] so an oversized trace is detectable.
    fn statement(trace_id: TraceId) -> String {
        format!(
            "SELECT {SPAN_COLUMNS} FROM bifrost.traces.spans \
             WHERE trace_id = X'{}' \
             ORDER BY start_time_unix_nano, span_id LIMIT {}",
            trace_id.to_hex(),
            TRACE_SPAN_LIMIT + 1,
        )
    }
}

#[async_trait]
impl TraceSource for CallerTraces {
    /// Read the visible spans of `trace_id` within `deadline`.
    ///
    /// # Errors
    /// Returns [`TraceUnavailable::NotYetLanded`] when no span is visible
    /// (including a tenant whose span table no export has created),
    /// [`TraceUnavailable::TimedOut`] past `deadline`, and
    /// [`TraceUnavailable::Failed`] for a refused or failed query, a trace
    /// over [`TRACE_SPAN_LIMIT`] spans, or a span that does not decode.
    async fn fetch(
        &self,
        trace_id: TraceId,
        deadline: Duration,
    ) -> Result<Arc<Vec<SpanRecord>>, TraceUnavailable> {
        let failed = |reason: String| TraceUnavailable::Failed { reason };
        let mut batches: Vec<RecordBatch> = Vec::new();
        let request = BifrostQueryRequest {
            params: Vec::new(),
            sql: Self::statement(trace_id),
            deadline_ms: i64::try_from(deadline.as_millis()).ok(),
        };
        let query = ScheduledQueryCaller::authenticated(
            self.state.clone(),
            super::statement_caller(&self.caller),
            CancellationToken::new(),
        )
        .map_err(|error| failed(error.to_string()))?;
        let read = query.run_with(request, |batch| {
            batches.push(batch);
            Ok(())
        });
        match tokio::time::timeout(deadline, read).await {
            Err(_) => return Err(TraceUnavailable::TimedOut { trace_id, deadline }),
            Ok(Err(error)) if error.code() == SPANS_NOT_CREATED => {}
            Ok(Err(error)) => return Err(failed(error.to_string())),
            Ok(Ok(_)) => {}
        }
        let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
        if rows == 0 {
            return Err(TraceUnavailable::NotYetLanded { trace_id });
        }
        if rows > TRACE_SPAN_LIMIT {
            return Err(failed(format!(
                "trace {trace_id} exceeds {TRACE_SPAN_LIMIT} spans"
            )));
        }
        let mut spans = Vec::with_capacity(rows);
        for batch in &batches {
            for row in 0..batch.num_rows() {
                spans.push(span_record(batch, row).map_err(|error| failed(error.clone()))?);
            }
        }
        Ok(Arc::new(spans))
    }
}
