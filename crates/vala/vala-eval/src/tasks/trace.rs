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

#[cfg(test)]
mod trace_executor {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::context::ExecutionContext;
    use crate::executor::{EvalReport, Executors, TaskRunOutcome, execute_plan};
    use crate::store::TaskRegistry;
    use crate::tasks::{
        AgentTaskExecutor, AssertionTaskExecutor, JudgeTaskExecutor, TraceTaskExecutor,
    };
    use crate::{InMemoryTraceSource, MockJudgeInvoker, MockTraceSource, TraceUnavailable};
    use chrono::{TimeZone, Utc};
    use serde_json::{Value, json};
    use uuid::Uuid;
    use wyrd_spec::vala::eval::{
        ComparisonOperator, EvalSpec, EvalTask, JsonPath, RecordId, RunId, TaskId,
        TraceAssertionTask,
    };
    use wyrd_spec::vala::ids::{SpanId, TraceId};
    use wyrd_spec::vala::trace::{
        InstrumentationScope, Resource, SpanKind, SpanRecord, SpanStatus,
    };

    fn tid(value: &str) -> TaskId {
        TaskId::new(value).expect("static task id is valid")
    }

    fn jp(value: &str) -> JsonPath {
        JsonPath::new(value).expect("static JSONPath is valid")
    }

    fn trace_id(value: &str) -> TraceId {
        TraceId::from_hex(value).expect("static trace id is valid")
    }

    fn span_id(value: &str) -> SpanId {
        SpanId::from_hex(value).expect("static span id is valid")
    }

    fn trace_task(
        id: &str,
        selector: &str,
        operator: ComparisonOperator,
        expected: Value,
    ) -> EvalTask {
        EvalTask::TraceAssertion(TraceAssertionTask {
            id: tid(id),
            span_selector: jp(selector),
            operator,
            expected,
            depends_on: Vec::new(),
            condition: None,
        })
    }

    fn spec_of(tasks: Vec<EvalTask>) -> EvalSpec {
        let mut map = BTreeMap::new();
        for task in tasks {
            map.insert(task.id().clone(), task);
        }
        EvalSpec {
            subject_ref: None,
            dataset: None,
            tasks: map,
            workflow: None,
            sampling: None,
            pass_gate: None,
            context_capture: None,
        }
    }

    fn span(trace: TraceId, span: SpanId, name: &str, scenario: &str) -> SpanRecord {
        let start_time = Utc
            .with_ymd_and_hms(2026, 6, 10, 12, 0, 0)
            .single()
            .expect("static timestamp is valid");
        let end_time = Utc
            .with_ymd_and_hms(2026, 6, 10, 12, 0, 1)
            .single()
            .expect("static timestamp is valid");
        let mut attributes = serde_json::Map::new();
        attributes.insert("gen_ai.system".to_owned(), json!("wyrd-test"));
        attributes.insert("wyrd.eval.scenario_id".to_owned(), json!(scenario));
        SpanRecord {
            trace_id: trace,
            span_id: span,
            parent_span_id: None,
            flags: 0,
            trace_state: String::new(),
            name: name.to_owned(),
            kind: SpanKind::Internal,
            start_time,
            end_time,
            duration_ms: 1000,
            status: SpanStatus::Ok,
            attributes,
            dropped_attributes_count: 0,
            events: Vec::new(),
            dropped_events_count: 0,
            links: Vec::new(),
            dropped_links_count: 0,
            scope: InstrumentationScope {
                name: "vala-eval-test".to_owned(),
                version: None,
                attributes: serde_json::Map::new(),
            },
            resource: Resource {
                service_name: "eval-test".to_owned(),
                service_namespace: None,
                service_version: None,
                service_instance_id: None,
                attributes: serde_json::Map::new(),
            },
        }
    }

    fn executors(source: Arc<dyn crate::TraceSource>) -> Executors {
        let mock = MockJudgeInvoker::new(std::iter::empty());
        Executors {
            assertion: Arc::new(AssertionTaskExecutor::new()),
            judge: Arc::new(JudgeTaskExecutor::new(mock)),
            trace: Arc::new(TraceTaskExecutor::new(source, Duration::from_millis(250))),
            agent: Arc::new(AgentTaskExecutor::new()),
        }
    }

    async fn drive(
        spec: EvalSpec,
        source: Arc<dyn crate::TraceSource>,
        trace_id: Option<TraceId>,
    ) -> EvalReport {
        let plan = spec.execution_plan().expect("test spec has valid DAG");
        let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
        let mut cx = ExecutionContext::new(
            json!({"response": "ok"}),
            RunId::from_string("run-trace".to_owned()),
            RecordId(Uuid::from_u128(12)),
            None,
        );
        if let Some(trace_id) = trace_id {
            cx = cx.with_trace_id(trace_id);
        }
        execute_plan(&plan, &cx, &registry, &executors(source))
            .await
            .expect("plan executes")
    }

    fn result<'a>(report: &'a EvalReport, id: &str) -> &'a wyrd_spec::vala::eval::AssertionResult {
        report
            .outcomes
            .iter()
            .find_map(|outcome| match outcome {
                TaskRunOutcome::Ran(result) if result.task_id == tid(id) => Some(result),
                _ => None,
            })
            .expect("task result exists")
    }

    #[tokio::test]
    async fn trace_assertion_passes_on_resolved_selector() {
        let trace = trace_id("11111111111111111111111111111111");
        let source = Arc::new(InMemoryTraceSource::new());
        source
            .insert(
                trace,
                vec![span(
                    trace,
                    span_id("2222222222222222"),
                    "agent_call",
                    "scenario-a",
                )],
            )
            .await;
        let report = drive(
            spec_of(vec![trace_task(
                "trace_name",
                "$.spans[0].name",
                ComparisonOperator::Equals,
                json!("agent_call"),
            )]),
            source,
            Some(trace),
        )
        .await;
        assert!(result(&report, "trace_name").passed);
    }

    #[tokio::test]
    async fn trace_assertion_trace_id_missing_errors() {
        let source = Arc::new(InMemoryTraceSource::new());
        let report = drive(
            spec_of(vec![trace_task(
                "trace_missing",
                "$.spans[0].name",
                ComparisonOperator::Equals,
                json!("agent_call"),
            )]),
            source,
            None,
        )
        .await;
        let result = result(&report, "trace_missing");
        assert!(!result.passed);
        assert!(
            result
                .message
                .as_deref()
                .expect("failed result has message")
                .contains("trace_id")
        );
    }

    #[tokio::test]
    async fn trace_assertion_unavailable_errors() {
        let trace = trace_id("33333333333333333333333333333333");
        let source = Arc::new(MockTraceSource::new([Err(
            TraceUnavailable::NotYetLanded { trace_id: trace },
        )]));
        let report = drive(
            spec_of(vec![trace_task(
                "trace_wait",
                "$.spans[0].name",
                ComparisonOperator::Equals,
                json!("agent_call"),
            )]),
            source,
            Some(trace),
        )
        .await;
        let result = result(&report, "trace_wait");
        let message = result
            .message
            .as_deref()
            .expect("failed result has message");
        assert!(!result.passed);
        assert!(message.contains("not yet landed"));
        assert!(message.contains("trace_wait"));
    }

    #[tokio::test]
    async fn unresolved_selector_marks_failed_with_message() {
        let trace = trace_id("44444444444444444444444444444444");
        let source = Arc::new(InMemoryTraceSource::new());
        source
            .insert(
                trace,
                vec![span(
                    trace,
                    span_id("5555555555555555"),
                    "agent_call",
                    "scenario-a",
                )],
            )
            .await;
        let report = drive(
            spec_of(vec![trace_task(
                "trace_no_match",
                "$.spans[42].name",
                ComparisonOperator::Equals,
                json!("agent_call"),
            )]),
            source,
            Some(trace),
        )
        .await;
        let result = result(&report, "trace_no_match");
        assert!(!result.passed);
        assert!(
            result
                .message
                .as_deref()
                .expect("failed result has message")
                .contains("$.spans[42].name")
        );
    }

    #[tokio::test]
    async fn engine_surfaces_all_spans_received_orchestrator_scopes_by_attribute() {
        let trace = trace_id("66666666666666666666666666666666");
        let source = Arc::new(InMemoryTraceSource::new());
        source
            .insert(
                trace,
                vec![
                    span(
                        trace,
                        span_id("7777777777777777"),
                        "scenario_a_span",
                        "scenario-a",
                    ),
                    span(
                        trace,
                        span_id("8888888888888888"),
                        "scenario_b_span",
                        "scenario-b",
                    ),
                ],
            )
            .await;
        let report = drive(
            spec_of(vec![trace_task(
                "trace_length",
                "$.spans",
                ComparisonOperator::Length { expected: 2 },
                json!(null),
            )]),
            source,
            Some(trace),
        )
        .await;
        assert!(result(&report, "trace_length").passed);
    }
}
