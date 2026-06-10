//! Per-record runtime state for one task-DAG execution.
//!
//! [`ExecutionContext`] is the per-record handle every task executor
//! (`tasks/assertion.rs`, `tasks/llm_judge.rs`, `tasks/trace.rs`,
//! `tasks/agent.rs`) receives. It carries:
//!
//! - `base_context` - the immutable observation document.
//! - `dependency_outputs` - filled in stage by stage as upstream tasks
//!   complete.
//! - typed identity (`run_id`, `record_id`, `scenario_id`) - every
//!   `AssertionResult` row aggregates against these.
//!
//! Stores are owned, not shared: one `ExecutionContext` is one scenario's
//! execution, so plain `HashMap` is enough. Parallelism happens between
//! contexts, never inside one.

use std::collections::HashMap;

use jsonpath_rust::JsonPathValue;
use serde_json::Value;
use wyrd_spec::vala::eval::ids::{JsonPath, RecordId, RunId, ScenarioId, TaskId};

use crate::error::EvalExecError;

/// Per-record runtime context handed to every task executor.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Immutable observation document for this record.
    base_context: Value,
    /// Outputs of upstream tasks, accumulated stage by stage.
    dependency_outputs: HashMap<TaskId, Value>,
    /// Enclosing scenario id, when running scenario-driven evaluation.
    scenario_id: Option<ScenarioId>,
    /// Eval run this record belongs to.
    run_id: RunId,
    /// Record this context represents.
    record_id: RecordId,
}

impl ExecutionContext {
    /// Build a fresh per-record context.
    #[must_use]
    pub fn new(
        base_context: Value,
        run_id: RunId,
        record_id: RecordId,
        scenario_id: Option<ScenarioId>,
    ) -> Self {
        Self {
            base_context,
            dependency_outputs: HashMap::new(),
            scenario_id,
            run_id,
            record_id,
        }
    }

    /// The immutable observation document this context wraps.
    #[must_use]
    pub fn base_context(&self) -> &Value {
        &self.base_context
    }

    /// The eval run this context belongs to.
    #[must_use]
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// The record this context represents.
    #[must_use]
    pub fn record_id(&self) -> &RecordId {
        &self.record_id
    }

    /// The enclosing scenario id, when present.
    #[must_use]
    pub fn scenario_id(&self) -> Option<&ScenarioId> {
        self.scenario_id.as_ref()
    }

    /// Read the output an upstream task produced.
    ///
    /// Returns `None` when the dependency is unsatisfied, skipped, or not yet
    /// executed. Callers decide whether absence is fatal.
    #[must_use]
    pub fn dependency_output(&self, task: &TaskId) -> Option<&Value> {
        self.dependency_outputs.get(task)
    }

    /// Builder-style: attach an upstream task's output before handing the
    /// context to the next stage.
    #[must_use]
    pub fn with_dependency_output(mut self, task: TaskId, output: Value) -> Self {
        self.dependency_outputs.insert(task, output);
        self
    }

    /// Record an upstream task's output in place.
    pub fn set_dependency_output(&mut self, task: TaskId, output: Value) {
        self.dependency_outputs.insert(task, output);
    }

    /// Extract the value addressed by `path` from `base_context`.
    ///
    /// This is the single Wyrd-typed surface over the workspace JSONPath
    /// dependency. Executor code should not depend on the upstream error type.
    ///
    /// # Errors
    /// Returns [`EvalExecError::JsonPathFailure`] when the path fails to
    /// compile or evaluate.
    pub fn extract_jsonpath(&self, path: &JsonPath) -> Result<Value, EvalExecError> {
        extract_jsonpath_from(&self.base_context, path)
    }
}

/// Extract a JSON value at `path` from any JSON value.
///
/// # Errors
/// See [`ExecutionContext::extract_jsonpath`].
pub fn extract_jsonpath_from(value: &Value, path: &JsonPath) -> Result<Value, EvalExecError> {
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

    Ok(match values.as_slice() {
        [] => Value::Null,
        [only] => only.clone(),
        _ => Value::Array(values),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

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

    #[test]
    fn extracts_scalar_from_base_context() {
        let context = ctx(json!({"response": "hello"}));
        let extracted = context
            .extract_jsonpath(&path("$.response"))
            .expect("scalar extraction succeeds");
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
        let extracted = context
            .extract_jsonpath(&path("$.tool_calls[*].name"))
            .expect("array extraction succeeds");
        assert_eq!(extracted, json!(["search", "summarise"]));
    }

    #[test]
    fn missing_path_returns_null_not_error() {
        let context = ctx(json!({"response": "hello"}));
        let extracted = context
            .extract_jsonpath(&path("$.never"))
            .expect("missing path resolves to null");
        assert_eq!(extracted, Value::Null);
    }

    #[test]
    fn invalid_runtime_path_surfaces_typed_error() {
        let context = ctx(json!({"a": 1}));
        let bad = path("$.a[?(@.missing>)]");
        let error = context
            .extract_jsonpath(&bad)
            .expect_err("malformed predicate fails");
        assert!(matches!(
            error,
            EvalExecError::JsonPathFailure { ref path, .. } if path == bad.as_str()
        ));
    }

    #[test]
    fn dependency_output_round_trip_is_builder_and_mutator() {
        let mut context =
            ctx(json!({})).with_dependency_output(task("upstream_a"), json!({"score": 0.9}));
        assert_eq!(
            context.dependency_output(&task("upstream_a")),
            Some(&json!({"score": 0.9}))
        );

        context.set_dependency_output(task("upstream_b"), json!(true));
        assert_eq!(
            context.dependency_output(&task("upstream_b")),
            Some(&json!(true))
        );

        assert!(context.dependency_output(&task("never")).is_none());
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
}
