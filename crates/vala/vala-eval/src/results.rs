//! Results aggregation and persistence for `vala-eval`.
//!
//! Owns the Wyrd-native result types emitted at the end of an eval run.
//! Aggregation walks per-task to per-scenario to per-subject to run-level,
//! then applies the spec's pass gate to produce a typed verdict.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{
    AssertionResult, EvalContextCapture, EvalPassGate, ScenarioId, TaskId,
};
use wyrd_spec::vala::ids::RunId;

use crate::error::EvalExecError;

/// Canonical key used to look up a subject in result maps.
///
/// `CardRef` is not `Ord` or `Hash`, so aggregation uses the canonical string
/// form `"{kind}::{space|-}::{name}::{version}"`. The full `CardRef` remains
/// on `SubjectResults::subject_ref`. UID is intentionally excluded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubjectKey(pub String);

impl SubjectKey {
    /// Build a stable subject key from a card reference.
    #[must_use]
    pub fn from_ref(card_ref: &CardRef) -> Self {
        let kind = format!("{:?}", card_ref.kind).to_lowercase();
        let space = card_ref
            .space
            .as_ref()
            .map_or("<missing-space>", |space| space.as_str());
        Self(format!(
            "{kind}::{space}::{}::{}",
            card_ref.name.as_str(),
            card_ref.version
        ))
    }

    /// Borrow the canonical key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Top-level results artifact emitted at the end of one eval run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalResults {
    /// Run identity.
    pub identity: RunIdentity,
    /// Per-scenario results keyed by scenario id.
    pub scenarios: BTreeMap<ScenarioId, ScenarioResult>,
    /// Per-subject rollup keyed by canonical subject key.
    pub subjects: BTreeMap<SubjectKey, SubjectResults>,
    /// Run-level aggregate metrics.
    pub metrics: EvalMetrics,
    /// Pass-gate verdict, absent when the spec declared no pass gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_gate_verdict: Option<PassGateVerdict>,
    /// Aggregation config carried for self-describing artifacts.
    pub config: ResultsConfig,
}

/// Identity of one eval run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunIdentity {
    /// Typed run identifier.
    pub run_id: RunId,
    /// Eval card whose execution produced this run.
    pub eval_ref: CardRef,
    /// Wall-clock UTC start.
    pub started_at: DateTime<Utc>,
    /// Wall-clock UTC end.
    pub ended_at: DateTime<Utc>,
}

/// Config captured by the results artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultsConfig {
    /// How `AssertionResult.actual` was transformed. Absent means `Full`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_capture: Option<EvalContextCapture>,
    /// Pass gate declared by the spec, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_gate: Option<EvalPassGate>,
}

/// Per-scenario result carrying mechanic and passenger views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioResult {
    /// Scenario identifier.
    pub scenario_id: ScenarioId,
    /// Mechanic view: per-subject, per-task assertion results.
    pub mechanic_results: BTreeMap<SubjectKey, BTreeMap<TaskId, Vec<AssertionResult>>>,
    /// Passenger view: one summary per scenario task.
    pub passenger_tasks: Vec<TaskSummary>,
    /// Conversation history captured by the orchestrator.
    #[serde(default)]
    pub conversation_history: Vec<String>,
    /// Whether both passenger and mechanic views passed.
    pub scenario_passed: bool,
    /// Wall-clock UTC start.
    pub started_at: DateTime<Utc>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Per-subject rollup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectResults {
    /// Full subject card reference.
    pub subject_ref: CardRef,
    /// Task results for this subject across all scenarios.
    pub task_results: BTreeMap<TaskId, Vec<AssertionResult>>,
    /// Aggregate metrics for this subject.
    pub metrics: SubjectMetrics,
}

/// Per-subject metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectMetrics {
    /// `passed_tasks / total_tasks`, or `0.0` when empty.
    pub pass_rate: f64,
    /// Per-task pass rate within this subject.
    pub per_task_pass_rate: BTreeMap<TaskId, f64>,
    /// Total assertion results for this subject.
    pub total_tasks: usize,
    /// Passed assertion results for this subject.
    pub passed_tasks: usize,
}

/// Run-level aggregate metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalMetrics {
    /// Total assertion results across the run.
    pub total_tasks: usize,
    /// Passed assertion results across the run.
    pub passed_tasks: usize,
    /// `passed_tasks / total_tasks`, or `0.0` when empty.
    pub pass_rate: f64,
    /// Per-task pass rate across all subjects.
    pub per_task_pass_rate: BTreeMap<TaskId, f64>,
    /// Per-subject pass rate.
    pub per_subject_pass_rate: BTreeMap<SubjectKey, f64>,
    /// Total scenarios in the run.
    pub total_scenarios: usize,
    /// Scenarios where `scenario_passed` is true.
    pub passed_scenarios: usize,
    /// `passed_scenarios / total_scenarios`, or `0.0` when empty.
    pub scenario_pass_rate: f64,
}

/// Passenger-view task summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSummary {
    /// Scenario task id.
    pub task_id: TaskId,
    /// Whether the task passed.
    pub passed: bool,
    /// Topological stage at execution time.
    pub stage: u32,
    /// Optional human-readable detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Verdict of applying an eval pass gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassGateVerdict {
    /// Gate used to compute the verdict.
    pub gate: EvalPassGate,
    /// Final pass/fail.
    pub passed: bool,
    /// Observed value compared against the threshold.
    pub observed: f64,
    /// Gate threshold.
    pub threshold: f64,
    /// Human-readable explanation.
    pub reason: String,
}

/// Input bundle for `aggregate_run`.
#[derive(Debug, Clone)]
pub struct AggregationInput {
    /// Run identity.
    pub identity: RunIdentity,
    /// Scenario inputs.
    pub scenarios: Vec<ScenarioAggregationInput>,
    /// Aggregation config.
    pub config: ResultsConfig,
}

/// Per-scenario aggregation input.
#[derive(Debug, Clone)]
pub struct ScenarioAggregationInput {
    /// Scenario id.
    pub scenario_id: ScenarioId,
    /// Mechanic view grouped by subject.
    pub mechanic_results: BTreeMap<SubjectKey, MechanicSubjectInput>,
    /// Passenger-view summaries.
    pub passenger_tasks: Vec<TaskSummary>,
    /// Captured conversation history.
    pub conversation_history: Vec<String>,
    /// Wall-clock UTC start.
    pub started_at: DateTime<Utc>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Per-subject mechanic input within a scenario.
#[derive(Debug, Clone)]
pub struct MechanicSubjectInput {
    /// Full subject card reference.
    pub subject_ref: CardRef,
    /// Task results emitted by this subject during the scenario.
    pub task_results: BTreeMap<TaskId, Vec<AssertionResult>>,
}

/// Aggregate one eval run into durable results.
///
/// # Errors
///
/// Returns `ContextHashFailed` if context capture hashing cannot serialize a
/// JSON value.
pub fn aggregate_run(input: AggregationInput) -> Result<EvalResults, EvalExecError> {
    let capture = input
        .config
        .context_capture
        .unwrap_or(EvalContextCapture::Full);
    let mut subject_accum = BTreeMap::new();
    let mut scenarios = BTreeMap::new();

    for scenario_input in input.scenarios {
        let mut mechanic_results = BTreeMap::new();
        let mut mechanic_total = 0usize;
        let mut all_mechanic_passed = true;

        for (subject_key, mechanic_input) in scenario_input.mechanic_results {
            let mut per_task_out: BTreeMap<TaskId, Vec<AssertionResult>> = BTreeMap::new();

            for (task_id, rows) in mechanic_input.task_results {
                let mut transformed = Vec::with_capacity(rows.len());
                for row in rows {
                    let row = apply_context_capture(row, capture)?;
                    if !row.passed {
                        all_mechanic_passed = false;
                    }
                    mechanic_total += 1;
                    transformed.push(row);
                }

                per_task_out
                    .entry(task_id.clone())
                    .or_default()
                    .extend(transformed.iter().cloned());

                let accumulator = subject_accum
                    .entry(subject_key.clone())
                    .or_insert_with(|| SubjectAccumulator::new(mechanic_input.subject_ref.clone()));
                accumulator.add_task_results(task_id, transformed);
            }

            mechanic_results.insert(subject_key, per_task_out);
        }

        let passenger_total = scenario_input.passenger_tasks.len();
        let all_passenger_passed = scenario_input
            .passenger_tasks
            .iter()
            .all(|task| task.passed);
        let scenario_passed = scenario_passed_from_views(
            mechanic_total,
            all_mechanic_passed,
            passenger_total,
            all_passenger_passed,
        );

        scenarios.insert(
            scenario_input.scenario_id.clone(),
            ScenarioResult {
                scenario_id: scenario_input.scenario_id,
                mechanic_results,
                passenger_tasks: scenario_input.passenger_tasks,
                conversation_history: scenario_input.conversation_history,
                scenario_passed,
                started_at: scenario_input.started_at,
                duration_ms: scenario_input.duration_ms,
            },
        );
    }

    let subjects = subject_accum
        .into_iter()
        .map(|(key, accumulator)| (key, accumulator.finalize()))
        .collect();
    let metrics = compute_run_metrics(&scenarios, &subjects);
    let pass_gate_verdict = input
        .config
        .pass_gate
        .as_ref()
        .map(|gate| evaluate_pass_gate(gate, &metrics, &subjects));

    Ok(EvalResults {
        identity: input.identity,
        scenarios,
        subjects,
        metrics,
        pass_gate_verdict,
        config: input.config,
    })
}

/// Apply context-capture policy to a single assertion result.
///
/// # Errors
///
/// Returns `ContextHashFailed` if hashing cannot serialize the actual value.
pub fn apply_context_capture(
    mut result: AssertionResult,
    capture: EvalContextCapture,
) -> Result<AssertionResult, EvalExecError> {
    match capture {
        EvalContextCapture::Full => Ok(result),
        EvalContextCapture::Hash => {
            result.actual = result
                .actual
                .as_ref()
                .map(hash_context_value)
                .transpose()?
                .map(serde_json::Value::String);
            Ok(result)
        }
        EvalContextCapture::Redact => {
            result.actual = None;
            Ok(result)
        }
    }
}

fn hash_context_value(value: &serde_json::Value) -> Result<String, EvalExecError> {
    let bytes = serde_json::to_vec(value).map_err(|error| EvalExecError::ContextHashFailed {
        reason: error.to_string(),
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

struct SubjectAccumulator {
    subject_ref: CardRef,
    task_results: BTreeMap<TaskId, Vec<AssertionResult>>,
}

impl SubjectAccumulator {
    fn new(subject_ref: CardRef) -> Self {
        Self {
            subject_ref,
            task_results: BTreeMap::new(),
        }
    }

    fn add_task_results(&mut self, task_id: TaskId, results: Vec<AssertionResult>) {
        self.task_results
            .entry(task_id)
            .or_default()
            .extend(results);
    }

    fn finalize(self) -> SubjectResults {
        let mut total_tasks = 0usize;
        let mut passed_tasks = 0usize;
        let mut per_task_pass_rate = BTreeMap::new();

        for (task_id, rows) in &self.task_results {
            let total = rows.len();
            let passed = rows.iter().filter(|row| row.passed).count();
            total_tasks += total;
            passed_tasks += passed;
            per_task_pass_rate.insert(task_id.clone(), rate(passed, total));
        }

        SubjectResults {
            subject_ref: self.subject_ref,
            task_results: self.task_results,
            metrics: SubjectMetrics {
                pass_rate: rate(passed_tasks, total_tasks),
                per_task_pass_rate,
                total_tasks,
                passed_tasks,
            },
        }
    }
}

fn compute_run_metrics(
    scenarios: &BTreeMap<ScenarioId, ScenarioResult>,
    subjects: &BTreeMap<SubjectKey, SubjectResults>,
) -> EvalMetrics {
    let mut total_tasks = 0usize;
    let mut passed_tasks = 0usize;
    let mut per_task_counts: BTreeMap<TaskId, (usize, usize)> = BTreeMap::new();

    for subject in subjects.values() {
        total_tasks += subject.metrics.total_tasks;
        passed_tasks += subject.metrics.passed_tasks;

        for (task_id, rows) in &subject.task_results {
            let counts = per_task_counts.entry(task_id.clone()).or_insert((0, 0));
            counts.0 += rows.len();
            counts.1 += rows.iter().filter(|row| row.passed).count();
        }
    }

    let per_task_pass_rate = per_task_counts
        .into_iter()
        .map(|(task_id, (total, passed))| (task_id, rate(passed, total)))
        .collect();
    let per_subject_pass_rate = subjects
        .iter()
        .map(|(key, subject)| (key.clone(), subject.metrics.pass_rate))
        .collect();
    let total_scenarios = scenarios.len();
    let passed_scenarios = scenarios
        .values()
        .filter(|scenario| scenario.scenario_passed)
        .count();

    EvalMetrics {
        total_tasks,
        passed_tasks,
        pass_rate: rate(passed_tasks, total_tasks),
        per_task_pass_rate,
        per_subject_pass_rate,
        total_scenarios,
        passed_scenarios,
        scenario_pass_rate: rate(passed_scenarios, total_scenarios),
    }
}

fn scenario_passed_from_views(
    mechanic_total: usize,
    mechanic_all_passed: bool,
    passenger_total: usize,
    passenger_all_passed: bool,
) -> bool {
    if mechanic_total == 0 && passenger_total == 0 {
        return false;
    }

    (mechanic_total == 0 || mechanic_all_passed) && (passenger_total == 0 || passenger_all_passed)
}

fn evaluate_pass_gate(
    gate: &EvalPassGate,
    metrics: &EvalMetrics,
    subjects: &BTreeMap<SubjectKey, SubjectResults>,
) -> PassGateVerdict {
    match gate {
        EvalPassGate::OverallPassRate { threshold } => {
            let passed = metrics.pass_rate >= *threshold;
            PassGateVerdict {
                gate: gate.clone(),
                passed,
                observed: metrics.pass_rate,
                threshold: *threshold,
                reason: format!(
                    "overall pass rate {:.4} {} threshold {:.4}",
                    metrics.pass_rate,
                    if passed { ">=" } else { "<" },
                    threshold
                ),
            }
        }
        EvalPassGate::PerJudgePassRate { threshold } => {
            let observed = subjects
                .values()
                .flat_map(|subject| subject.metrics.per_task_pass_rate.values().copied())
                .fold(f64::INFINITY, f64::min);
            let observed = if observed.is_finite() { observed } else { 0.0 };
            let passed = observed >= *threshold;
            PassGateVerdict {
                gate: gate.clone(),
                passed,
                observed,
                threshold: *threshold,
                reason: format!(
                    "min per-task pass rate {:.4} {} threshold {:.4}",
                    observed,
                    if passed { ">=" } else { "<" },
                    threshold
                ),
            }
        }
        EvalPassGate::AllPass => {
            let passed = metrics.total_tasks > 0 && metrics.passed_tasks == metrics.total_tasks;
            PassGateVerdict {
                gate: gate.clone(),
                passed,
                observed: metrics.passed_tasks as f64,
                threshold: metrics.total_tasks as f64,
                reason: if metrics.total_tasks == 0 {
                    "all_pass: no tasks attested; fail".to_owned()
                } else if passed {
                    format!("all {} tasks passed", metrics.total_tasks)
                } else {
                    format!(
                        "{} / {} tasks passed",
                        metrics.passed_tasks, metrics.total_tasks
                    )
                },
            }
        }
    }
}

fn rate(passed: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        passed as f64 / total as f64
    }
}

impl EvalResults {
    /// Serialize results to pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns `ResultsSerializeFailed` when JSON serialization fails.
    pub fn to_json(&self) -> Result<String, EvalExecError> {
        serde_json::to_string_pretty(self).map_err(|error| EvalExecError::ResultsSerializeFailed {
            reason: error.to_string(),
        })
    }

    /// Deserialize results from JSON.
    ///
    /// # Errors
    ///
    /// Returns `ResultsDeserializeFailed` when JSON deserialization fails.
    pub fn from_json(raw: &str) -> Result<Self, EvalExecError> {
        serde_json::from_str(raw).map_err(|error| EvalExecError::ResultsDeserializeFailed {
            reason: error.to_string(),
        })
    }

    /// Save results to a caller-provided local path.
    ///
    /// # Errors
    ///
    /// Returns serialization or filesystem errors as `EvalExecError`.
    pub fn save(&self, path: &Path) -> Result<(), EvalExecError> {
        let raw = self.to_json()?;
        fs::write(path, raw).map_err(|error| EvalExecError::ResultsIoFailed {
            path: path.display().to_string(),
            reason: error.to_string(),
        })
    }

    /// Load results from a local path.
    ///
    /// # Errors
    ///
    /// Returns filesystem or deserialization errors as `EvalExecError`.
    pub fn load(path: &Path) -> Result<Self, EvalExecError> {
        let raw = fs::read_to_string(path).map_err(|error| EvalExecError::ResultsIoFailed {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;
        Self::from_json(&raw)
    }

    /// Render a plain-text summary table without extra rendering dependencies.
    #[must_use]
    pub fn as_table(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "EvalRun {} | eval_ref={}::{} | started {} | ended {}",
            self.identity.run_id,
            format!("{:?}", self.identity.eval_ref.kind).to_lowercase(),
            self.identity.eval_ref.name.as_str(),
            self.identity.started_at.to_rfc3339(),
            self.identity.ended_at.to_rfc3339()
        );
        let _ = writeln!(
            out,
            "overall: {} / {} tasks passed ({:.4}); {} / {} scenarios passed ({:.4})",
            self.metrics.passed_tasks,
            self.metrics.total_tasks,
            self.metrics.pass_rate,
            self.metrics.passed_scenarios,
            self.metrics.total_scenarios,
            self.metrics.scenario_pass_rate
        );

        if let Some(verdict) = &self.pass_gate_verdict {
            let _ = writeln!(
                out,
                "pass_gate: {} - {}",
                if verdict.passed { "PASS" } else { "FAIL" },
                verdict.reason
            );
        } else {
            let _ = writeln!(out, "pass_gate: (none)");
        }

        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{:<48} | {:>10} | {:<14}",
            "subject_key", "pass_rate", "passed / total"
        );
        let _ = writeln!(out, "{}", "-".repeat(78));
        for (key, subject) in &self.subjects {
            let _ = writeln!(
                out,
                "{:<48} | {:>10.4} | {} / {}",
                truncate(key.as_str(), 48),
                subject.metrics.pass_rate,
                subject.metrics.passed_tasks,
                subject.metrics.total_tasks
            );
        }

        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{:<32} | {:>6} | {:>10}",
            "scenario_id", "passed", "duration_ms"
        );
        let _ = writeln!(out, "{}", "-".repeat(56));
        for (scenario_id, scenario) in &self.scenarios {
            let _ = writeln!(
                out,
                "{:<32} | {:>6} | {:>10}",
                truncate(scenario_id.as_str(), 32),
                scenario.scenario_passed,
                scenario.duration_ms
            );
        }

        out
    }
}

fn truncate(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        value.to_owned()
    } else {
        format!("{}...", &value[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod results_aggregation {
    //! Aggregation pipeline, context-capture, and persistence tests.

    use std::collections::BTreeMap;

    use chrono::Utc;
    use serde_json::json;

    use crate::{
        AggregationInput, EvalResults, MechanicSubjectInput, ResultsConfig, RunIdentity,
        ScenarioAggregationInput, SubjectKey, TaskSummary, aggregate_run, apply_context_capture,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::vala::eval::{
        AssertionResult, ComparisonOperator, EvalContextCapture, EvalPassGate, ScenarioId, TaskId,
    };
    use wyrd_spec::vala::ids::RunId;

    type TaskRows = Vec<(TaskId, Vec<AssertionResult>)>;
    type SubjectRows = Vec<(CardRef, TaskRows)>;

    fn card_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Agent,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("tests").expect("static space is valid")),
            uid: None,
        }
    }

    fn eval_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Eval,
            name: CardName::new("rubric").expect("static card name is valid"),
            version: VersionBlock::parse("0.1.0").expect("static version is valid"),
            space: Some(SpaceName::new("tests").expect("static space is valid")),
            uid: None,
        }
    }

    fn assertion(task: &str, passed: bool, actual: Option<serde_json::Value>) -> AssertionResult {
        AssertionResult {
            task_id: TaskId::new(task).expect("static task id is valid"),
            passed,
            actual,
            expected: json!(null),
            operator: ComparisonOperator::Equals,
            message: None,
            stage: 0,
            started_at: Utc::now(),
            duration_ms: 0,
        }
    }

    fn identity() -> RunIdentity {
        RunIdentity {
            run_id: RunId::from_string("run-test".to_owned()),
            eval_ref: eval_card_ref(),
            started_at: Utc::now(),
            ended_at: Utc::now(),
        }
    }

    fn input_with(
        scenarios: Vec<ScenarioAggregationInput>,
        config: ResultsConfig,
    ) -> AggregationInput {
        AggregationInput {
            identity: identity(),
            scenarios,
            config,
        }
    }

    fn scenario_input(
        scenario_id: &str,
        subjects: SubjectRows,
        passenger_tasks: Vec<TaskSummary>,
    ) -> ScenarioAggregationInput {
        let mut mechanic_results = BTreeMap::new();
        for (subject_ref, per_task) in subjects {
            let task_results = per_task.into_iter().collect();
            let key = SubjectKey::from_ref(&subject_ref);
            mechanic_results.insert(
                key,
                MechanicSubjectInput {
                    subject_ref,
                    task_results,
                },
            );
        }
        ScenarioAggregationInput {
            scenario_id: ScenarioId::new(scenario_id).expect("static scenario id is valid"),
            mechanic_results,
            passenger_tasks,
            conversation_history: vec![],
            started_at: Utc::now(),
            duration_ms: 10,
        }
    }

    #[test]
    fn aggregation_pass_rates_match_per_level() {
        let task_id = TaskId::new("non_empty").expect("static task id is valid");
        let subject = card_ref("agent_a");
        let scenario = scenario_input(
            "scen_a",
            vec![(
                subject.clone(),
                vec![(
                    task_id.clone(),
                    vec![
                        assertion("non_empty", true, Some(json!("hi"))),
                        assertion("non_empty", false, Some(json!(""))),
                    ],
                )],
            )],
            vec![TaskSummary {
                task_id: TaskId::new("final_response_ok").expect("static task id is valid"),
                passed: true,
                stage: 0,
                message: None,
                duration_ms: 1,
            }],
        );
        let output = aggregate_run(input_with(
            vec![scenario],
            ResultsConfig {
                context_capture: None,
                pass_gate: None,
            },
        ))
        .expect("aggregation succeeds");

        let key = SubjectKey::from_ref(&subject);
        assert_eq!(output.metrics.total_tasks, 2);
        assert_eq!(output.metrics.passed_tasks, 1);
        assert!((output.metrics.pass_rate - 0.5).abs() < f64::EPSILON);
        assert!((output.subjects[&key].metrics.pass_rate - 0.5).abs() < f64::EPSILON);
        assert_eq!(output.metrics.per_task_pass_rate[&task_id], 0.5);
        let scenario_id = ScenarioId::new("scen_a").expect("static scenario id is valid");
        assert!(!output.scenarios[&scenario_id].scenario_passed);
        assert_eq!(output.metrics.scenario_pass_rate, 0.0);
    }

    #[test]
    fn pass_gate_overall_threshold_edge() {
        let scenario = scenario_input(
            "scen_a",
            vec![(
                card_ref("agent_a"),
                vec![(
                    TaskId::new("t").expect("static task id is valid"),
                    vec![
                        assertion("t", true, None),
                        assertion("t", true, None),
                        assertion("t", false, None),
                        assertion("t", false, None),
                    ],
                )],
            )],
            vec![],
        );
        let output = aggregate_run(input_with(
            vec![scenario],
            ResultsConfig {
                context_capture: None,
                pass_gate: Some(EvalPassGate::OverallPassRate { threshold: 0.5 }),
            },
        ))
        .expect("aggregation succeeds");
        let verdict = output.pass_gate_verdict.as_ref().expect("verdict exists");
        assert!(verdict.passed, "0.5 >= 0.5 must pass");

        let scenario = scenario_input(
            "scen_b",
            vec![(
                card_ref("agent_b"),
                vec![(
                    TaskId::new("t").expect("static task id is valid"),
                    vec![assertion("t", true, None), assertion("t", false, None)],
                )],
            )],
            vec![],
        );
        let output = aggregate_run(input_with(
            vec![scenario],
            ResultsConfig {
                context_capture: None,
                pass_gate: Some(EvalPassGate::OverallPassRate { threshold: 0.51 }),
            },
        ))
        .expect("aggregation succeeds");
        let verdict = output.pass_gate_verdict.as_ref().expect("verdict exists");
        assert!(!verdict.passed, "0.5 < 0.51 must fail");
    }

    #[test]
    fn pass_gate_all_pass_on_zero_tasks_fails() {
        let output = aggregate_run(input_with(
            vec![],
            ResultsConfig {
                context_capture: None,
                pass_gate: Some(EvalPassGate::AllPass),
            },
        ))
        .expect("aggregation succeeds");
        let verdict = output.pass_gate_verdict.as_ref().expect("verdict exists");
        assert!(!verdict.passed);
        assert!(verdict.reason.contains("no tasks"));
    }

    #[test]
    fn pass_gate_all_pass_on_all_skipped_subject_set_fails() {
        let scenario = scenario_input(
            "s",
            vec![(
                card_ref("agent_a"),
                vec![(
                    TaskId::new("skipped").expect("static task id is valid"),
                    vec![assertion("skipped", false, None)],
                )],
            )],
            vec![],
        );
        let output = aggregate_run(input_with(
            vec![scenario],
            ResultsConfig {
                context_capture: None,
                pass_gate: Some(EvalPassGate::AllPass),
            },
        ))
        .expect("aggregation succeeds");
        assert!(!output.pass_gate_verdict.expect("verdict exists").passed);
    }

    #[test]
    fn context_capture_full_keeps_actual_verbatim() {
        let result = assertion("t", true, Some(json!({"x": 1})));
        let output = apply_context_capture(result, EvalContextCapture::Full)
            .expect("context capture succeeds");
        assert_eq!(output.actual, Some(json!({"x": 1})));
    }

    #[test]
    fn context_capture_hash_replaces_with_sha256_hex() {
        let result = assertion("t", true, Some(json!({"x": 1, "y": "v"})));
        let output = apply_context_capture(result, EvalContextCapture::Hash)
            .expect("context capture succeeds");
        let actual = output.actual.expect("actual is present");
        let hex = actual.as_str().expect("actual is hash string");
        assert_eq!(hex.len(), 64, "sha256 hex must be 64 chars");
        assert!(
            hex.chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
        );
    }

    #[test]
    fn context_capture_redact_drops_actual() {
        let result = assertion("t", true, Some(json!("pii")));
        let output = apply_context_capture(result, EvalContextCapture::Redact)
            .expect("context capture succeeds");
        assert!(output.actual.is_none());
    }

    #[test]
    fn save_load_json_round_trip_preserves_everything() {
        let scenario = scenario_input(
            "s",
            vec![(
                card_ref("agent_a"),
                vec![(
                    TaskId::new("t").expect("static task id is valid"),
                    vec![assertion("t", true, Some(json!("hi")))],
                )],
            )],
            vec![TaskSummary {
                task_id: TaskId::new("final").expect("static task id is valid"),
                passed: true,
                stage: 0,
                message: Some("ok".to_owned()),
                duration_ms: 2,
            }],
        );
        let output = aggregate_run(input_with(
            vec![scenario],
            ResultsConfig {
                context_capture: Some(EvalContextCapture::Full),
                pass_gate: Some(EvalPassGate::AllPass),
            },
        ))
        .expect("aggregation succeeds");

        let raw = output.to_json().expect("serialize results");
        let from_json = EvalResults::from_json(&raw).expect("deserialize results");
        assert_eq!(output, from_json);

        let temp_dir = tempfile::tempdir().expect("temp dir exists");
        let path = temp_dir.path().join("results.json");
        output.save(&path).expect("save results");
        let from_file = EvalResults::load(&path).expect("load results");
        assert_eq!(output, from_file);
    }

    #[test]
    fn empty_run_aggregation_does_not_panic() {
        let output = aggregate_run(input_with(
            vec![],
            ResultsConfig {
                context_capture: None,
                pass_gate: None,
            },
        ))
        .expect("aggregation succeeds");
        assert_eq!(output.scenarios.len(), 0);
        assert_eq!(output.subjects.len(), 0);
        assert_eq!(output.metrics.total_tasks, 0);
        assert_eq!(output.metrics.pass_rate, 0.0);
        assert_eq!(output.metrics.scenario_pass_rate, 0.0);
        assert!(output.pass_gate_verdict.is_none());

        let table = output.as_table();
        assert!(table.contains("EvalRun run-test"));
    }
}
