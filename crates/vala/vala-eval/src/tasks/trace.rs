//! `TraceAssertionTask` executor.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use wyrd_spec::vala::eval::{AssertionResult, EvalTask, TraceAssertionTask};
use wyrd_spec::vala::ids::TraceId;
use wyrd_spec::vala::trace::SpanRecord;

use crate::context::{ContextSnapshot, TaskOutput, extract_jsonpath_from};
use crate::error::EvalExecError;
use crate::executor::TaskExecutor;
use crate::operators;
use crate::trace_source::TraceSource;

/// Executor for trace assertion tasks.
pub struct TraceTaskExecutor {
    source: Arc<dyn TraceSource>,
    fetch_deadline: Duration,
}

impl TraceTaskExecutor {
    /// Fresh trace executor with an injected source and per-fetch deadline.
    #[must_use]
    pub fn new(source: Arc<dyn TraceSource>, fetch_deadline: Duration) -> Self {
        Self {
            source,
            fetch_deadline,
        }
    }
}

#[async_trait]
impl TaskExecutor for TraceTaskExecutor {
    async fn execute(
        &self,
        task: &EvalTask,
        snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<TaskOutput, EvalExecError> {
        let EvalTask::TraceAssertion(trace_task) = task else {
            return Err(EvalExecError::DagInvalid {
                reason: format!(
                    "stage {stage} dispatched {} task {:?} to trace executor",
                    task.discriminator(),
                    task.id().as_str()
                ),
            });
        };
        let result = execute_trace(
            trace_task,
            snapshot,
            stage,
            self.source.as_ref(),
            self.fetch_deadline,
        )
        .await?;
        Ok(TaskOutput::Trace(result))
    }
}

async fn execute_trace(
    task: &TraceAssertionTask,
    snapshot: &ContextSnapshot,
    stage: u32,
    source: &dyn TraceSource,
    fetch_deadline: Duration,
) -> Result<AssertionResult, EvalExecError> {
    let trace_id = snapshot
        .trace_id
        .ok_or_else(|| EvalExecError::TraceIdMissing {
            task_id: task.id.clone(),
        })?;
    let spans = source
        .fetch(trace_id, fetch_deadline)
        .await
        .map_err(|error| EvalExecError::TraceUnavailableForTask {
            task_id: task.id.clone(),
            reason: error.to_string(),
        })?;

    let started_at = Utc::now();
    let document = project_spans(trace_id, spans.as_slice());
    let observed = extract_jsonpath_from(&document, &task.span_selector)?;
    let verdict = operators::evaluate_operator(&observed, &task.operator, &task.expected).map_err(
        |error| EvalExecError::OperatorTypeMismatch {
            task_id: task.id.clone(),
            operator: format!("{:?}", task.operator),
            reason: error.to_string(),
        },
    )?;
    let message = if !verdict.passed && observed == Value::Null {
        Some(format!(
            "span_selector {} did not resolve on the trace document",
            task.span_selector.as_str()
        ))
    } else if !verdict.passed {
        Some(format!(
            "operator {:?} returned false for span_selector {}",
            task.operator,
            task.span_selector.as_str()
        ))
    } else {
        None
    };

    Ok(AssertionResult {
        task_id: task.id.clone(),
        passed: verdict.passed,
        actual: verdict.observed,
        expected: verdict.expected.unwrap_or_else(|| task.expected.clone()),
        operator: task.operator.clone(),
        message,
        stage,
        started_at,
        duration_ms: elapsed_ms(started_at),
    })
}

fn project_spans(trace_id: TraceId, spans: &[SpanRecord]) -> Value {
    let projected: Vec<Value> = spans.iter().map(project_one).collect();
    serde_json::json!({
        "trace_id": trace_id.to_hex(),
        "spans": projected,
    })
}

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

fn elapsed_ms(start: chrono::DateTime<chrono::Utc>) -> u64 {
    let diff = Utc::now().signed_duration_since(start);
    diff.num_milliseconds().max(0) as u64
}
