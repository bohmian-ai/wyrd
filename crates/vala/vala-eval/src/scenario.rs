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

#[cfg(test)]
mod scenario_execution {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::scenario::{RecordWithMedia, ScenarioExecutionInputs, execute_scenario};
    use crate::store::TaskRegistry;
    use crate::tasks::{
        AgentTaskExecutor, AssertionTaskExecutor, EvalMediaBinding, JudgeTaskExecutor,
        MediaBindings, TraceTaskExecutor,
    };
    use crate::{Executors, InMemoryTraceSource, MockJudgeInvoker};
    use serde_json::{Value, json};
    use uuid::Uuid;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::vala::eval::{
        AssertionTask, ComparisonOperator, EvalScenario, EvalSpec, EvalTask, LlmJudgeTask,
        RecordId, RunId, ScenarioId, ScenarioTask, TaskId,
    };
    use wyrd_spec::vala::ids::TraceId;

    fn tid(value: &str) -> TaskId {
        TaskId::new(value).expect("static task id is valid")
    }

    fn sid(value: &str) -> ScenarioId {
        ScenarioId::new(value).expect("static scenario id is valid")
    }

    fn judge_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Prompt,
            name: CardName::new("scenario-judge").expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("default").expect("valid space"),
            uid: None,
        }
    }

    fn assertion_task() -> EvalTask {
        EvalTask::Assertion(AssertionTask {
            id: tid("assert_score"),
            context_path: Some(
                wyrd_spec::vala::eval::JsonPath::new("$.score").expect("static JSONPath is valid"),
            ),
            item_context_path: None,
            operator: ComparisonOperator::Equals,
            expected: json!(true),
            depends_on: Vec::new(),
            condition: None,
        })
    }

    fn judge_task() -> EvalTask {
        EvalTask::LlmJudge(LlmJudgeTask {
            id: tid("judge_response"),
            judge_ref: judge_card_ref(),
            context_path: None,
            expected: json!({"passed": true}),
            operator: ComparisonOperator::Equals,
            depends_on: Vec::new(),
            max_retries: 0,
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

    fn scenario() -> EvalScenario {
        EvalScenario {
            id: sid("happy_path"),
            initial_query: "Say hello".to_owned(),
            expected_outcome: Some("friendly greeting".to_owned()),
            predefined_turns: Vec::new(),
            simulated_user_persona: None,
            termination_signal: None,
            max_turns: 8,
            tasks: vec![ScenarioTask {
                id: tid("contains_hello"),
                operator: ComparisonOperator::Contains,
                expected: json!("hello"),
                condition: None,
            }],
        }
    }

    fn executors(mock: Arc<MockJudgeInvoker>) -> Executors {
        Executors {
            assertion: Arc::new(AssertionTaskExecutor::new()),
            judge: Arc::new(JudgeTaskExecutor::new(mock)),
            trace: Arc::new(TraceTaskExecutor::new(
                Arc::new(InMemoryTraceSource::new()),
                Duration::from_millis(250),
            )),
            agent: Arc::new(AgentTaskExecutor::new()),
        }
    }

    fn media() -> MediaBindings {
        let mut media = MediaBindings::new();
        media.insert(EvalMediaBinding {
            id: "screenshot".to_owned(),
            payload: json!({"kind": "image", "uri": "file:///tmp/screenshot.png"}),
        });
        media
    }

    fn record(id: u128, trace_hex: &str, context: Value) -> RecordWithMedia {
        RecordWithMedia {
            record_id: RecordId(Uuid::from_u128(id)),
            trace_id: Some(TraceId::from_hex(trace_hex).expect("static trace id is valid")),
            context,
            media: media(),
            required_media: vec!["screenshot".to_owned()],
        }
    }

    #[tokio::test]
    async fn one_pass_emits_mechanic_and_passenger_results() {
        let spec = spec_of(vec![assertion_task(), judge_task()]);
        let plan = spec.execution_plan().expect("test spec has valid DAG");
        let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
        let mock =
            MockJudgeInvoker::new([Ok(json!({"passed": true})), Ok(json!({"passed": true}))]);
        let executors = executors(Arc::clone(&mock));
        let scenario = scenario();
        let records = vec![
            record(
                1,
                "00000000000000000000000000000001",
                json!({"score": true, "response": "first"}),
            ),
            record(
                2,
                "00000000000000000000000000000002",
                json!({"score": true, "response": "second"}),
            ),
        ];
        let final_response = json!("hello from the agent");

        let results = execute_scenario(ScenarioExecutionInputs {
            scenario: &scenario,
            plan: &plan,
            registry: &registry,
            executors: &executors,
            records: &records,
            final_response: &final_response,
            run_id: RunId::from_string("run-scenario".to_owned()),
        })
        .await
        .expect("scenario executes");

        assert_eq!(results.scenario_id, sid("happy_path"));
        assert_eq!(results.mechanic.len(), 4);
        assert!(results.mechanic.iter().all(|result| result.result.passed));
        assert_eq!(results.passenger.len(), 1);
        assert!(results.passenger[0].passed);
        assert_eq!(results.passenger[0].actual, Some(final_response.clone()));

        let calls = mock.calls().await;
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0].1["media"]["screenshot"]["payload"]["uri"],
            json!("file:///tmp/screenshot.png")
        );
        assert_eq!(
            calls[1].1["media"]["screenshot"]["payload"]["uri"],
            json!("file:///tmp/screenshot.png")
        );
    }

    #[tokio::test]
    async fn passenger_pass_runs_even_when_no_records() {
        let spec = spec_of(vec![assertion_task(), judge_task()]);
        let plan = spec.execution_plan().expect("test spec has valid DAG");
        let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
        let mock = MockJudgeInvoker::new([]);
        let executors = executors(Arc::clone(&mock));
        let scenario = scenario();
        let final_response = json!("hello after a crash");

        let results = execute_scenario(ScenarioExecutionInputs {
            scenario: &scenario,
            plan: &plan,
            registry: &registry,
            executors: &executors,
            records: &[],
            final_response: &final_response,
            run_id: RunId::from_string("run-no-records".to_owned()),
        })
        .await
        .expect("scenario executes");

        assert!(results.mechanic.is_empty());
        assert_eq!(results.passenger.len(), 1);
        assert!(results.passenger[0].passed);
        assert_eq!(results.passenger[0].actual, Some(final_response));
        assert_eq!(mock.calls().await.len(), 0);
    }
}

#[cfg(test)]
mod scenario_loader {
    use std::path::{Path, PathBuf};

    use crate::{EvalPlanError, load_scenario_collection};
    use serde_json::json;
    use tempfile::TempDir;
    use wyrd_spec::vala::eval::{
        ComparisonOperator, EvalScenario, ScenarioId, ScenarioTask, TaskId,
    };

    fn sid(value: &str) -> ScenarioId {
        ScenarioId::new(value).expect("static scenario id is valid")
    }

    fn tid(value: &str) -> TaskId {
        TaskId::new(value).expect("static task id is valid")
    }

    fn scenario(id: &str) -> EvalScenario {
        EvalScenario {
            id: sid(id),
            initial_query: format!("question for {id}"),
            expected_outcome: Some("answer".to_owned()),
            predefined_turns: Vec::new(),
            simulated_user_persona: None,
            termination_signal: None,
            max_turns: 8,
            tasks: vec![ScenarioTask {
                id: tid("contains_answer"),
                operator: ComparisonOperator::Contains,
                expected: json!("answer"),
                condition: None,
            }],
        }
    }

    fn invalid_scenario(id: &str) -> EvalScenario {
        EvalScenario {
            initial_query: String::new(),
            ..scenario(id)
        }
    }

    fn write_file(dir: &TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, body).expect("test fixture writes");
        path
    }

    fn load(path: &Path) -> Result<crate::EvalScenarioCollection, EvalPlanError> {
        load_scenario_collection(path)
    }

    #[test]
    fn loads_json_collection() {
        let dir = TempDir::new().expect("temp dir creates");
        let body = serde_json::to_string(&vec![scenario("alpha")]).expect("scenario serializes");
        let path = write_file(&dir, "scenarios.json", &body);

        let collection = load(&path).expect("json collection loads");

        assert_eq!(collection.scenarios.len(), 1);
        assert_eq!(collection.scenarios[0].id, sid("alpha"));
    }

    #[test]
    fn loads_yaml_collection() {
        let dir = TempDir::new().expect("temp dir creates");
        let body = serde_yaml::to_string(&vec![scenario("alpha")]).expect("scenario serializes");
        let path = write_file(&dir, "scenarios.yaml", &body);

        let collection = load(&path).expect("yaml collection loads");

        assert_eq!(collection.scenarios.len(), 1);
        assert_eq!(collection.scenarios[0].id, sid("alpha"));
    }

    #[test]
    fn loads_jsonl_collection() {
        let dir = TempDir::new().expect("temp dir creates");
        let first = serde_json::to_string(&scenario("alpha")).expect("scenario serializes");
        let second = serde_json::to_string(&scenario("beta")).expect("scenario serializes");
        let path = write_file(&dir, "scenarios.jsonl", &format!("{first}\n\n{second}\n"));

        let collection = load(&path).expect("jsonl collection loads");

        assert_eq!(collection.scenarios.len(), 2);
        assert_eq!(collection.scenarios[0].id, sid("alpha"));
        assert_eq!(collection.scenarios[1].id, sid("beta"));
    }

    #[test]
    fn missing_file_errors_typed() {
        let dir = TempDir::new().expect("temp dir creates");
        let path = dir.path().join("missing.json");

        let error = load(&path).expect_err("missing file fails");

        assert!(matches!(error, EvalPlanError::ScenarioFileMissing { .. }));
    }

    #[test]
    fn malformed_json_errors_typed() {
        let dir = TempDir::new().expect("temp dir creates");
        let path = write_file(&dir, "scenarios.json", "{");

        let error = load(&path).expect_err("malformed json fails");

        assert!(matches!(
            error,
            EvalPlanError::ScenarioFileUnreadable { line: None, .. }
        ));
    }

    #[test]
    fn malformed_jsonl_carries_line_number() {
        let dir = TempDir::new().expect("temp dir creates");
        let first = serde_json::to_string(&scenario("alpha")).expect("scenario serializes");
        let path = write_file(&dir, "scenarios.jsonl", &format!("{first}\n{{\n"));

        let error = load(&path).expect_err("malformed jsonl fails");

        assert!(matches!(
            error,
            EvalPlanError::ScenarioFileUnreadable { line: Some(2), .. }
        ));
    }

    #[test]
    fn unknown_extension_errors_typed() {
        let dir = TempDir::new().expect("temp dir creates");
        let path = write_file(&dir, "scenarios.txt", "[]");

        let error = load(&path).expect_err("unknown extension fails");

        assert!(matches!(
            error,
            EvalPlanError::ScenarioFileExtUnsupported { .. }
        ));
    }

    #[test]
    fn duplicate_scenario_id_errors_typed_before_generic_validator() {
        let dir = TempDir::new().expect("temp dir creates");
        let body =
            serde_json::to_string(&vec![invalid_scenario("alpha"), invalid_scenario("alpha")])
                .expect("scenario serializes");
        let path = write_file(&dir, "scenarios.json", &body);

        let error = load(&path).expect_err("duplicate id fails first");

        assert!(matches!(
            error,
            EvalPlanError::ScenarioCollectionDuplicateId {
                ref scenario_id,
                ..
            } if scenario_id == "alpha"
        ));
    }

    #[test]
    fn empty_collection_errors_typed() {
        let dir = TempDir::new().expect("temp dir creates");
        let path = write_file(&dir, "scenarios.json", "[]");

        let error = load(&path).expect_err("empty collection fails");

        assert!(matches!(
            error,
            EvalPlanError::ScenarioCollectionEmpty { .. }
        ));
    }
}
