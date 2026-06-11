//! Per-record runtime state for one task-DAG execution.
//!
//! The engine models per-record state as an immutable [`ContextSnapshot`] that
//! is rebuilt at each stage barrier. Executors receive the snapshot by `&` and
//! return their [`TaskOutput`]; the driver folds new outputs into the next
//! snapshot via [`ContextSnapshot::extend`], which is a sequence of `Arc::clone`
//! bumps rather than JSON deep clones.
//!
//! [`ExecutionContext`] is a thin per-record wrapper the orchestrator
//! constructs once per [`crate::context::RecordIdentity`]. It exposes the current
//! snapshot to the driver.

use std::collections::HashMap;
use std::sync::Arc;

use jsonpath_rust::JsonPathValue;
use serde_json::{Map, Value};
use wyrd_spec::vala::eval::ids::{JsonPath, RecordId, RunId, ScenarioId, TaskId};
use wyrd_spec::vala::eval::result::AssertionResult;
use wyrd_spec::vala::ids::TraceId;

use crate::error::EvalExecError;
use crate::store::JudgeOutcome;
use crate::tasks::MediaBindings;

/// Typed identity tuple every [`AssertionResult`] row aggregates against.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordIdentity {
    /// The eval run.
    pub run_id: RunId,
    /// The record this context represents.
    pub record_id: RecordId,
    /// Enclosing scenario id, when running scenario-driven evaluation.
    pub scenario_id: Option<ScenarioId>,
}

/// Engine-local payload of one task.
///
/// `AssertionResult` is the wire shape the aggregator (commit 12) and the
/// persistence layer consume; `TaskOutput` is the engine-local view that
/// retains the judge raw / parsed payload so a downstream task can JSONPath
/// into `$.task.<judge_id>.parsed` per the scoped-view contract.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskOutput {
    /// An [`crate::tasks::AssertionTaskExecutor`] result.
    Assertion(AssertionResult),
    /// An [`crate::tasks::JudgeTaskExecutor`] result plus the judge invocation
    /// payload retained for downstream tasks.
    Judge {
        /// Persisted assertion-result row.
        result: AssertionResult,
        /// Judge raw / parsed payload retained on the snapshot.
        outcome: JudgeOutcome,
    },
    /// An [`crate::tasks::TraceTaskExecutor`] result.
    Trace(AssertionResult),
    /// An [`crate::tasks::AgentTaskExecutor`] result.
    Agent(AssertionResult),
}

impl TaskOutput {
    /// Borrow the persisted assertion-result row for this output.
    #[must_use]
    pub fn result(&self) -> &AssertionResult {
        match self {
            Self::Assertion(result) | Self::Trace(result) | Self::Agent(result) => result,
            Self::Judge { result, .. } => result,
        }
    }
}

/// Immutable snapshot of all workflow state visible to one stage's executors.
///
/// Cheap to clone via Arc bumps: `Arc<Value>` on the observation document and
/// `Arc<HashMap<TaskId, Arc<TaskOutput>>>` on the per-task results. Rebuilt
/// stage by stage by the driver; never mutated in place.
#[derive(Debug, Clone)]
pub struct ContextSnapshot {
    /// Typed identity of the enclosing run / record / scenario.
    pub identity: RecordIdentity,
    /// Per-record trace id used by trace assertion tasks.
    pub trace_id: Option<TraceId>,
    /// The immutable observation document.
    pub base_context: Arc<Value>,
    /// Per-task outputs accumulated by prior stages.
    pub task_outputs: Arc<HashMap<TaskId, Arc<TaskOutput>>>,
    /// Per-record opaque media bindings.
    pub media: Arc<MediaBindings>,
    /// Media ids required by this record's judge context.
    pub required_media: Arc<Vec<String>>,
}

impl ContextSnapshot {
    /// Build a fresh snapshot with no recorded outputs.
    #[must_use]
    pub fn new(base_context: Arc<Value>, identity: RecordIdentity) -> Self {
        Self {
            identity,
            trace_id: None,
            base_context,
            task_outputs: Arc::new(HashMap::new()),
            media: Arc::new(MediaBindings::new()),
            required_media: Arc::new(Vec::new()),
        }
    }

    /// Attach media and required media ids to this snapshot.
    #[must_use]
    pub fn with_media(self, media: MediaBindings, required_media: Vec<String>) -> Self {
        Self {
            media: Arc::new(media),
            required_media: Arc::new(required_media),
            ..self
        }
    }

    /// Attach the per-record trace id used by trace assertions.
    #[must_use]
    pub fn with_trace_id(self, trace_id: TraceId) -> Self {
        Self {
            trace_id: Some(trace_id),
            ..self
        }
    }

    /// Build the next snapshot from this one plus a batch of fresh task
    /// outputs. The base context and prior outputs are carried forward via
    /// `Arc::clone`; no JSON cloning happens here.
    #[must_use]
    pub fn extend<I>(&self, fresh: I) -> Self
    where
        I: IntoIterator<Item = (TaskId, Arc<TaskOutput>)>,
    {
        let prior = self.task_outputs.as_ref();
        let mut next = HashMap::with_capacity(prior.len() + 8);
        for (id, output) in prior.iter() {
            next.insert(id.clone(), Arc::clone(output));
        }
        for (id, output) in fresh {
            next.insert(id, output);
        }
        Self {
            identity: self.identity.clone(),
            trace_id: self.trace_id,
            base_context: Arc::clone(&self.base_context),
            task_outputs: Arc::new(next),
            media: Arc::clone(&self.media),
            required_media: Arc::clone(&self.required_media),
        }
    }

    /// Materialize the JSON view executors JSONPath into.
    ///
    /// Includes the full observation document plus a `$.task` subtree
    /// containing only the declared deps' outputs. Built once per task call
    /// and dropped at the end of the executor body; not cached.
    #[must_use]
    pub fn build_scoped_view(&self, deps: &[TaskId]) -> Value {
        if deps.is_empty() {
            return (*self.base_context).clone();
        }
        let mut root = match self.base_context.as_ref() {
            Value::Object(map) => map.clone(),
            other => {
                let mut wrapped = Map::new();
                wrapped.insert("value".to_owned(), other.clone());
                wrapped
            }
        };
        let mut task_map = Map::new();
        for dep in deps {
            if let Some(output) = self.task_outputs.get(dep) {
                task_map.insert(dep.as_str().to_owned(), output_to_view(output));
            }
        }
        root.insert("task".to_owned(), Value::Object(task_map));
        Value::Object(root)
    }
}

fn output_to_view(output: &TaskOutput) -> Value {
    match output {
        TaskOutput::Assertion(result) | TaskOutput::Trace(result) | TaskOutput::Agent(result) => {
            Value::Object(result_view(result))
        }
        TaskOutput::Judge { result, outcome } => {
            let mut map = result_view(result);
            map.insert("parsed".to_owned(), outcome.parsed.clone());
            map.insert("raw".to_owned(), outcome.raw.clone());
            Value::Object(map)
        }
    }
}

fn result_view(result: &AssertionResult) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("passed".to_owned(), Value::Bool(result.passed));
    map.insert(
        "actual".to_owned(),
        result.actual.clone().unwrap_or(Value::Null),
    );
    map.insert("expected".to_owned(), result.expected.clone());
    map.insert(
        "message".to_owned(),
        match &result.message {
            Some(text) => Value::String(text.clone()),
            None => Value::Null,
        },
    );
    map.insert("stage".to_owned(), Value::Number(result.stage.into()));
    map
}

/// Per-record handle the orchestrator constructs once per [`RecordIdentity`].
///
/// Wraps an `Arc<ContextSnapshot>` so the driver can swap snapshots between
/// stages without rebuilding the wrapper. Tests construct one of these per
/// record; the driver internally rebuilds the snapshot at each stage barrier.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    snapshot: Arc<ContextSnapshot>,
}

impl ExecutionContext {
    /// Build a fresh per-record context from an initial snapshot.
    #[must_use]
    pub fn from_snapshot(snapshot: Arc<ContextSnapshot>) -> Self {
        Self { snapshot }
    }

    /// Build a fresh per-record context from the observation document and
    /// identity. The initial snapshot has empty task outputs.
    #[must_use]
    pub fn new(
        base_context: Value,
        run_id: RunId,
        record_id: RecordId,
        scenario_id: Option<ScenarioId>,
    ) -> Self {
        let identity = RecordIdentity {
            run_id,
            record_id,
            scenario_id,
        };
        let snapshot = ContextSnapshot::new(Arc::new(base_context), identity);
        Self::from_snapshot(Arc::new(snapshot))
    }

    /// Return a context with per-record media attached.
    #[must_use]
    pub fn with_media(self, media: MediaBindings, required_media: Vec<String>) -> Self {
        let snapshot = self
            .snapshot
            .as_ref()
            .clone()
            .with_media(media, required_media);
        Self::from_snapshot(Arc::new(snapshot))
    }

    /// Return a context with the per-record trace id attached.
    #[must_use]
    pub fn with_trace_id(self, trace_id: TraceId) -> Self {
        let snapshot = self.snapshot.as_ref().clone().with_trace_id(trace_id);
        Self::from_snapshot(Arc::new(snapshot))
    }

    /// Borrow the current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> &Arc<ContextSnapshot> {
        &self.snapshot
    }

    /// The observation document.
    #[must_use]
    pub fn base_context(&self) -> &Value {
        &self.snapshot.base_context
    }

    /// The eval run this context belongs to.
    #[must_use]
    pub fn run_id(&self) -> &RunId {
        &self.snapshot.identity.run_id
    }

    /// The record this context represents.
    #[must_use]
    pub fn record_id(&self) -> &RecordId {
        &self.snapshot.identity.record_id
    }

    /// The enclosing scenario id, when present.
    #[must_use]
    pub fn scenario_id(&self) -> Option<&ScenarioId> {
        self.snapshot.identity.scenario_id.as_ref()
    }

    /// Borrow a recorded task output.
    #[must_use]
    pub fn task_output(&self, task: &TaskId) -> Option<&TaskOutput> {
        self.snapshot.task_outputs.get(task).map(Arc::as_ref)
    }
}

/// Extract a JSON value at `path` from any JSON value.
///
/// The single Wyrd-typed surface over the workspace JSONPath dependency.
/// Executor code goes through this so the upstream error type never leaks
/// into engine code.
///
/// # Errors
/// Returns [`EvalExecError::JsonPathFailure`] when the path fails to compile
/// or evaluate.
pub fn extract_jsonpath_from(value: &Value, path: &JsonPath) -> Result<Value, EvalExecError> {
    let values = extract_jsonpath_values(value, path)?;
    Ok(values_to_value(values))
}

/// Extract a required JSON value at `path` from any JSON value.
///
/// # Errors
/// Returns [`EvalExecError::ExtractPathMissing`] when the path resolves to no
/// value, or [`EvalExecError::JsonPathFailure`] for compile / eval errors.
pub fn extract_required_jsonpath_from(
    value: &Value,
    path: &JsonPath,
    task_id: &TaskId,
) -> Result<Value, EvalExecError> {
    let values = extract_jsonpath_values(value, path)?;
    if values.is_empty() {
        return Err(EvalExecError::ExtractPathMissing {
            task_id: task_id.clone(),
            path: path.as_str().to_owned(),
        });
    }
    Ok(values_to_value(values))
}

fn extract_jsonpath_values(value: &Value, path: &JsonPath) -> Result<Vec<Value>, EvalExecError> {
    let query = jsonpath_rust::JsonPath::<Value>::try_from(path.as_str()).map_err(|error| {
        EvalExecError::JsonPathFailure {
            path: path.as_str().to_owned(),
            reason: error.to_string(),
        }
    })?;

    let mut values = Vec::new();
    for candidate in query.find_slice(value) {
        match candidate {
            JsonPathValue::Slice(value, _) => values.push(value.clone()),
            JsonPathValue::NewValue(value) => values.push(value),
            JsonPathValue::NoValue => {}
        }
    }

    Ok(values)
}

fn values_to_value(values: Vec<Value>) -> Value {
    match values.as_slice() {
        [] => Value::Null,
        [only] => only.clone(),
        _ => Value::Array(values),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;
    use uuid::Uuid;
    use wyrd_spec::vala::eval::ComparisonOperator;

    fn ctx(value: Value) -> ExecutionContext {
        ExecutionContext::new(
            value,
            RunId::from_string("run-1".to_owned()),
            RecordId(Uuid::nil()),
            None,
        )
    }

    fn task(id: &str) -> TaskId {
        TaskId::new(id).expect("static test ids are valid")
    }

    fn path(expr: &str) -> JsonPath {
        JsonPath::new(expr).expect("static test paths are valid")
    }

    fn assertion_result(id: &str, passed: bool, stage: u32) -> AssertionResult {
        AssertionResult {
            task_id: task(id),
            passed,
            actual: Some(json!(true)),
            expected: json!(true),
            operator: ComparisonOperator::Equals,
            message: None,
            stage,
            started_at: chrono::Utc
                .with_ymd_and_hms(2026, 6, 10, 0, 0, 0)
                .single()
                .expect("static timestamp is valid"),
            duration_ms: 0,
        }
    }

    #[test]
    fn extracts_scalar_from_base_context() {
        let context = ctx(json!({"response": "hello"}));
        let view = context.snapshot().build_scoped_view(&[]);
        let extracted =
            extract_jsonpath_from(&view, &path("$.response")).expect("scalar extraction succeeds");
        assert_eq!(extracted, json!("hello"));
    }

    #[test]
    fn extracts_nested_array_from_base_context() {
        let context = ctx(json!({
            "tool_calls": [
                {"name": "search"},
                {"name": "summarise"}
            ]
        }));
        let view = context.snapshot().build_scoped_view(&[]);
        let extracted = extract_jsonpath_from(&view, &path("$.tool_calls[*].name"))
            .expect("array extraction succeeds");
        assert_eq!(extracted, json!(["search", "summarise"]));
    }

    #[test]
    fn missing_path_returns_null_not_error() {
        let context = ctx(json!({"response": "hello"}));
        let view = context.snapshot().build_scoped_view(&[]);
        let extracted =
            extract_jsonpath_from(&view, &path("$.never")).expect("missing path resolves to null");
        assert_eq!(extracted, Value::Null);
    }

    #[test]
    fn invalid_runtime_path_surfaces_typed_error() {
        let context = ctx(json!({"a": 1}));
        let view = context.snapshot().build_scoped_view(&[]);
        let bad = path("$.a[?(@.missing>)]");
        let error = extract_jsonpath_from(&view, &bad).expect_err("malformed predicate fails");
        assert!(matches!(
            error,
            EvalExecError::JsonPathFailure { ref path, .. } if path == bad.as_str()
        ));
    }

    #[test]
    fn scoped_view_exposes_dep_outputs_under_task_namespace() {
        let context = ctx(json!({"base": "value"}));
        let upstream = Arc::new(TaskOutput::Assertion(assertion_result("upstream", true, 0)));
        let snapshot = context
            .snapshot()
            .extend(std::iter::once((task("upstream"), upstream)));
        let snapshot = Arc::new(snapshot);
        let view = snapshot.build_scoped_view(&[task("upstream")]);

        let passed = extract_jsonpath_from(&view, &path("$.task.upstream.passed"))
            .expect("dep output projected");
        assert_eq!(passed, json!(true));
        let base_visible = extract_jsonpath_from(&view, &path("$.base")).expect("base visible");
        assert_eq!(base_visible, json!("value"));
    }

    #[test]
    fn scoped_view_omits_dep_outputs_not_declared_in_depends_on() {
        let context = ctx(json!({"base": "value"}));
        let upstream = Arc::new(TaskOutput::Assertion(assertion_result("upstream", true, 0)));
        let snapshot = context
            .snapshot()
            .extend(std::iter::once((task("upstream"), upstream)));
        let snapshot = Arc::new(snapshot);
        let view = snapshot.build_scoped_view(&[]);

        // empty deps => the `$.task` subtree is not materialised at all
        let probe = extract_jsonpath_from(&view, &path("$.task.upstream.passed"))
            .expect("probe resolves to null when subtree absent");
        assert_eq!(probe, Value::Null);
    }

    #[test]
    fn identity_accessors_round_trip() {
        let run = RunId::from_string("run-2".to_owned());
        let record = RecordId(Uuid::from_u128(2));
        let scenario = Some(ScenarioId::new("greeting").expect("static scenario id is valid"));
        let context =
            ExecutionContext::new(json!({}), run.clone(), record.clone(), scenario.clone());

        assert_eq!(context.run_id(), &run);
        assert_eq!(context.record_id(), &record);
        assert_eq!(context.scenario_id(), scenario.as_ref());
    }

    #[test]
    fn extend_uses_arc_bumps_not_data_clones() {
        let context = ctx(json!({"base": "value"}));
        let upstream = Arc::new(TaskOutput::Assertion(assertion_result("upstream", true, 0)));
        let initial = context.snapshot();
        let next = initial.extend(std::iter::once((task("upstream"), Arc::clone(&upstream))));
        let later = next.extend(std::iter::empty());

        // The same Arc<TaskOutput> lives in both snapshots without re-cloning
        // the payload.
        let from_next = next.task_outputs.get(&task("upstream")).expect("present");
        let from_later = later.task_outputs.get(&task("upstream")).expect("present");
        assert!(Arc::ptr_eq(from_next, &upstream));
        assert!(Arc::ptr_eq(from_later, &upstream));
    }
}
