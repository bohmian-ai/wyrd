//! Four-quadrant baseline-vs-candidate comparison for `EvalResults`.
//!
//! Quadrants per DESIGN section 8:
//!
//! |              | aggregate                         | per-task                           |
//! |--------------|-----------------------------------|------------------------------------|
//! | per-subject  | subject pass-rate deltas          | task deltas and status changes     |
//! | system       | overall and scenario transitions  | cross-cutting per-task deltas      |
//!
//! All subject maps use [`SubjectKey`] as the canonical key. The full
//! [`CardRef`] for a subject is carried from [`SubjectResults::subject_ref`].

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{AssertionResult, ScenarioId, TaskId};
use wyrd_spec::vala::ids::RunId;

use crate::error::EvalExecError;
use crate::results::{EvalResults, ScenarioResult, SubjectKey, SubjectResults};

/// Configuration for [`compare`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompareConfig {
    /// Magnitude beyond which a pass-rate delta is flagged as changed.
    ///
    /// Negative deltas below `-regression_threshold` are regressions. Positive
    /// deltas above `regression_threshold` are improvements.
    pub regression_threshold: f64,
    /// Per-task pass-rate cutoff used to classify a task as passed.
    pub per_task_pass_cut: f64,
}

impl Default for CompareConfig {
    fn default() -> Self {
        Self {
            regression_threshold: 0.05,
            per_task_pass_cut: 0.5,
        }
    }
}

/// Direction flag for a comparison delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeFlag {
    /// Candidate dropped by more than the configured threshold.
    Regressed,
    /// Candidate improved by more than the configured threshold.
    Improved,
    /// Delta is within the configured threshold, including exact boundary.
    Unchanged,
}

impl ChangeFlag {
    fn from_delta(delta: f64, threshold: f64) -> Self {
        let boundary_epsilon = f64::EPSILON * threshold.abs().max(1.0) * 8.0;
        if delta < -threshold && (delta + threshold).abs() > boundary_epsilon {
            Self::Regressed
        } else if delta > threshold && (delta - threshold).abs() > boundary_epsilon {
            Self::Improved
        } else {
            Self::Unchanged
        }
    }
}

/// Top-level four-quadrant comparison artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonResults {
    /// Baseline run id.
    pub baseline_run_id: RunId,
    /// Candidate run id.
    pub candidate_run_id: RunId,
    /// Per-subject aggregate and per-task deltas for subjects present in both
    /// runs.
    pub subjects: BTreeMap<SubjectKey, SubjectDelta>,
    /// Per-scenario transitions for scenarios present in both runs.
    pub scenarios: BTreeMap<ScenarioId, ScenarioDelta>,
    /// System aggregate deltas.
    pub system: SystemDelta,
    /// System per-task deltas computed from run-level per-task pass rates.
    pub cross_cutting_task_deltas: BTreeMap<TaskId, TaskDelta>,
    /// Subjects present only in the baseline run.
    pub subjects_baseline_only: Vec<SubjectAbsence>,
    /// Subjects present only in the candidate run.
    pub subjects_candidate_only: Vec<SubjectAbsence>,
    /// Scenarios present only in the baseline run.
    pub scenarios_baseline_only: Vec<ScenarioId>,
    /// Scenarios present only in the candidate run.
    pub scenarios_candidate_only: Vec<ScenarioId>,
    /// Subjects whose aggregate pass rate regressed.
    pub regressed_subjects: Vec<SubjectKey>,
    /// Subjects whose aggregate pass rate improved.
    pub improved_subjects: Vec<SubjectKey>,
    /// Threshold used for this comparison.
    pub regression_threshold: f64,
    /// Per-task pass cutoff used for this comparison.
    pub per_task_pass_cut: f64,
    /// UTC creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// One subject in the comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectDelta {
    /// Canonical subject key.
    pub subject_key: SubjectKey,
    /// Full subject card reference.
    pub subject_ref: CardRef,
    /// Baseline subject pass rate.
    pub baseline_pass_rate: f64,
    /// Candidate subject pass rate.
    pub candidate_pass_rate: f64,
    /// Candidate pass rate minus baseline pass rate.
    pub pass_rate_delta: f64,
    /// Direction flag derived from the configured threshold.
    pub flag: ChangeFlag,
    /// Per-task pass-rate deltas for tasks present on both sides.
    pub per_task_deltas: BTreeMap<TaskId, TaskDelta>,
    /// Tasks whose pass/fail label changed.
    pub task_status_changes: Vec<TaskStatusChange>,
    /// Tasks present in baseline subject but absent from candidate subject.
    pub missing_tasks_candidate_only: Vec<TaskId>,
    /// Tasks present in candidate subject but absent from baseline subject.
    pub missing_tasks_baseline_only: Vec<TaskId>,
}

/// Per-task delta used in subject and system quadrants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskDelta {
    /// Task identifier.
    pub task_id: TaskId,
    /// Baseline pass rate.
    pub baseline_pass_rate: f64,
    /// Candidate pass rate.
    pub candidate_pass_rate: f64,
    /// Candidate pass rate minus baseline pass rate.
    pub delta: f64,
    /// Direction flag derived from the configured threshold.
    pub flag: ChangeFlag,
}

/// Per-task pass/fail label transition within a subject.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStatusChange {
    /// Task identifier.
    pub task_id: TaskId,
    /// Baseline label, using `CompareConfig::per_task_pass_cut`.
    pub baseline_passed: bool,
    /// Candidate label, using `CompareConfig::per_task_pass_cut`.
    pub candidate_passed: bool,
}

/// Subject present on only one side of a comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectAbsence {
    /// Canonical subject key.
    pub subject_key: SubjectKey,
    /// Full subject card reference.
    pub subject_ref: CardRef,
}

/// Per-scenario transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioDelta {
    /// Scenario identifier.
    pub scenario_id: ScenarioId,
    /// Whether the baseline scenario passed.
    pub baseline_passed: bool,
    /// Whether the candidate scenario passed.
    pub candidate_passed: bool,
    /// Whether the scenario pass/fail label changed.
    pub status_changed: bool,
    /// Per-subject pass-rate deltas inside the scenario.
    pub per_subject_deltas: BTreeMap<SubjectKey, f64>,
    /// Subjects present in baseline scenario but absent from candidate scenario.
    pub missing_subjects_candidate_only: Vec<SubjectKey>,
    /// Subjects present in candidate scenario but absent from baseline scenario.
    pub missing_subjects_baseline_only: Vec<SubjectKey>,
}

/// System-level aggregate quadrant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemDelta {
    /// Baseline overall pass rate.
    pub baseline_pass_rate: f64,
    /// Candidate overall pass rate.
    pub candidate_pass_rate: f64,
    /// Candidate overall pass rate minus baseline overall pass rate.
    pub pass_rate_delta: f64,
    /// Direction flag for the overall pass-rate delta.
    pub flag: ChangeFlag,
    /// Baseline scenario pass rate.
    pub baseline_scenario_pass_rate: f64,
    /// Candidate scenario pass rate.
    pub candidate_scenario_pass_rate: f64,
    /// Candidate scenario pass rate minus baseline scenario pass rate.
    pub scenario_pass_rate_delta: f64,
    /// Count of common scenarios whose pass/fail label changed.
    pub scenario_status_changes: usize,
}

/// Compute the four-quadrant comparison between two [`EvalResults`] artifacts.
#[must_use]
pub fn compare(
    baseline: &EvalResults,
    candidate: &EvalResults,
    config: &CompareConfig,
) -> ComparisonResults {
    let threshold = config.regression_threshold;
    let cut = config.per_task_pass_cut;

    let baseline_keys: BTreeSet<&SubjectKey> = baseline.subjects.keys().collect();
    let candidate_keys: BTreeSet<&SubjectKey> = candidate.subjects.keys().collect();

    let mut subjects = BTreeMap::new();
    let mut regressed_subjects = Vec::new();
    let mut improved_subjects = Vec::new();
    for key in baseline_keys.intersection(&candidate_keys).copied() {
        let baseline_subject = &baseline.subjects[key];
        let candidate_subject = &candidate.subjects[key];
        let delta = compute_subject_delta(key, baseline_subject, candidate_subject, threshold, cut);
        match delta.flag {
            ChangeFlag::Regressed => regressed_subjects.push(key.clone()),
            ChangeFlag::Improved => improved_subjects.push(key.clone()),
            ChangeFlag::Unchanged => {}
        }
        subjects.insert(key.clone(), delta);
    }

    let subjects_baseline_only = baseline_keys
        .difference(&candidate_keys)
        .copied()
        .map(|key| SubjectAbsence {
            subject_key: key.clone(),
            subject_ref: baseline.subjects[key].subject_ref.clone(),
        })
        .collect();
    let subjects_candidate_only = candidate_keys
        .difference(&baseline_keys)
        .copied()
        .map(|key| SubjectAbsence {
            subject_key: key.clone(),
            subject_ref: candidate.subjects[key].subject_ref.clone(),
        })
        .collect();

    let baseline_scenarios: BTreeSet<&ScenarioId> = baseline.scenarios.keys().collect();
    let candidate_scenarios: BTreeSet<&ScenarioId> = candidate.scenarios.keys().collect();
    let mut scenarios = BTreeMap::new();
    for scenario_id in baseline_scenarios
        .intersection(&candidate_scenarios)
        .copied()
    {
        scenarios.insert(
            scenario_id.clone(),
            compute_scenario_delta(
                scenario_id,
                &baseline.scenarios[scenario_id],
                &candidate.scenarios[scenario_id],
            ),
        );
    }

    let scenarios_baseline_only = baseline_scenarios
        .difference(&candidate_scenarios)
        .copied()
        .cloned()
        .collect();
    let scenarios_candidate_only = candidate_scenarios
        .difference(&baseline_scenarios)
        .copied()
        .cloned()
        .collect();

    let pass_rate_delta = candidate.metrics.pass_rate - baseline.metrics.pass_rate;
    let system = SystemDelta {
        baseline_pass_rate: baseline.metrics.pass_rate,
        candidate_pass_rate: candidate.metrics.pass_rate,
        pass_rate_delta,
        flag: ChangeFlag::from_delta(pass_rate_delta, threshold),
        baseline_scenario_pass_rate: baseline.metrics.scenario_pass_rate,
        candidate_scenario_pass_rate: candidate.metrics.scenario_pass_rate,
        scenario_pass_rate_delta: candidate.metrics.scenario_pass_rate
            - baseline.metrics.scenario_pass_rate,
        scenario_status_changes: scenarios
            .values()
            .filter(|delta| delta.status_changed)
            .count(),
    };

    let cross_cutting_task_deltas = compute_cross_cutting_task_deltas(
        &baseline.metrics.per_task_pass_rate,
        &candidate.metrics.per_task_pass_rate,
        threshold,
    );

    ComparisonResults {
        baseline_run_id: baseline.identity.run_id.clone(),
        candidate_run_id: candidate.identity.run_id.clone(),
        subjects,
        scenarios,
        system,
        cross_cutting_task_deltas,
        subjects_baseline_only,
        subjects_candidate_only,
        scenarios_baseline_only,
        scenarios_candidate_only,
        regressed_subjects,
        improved_subjects,
        regression_threshold: threshold,
        per_task_pass_cut: cut,
        created_at: Utc::now(),
    }
}

fn compute_subject_delta(
    key: &SubjectKey,
    baseline: &SubjectResults,
    candidate: &SubjectResults,
    threshold: f64,
    cut: f64,
) -> SubjectDelta {
    let pass_rate_delta = candidate.metrics.pass_rate - baseline.metrics.pass_rate;
    let baseline_tasks: BTreeSet<&TaskId> = baseline.metrics.per_task_pass_rate.keys().collect();
    let candidate_tasks: BTreeSet<&TaskId> = candidate.metrics.per_task_pass_rate.keys().collect();

    let mut per_task_deltas = BTreeMap::new();
    let mut task_status_changes = Vec::new();
    for task_id in baseline_tasks.intersection(&candidate_tasks).copied() {
        let baseline_pass_rate = baseline.metrics.per_task_pass_rate[task_id];
        let candidate_pass_rate = candidate.metrics.per_task_pass_rate[task_id];
        let delta = candidate_pass_rate - baseline_pass_rate;
        per_task_deltas.insert(
            task_id.clone(),
            TaskDelta {
                task_id: task_id.clone(),
                baseline_pass_rate,
                candidate_pass_rate,
                delta,
                flag: ChangeFlag::from_delta(delta, threshold),
            },
        );

        let baseline_passed = baseline_pass_rate >= cut;
        let candidate_passed = candidate_pass_rate >= cut;
        if baseline_passed != candidate_passed {
            task_status_changes.push(TaskStatusChange {
                task_id: task_id.clone(),
                baseline_passed,
                candidate_passed,
            });
        }
    }

    let missing_tasks_candidate_only = baseline_tasks
        .difference(&candidate_tasks)
        .copied()
        .cloned()
        .collect();
    let missing_tasks_baseline_only = candidate_tasks
        .difference(&baseline_tasks)
        .copied()
        .cloned()
        .collect();

    SubjectDelta {
        subject_key: key.clone(),
        subject_ref: baseline.subject_ref.clone(),
        baseline_pass_rate: baseline.metrics.pass_rate,
        candidate_pass_rate: candidate.metrics.pass_rate,
        pass_rate_delta,
        flag: ChangeFlag::from_delta(pass_rate_delta, threshold),
        per_task_deltas,
        task_status_changes,
        missing_tasks_candidate_only,
        missing_tasks_baseline_only,
    }
}

fn compute_scenario_delta(
    scenario_id: &ScenarioId,
    baseline: &ScenarioResult,
    candidate: &ScenarioResult,
) -> ScenarioDelta {
    let baseline_subjects: BTreeSet<&SubjectKey> = baseline.mechanic_results.keys().collect();
    let candidate_subjects: BTreeSet<&SubjectKey> = candidate.mechanic_results.keys().collect();

    let mut per_subject_deltas = BTreeMap::new();
    for subject_key in baseline_subjects.intersection(&candidate_subjects).copied() {
        let baseline_rate = scenario_subject_pass_rate(&baseline.mechanic_results[subject_key]);
        let candidate_rate = scenario_subject_pass_rate(&candidate.mechanic_results[subject_key]);
        per_subject_deltas.insert(subject_key.clone(), candidate_rate - baseline_rate);
    }

    let missing_subjects_candidate_only = baseline_subjects
        .difference(&candidate_subjects)
        .copied()
        .cloned()
        .collect();
    let missing_subjects_baseline_only = candidate_subjects
        .difference(&baseline_subjects)
        .copied()
        .cloned()
        .collect();

    ScenarioDelta {
        scenario_id: scenario_id.clone(),
        baseline_passed: baseline.scenario_passed,
        candidate_passed: candidate.scenario_passed,
        status_changed: baseline.scenario_passed != candidate.scenario_passed,
        per_subject_deltas,
        missing_subjects_candidate_only,
        missing_subjects_baseline_only,
    }
}

fn scenario_subject_pass_rate(per_task: &BTreeMap<TaskId, Vec<AssertionResult>>) -> f64 {
    let total = per_task.values().map(Vec::len).sum::<usize>();
    if total == 0 {
        return 0.0;
    }

    let passed = per_task
        .values()
        .flat_map(|rows| rows.iter())
        .filter(|row| row.passed)
        .count();
    passed as f64 / total as f64
}

fn compute_cross_cutting_task_deltas(
    baseline: &BTreeMap<TaskId, f64>,
    candidate: &BTreeMap<TaskId, f64>,
    threshold: f64,
) -> BTreeMap<TaskId, TaskDelta> {
    let baseline_tasks: BTreeSet<&TaskId> = baseline.keys().collect();
    let candidate_tasks: BTreeSet<&TaskId> = candidate.keys().collect();
    let mut out = BTreeMap::new();

    for task_id in baseline_tasks.intersection(&candidate_tasks).copied() {
        let baseline_pass_rate = baseline[task_id];
        let candidate_pass_rate = candidate[task_id];
        let delta = candidate_pass_rate - baseline_pass_rate;
        out.insert(
            task_id.clone(),
            TaskDelta {
                task_id: task_id.clone(),
                baseline_pass_rate,
                candidate_pass_rate,
                delta,
                flag: ChangeFlag::from_delta(delta, threshold),
            },
        );
    }

    out
}

impl ComparisonResults {
    /// Serialize comparison results to pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns `ResultsSerializeFailed` when JSON serialization fails.
    pub fn to_json(&self) -> Result<String, EvalExecError> {
        serde_json::to_string_pretty(self).map_err(|error| EvalExecError::ResultsSerializeFailed {
            reason: error.to_string(),
        })
    }

    /// Deserialize comparison results from JSON.
    ///
    /// # Errors
    ///
    /// Returns `ResultsDeserializeFailed` when JSON deserialization fails.
    pub fn from_json(raw: &str) -> Result<Self, EvalExecError> {
        serde_json::from_str(raw).map_err(|error| EvalExecError::ResultsDeserializeFailed {
            reason: error.to_string(),
        })
    }

    /// Save comparison results to a caller-provided local path.
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

    /// Load comparison results from a local path.
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
}

#[cfg(test)]
mod comparison_four_quadrant {
    //! Four-quadrant comparison tests.

    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use chrono::Utc;
    use serde_json::json;

    use crate::{
        AggregationInput, ChangeFlag, CompareConfig, EvalResults, MechanicSubjectInput,
        ResultsConfig, RunIdentity, ScenarioAggregationInput, SubjectKey, TaskSummary,
        aggregate_run, compare,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::vala::eval::{AssertionResult, ComparisonOperator, ScenarioId, TaskId};
    use wyrd_spec::vala::ids::RunId;

    type TaskCounts<'a> = Vec<(&'a str, usize, usize)>;
    type SubjectCounts<'a> = Vec<(CardRef, TaskCounts<'a>)>;

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("tests").expect("static space is valid"),
            uid: None,
        }
    }

    fn eval_ref() -> CardRef {
        card_ref(CardKind::Eval, "rubric")
    }

    fn assertion(task: &str, passed: bool) -> AssertionResult {
        AssertionResult {
            task_id: TaskId::new(task).expect("static task id is valid"),
            passed,
            actual: None,
            expected: json!(null),
            operator: ComparisonOperator::Equals,
            message: None,
            stage: 0,
            started_at: Utc::now(),
            duration_ms: 0,
        }
    }

    fn build_run(
        run_id: &str,
        subjects: SubjectCounts<'_>,
        scenario_id: &str,
        passenger_passed: Option<bool>,
    ) -> EvalResults {
        let mut mechanic_results = BTreeMap::new();
        for (subject_ref, task_counts) in subjects {
            let mut task_results = BTreeMap::new();
            for (task, pass_count, fail_count) in task_counts {
                let task_id = TaskId::new(task).expect("static task id is valid");
                let mut rows = Vec::with_capacity(pass_count + fail_count);
                for _ in 0..pass_count {
                    rows.push(assertion(task, true));
                }
                for _ in 0..fail_count {
                    rows.push(assertion(task, false));
                }
                task_results.insert(task_id, rows);
            }

            let subject_key = SubjectKey::from_ref(&subject_ref);
            mechanic_results.insert(
                subject_key,
                MechanicSubjectInput {
                    subject_ref,
                    task_results,
                },
            );
        }

        let passenger_tasks = passenger_passed
            .map(|passed| {
                vec![TaskSummary {
                    task_id: TaskId::new("final").expect("static task id is valid"),
                    passed,
                    stage: 0,
                    message: None,
                    duration_ms: 1,
                }]
            })
            .unwrap_or_default();

        aggregate_run(AggregationInput {
            identity: RunIdentity {
                run_id: RunId::from_string(run_id.to_owned()),
                eval_ref: eval_ref(),
                started_at: Utc::now(),
                ended_at: Utc::now(),
            },
            scenarios: vec![ScenarioAggregationInput {
                scenario_id: ScenarioId::new(scenario_id).expect("static scenario id is valid"),
                mechanic_results,
                passenger_tasks,
                conversation_history: vec![],
                started_at: Utc::now(),
                duration_ms: 10,
            }],
            config: ResultsConfig {
                context_capture: None,
                pass_gate: None,
            },
        })
        .expect("aggregation succeeds")
    }

    #[test]
    fn regression_detected_on_pass_rate_drop_above_threshold() {
        let subject = card_ref(CardKind::Agent, "agent_a");
        let baseline = build_run(
            "b",
            vec![(subject.clone(), vec![("t", 10, 0)])],
            "s",
            Some(true),
        );
        let candidate = build_run(
            "c",
            vec![(subject.clone(), vec![("t", 5, 5)])],
            "s",
            Some(true),
        );

        let out = compare(&baseline, &candidate, &CompareConfig::default());
        let key = SubjectKey::from_ref(&subject);

        assert_eq!(out.subjects[&key].flag, ChangeFlag::Regressed);
        assert!(out.regressed_subjects.contains(&key));
        assert_eq!(out.system.flag, ChangeFlag::Regressed);
        assert!((out.system.pass_rate_delta + 0.5).abs() < 1e-9);
    }

    #[test]
    fn stable_run_produces_no_regression() {
        let subject = card_ref(CardKind::Agent, "agent_a");
        let baseline = build_run(
            "b",
            vec![(subject.clone(), vec![("t", 10, 0)])],
            "s",
            Some(true),
        );
        let candidate = build_run(
            "c",
            vec![(subject.clone(), vec![("t", 10, 0)])],
            "s",
            Some(true),
        );

        let out = compare(&baseline, &candidate, &CompareConfig::default());
        let key = SubjectKey::from_ref(&subject);

        assert_eq!(out.subjects[&key].flag, ChangeFlag::Unchanged);
        assert!(out.regressed_subjects.is_empty());
        assert!(out.improved_subjects.is_empty());
    }

    #[test]
    fn improvement_detected_on_pass_rate_rise_above_threshold() {
        let subject = card_ref(CardKind::Agent, "agent_a");
        let baseline = build_run(
            "b",
            vec![(subject.clone(), vec![("t", 0, 10)])],
            "s",
            Some(false),
        );
        let candidate = build_run(
            "c",
            vec![(subject.clone(), vec![("t", 10, 0)])],
            "s",
            Some(true),
        );

        let out = compare(&baseline, &candidate, &CompareConfig::default());
        let key = SubjectKey::from_ref(&subject);

        assert_eq!(out.subjects[&key].flag, ChangeFlag::Improved);
        assert!(out.improved_subjects.contains(&key));
    }

    #[test]
    fn task_status_change_surfaced() {
        let subject = card_ref(CardKind::Agent, "agent_a");
        let baseline = build_run(
            "b",
            vec![(subject.clone(), vec![("t", 10, 0)])],
            "s",
            Some(true),
        );
        let candidate = build_run(
            "c",
            vec![(subject.clone(), vec![("t", 0, 10)])],
            "s",
            Some(false),
        );

        let out = compare(&baseline, &candidate, &CompareConfig::default());
        let key = SubjectKey::from_ref(&subject);
        let changes = &out.subjects[&key].task_status_changes;

        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].task_id,
            TaskId::new("t").expect("static task id is valid")
        );
        assert!(changes[0].baseline_passed);
        assert!(!changes[0].candidate_passed);
    }

    #[test]
    fn missing_tasks_surfaced_in_both_directions() {
        let subject = card_ref(CardKind::Agent, "agent_a");
        let baseline = build_run(
            "b",
            vec![(subject.clone(), vec![("t1", 5, 5), ("t2", 3, 0)])],
            "s",
            Some(true),
        );
        let candidate = build_run(
            "c",
            vec![(subject.clone(), vec![("t1", 5, 5), ("t3", 3, 0)])],
            "s",
            Some(true),
        );

        let out = compare(&baseline, &candidate, &CompareConfig::default());
        let key = SubjectKey::from_ref(&subject);
        let subject_delta = &out.subjects[&key];

        assert_eq!(
            subject_delta.missing_tasks_candidate_only,
            vec![TaskId::new("t2").expect("static task id is valid")]
        );
        assert_eq!(
            subject_delta.missing_tasks_baseline_only,
            vec![TaskId::new("t3").expect("static task id is valid")]
        );
    }

    #[test]
    fn new_and_removed_scenarios_surfaced() {
        let subject = card_ref(CardKind::Agent, "agent_a");
        let baseline = build_run(
            "b",
            vec![(subject.clone(), vec![("t", 5, 5)])],
            "s_only_b",
            Some(true),
        );
        let candidate = build_run(
            "c",
            vec![(subject, vec![("t", 5, 5)])],
            "s_only_c",
            Some(true),
        );

        let out = compare(&baseline, &candidate, &CompareConfig::default());

        assert_eq!(
            out.scenarios_baseline_only,
            vec![ScenarioId::new("s_only_b").expect("static scenario id is valid")]
        );
        assert_eq!(
            out.scenarios_candidate_only,
            vec![ScenarioId::new("s_only_c").expect("static scenario id is valid")]
        );
        assert!(out.scenarios.is_empty());
    }

    #[test]
    fn new_and_removed_subjects_surfaced() {
        let agent_a = card_ref(CardKind::Agent, "agent_a");
        let agent_b = card_ref(CardKind::Agent, "agent_b");
        let baseline = build_run(
            "b",
            vec![(agent_a.clone(), vec![("t", 5, 5)])],
            "s",
            Some(true),
        );
        let candidate = build_run(
            "c",
            vec![(agent_b.clone(), vec![("t", 5, 5)])],
            "s",
            Some(true),
        );

        let out = compare(&baseline, &candidate, &CompareConfig::default());

        assert_eq!(out.subjects_baseline_only.len(), 1);
        assert_eq!(
            out.subjects_baseline_only[0].subject_key,
            SubjectKey::from_ref(&agent_a)
        );
        assert_eq!(out.subjects_candidate_only.len(), 1);
        assert_eq!(
            out.subjects_candidate_only[0].subject_key,
            SubjectKey::from_ref(&agent_b)
        );
        assert!(out.subjects.is_empty());
    }

    #[test]
    fn empty_baseline_comparison_yields_zero_deltas_and_no_panic() {
        let empty = aggregate_run(AggregationInput {
            identity: RunIdentity {
                run_id: RunId::from_string("empty".to_owned()),
                eval_ref: eval_ref(),
                started_at: Utc::now(),
                ended_at: Utc::now(),
            },
            scenarios: vec![],
            config: ResultsConfig {
                context_capture: None,
                pass_gate: None,
            },
        })
        .expect("aggregation succeeds");
        let subject = card_ref(CardKind::Agent, "agent_a");
        let candidate = build_run("c", vec![(subject, vec![("t", 5, 5)])], "s", Some(true));

        let out = compare(&empty, &candidate, &CompareConfig::default());

        assert!(out.subjects.is_empty());
        assert!(out.scenarios.is_empty());
        assert_eq!(out.subjects_candidate_only.len(), 1);
        assert_eq!(out.scenarios_candidate_only.len(), 1);
        assert_eq!(out.system.flag, ChangeFlag::Improved);
    }

    #[test]
    fn threshold_exact_boundary_does_not_flag() {
        let subject = card_ref(CardKind::Agent, "agent_a");
        let baseline = build_run(
            "b",
            vec![(subject.clone(), vec![("t", 20, 0)])],
            "s",
            Some(true),
        );
        let candidate = build_run(
            "c",
            vec![(subject.clone(), vec![("t", 19, 1)])],
            "s",
            Some(true),
        );

        let out = compare(&baseline, &candidate, &CompareConfig::default());
        let key = SubjectKey::from_ref(&subject);

        assert_eq!(out.subjects[&key].flag, ChangeFlag::Unchanged);
        assert_eq!(out.system.flag, ChangeFlag::Unchanged);
    }

    #[test]
    fn zero_threshold_flags_any_movement() {
        let subject = card_ref(CardKind::Agent, "agent_a");
        let baseline = build_run(
            "b",
            vec![(subject.clone(), vec![("t", 10, 0)])],
            "s",
            Some(true),
        );
        let candidate = build_run(
            "c",
            vec![(subject.clone(), vec![("t", 9, 1)])],
            "s",
            Some(true),
        );
        let config = CompareConfig {
            regression_threshold: 0.0,
            per_task_pass_cut: 0.5,
        };

        let out = compare(&baseline, &candidate, &config);
        let key = SubjectKey::from_ref(&subject);

        assert_eq!(out.subjects[&key].flag, ChangeFlag::Regressed);
    }

    #[test]
    fn huge_threshold_never_flags() {
        let subject = card_ref(CardKind::Agent, "agent_a");
        let baseline = build_run(
            "b",
            vec![(subject.clone(), vec![("t", 10, 0)])],
            "s",
            Some(true),
        );
        let candidate = build_run(
            "c",
            vec![(subject.clone(), vec![("t", 0, 10)])],
            "s",
            Some(false),
        );
        let config = CompareConfig {
            regression_threshold: 10.0,
            per_task_pass_cut: 0.5,
        };

        let out = compare(&baseline, &candidate, &config);
        let key = SubjectKey::from_ref(&subject);

        assert_eq!(out.subjects[&key].flag, ChangeFlag::Unchanged);
        assert_eq!(out.system.flag, ChangeFlag::Unchanged);
    }

    #[test]
    fn comparison_save_load_round_trip() {
        let subject = card_ref(CardKind::Agent, "agent_a");
        let baseline = build_run(
            "b",
            vec![(subject.clone(), vec![("t", 10, 0)])],
            "s",
            Some(true),
        );
        let candidate = build_run("c", vec![(subject, vec![("t", 5, 5)])], "s", Some(true));
        let out = compare(&baseline, &candidate, &CompareConfig::default());

        let raw = out.to_json().expect("comparison serializes");
        let back = crate::ComparisonResults::from_json(&raw).expect("comparison deserializes");
        assert_eq!(out, back);

        let mut tmp: PathBuf = std::env::temp_dir();
        tmp.push(format!("vala-eval-compare-{}.json", std::process::id()));
        out.save(&tmp).expect("comparison saves");
        let back = crate::ComparisonResults::load(&tmp).expect("comparison loads");
        std::fs::remove_file(&tmp).expect("temporary comparison file is removed");
        assert_eq!(out, back);
    }
}
