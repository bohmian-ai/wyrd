//! `AssertionTask` executor.
//!
//! Pure compute over JSON wrapped in `async` so the trait stays uniform. The
//! body returns immediately; no real `await` happens here.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use wyrd_spec::vala::eval::{AssertionResult, AssertionTask, EvalTask, JsonPath, TaskId};

use crate::context::{ContextSnapshot, TaskOutput, extract_required_jsonpath_from};
use crate::error::EvalExecError;
use crate::executor::TaskExecutor;
use crate::operators;

/// Executor for deterministic assertion tasks.
#[derive(Default)]
pub struct AssertionTaskExecutor;

impl AssertionTaskExecutor {
    /// Fresh assertion executor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TaskExecutor for AssertionTaskExecutor {
    async fn execute(
        &self,
        task: &EvalTask,
        snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<TaskOutput, EvalExecError> {
        let EvalTask::Assertion(assertion) = task else {
            return Err(EvalExecError::DagInvalid {
                reason: format!(
                    "stage {stage} dispatched {} task {:?} to assertion executor",
                    task.discriminator(),
                    task.id().as_str()
                ),
            });
        };
        let result = execute_assertion(assertion, snapshot, stage)?;
        Ok(TaskOutput::Assertion(result))
    }
}

fn execute_assertion(
    task: &AssertionTask,
    snapshot: &ContextSnapshot,
    stage: u32,
) -> Result<AssertionResult, EvalExecError> {
    let started_at = Utc::now();
    let view = snapshot.build_scoped_view(&task.depends_on);
    let observed = resolve_observed(task, &view, snapshot)?;
    let (passed, actual, expected, message) = match &task.item_context_path {
        Some(_) => evaluate_item_mode(task, &observed)?,
        None => evaluate_single(task, &observed)?,
    };

    Ok(AssertionResult {
        task_id: task.id.clone(),
        passed,
        actual,
        expected,
        operator: task.operator.clone(),
        message,
        stage,
        started_at,
        duration_ms: elapsed_ms(started_at),
    })
}

fn resolve_observed(
    task: &AssertionTask,
    view: &Value,
    snapshot: &ContextSnapshot,
) -> Result<Value, EvalExecError> {
    if let Some(item_path) = &task.item_context_path {
        return extract(view, item_path, &task.id);
    }
    if let Some(path) = &task.context_path {
        return extract(view, path, &task.id);
    }
    // No explicit path: compare against the whole scoped view when deps
    // contribute outputs, otherwise the bare observation document.
    if task.depends_on.is_empty() {
        Ok((*snapshot.base_context).clone())
    } else {
        Ok(view.clone())
    }
}

fn extract(value: &Value, path: &JsonPath, task_id: &TaskId) -> Result<Value, EvalExecError> {
    extract_required_jsonpath_from(value, path, task_id)
}

fn evaluate_single(
    task: &AssertionTask,
    observed: &Value,
) -> Result<(bool, Option<Value>, Value, Option<String>), EvalExecError> {
    let verdict = operators::evaluate_operator(observed, &task.operator, &task.expected).map_err(
        |error| EvalExecError::OperatorTypeMismatch {
            task_id: task.id.clone(),
            operator: format!("{:?}", task.operator),
            reason: error.to_string(),
        },
    )?;
    let message = if verdict.passed {
        None
    } else {
        Some(format!(
            "operator {:?} returned false for observed={observed}",
            task.operator
        ))
    };
    Ok((
        verdict.passed,
        verdict.observed,
        verdict.expected.unwrap_or_else(|| task.expected.clone()),
        message,
    ))
}

fn evaluate_item_mode(
    task: &AssertionTask,
    observed: &Value,
) -> Result<(bool, Option<Value>, Value, Option<String>), EvalExecError> {
    let items = match observed {
        Value::Array(items) => items,
        _ => {
            return Err(EvalExecError::OperatorTypeMismatch {
                task_id: task.id.clone(),
                operator: format!("{:?}", task.operator),
                reason: "item_context_path resolved to a non-array value".to_string(),
            });
        }
    };

    let mut failures = 0_usize;
    for item in items {
        let verdict = operators::evaluate_operator(item, &task.operator, &task.expected).map_err(
            |error| EvalExecError::OperatorTypeMismatch {
                task_id: task.id.clone(),
                operator: format!("{:?}", task.operator),
                reason: error.to_string(),
            },
        )?;
        if !verdict.passed {
            failures += 1;
        }
    }

    let message = if failures == 0 {
        None
    } else {
        Some(format!(
            "{failures}/{} items failed operator {:?}",
            items.len(),
            task.operator
        ))
    };
    Ok((
        failures == 0,
        Some(observed.clone()),
        task.expected.clone(),
        message,
    ))
}

fn elapsed_ms(start: chrono::DateTime<chrono::Utc>) -> u64 {
    let diff = Utc::now().signed_duration_since(start);
    diff.num_milliseconds().max(0) as u64
}
