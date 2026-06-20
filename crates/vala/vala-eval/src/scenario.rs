//! Scenario loader plus per-scenario execution pass.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use wyrd_spec::vala::eval::{
    AssertionResult, ConditionCombinator, EvalCondition, EvalScenario, ExecutionPlan, JsonPath,
    RecordId, RunId, ScenarioId, ScenarioTask, TaskId,
};
use wyrd_spec::vala::ids::TraceId;

use crate::context::{
    ContextSnapshot, ExecutionContext, RecordIdentity, extract_jsonpath_from,
    extract_required_jsonpath_from,
};
use crate::error::{EvalExecError, EvalPlanError};
use crate::executor::{Executors, TaskRunOutcome, execute_plan};
use crate::operators;
use crate::store::TaskRegistry;
use crate::tasks::MediaBindings;

/// Engine-local thin wrapper around scenario rows.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalScenarioCollection {
    /// Scenarios loaded from JSON, YAML, or JSONL.
    pub scenarios: Vec<EvalScenario>,
}

/// Load a scenario collection from `.json`, `.yaml`, `.yml`, or `.jsonl`.
///
/// JSON and YAML files are top-level arrays of `EvalScenario`. JSONL files
/// contain one `EvalScenario` per non-empty line with no collection header.
///
/// # Errors
/// Returns [`EvalPlanError`] for missing files, unreadable or malformed
/// payloads, unsupported extensions, duplicate ids, empty collections, or
/// per-scenario validation failures.
pub fn load_scenario_collection(path: &Path) -> Result<EvalScenarioCollection, EvalPlanError> {
    let path_display = path.display().to_string();
    if !path.exists() {
        return Err(EvalPlanError::ScenarioFileMissing { path: path_display });
    }

    let raw =
        std::fs::read_to_string(path).map_err(|error| EvalPlanError::ScenarioFileUnreadable {
            path: path_display.clone(),
            line: None,
            reason: error.to_string(),
        })?;

    let scenarios = match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => parse_json(&raw, &path_display)?,
        Some("yaml") | Some("yml") => parse_yaml(&raw, &path_display)?,
        Some("jsonl") => parse_jsonl(&raw, &path_display)?,
        other => {
            return Err(EvalPlanError::ScenarioFileExtUnsupported {
                path: path_display,
                ext: other.map(ToOwned::to_owned),
            });
        }
    };

    validate_collection(scenarios, &path_display)
}

fn parse_json(raw: &str, path: &str) -> Result<Vec<EvalScenario>, EvalPlanError> {
    serde_json::from_str(raw).map_err(|error| EvalPlanError::ScenarioFileUnreadable {
        path: path.to_owned(),
        line: None,
        reason: error.to_string(),
    })
}

fn parse_yaml(raw: &str, path: &str) -> Result<Vec<EvalScenario>, EvalPlanError> {
    serde_yaml::from_str(raw).map_err(|error| EvalPlanError::ScenarioFileUnreadable {
        path: path.to_owned(),
        line: None,
        reason: error.to_string(),
    })
}

fn parse_jsonl(raw: &str, path: &str) -> Result<Vec<EvalScenario>, EvalPlanError> {
    let mut scenarios = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let scenario = serde_json::from_str(trimmed).map_err(|error| {
            EvalPlanError::ScenarioFileUnreadable {
                path: path.to_owned(),
                line: Some(index + 1),
                reason: error.to_string(),
            }
        })?;
        scenarios.push(scenario);
    }
    Ok(scenarios)
}

fn validate_collection(
    scenarios: Vec<EvalScenario>,
    path: &str,
) -> Result<EvalScenarioCollection, EvalPlanError> {
    if scenarios.is_empty() {
        return Err(EvalPlanError::ScenarioCollectionEmpty {
            path: path.to_owned(),
        });
    }

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for scenario in &scenarios {
        if !seen.insert(scenario.id.as_str()) {
            return Err(EvalPlanError::ScenarioCollectionDuplicateId {
                path: path.to_owned(),
                scenario_id: scenario.id.as_str().to_owned(),
            });
        }
    }

    for scenario in &scenarios {
        scenario
            .validate()
            .map_err(|error| EvalPlanError::ScenarioCollectionInvalid {
                path: path.to_owned(),
                scenario_id: Some(scenario.id.as_str().to_owned()),
                reason: error.to_string(),
            })?;
    }

    Ok(EvalScenarioCollection { scenarios })
}

/// One eval record plus media and trace bindings supplied by the orchestrator.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordWithMedia {
    /// Record identity.
    pub record_id: RecordId,
    /// Optional trace id for trace assertion tasks.
    pub trace_id: Option<TraceId>,
    /// Per-record JSON context.
    pub context: Value,
    /// Media bindings visible to judge tasks.
    pub media: MediaBindings,
    /// Media ids that must be bound before judge tasks invoke.
    pub required_media: Vec<String>,
}

/// One mechanic task result associated with its record.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordTaskResult {
    /// Record that produced the task result.
    pub record_id: RecordId,
    /// Assertion result emitted by the task.
    pub result: AssertionResult,
}

/// Inputs for executing one completed scenario.
pub struct ScenarioExecutionInputs<'a> {
    /// Scenario being evaluated.
    pub scenario: &'a EvalScenario,
    /// Precomputed task execution plan.
    pub plan: &'a ExecutionPlan,
    /// Task registry for the plan.
    pub registry: &'a TaskRegistry,
    /// Injected executor bundle.
    pub executors: &'a Executors,
    /// Completed records for the scenario.
    pub records: &'a [RecordWithMedia],
    /// Final response produced by the orchestrator.
    pub final_response: &'a Value,
    /// Run id shared by all records in this scenario execution.
    pub run_id: RunId,
}

/// Results from mechanic and passenger scenario evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioExecutionResults {
    /// Scenario that was evaluated.
    pub scenario_id: ScenarioId,
    /// Flat mechanic results by record and task.
    pub mechanic: Vec<RecordTaskResult>,
    /// Passenger task results against the final response.
    pub passenger: Vec<AssertionResult>,
}

/// Execute one completed scenario through mechanic and passenger passes.
///
/// # Errors
/// Returns [`EvalExecError`] when the execution plan cannot run or passenger
/// task conditions fail to evaluate.
pub async fn execute_scenario(
    inputs: ScenarioExecutionInputs<'_>,
) -> Result<ScenarioExecutionResults, EvalExecError> {
    let mut mechanic = Vec::new();

    for record in inputs.records {
        let identity = RecordIdentity {
            run_id: inputs.run_id.clone(),
            record_id: record.record_id.clone(),
            scenario_id: Some(inputs.scenario.id.clone()),
        };
        let mut snapshot = ContextSnapshot::new(Arc::new(record.context.clone()), identity);
        snapshot.trace_id = record.trace_id;
        snapshot.media = Arc::new(record.media.clone());
        snapshot.required_media = Arc::new(record.required_media.clone());

        let cx = ExecutionContext::from_snapshot(Arc::new(snapshot));
        let report = execute_plan(inputs.plan, &cx, inputs.registry, inputs.executors).await?;

        for outcome in report.outcomes {
            if let TaskRunOutcome::Ran(result) = outcome {
                mechanic.push(RecordTaskResult {
                    record_id: record.record_id.clone(),
                    result,
                });
            }
        }
    }

    let passenger = run_passenger_pass(inputs.scenario, inputs.final_response)?;

    Ok(ScenarioExecutionResults {
        scenario_id: inputs.scenario.id.clone(),
        mechanic,
        passenger,
    })
}

fn run_passenger_pass(
    scenario: &EvalScenario,
    final_response: &Value,
) -> Result<Vec<AssertionResult>, EvalExecError> {
    let pair = serde_json::json!({
        "response": final_response,
        "expected_outcome": scenario.expected_outcome,
    });
    let response_path = response_json_path()?;
    let mut out = Vec::with_capacity(scenario.tasks.len());
    for task in &scenario.tasks {
        if let Some(condition) = &task.condition
            && !evaluate_passenger_condition(&task.id, condition, &pair)?
        {
            continue;
        }
        out.push(evaluate_passenger_task(task, &pair, &response_path)?);
    }
    Ok(out)
}

fn evaluate_passenger_task(
    task: &ScenarioTask,
    pair: &Value,
    response_path: &JsonPath,
) -> Result<AssertionResult, EvalExecError> {
    let started_at = Utc::now();
    let response = extract_jsonpath_from(pair, response_path)?;
    let verdict = operators::evaluate_operator(&response, &task.operator, &task.expected);
    let (passed, actual, expected, message) = match verdict {
        Ok(verdict) => (
            verdict.passed,
            verdict.observed,
            verdict.expected.unwrap_or_else(|| task.expected.clone()),
            None,
        ),
        Err(error) => (
            false,
            Some(response),
            task.expected.clone(),
            Some(error.to_string()),
        ),
    };

    Ok(AssertionResult {
        task_id: task.id.clone(),
        passed,
        actual,
        expected,
        operator: task.operator.clone(),
        message,
        stage: 0,
        started_at,
        duration_ms: elapsed_ms(started_at),
    })
}

fn evaluate_passenger_condition(
    task_id: &TaskId,
    condition: &EvalCondition,
    pair: &Value,
) -> Result<bool, EvalExecError> {
    let mut current = Some(condition);
    let mut acc: Option<bool> = None;
    let mut pending: Option<ConditionCombinator> = None;
    while let Some(node) = current {
        let observed = extract_required_jsonpath_from(pair, &node.path, task_id).map_err(
            |error| match error {
                EvalExecError::ExtractPathMissing { .. } => EvalExecError::ConditionPathMissing {
                    task_id: task_id.clone(),
                    path: node.path.as_str().to_owned(),
                },
                other => other,
            },
        )?;
        let this = operators::evaluate_operator(&observed, &node.operator, &node.expected)
            .map_err(|error| EvalExecError::OperatorTypeMismatch {
                task_id: task_id.clone(),
                operator: format!("{:?}", node.operator),
                reason: error.to_string(),
            })?
            .passed;

        acc = match (acc, pending) {
            (None, _) => Some(this),
            (Some(prev), Some(ConditionCombinator::And)) => Some(prev && this),
            (Some(prev), Some(ConditionCombinator::Or)) => Some(prev || this),
            (Some(prev), None) => Some(prev && this),
        };
        pending = node.combinator;

        match (acc, pending) {
            (Some(false), Some(ConditionCombinator::And)) => return Ok(false),
            (Some(true), Some(ConditionCombinator::Or)) => return Ok(true),
            _ => {}
        }

        current = node.subsequent.as_deref();
    }
    Ok(acc.unwrap_or(true))
}

fn response_json_path() -> Result<JsonPath, EvalExecError> {
    JsonPath::new("$.response").map_err(|error| EvalExecError::JsonPathFailure {
        path: "$.response".to_owned(),
        reason: format!("hard-coded scenario JSONPath invalid: {error}"),
    })
}

fn elapsed_ms(start: chrono::DateTime<chrono::Utc>) -> u64 {
    let diff = Utc::now().signed_duration_since(start);
    diff.num_milliseconds().max(0) as u64
}
